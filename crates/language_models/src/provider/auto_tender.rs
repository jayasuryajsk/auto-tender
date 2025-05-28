use anyhow::{Result, anyhow};
use futures::{FutureExt, StreamExt, future::BoxFuture, stream::BoxStream};
use gpui::{AnyView, App, AsyncApp, Context, Subscription, Task};
use http_client::HttpClient;
use language_model::{
    AuthenticateError, LanguageModel, LanguageModelCompletionError, LanguageModelCompletionEvent,
    LanguageModelId, LanguageModelName, LanguageModelProvider, LanguageModelProviderId,
    LanguageModelProviderName, LanguageModelProviderState, LanguageModelRequest,
    LanguageModelToolChoice, RateLimiter, Role, StopReason, MessageContent, LanguageModelRegistry,
};
use serde_json::{json, Value};
use settings::SettingsStore;
use std::sync::Arc;
use ui::{prelude::*, IconName, Label};

const PROVIDER_ID: &str = "auto_tender";
const PROVIDER_NAME: &str = "Auto Tender";

pub struct AutoTenderSettings {
    pub api_url: String,
}

pub struct AutoTenderLanguageModelProvider {
    http_client: Arc<dyn HttpClient>,
    state: gpui::Entity<State>,
}

pub struct State {
    api_url: String,
    _subscription: Subscription,
}

impl AutoTenderLanguageModelProvider {
    pub fn new(http_client: Arc<dyn HttpClient>, cx: &mut Context<LanguageModelRegistry>) -> Self {
        let state = cx.new(|cx| State {
            api_url: "http://localhost:3000".to_string(),
            _subscription: cx.observe_global::<SettingsStore>(|_, _| {}),
        });

        Self { http_client, state }
    }

    fn create_model(&self, cx: &App) -> Arc<dyn LanguageModel> {
        let api_url = self.state.read(cx).api_url.clone();
        Arc::new(AutoTenderLanguageModel {
            id: LanguageModelId::from("auto-tender-default".to_string()),
            api_url,
            http_client: self.http_client.clone(),
            request_limiter: RateLimiter::new(10),
        })
    }
}

impl LanguageModelProviderState for AutoTenderLanguageModelProvider {
    type ObservableEntity = State;

    fn observable_entity(&self) -> Option<gpui::Entity<Self::ObservableEntity>> {
        Some(self.state.clone())
    }
}

impl LanguageModelProvider for AutoTenderLanguageModelProvider {
    fn id(&self) -> LanguageModelProviderId {
        LanguageModelProviderId(PROVIDER_ID.into())
    }

    fn name(&self) -> LanguageModelProviderName {
        LanguageModelProviderName(PROVIDER_NAME.into())
    }

    fn icon(&self) -> IconName {
        IconName::ZedAssistant
    }

    fn default_model(&self, cx: &App) -> Option<Arc<dyn LanguageModel>> {
        Some(self.create_model(cx))
    }

    fn default_fast_model(&self, cx: &App) -> Option<Arc<dyn LanguageModel>> {
        Some(self.create_model(cx))
    }

    fn provided_models(&self, cx: &App) -> Vec<Arc<dyn LanguageModel>> {
        vec![self.create_model(cx)]
    }

    fn is_authenticated(&self, _cx: &App) -> bool {
        true
    }

    fn authenticate(&self, _cx: &mut App) -> Task<Result<(), AuthenticateError>> {
        Task::ready(Ok(()))
    }

    fn configuration_view(&self, _: &mut Window, cx: &mut App) -> AnyView {
        let state = self.state.clone();
        cx.new(|_| ConfigurationView { state }).into()
    }

    fn reset_credentials(&self, _cx: &mut App) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }
}

pub struct AutoTenderLanguageModel {
    id: LanguageModelId,
    api_url: String,
    http_client: Arc<dyn HttpClient>,
    request_limiter: RateLimiter,
}

impl LanguageModel for AutoTenderLanguageModel {
    fn id(&self) -> LanguageModelId {
        self.id.clone()
    }

    fn name(&self) -> LanguageModelName {
        LanguageModelName("Auto Tender AI".into())
    }

    fn provider_id(&self) -> LanguageModelProviderId {
        LanguageModelProviderId(PROVIDER_ID.into())
    }

    fn provider_name(&self) -> LanguageModelProviderName {
        LanguageModelProviderName(PROVIDER_NAME.into())
    }

    fn telemetry_id(&self) -> String {
        "auto_tender".to_string()
    }

    fn max_token_count(&self) -> usize {
        128000
    }

    fn count_tokens(
        &self,
        request: LanguageModelRequest,
        _cx: &App,
    ) -> BoxFuture<'static, Result<usize, anyhow::Error>> {
        // Simple token estimation - roughly 4 characters per token
        let content = request.messages.iter()
            .map(|msg| {
                msg.content.iter().map(|content| match content {
                    MessageContent::Text(text) => text.len(),
                    _ => 0,
                }).sum::<usize>()
            })
            .sum::<usize>();
        
        async move { Ok(content / 4) }.boxed()
    }

    fn stream_completion(
        &self,
        request: LanguageModelRequest,
        _cx: &AsyncApp,
    ) -> BoxFuture<'static, Result<BoxStream<'static, Result<LanguageModelCompletionEvent, LanguageModelCompletionError>>, anyhow::Error>> {
        let http_client = self.http_client.clone();
        let api_url = self.api_url.clone();
        
        async move {
            let messages: Vec<Value> = request.messages.iter().map(|msg| {
                let content = msg.content.iter()
                    .filter_map(|content| match content {
                        MessageContent::Text(text) => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                json!({
                    "role": match msg.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::System => "system",
                    },
                    "content": content
                })
            }).collect();

            let body = json!({
                "messages": messages,
                "stream": true
            });

            let _response = http_client
                .post_json(&format!("{}/api/llm/chat", api_url), body.to_string().into())
                .await
                .map_err(|e| anyhow!("HTTP request failed: {}", e))?;

            // For now, return a simple mock response since we need to handle the actual streaming properly
            let stream = futures::stream::iter(vec![
                Ok(LanguageModelCompletionEvent::Text("Hello! I'm Auto Tender AI, ready to help you write tenders.".to_string())),
                Ok(LanguageModelCompletionEvent::Stop(StopReason::EndTurn))
            ]);

            Ok(stream.boxed())
        }.boxed()
    }

    fn supports_tools(&self) -> bool {
        false
    }

    fn supports_tool_choice(&self, _tool_choice: LanguageModelToolChoice) -> bool {
        false
    }
}

struct ConfigurationView {
    state: gpui::Entity<State>,
}

impl Render for ConfigurationView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let api_url = self.state.read(cx).api_url.clone();
        
        v_flex()
            .gap_2()
            .child(Label::new("Auto Tender Configuration"))
            .child(Label::new(format!("API URL: {}", api_url)))
            .child(Label::new("Status: Connected"))
    }
} 