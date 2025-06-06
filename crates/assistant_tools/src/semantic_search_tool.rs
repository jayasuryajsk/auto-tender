use crate::schema::json_schema_for;
use anyhow::{Context as _, Result, anyhow};
use assistant_tool::{ActionLog, Tool, ToolResult, ToolResultOutput};
use gpui::{AnyWindowHandle, App, Entity, Task};
use language_model::{LanguageModel, LanguageModelRequest, LanguageModelToolSchemaFormat};
use project::Project;
use schemars::JsonSchema;
use semantic_index::SemanticDb;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ui::IconName;

#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
pub struct SemanticSearchToolInput {
    /// The search query to find relevant documents
    pub query: String,
    /// Maximum number of results to return (default: 5)
    #[serde(default = "default_limit")]
    pub limit: Option<usize>,
}

fn default_limit() -> Option<usize> {
    Some(5)
}

pub struct SemanticSearchTool;

impl Tool for SemanticSearchTool {
    fn name(&self) -> String {
        "semantic_search".to_string()
    }

    fn description(&self) -> String {
        "Search through all indexed documents in the project using semantic similarity. Use this when you need to find relevant information that might be scattered across multiple documents.".to_string()
    }

    fn icon(&self) -> IconName {
        IconName::MagnifyingGlass
    }

    fn needs_confirmation(&self, _input: &serde_json::Value, _cx: &App) -> bool {
        false
    }

    fn input_schema(&self, format: LanguageModelToolSchemaFormat) -> Result<serde_json::Value> {
        json_schema_for::<SemanticSearchToolInput>(format)
    }

    fn ui_text(&self, input: &serde_json::Value) -> String {
        match serde_json::from_value::<SemanticSearchToolInput>(input.clone()) {
            Ok(input) => format!("Search documents for: \"{}\"", input.query),
            Err(_) => "Search documents".to_string(),
        }
    }

    fn run(
        self: Arc<Self>,
        input: serde_json::Value,
        _request: Arc<LanguageModelRequest>,
        project: Entity<Project>,
        _action_log: Entity<ActionLog>,
        _model: Arc<dyn LanguageModel>,
        _window: Option<AnyWindowHandle>,
        cx: &mut App,
    ) -> ToolResult {
        let input = match serde_json::from_value::<SemanticSearchToolInput>(input) {
            Ok(input) => input,
            Err(err) => return Task::ready(Err(anyhow!(err))).into(),
        };

        let query = input.query.clone();
        let limit = input.limit.unwrap_or(5);

        let output = cx.spawn(async move |cx| {
            // Check if SemanticDb is available
            let semantic_db_available = cx.update_global::<SemanticDb, _>(|_db, _cx| true)
                .unwrap_or(false);

            if !semantic_db_available {
                return Ok(ToolResultOutput {
                    content: "Semantic search is not available. Please ensure documents are indexed.".to_string(),
                    output: Some(serde_json::json!({
                        "results": [],
                        "message": "Semantic search is not available. Please ensure documents are indexed."
                    }))
                });
            }

            // Get the project index
            let project_index = cx.update_global::<SemanticDb, _>(|db, cx| {
                db.project_index(project.clone(), cx)
            }).ok().flatten();

            let Some(project_index) = project_index else {
                return Ok(ToolResultOutput {
                    content: "No semantic index found for this project.".to_string(),
                    output: Some(serde_json::json!({
                        "results": [],
                        "message": "No semantic index found for this project."
                    }))
                });
            };

            log::info!("🔍 Semantic search for: \"{}\" (limit: {})", query, limit);

            // Perform the search
            let search_results = project_index.read_with(cx, |index, cx| {
                index.search(vec![query.clone()], limit, cx)
            }).ok();

            let Some(search_task) = search_results else {
                return Ok(ToolResultOutput {
                    content: "Failed to perform semantic search.".to_string(),
                    output: Some(serde_json::json!({
                        "results": [],
                        "message": "Failed to perform semantic search."
                    }))
                });
            };

            let search_results = search_task.await.context("Search failed")?;
            log::info!("📄 Found {} search results", search_results.len());
            
            let search_results_count = search_results.len();

            if search_results.is_empty() {
                return Ok(ToolResultOutput {
                    content: format!("No relevant documents found for query: \"{}\"", query),
                    output: Some(serde_json::json!({
                        "results": [],
                        "message": format!("No relevant documents found for query: \"{}\"", query)
                    }))
                });
            }

            // Load the search results content from database
            
            // Get database connection from SemanticDb
            let db_connection = match cx.update_global::<SemanticDb, _>(|db, _cx| {
                db.get_db_connection()
            }) {
                Ok(conn) => conn,
                Err(e) => {
                    log::warn!("⚠️ Failed to access semantic database: {}", e);
                    return Ok(ToolResultOutput {
                        content: format!(
                            "Found {} relevant document(s) for query: \"{}\", but could not access the semantic database.",
                            search_results_count,
                            query
                        ),
                        output: Some(serde_json::json!({
                            "results": [],
                            "message": format!(
                                "Found {} relevant document(s) for query: \"{}\", but could not access database.",
                                search_results_count,
                                query
                            ),
                            "error": "database_access_error"
                        }))
                    });
                }
            };
            
            let loaded_results = match semantic_index::SemanticDb::load_results(db_connection, search_results, &cx).await {
                Ok(results) => {
                    log::info!("✅ Successfully loaded {} file contents", results.len());
                    results
                }
                Err(e) => {
                    log::warn!("⚠️ Failed to load some search results due to database issues: {}", e);
                    // Return a helpful message instead of failing completely
                    return Ok(ToolResultOutput {
                        content: format!(
                            "Found {} relevant document(s) for query: \"{}\", but encountered issues loading content from the database.",
                            search_results_count,
                            query
                        ),
                        output: Some(serde_json::json!({
                            "results": [],
                            "message": format!(
                                "Found {} relevant document(s) for query: \"{}\", but encountered database issues.",
                                search_results_count,
                                query
                            ),
                            "error": "database_error"
                        }))
                    });
                }
            };

            // Convert to output format
            let results: Vec<SearchResult> = loaded_results
                .into_iter()
                .map(|result| SearchResult {
                    file_path: result.path.to_string_lossy().to_string(),
                    excerpt: result.excerpt_content,
                    line_start: *result.row_range.start(),
                    line_end: *result.row_range.end(),
                })
                .collect();

            let message = format!("Found {} relevant document(s) for query: \"{}\"", results.len(), query);
            
            // Create content summary for the AI
            let content = if results.is_empty() {
                message.clone()
            } else {
                format!("{}:\n\n{}", 
                    message,
                    results.iter()
                        .map(|r| format!("**{}** (lines {}-{}):\n{}\n", 
                            r.file_path, 
                            r.line_start, 
                            r.line_end,
                            r.excerpt))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            };

            Ok(ToolResultOutput {
                content,
                output: Some(serde_json::json!({
                    "results": results,
                    "message": message
                }))
            })
        });

        ToolResult {
            output,
            card: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    pub file_path: String,
    pub excerpt: String,
    pub line_start: u32,
    pub line_end: u32,
}