use std::fmt::{self, Display, Formatter, Write as _};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::{ops::Range, path::Path, sync::Arc};

use assistant_context_editor::AssistantContext;
use assistant_tool::outline;
use collections::{HashMap, HashSet};
use editor::display_map::CreaseId;
use editor::{Addon, Editor};
use futures::future;
use futures::{FutureExt, future::Shared};
use gpui::{App, AppContext as _, Entity, SharedString, Subscription, Task};
use language::{Buffer, ParseStatus};
use language_model::{LanguageModelImage, LanguageModelRequestMessage, MessageContent};
use project::{Project, ProjectEntryId, ProjectPath, Worktree};
use prompt_store::{PromptStore, UserPromptId};
use ref_cast::RefCast;
use rope::Point;
use semantic_index::SemanticDb;
use text::{Anchor, OffsetRangeExt as _};
use ui::{Context, ElementId, IconName};
use util::markdown::MarkdownCodeBlock;
use util::{ResultExt as _, post_inc};

use crate::context_store::{ContextStore, ContextStoreEvent};
use crate::thread::Thread;


pub const RULES_ICON: IconName = IconName::Context;

pub enum ContextKind {
    File,
    Directory,
    Symbol,
    Selection,
    FetchedUrl,
    Thread,
    TextThread,
    Rules,
    Image,
}

impl ContextKind {
    pub fn icon(&self) -> IconName {
        match self {
            ContextKind::File => IconName::File,
            ContextKind::Directory => IconName::Folder,
            ContextKind::Symbol => IconName::Code,
            ContextKind::Selection => IconName::Context,
            ContextKind::FetchedUrl => IconName::Globe,
            ContextKind::Thread => IconName::MessageBubbles,
            ContextKind::TextThread => IconName::MessageBubbles,
            ContextKind::Rules => RULES_ICON,
            ContextKind::Image => IconName::Image,
        }
    }
}

/// Handle for context that can be attached to a user message.
///
/// This uses IDs that are stable enough for tracking renames and identifying when context has
/// already been added to the thread. To use this in a set, wrap it in `AgentContextKey` to opt in
/// to `PartialEq` and `Hash` impls that use the subset of the fields used for this stable identity.
#[derive(Debug, Clone)]
pub enum AgentContextHandle {
    File(FileContextHandle),
    Directory(DirectoryContextHandle),
    Symbol(SymbolContextHandle),
    Selection(SelectionContextHandle),
    FetchedUrl(FetchedUrlContext),
    Thread(ThreadContextHandle),
    TextThread(TextThreadContextHandle),
    Rules(RulesContextHandle),
    Image(ImageContext),
}

impl AgentContextHandle {
    pub fn id(&self) -> ContextId {
        match self {
            Self::File(context) => context.context_id,
            Self::Directory(context) => context.context_id,
            Self::Symbol(context) => context.context_id,
            Self::Selection(context) => context.context_id,
            Self::FetchedUrl(context) => context.context_id,
            Self::Thread(context) => context.context_id,
            Self::TextThread(context) => context.context_id,
            Self::Rules(context) => context.context_id,
            Self::Image(context) => context.context_id,
        }
    }

    pub fn element_id(&self, name: SharedString) -> ElementId {
        ElementId::NamedInteger(name, self.id().0)
    }
}

/// Loaded context that can be attached to a user message. This can be thought of as a
/// snapshot of the context along with an `AgentContextHandle`.
#[derive(Debug, Clone)]
pub enum AgentContext {
    File(FileContext),
    Directory(DirectoryContext),
    Symbol(SymbolContext),
    Selection(SelectionContext),
    FetchedUrl(FetchedUrlContext),
    Thread(ThreadContext),
    TextThread(TextThreadContext),
    Rules(RulesContext),
    Image(ImageContext),
}

impl AgentContext {
    pub fn handle(&self) -> AgentContextHandle {
        match self {
            AgentContext::File(context) => AgentContextHandle::File(context.handle.clone()),
            AgentContext::Directory(context) => {
                AgentContextHandle::Directory(context.handle.clone())
            }
            AgentContext::Symbol(context) => AgentContextHandle::Symbol(context.handle.clone()),
            AgentContext::Selection(context) => {
                AgentContextHandle::Selection(context.handle.clone())
            }
            AgentContext::FetchedUrl(context) => AgentContextHandle::FetchedUrl(context.clone()),
            AgentContext::Thread(context) => AgentContextHandle::Thread(context.handle.clone()),
            AgentContext::TextThread(context) => {
                AgentContextHandle::TextThread(context.handle.clone())
            }
            AgentContext::Rules(context) => AgentContextHandle::Rules(context.handle.clone()),
            AgentContext::Image(context) => AgentContextHandle::Image(context.clone()),
        }
    }
}

/// ID created at time of context add, for use in ElementId. This is not the stable identity of a
/// context, instead that's handled by the `PartialEq` and `Hash` impls of `AgentContextKey`.
#[derive(Debug, Copy, Clone)]
pub struct ContextId(u64);

impl ContextId {
    pub fn zero() -> Self {
        ContextId(0)
    }

    fn for_lookup() -> Self {
        ContextId(u64::MAX)
    }

    pub fn post_inc(&mut self) -> Self {
        Self(post_inc(&mut self.0))
    }
}

/// File context provides the entire contents of a file.
///
/// This holds an `Entity<Buffer>` so that file path renames affect its display and so that it can
/// be opened even if the file has been deleted. An alternative might be to use `ProjectEntryId`,
/// but then when deleted there is no path info or ability to open.
#[derive(Debug, Clone)]
pub struct FileContextHandle {
    pub buffer: Entity<Buffer>,
    pub context_id: ContextId,
}

#[derive(Debug, Clone)]
pub struct FileContext {
    pub handle: FileContextHandle,
    pub full_path: Arc<Path>,
    pub text: SharedString,
    pub is_outline: bool,
}

impl FileContextHandle {
    pub fn eq_for_key(&self, other: &Self) -> bool {
        self.buffer == other.buffer
    }

    pub fn hash_for_key<H: Hasher>(&self, state: &mut H) {
        self.buffer.hash(state)
    }

    pub fn project_path(&self, cx: &App) -> Option<ProjectPath> {
        let file = self.buffer.read(cx).file()?;
        Some(ProjectPath {
            worktree_id: file.worktree_id(cx),
            path: file.path().clone(),
        })
    }

    fn load(self, cx: &App) -> Task<Option<(AgentContext, Vec<Entity<Buffer>>)>> {
        let buffer_ref = self.buffer.read(cx);
        let Some(file) = buffer_ref.file() else {
            log::error!("file context missing path");
            return Task::ready(None);
        };
        let full_path: Arc<Path> = file.full_path(cx).into();
        let rope = buffer_ref.as_rope().clone();
        let buffer = self.buffer.clone();

        cx.spawn(async move |cx| {
            // For large files, use outline instead of full content
            if rope.len() > outline::AUTO_OUTLINE_SIZE {
                // Wait until the buffer has been fully parsed, so we can read its outline
                if let Ok(mut parse_status) =
                    buffer.read_with(cx, |buffer, _| buffer.parse_status())
                {
                    while *parse_status.borrow() != ParseStatus::Idle {
                        parse_status.changed().await.log_err();
                    }

                    if let Ok(snapshot) = buffer.read_with(cx, |buffer, _| buffer.snapshot()) {
                        if let Some(outline) = snapshot.outline(None) {
                            let items = outline
                                .items
                                .into_iter()
                                .map(|item| item.to_point(&snapshot));

                            if let Ok(outline_text) =
                                outline::render_outline(items, None, 0, usize::MAX).await
                            {
                                let context = AgentContext::File(FileContext {
                                    handle: self,
                                    full_path,
                                    text: outline_text.into(),
                                    is_outline: true,
                                });
                                return Some((context, vec![buffer]));
                            }
                        }
                    }
                }
            }

            // Fallback to full content if we couldn't build an outline
            // (or didn't need to because the file was small enough)
            let context = AgentContext::File(FileContext {
                handle: self,
                full_path,
                text: rope.to_string().into(),
                is_outline: false,
            });
            Some((context, vec![buffer]))
        })
    }
}

impl Display for FileContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            MarkdownCodeBlock {
                tag: &codeblock_tag(&self.full_path, None),
                text: &self.text,
            }
        )
    }
}

/// Directory contents provides the entire contents of text files in a directory.
///
/// This has a `ProjectEntryId` so that it follows renames.
#[derive(Debug, Clone)]
pub struct DirectoryContextHandle {
    pub entry_id: ProjectEntryId,
    pub context_id: ContextId,
}

#[derive(Debug, Clone)]
pub struct DirectoryContext {
    pub handle: DirectoryContextHandle,
    pub full_path: Arc<Path>,
    pub descendants: Vec<DirectoryContextDescendant>,
}

#[derive(Debug, Clone)]
pub struct DirectoryContextDescendant {
    /// Path within the directory.
    pub rel_path: Arc<Path>,
    pub fenced_codeblock: SharedString,
}

impl DirectoryContextHandle {
    pub fn eq_for_key(&self, other: &Self) -> bool {
        self.entry_id == other.entry_id
    }

    pub fn hash_for_key<H: Hasher>(&self, state: &mut H) {
        self.entry_id.hash(state)
    }

    fn load(
        self,
        project: Entity<Project>,
        cx: &mut App,
    ) -> Task<Option<(AgentContext, Vec<Entity<Buffer>>)>> {
        let Some(worktree) = project.read(cx).worktree_for_entry(self.entry_id, cx) else {
            return Task::ready(None);
        };
        let worktree_ref = worktree.read(cx);
        let Some(entry) = worktree_ref.entry_for_id(self.entry_id) else {
            return Task::ready(None);
        };
        if entry.is_file() {
            log::error!("DirectoryContext unexpectedly refers to a file.");
            return Task::ready(None);
        }

        let directory_path = entry.path.clone();
        let directory_full_path = worktree_ref.full_path(&directory_path).into();

        let file_paths = collect_files_in_path(worktree_ref, &directory_path);
        let descendants_future = future::join_all(file_paths.into_iter().map(|path| {
            let worktree_ref = worktree.read(cx);
            let worktree_id = worktree_ref.id();
            let full_path = worktree_ref.full_path(&path);

            let rel_path = path
                .strip_prefix(&directory_path)
                .log_err()
                .map_or_else(|| path.clone(), |rel_path| rel_path.into());

            let open_task = project.update(cx, |project, cx| {
                project.buffer_store().update(cx, |buffer_store, cx| {
                    let project_path = ProjectPath { worktree_id, path };
                    buffer_store.open_buffer(project_path, cx)
                })
            });

            // TODO: report load errors instead of just logging
            let rope_task = cx.spawn(async move |cx| {
                let buffer = open_task.await.log_err()?;
                let rope = buffer
                    .read_with(cx, |buffer, _cx| buffer.as_rope().clone())
                    .log_err()?;
                Some((rope, buffer))
            });

            cx.background_spawn(async move {
                let (rope, buffer) = rope_task.await?;
                let fenced_codeblock = MarkdownCodeBlock {
                    tag: &codeblock_tag(&full_path, None),
                    text: &rope.to_string(),
                }
                .to_string()
                .into();
                let descendant = DirectoryContextDescendant {
                    rel_path,
                    fenced_codeblock,
                };
                Some((descendant, buffer))
            })
        }));

        cx.background_spawn(async move {
            let (descendants, buffers) = descendants_future.await.into_iter().flatten().unzip();
            let context = AgentContext::Directory(DirectoryContext {
                handle: self,
                full_path: directory_full_path,
                descendants,
            });
            Some((context, buffers))
        })
    }
}

impl Display for DirectoryContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut is_first = true;
        for descendant in &self.descendants {
            if !is_first {
                write!(f, "\n")?;
            } else {
                is_first = false;
            }
            write!(f, "{}", descendant.fenced_codeblock)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SymbolContextHandle {
    pub buffer: Entity<Buffer>,
    pub symbol: SharedString,
    pub range: Range<Anchor>,
    /// The range that fully contains the symbol. e.g. for function symbol, this will include not
    /// only the signature, but also the body. Not used by `PartialEq` or `Hash` for
    /// `AgentContextKey`.
    pub enclosing_range: Range<Anchor>,
    pub context_id: ContextId,
}

#[derive(Debug, Clone)]
pub struct SymbolContext {
    pub handle: SymbolContextHandle,
    pub full_path: Arc<Path>,
    pub line_range: Range<Point>,
    pub text: SharedString,
}

impl SymbolContextHandle {
    pub fn eq_for_key(&self, other: &Self) -> bool {
        self.buffer == other.buffer && self.symbol == other.symbol && self.range == other.range
    }

    pub fn hash_for_key<H: Hasher>(&self, state: &mut H) {
        self.buffer.hash(state);
        self.symbol.hash(state);
        self.range.hash(state);
    }

    pub fn full_path(&self, cx: &App) -> Option<PathBuf> {
        Some(self.buffer.read(cx).file()?.full_path(cx))
    }

    pub fn enclosing_line_range(&self, cx: &App) -> Range<Point> {
        self.enclosing_range
            .to_point(&self.buffer.read(cx).snapshot())
    }

    pub fn text(&self, cx: &App) -> SharedString {
        self.buffer
            .read(cx)
            .text_for_range(self.enclosing_range.clone())
            .collect::<String>()
            .into()
    }

    fn load(self, cx: &App) -> Task<Option<(AgentContext, Vec<Entity<Buffer>>)>> {
        let buffer_ref = self.buffer.read(cx);
        let Some(file) = buffer_ref.file() else {
            log::error!("symbol context's file has no path");
            return Task::ready(None);
        };
        let full_path = file.full_path(cx).into();
        let line_range = self.enclosing_range.to_point(&buffer_ref.snapshot());
        let text = self.text(cx);
        let buffer = self.buffer.clone();
        let context = AgentContext::Symbol(SymbolContext {
            handle: self,
            full_path,
            line_range,
            text,
        });
        Task::ready(Some((context, vec![buffer])))
    }
}

impl Display for SymbolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code_block = MarkdownCodeBlock {
            tag: &codeblock_tag(&self.full_path, Some(self.line_range.clone())),
            text: &self.text,
        };
        write!(f, "{code_block}",)
    }
}

#[derive(Debug, Clone)]
pub struct SelectionContextHandle {
    pub buffer: Entity<Buffer>,
    pub range: Range<Anchor>,
    pub context_id: ContextId,
}

#[derive(Debug, Clone)]
pub struct SelectionContext {
    pub handle: SelectionContextHandle,
    pub full_path: Arc<Path>,
    pub line_range: Range<Point>,
    pub text: SharedString,
}

impl SelectionContextHandle {
    pub fn eq_for_key(&self, other: &Self) -> bool {
        self.buffer == other.buffer && self.range == other.range
    }

    pub fn hash_for_key<H: Hasher>(&self, state: &mut H) {
        self.buffer.hash(state);
        self.range.hash(state);
    }

    pub fn full_path(&self, cx: &App) -> Option<PathBuf> {
        Some(self.buffer.read(cx).file()?.full_path(cx))
    }

    pub fn line_range(&self, cx: &App) -> Range<Point> {
        self.range.to_point(&self.buffer.read(cx).snapshot())
    }

    pub fn text(&self, cx: &App) -> SharedString {
        self.buffer
            .read(cx)
            .text_for_range(self.range.clone())
            .collect::<String>()
            .into()
    }

    fn load(self, cx: &App) -> Task<Option<(AgentContext, Vec<Entity<Buffer>>)>> {
        let Some(full_path) = self.full_path(cx) else {
            log::error!("selection context's file has no path");
            return Task::ready(None);
        };
        let text = self.text(cx);
        let buffer = self.buffer.clone();
        let context = AgentContext::Selection(SelectionContext {
            full_path: full_path.into(),
            line_range: self.line_range(cx),
            text,
            handle: self,
        });

        Task::ready(Some((context, vec![buffer])))
    }
}

impl Display for SelectionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code_block = MarkdownCodeBlock {
            tag: &codeblock_tag(&self.full_path, Some(self.line_range.clone())),
            text: &self.text,
        };
        write!(f, "{code_block}",)
    }
}

#[derive(Debug, Clone)]
pub struct FetchedUrlContext {
    pub url: SharedString,
    /// Text contents of the fetched url. Unlike other context types, the contents of this gets
    /// populated when added rather than when sending the message. Not used by `PartialEq` or `Hash`
    /// for `AgentContextKey`.
    pub text: SharedString,
    pub context_id: ContextId,
}

impl FetchedUrlContext {
    pub fn eq_for_key(&self, other: &Self) -> bool {
        self.url == other.url
    }

    pub fn hash_for_key<H: Hasher>(&self, state: &mut H) {
        self.url.hash(state);
    }

    pub fn lookup_key(url: SharedString) -> AgentContextKey {
        AgentContextKey(AgentContextHandle::FetchedUrl(FetchedUrlContext {
            url,
            text: "".into(),
            context_id: ContextId::for_lookup(),
        }))
    }

    pub fn load(self) -> Task<Option<(AgentContext, Vec<Entity<Buffer>>)>> {
        Task::ready(Some((AgentContext::FetchedUrl(self), vec![])))
    }
}

impl Display for FetchedUrlContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: Better format - url and contents are not delimited.
        write!(f, "{}\n{}\n", self.url, self.text)
    }
}

#[derive(Debug, Clone)]
pub struct ThreadContextHandle {
    pub thread: Entity<Thread>,
    pub context_id: ContextId,
}

#[derive(Debug, Clone)]
pub struct ThreadContext {
    pub handle: ThreadContextHandle,
    pub title: SharedString,
    pub text: SharedString,
}

impl ThreadContextHandle {
    pub fn eq_for_key(&self, other: &Self) -> bool {
        self.thread == other.thread
    }

    pub fn hash_for_key<H: Hasher>(&self, state: &mut H) {
        self.thread.hash(state)
    }

    pub fn title(&self, cx: &App) -> SharedString {
        self.thread
            .read(cx)
            .summary()
            .unwrap_or_else(|| "New thread".into())
    }

    fn load(self, cx: &App) -> Task<Option<(AgentContext, Vec<Entity<Buffer>>)>> {
        cx.spawn(async move |cx| {
            let text = Thread::wait_for_detailed_summary_or_text(&self.thread, cx).await?;
            let title = self
                .thread
                .read_with(cx, |thread, _cx| {
                    thread.summary().unwrap_or_else(|| "New thread".into())
                })
                .ok()?;
            let context = AgentContext::Thread(ThreadContext {
                title,
                text,
                handle: self,
            });
            Some((context, vec![]))
        })
    }
}

impl Display for ThreadContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // TODO: Better format for this - doesn't distinguish title and contents.
        write!(f, "{}\n{}\n", &self.title, &self.text.trim())
    }
}

#[derive(Debug, Clone)]
pub struct TextThreadContextHandle {
    pub context: Entity<AssistantContext>,
    pub context_id: ContextId,
}

#[derive(Debug, Clone)]
pub struct TextThreadContext {
    pub handle: TextThreadContextHandle,
    pub title: SharedString,
    pub text: SharedString,
}

impl TextThreadContextHandle {
    // pub fn lookup_key() ->
    pub fn eq_for_key(&self, other: &Self) -> bool {
        self.context == other.context
    }

    pub fn hash_for_key<H: Hasher>(&self, state: &mut H) {
        self.context.hash(state)
    }

    pub fn title(&self, cx: &App) -> SharedString {
        self.context.read(cx).summary_or_default()
    }

    fn load(self, cx: &App) -> Task<Option<(AgentContext, Vec<Entity<Buffer>>)>> {
        let title = self.title(cx);
        let text = self.context.read(cx).to_xml(cx);
        let context = AgentContext::TextThread(TextThreadContext {
            title,
            text: text.into(),
            handle: self,
        });
        Task::ready(Some((context, vec![])))
    }
}

impl Display for TextThreadContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // TODO: escape title?
        write!(f, "<text_thread title=\"{}\">\n", self.title)?;
        write!(f, "{}", self.text.trim())?;
        write!(f, "\n</text_thread>")
    }
}

#[derive(Debug, Clone)]
pub struct RulesContextHandle {
    pub prompt_id: UserPromptId,
    pub context_id: ContextId,
}

#[derive(Debug, Clone)]
pub struct RulesContext {
    pub handle: RulesContextHandle,
    pub title: Option<SharedString>,
    pub text: SharedString,
}

impl RulesContextHandle {
    pub fn eq_for_key(&self, other: &Self) -> bool {
        self.prompt_id == other.prompt_id
    }

    pub fn hash_for_key<H: Hasher>(&self, state: &mut H) {
        self.prompt_id.hash(state)
    }

    pub fn lookup_key(prompt_id: UserPromptId) -> AgentContextKey {
        AgentContextKey(AgentContextHandle::Rules(RulesContextHandle {
            prompt_id,
            context_id: ContextId::for_lookup(),
        }))
    }

    pub fn load(
        self,
        prompt_store: &Option<Entity<PromptStore>>,
        cx: &App,
    ) -> Task<Option<(AgentContext, Vec<Entity<Buffer>>)>> {
        let Some(prompt_store) = prompt_store.as_ref() else {
            return Task::ready(None);
        };
        let prompt_store = prompt_store.read(cx);
        let prompt_id = self.prompt_id.into();
        let Some(metadata) = prompt_store.metadata(prompt_id) else {
            return Task::ready(None);
        };
        let title = metadata.title;
        let text_task = prompt_store.load(prompt_id, cx);
        cx.background_spawn(async move {
            // TODO: report load errors instead of just logging
            let text = text_task.await.log_err()?.into();
            let context = AgentContext::Rules(RulesContext {
                handle: self,
                title,
                text,
            });
            Some((context, vec![]))
        })
    }
}

impl Display for RulesContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(title) = &self.title {
            write!(f, "Rules title: {}\n", title)?;
        }
        let code_block = MarkdownCodeBlock {
            tag: "",
            text: self.text.trim(),
        };
        write!(f, "{code_block}")
    }
}

#[derive(Debug, Clone)]
pub struct ImageContext {
    pub project_path: Option<ProjectPath>,
    pub original_image: Arc<gpui::Image>,
    // TODO: handle this elsewhere and remove `ignore-interior-mutability` opt-out in clippy.toml
    // needed due to a false positive of `clippy::mutable_key_type`.
    pub image_task: Shared<Task<Option<LanguageModelImage>>>,
    pub context_id: ContextId,
}

pub enum ImageStatus {
    Loading,
    Error,
    Ready,
}

impl ImageContext {
    pub fn eq_for_key(&self, other: &Self) -> bool {
        self.original_image.id() == other.original_image.id()
    }

    pub fn hash_for_key<H: Hasher>(&self, state: &mut H) {
        self.original_image.id().hash(state);
    }

    pub fn image(&self) -> Option<LanguageModelImage> {
        self.image_task.clone().now_or_never().flatten()
    }

    pub fn status(&self) -> ImageStatus {
        match self.image_task.clone().now_or_never() {
            None => ImageStatus::Loading,
            Some(None) => ImageStatus::Error,
            Some(Some(_)) => ImageStatus::Ready,
        }
    }

    pub fn load(self, cx: &App) -> Task<Option<(AgentContext, Vec<Entity<Buffer>>)>> {
        cx.background_spawn(async move {
            self.image_task.clone().await;
            Some((AgentContext::Image(self), vec![]))
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ContextLoadResult {
    pub loaded_context: LoadedContext,
    pub referenced_buffers: HashSet<Entity<Buffer>>,
}

#[derive(Debug, Clone, Default)]
pub struct LoadedContext {
    pub contexts: Vec<AgentContext>,
    pub text: String,
    pub images: Vec<LanguageModelImage>,
}

impl LoadedContext {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.images.is_empty()
    }

    pub fn add_to_request_message(&self, request_message: &mut LanguageModelRequestMessage) {
        if !self.text.is_empty() {
            request_message
                .content
                .push(MessageContent::Text(self.text.to_string()));
        }

        if !self.images.is_empty() {
            // Some providers only support image parts after an initial text part
            if request_message.content.is_empty() {
                request_message
                    .content
                    .push(MessageContent::Text("Images attached by user:".to_string()));
            }

            for image in &self.images {
                request_message
                    .content
                    .push(MessageContent::Image(image.clone()))
            }
        }
    }
}

/// Loads and formats a collection of contexts.
pub fn load_context(
    contexts: Vec<AgentContextHandle>,
    project: &Entity<Project>,
    prompt_store: &Option<Entity<PromptStore>>,
    cx: &mut App,
) -> Task<ContextLoadResult> {
    let mut load_tasks = Vec::new();

    for context in contexts.iter().cloned() {
        match context {
            AgentContextHandle::File(context) => load_tasks.push(context.load(cx)),
            AgentContextHandle::Directory(context) => {
                load_tasks.push(context.load(project.clone(), cx))
            }
            AgentContextHandle::Symbol(context) => load_tasks.push(context.load(cx)),
            AgentContextHandle::Selection(context) => load_tasks.push(context.load(cx)),
            AgentContextHandle::FetchedUrl(context) => load_tasks.push(context.load()),
            AgentContextHandle::Thread(context) => load_tasks.push(context.load(cx)),
            AgentContextHandle::TextThread(context) => load_tasks.push(context.load(cx)),
            AgentContextHandle::Rules(context) => load_tasks.push(context.load(prompt_store, cx)),
            AgentContextHandle::Image(context) => load_tasks.push(context.load(cx)),
        }
    }

    // Semantic search is now on-demand via tools, not pre-fetched
    // let semantic_search_task = search_semantic_index(project, cx);

    cx.background_spawn(async move {
        let load_results = future::join_all(load_tasks).await;

        let mut contexts = Vec::new();
        let mut text = String::new();
        let mut referenced_buffers = HashSet::default();
        
        // Process regular context results
        for context in load_results {
            let Some((context, buffers)) = context else {
                continue;
            };
            contexts.push(context);
            referenced_buffers.extend(buffers);
        }

        let mut file_context = Vec::new();
        let mut directory_context = Vec::new();
        let mut symbol_context = Vec::new();
        let mut selection_context = Vec::new();
        let mut fetched_url_context = Vec::new();
        let mut thread_context = Vec::new();
        let mut text_thread_context = Vec::new();
        let mut rules_context = Vec::new();
        let mut images = Vec::new();
        for context in &contexts {
            match context {
                AgentContext::File(context) => file_context.push(context),
                AgentContext::Directory(context) => directory_context.push(context),
                AgentContext::Symbol(context) => symbol_context.push(context),
                AgentContext::Selection(context) => selection_context.push(context),
                AgentContext::FetchedUrl(context) => fetched_url_context.push(context),
                AgentContext::Thread(context) => thread_context.push(context),
                AgentContext::TextThread(context) => text_thread_context.push(context),
                AgentContext::Rules(context) => rules_context.push(context),
                AgentContext::Image(context) => images.extend(context.image()),
            }
        }

        // Use empty text if there are no contexts that contribute to text (everything but image
        // context).
        if file_context.is_empty()
            && directory_context.is_empty()
            && symbol_context.is_empty()
            && selection_context.is_empty()
            && fetched_url_context.is_empty()
            && thread_context.is_empty()
            && text_thread_context.is_empty()
            && rules_context.is_empty()
        {
            return ContextLoadResult {
                loaded_context: LoadedContext {
                    contexts,
                    text,
                    images,
                },
                referenced_buffers,
            };
        }

        text.push_str(
            "\n<context>\n\
            The following items were attached by the user. \
            They are up-to-date and don't need to be re-read.\n\n",
        );

        if !file_context.is_empty() {
            text.push_str("<files>");
            for context in file_context {
                text.push('\n');
                let _ = write!(text, "{context}");
            }
            text.push_str("</files>\n");
        }

        if !directory_context.is_empty() {
            text.push_str("<directories>");
            for context in directory_context {
                text.push('\n');
                let _ = write!(text, "{context}");
            }
            text.push_str("</directories>\n");
        }

        if !symbol_context.is_empty() {
            text.push_str("<symbols>");
            for context in symbol_context {
                text.push('\n');
                let _ = write!(text, "{context}");
            }
            text.push_str("</symbols>\n");
        }

        if !selection_context.is_empty() {
            text.push_str("<selections>");
            for context in selection_context {
                text.push('\n');
                let _ = write!(text, "{context}");
            }
            text.push_str("</selections>\n");
        }

        if !fetched_url_context.is_empty() {
            text.push_str("<fetched_urls>");
            for context in fetched_url_context {
                text.push('\n');
                let _ = write!(text, "{context}");
            }
            text.push_str("</fetched_urls>\n");
        }

        if !thread_context.is_empty() {
            text.push_str("<conversation_threads>");
            for context in thread_context {
                text.push('\n');
                let _ = write!(text, "{context}");
            }
            text.push_str("</conversation_threads>\n");
        }

        if !text_thread_context.is_empty() {
            text.push_str("<text_threads>");
            for context in text_thread_context {
                text.push('\n');
                let _ = writeln!(text, "{context}");
            }
            text.push_str("<text_threads>");
        }

        if !rules_context.is_empty() {
            text.push_str(
                "<user_rules>\n\
                The user has specified the following rules that should be applied:\n",
            );
            for context in rules_context {
                text.push('\n');
                let _ = write!(text, "{context}");
            }
            text.push_str("</user_rules>\n");
        }

        text.push_str("</context>\n");

        ContextLoadResult {
            loaded_context: LoadedContext {
                contexts,
                text,
                images,
            },
            referenced_buffers,
        }
    })
}

/// Search the semantic index for relevant documents based on the current thread context
fn search_semantic_index(
    project: &Entity<Project>,
    cx: &mut App,
) -> Task<Option<Vec<AgentContext>>> {
    let project = project.clone();
    
    cx.spawn(async move |cx| {
        log::info!("🔍 Starting semantic index search for agent context");
        
        // Check if SemanticDb is available and get project index
        let project_index = cx.update_global::<SemanticDb, _>(|semantic_db, cx| {
            let result = semantic_db.project_index(project.clone(), cx);
            log::info!("📊 Project index lookup result: {:?}", result.is_some());
            result
        }).ok()??;
        
        log::info!("✅ Successfully got project index, proceeding with search");
        
        // Debug: Check what files are in the project
        let file_count = project.read_with(cx, |project, cx| {
            let mut total_files = 0;
            for worktree in project.worktrees(cx) {
                let worktree = worktree.read(cx);
                let file_count = worktree.entries(false, 0).count();
                total_files += file_count;
                log::info!("📁 Worktree '{}' contains {} entries", worktree.root_name(), file_count);
                
                // Log some sample files
                for (i, entry) in worktree.entries(false, 0).enumerate() {
                    if i < 5 {
                        log::info!("📄 Sample file {}: {:?} (is_file: {})", i+1, entry.path, entry.is_file());
                    }
                }
                if file_count > 5 {
                    log::info!("... and {} more files", file_count - 5);
                }
            }
            total_files
        }).unwrap_or(0);
        
        log::info!("📊 Total files in project: {}", file_count);
        
        // Debug: Check indexing status
        let indexing_status = project_index.read_with(cx, |index, _cx| {
            index.status()
        }).unwrap_or_else(|_| {
            log::warn!("❌ Failed to read indexing status");
            semantic_index::Status::Idle
        });
        
        log::info!("📊 Indexing status: {:?}", indexing_status);
        
        // If still indexing, wait a moment
        if matches!(indexing_status, semantic_index::Status::Loading | semantic_index::Status::Scanning { .. }) {
            log::info!("⏳ Files are still being indexed, this may affect search results");
        }
        
        // First try tender and business document specific searches
        let specific_queries = vec![
            "tender".to_string(),
            "proposal".to_string(),
            "technical capacity".to_string(),
            "experience".to_string(),
            "qualifications".to_string(),
            "methodology".to_string(),
            "approach".to_string(),
            "requirements".to_string(),
            "compliance".to_string(),
            "appendix".to_string(),
        ];
        
        log::info!("🔎 First trying specific searches: {:?}", specific_queries);
        let search_limit = 15;
        
        // Debug: Wait longer for indexing if status shows scanning
        if matches!(indexing_status, semantic_index::Status::Scanning { .. }) {
            log::info!("⏳ Files are currently being indexed. Waiting 5 seconds...");
            cx.background_spawn(async move {
                smol::Timer::after(std::time::Duration::from_secs(5)).await;
            }).await;
            log::info!("⏳ Wait complete, proceeding with search");
        }
        
        // Perform the semantic search with specific queries first
        let search_results = project_index.read_with(cx, |index, cx| {
            index.search(specific_queries.clone(), search_limit, cx)
        }).ok()?;
        
        let mut search_results = search_results.await.log_err()?;
        log::info!("📄 Specific search returned {} results", search_results.len());
        
        // If we didn't find enough results, try broader searches
        if search_results.len() < 5 {
            log::info!("🔄 Trying broader search terms...");
            let broad_queries = vec![
                "project".to_string(),
                "management".to_string(),
                "team".to_string(),
                "delivery".to_string(),
                "services".to_string(),
                "capability".to_string(),
                "portfolio".to_string(),
                "client".to_string(),
                "contract".to_string(),
                "outcomes".to_string(),
            ];
            
            let broad_search = project_index.read_with(cx, |index, cx| {
                index.search(broad_queries, search_limit, cx)
            }).ok()?;
            
            let mut broad_results = broad_search.await.log_err()?;
            log::info!("📄 Broad search returned {} additional results", broad_results.len());
            search_results.append(&mut broad_results);
        }
        
        // If still no results, try the most basic possible searches
        if search_results.len() == 0 {
            log::info!("🔄 Still no results, trying most basic search terms...");
            let basic_queries = vec![
                "the".to_string(),
                "and".to_string(),
                "or".to_string(),
                "in".to_string(),
                "on".to_string(),
                "a".to_string(),
                "to".to_string(),
                "of".to_string(),
                "is".to_string(),
                "it".to_string(),
            ];
            
            let basic_search = project_index.read_with(cx, |index, cx| {
                index.search(basic_queries, 5, cx)
            }).ok()?;
            
            let mut basic_results = basic_search.await.log_err()?;
            log::info!("📄 Basic search returned {} additional results", basic_results.len());
            search_results.append(&mut basic_results);
        }
        
        // Load the search results content from database
        log::info!("📚 Attempting to load {} search results", search_results.len());
        
        // Get database connection from SemanticDb
        let db_connection = cx.update_global::<SemanticDb, _>(|db, _cx| {
            db.get_db_connection()
        }).ok()?;
        
        let loaded_results = match SemanticDb::load_results(db_connection, search_results, &cx).await {
            Ok(results) => {
                log::info!("✅ Successfully loaded {} file contents", results.len());
                results
            }
            Err(e) => {
                log::warn!("❌ Failed to load search results: {}", e);
                return None;
            }
        };
        log::info!("💾 Loaded {} file contents from search results", loaded_results.len());
        
        // Convert loaded results to AgentContext by opening buffers
        let mut agent_contexts = Vec::new();
        let mut context_id = ContextId::zero();
        
        // Try to process search results one by one, skipping those with UTF-8 issues
        for result in loaded_results {
            log::info!("📁 Processing search result: {:?} (rows {}-{})", 
                      result.path, result.row_range.start(), result.row_range.end());
            
            // Check if the excerpt content is valid UTF-8 
            if result.excerpt_content.is_empty() {
                log::warn!("⚠️ Skipping empty excerpt for: {:?}", result.path);
                continue;
            }
            
            // Find the worktree for this path
            let Some(worktree) = project.read_with(cx, |project, cx| {
                project.worktrees(cx)
                    .find(|worktree| {
                        let worktree = worktree.read(cx);
                        worktree.abs_path().join(&result.path) == result.full_path
                    })
            }).ok()? else {
                log::warn!("❌ Could not find worktree for path: {:?}", result.path);
                continue;
            };
            
            let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id()).ok()?;
            
            // Open buffer for this file
            let project_path = ProjectPath { 
                worktree_id, 
                path: result.path.clone() 
            };
            
            let buffer = project.update(cx, |project, cx| {
                project.buffer_store().update(cx, |buffer_store, cx| {
                    buffer_store.open_buffer(project_path, cx)
                })
            }).ok()?.await.log_err();
            
            if let Some(buffer) = buffer {
                let context_handle = FileContextHandle {
                    buffer: buffer.clone(),
                    context_id: context_id.post_inc(),
                };
                
                let file_context = FileContext {
                    handle: context_handle,
                    full_path: result.full_path.into(),
                    text: result.excerpt_content.into(),
                    is_outline: false,
                };
                
                log::info!("✅ Successfully created agent context for: {:?}", result.path);
                agent_contexts.push(AgentContext::File(file_context));
                
                // Limit to 5 contexts to avoid overwhelming the agent
                if agent_contexts.len() >= 5 {
                    log::info!("📊 Reached limit of 5 contexts, stopping");
                    break;
                }
            } else {
                log::warn!("❌ Failed to open buffer for: {:?}", result.path);
            }
        }
        
        if agent_contexts.is_empty() {
            log::info!("❌ No agent contexts created from semantic search");
            None
        } else {
            log::info!("🎉 Successfully created {} agent contexts from semantic search", agent_contexts.len());
            Some(agent_contexts)
        }
    })
}

fn collect_files_in_path(worktree: &Worktree, path: &Path) -> Vec<Arc<Path>> {
    let mut files = Vec::new();

    for entry in worktree.child_entries(path) {
        if entry.is_dir() {
            files.extend(collect_files_in_path(worktree, &entry.path));
        } else if entry.is_file() {
            files.push(entry.path.clone());
        }
    }

    files
}

fn codeblock_tag(full_path: &Path, line_range: Option<Range<Point>>) -> String {
    let mut result = String::new();

    if let Some(extension) = full_path.extension().and_then(|ext| ext.to_str()) {
        let _ = write!(result, "{} ", extension);
    }

    let _ = write!(result, "{}", full_path.display());

    if let Some(range) = line_range {
        if range.start.row == range.end.row {
            let _ = write!(result, ":{}", range.start.row + 1);
        } else {
            let _ = write!(result, ":{}-{}", range.start.row + 1, range.end.row + 1);
        }
    }

    result
}

/// Wraps `AgentContext` to opt-in to `PartialEq` and `Hash` impls which use a subset of fields
/// needed for stable context identity.
#[derive(Debug, Clone, RefCast)]
#[repr(transparent)]
pub struct AgentContextKey(pub AgentContextHandle);

impl AsRef<AgentContextHandle> for AgentContextKey {
    fn as_ref(&self) -> &AgentContextHandle {
        &self.0
    }
}

impl Eq for AgentContextKey {}

impl PartialEq for AgentContextKey {
    fn eq(&self, other: &Self) -> bool {
        match &self.0 {
            AgentContextHandle::File(context) => {
                if let AgentContextHandle::File(other_context) = &other.0 {
                    return context.eq_for_key(other_context);
                }
            }
            AgentContextHandle::Directory(context) => {
                if let AgentContextHandle::Directory(other_context) = &other.0 {
                    return context.eq_for_key(other_context);
                }
            }
            AgentContextHandle::Symbol(context) => {
                if let AgentContextHandle::Symbol(other_context) = &other.0 {
                    return context.eq_for_key(other_context);
                }
            }
            AgentContextHandle::Selection(context) => {
                if let AgentContextHandle::Selection(other_context) = &other.0 {
                    return context.eq_for_key(other_context);
                }
            }
            AgentContextHandle::FetchedUrl(context) => {
                if let AgentContextHandle::FetchedUrl(other_context) = &other.0 {
                    return context.eq_for_key(other_context);
                }
            }
            AgentContextHandle::Thread(context) => {
                if let AgentContextHandle::Thread(other_context) = &other.0 {
                    return context.eq_for_key(other_context);
                }
            }
            AgentContextHandle::Rules(context) => {
                if let AgentContextHandle::Rules(other_context) = &other.0 {
                    return context.eq_for_key(other_context);
                }
            }
            AgentContextHandle::Image(context) => {
                if let AgentContextHandle::Image(other_context) = &other.0 {
                    return context.eq_for_key(other_context);
                }
            }
            AgentContextHandle::TextThread(context) => {
                if let AgentContextHandle::TextThread(other_context) = &other.0 {
                    return context.eq_for_key(other_context);
                }
            }
        }
        false
    }
}

impl Hash for AgentContextKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.0 {
            AgentContextHandle::File(context) => context.hash_for_key(state),
            AgentContextHandle::Directory(context) => context.hash_for_key(state),
            AgentContextHandle::Symbol(context) => context.hash_for_key(state),
            AgentContextHandle::Selection(context) => context.hash_for_key(state),
            AgentContextHandle::FetchedUrl(context) => context.hash_for_key(state),
            AgentContextHandle::Thread(context) => context.hash_for_key(state),
            AgentContextHandle::TextThread(context) => context.hash_for_key(state),
            AgentContextHandle::Rules(context) => context.hash_for_key(state),
            AgentContextHandle::Image(context) => context.hash_for_key(state),
        }
    }
}

#[derive(Default)]
pub struct ContextCreasesAddon {
    creases: HashMap<AgentContextKey, Vec<(CreaseId, SharedString)>>,
    _subscription: Option<Subscription>,
}

impl Addon for ContextCreasesAddon {
    fn to_any(&self) -> &dyn std::any::Any {
        self
    }

    fn to_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl ContextCreasesAddon {
    pub fn new() -> Self {
        Self {
            creases: HashMap::default(),
            _subscription: None,
        }
    }

    pub fn add_creases(
        &mut self,
        context_store: &Entity<ContextStore>,
        key: AgentContextKey,
        creases: impl IntoIterator<Item = (CreaseId, SharedString)>,
        cx: &mut Context<Editor>,
    ) {
        self.creases.entry(key).or_default().extend(creases);
        self._subscription = Some(cx.subscribe(
            &context_store,
            |editor, _, event, cx| match event {
                ContextStoreEvent::ContextRemoved(key) => {
                    let Some(this) = editor.addon_mut::<Self>() else {
                        return;
                    };
                    let (crease_ids, replacement_texts): (Vec<_>, Vec<_>) = this
                        .creases
                        .remove(key)
                        .unwrap_or_default()
                        .into_iter()
                        .unzip();
                    let ranges = editor
                        .remove_creases(crease_ids, cx)
                        .into_iter()
                        .map(|(_, range)| range)
                        .collect::<Vec<_>>();
                    editor.unfold_ranges(&ranges, false, false, cx);
                    editor.edit(ranges.into_iter().zip(replacement_texts), cx);
                    cx.notify();
                }
            },
        ))
    }

    pub fn into_inner(self) -> HashMap<AgentContextKey, Vec<(CreaseId, SharedString)>> {
        self.creases
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use project::{FakeFs, Project};
    use serde_json::json;
    use settings::SettingsStore;
    use util::path;

    fn init_test_settings(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            language::init(cx);
            Project::init_settings(cx);
        });
    }

    // Helper to create a test project with test files
    async fn create_test_project(
        cx: &mut TestAppContext,
        files: serde_json::Value,
    ) -> Entity<Project> {
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(path!("/test"), files).await;
        Project::test(fs, [path!("/test").as_ref()], cx).await
    }

    #[gpui::test]
    async fn test_large_file_uses_outline(cx: &mut TestAppContext) {
        init_test_settings(cx);

        // Create a large file that exceeds AUTO_OUTLINE_SIZE
        const LINE: &str = "Line with some text\n";
        let large_content = LINE.repeat(2 * (outline::AUTO_OUTLINE_SIZE / LINE.len()));
        let content_len = large_content.len();

        assert!(content_len > outline::AUTO_OUTLINE_SIZE);

        let file_context = file_context_for(large_content, cx).await;

        assert!(
            file_context.is_outline,
            "Large file should use outline format"
        );

        assert!(
            file_context.text.len() < content_len,
            "Outline should be smaller than original content"
        );
    }

    #[gpui::test]
    async fn test_small_file_uses_full_content(cx: &mut TestAppContext) {
        init_test_settings(cx);

        let small_content = "This is a small file.\n";
        let content_len = small_content.len();

        assert!(content_len < outline::AUTO_OUTLINE_SIZE);

        let file_context = file_context_for(small_content.to_string(), cx).await;

        assert!(
            !file_context.is_outline,
            "Small files should not get an outline"
        );

        assert_eq!(file_context.text, small_content);
    }

    async fn file_context_for(content: String, cx: &mut TestAppContext) -> FileContext {
        // Create a test project with the file
        let project = create_test_project(
            cx,
            json!({
                "file.txt": content,
            }),
        )
        .await;

        // Open the buffer
        let buffer_path = project
            .read_with(cx, |project, cx| project.find_project_path("file.txt", cx))
            .unwrap();

        let buffer = project
            .update(cx, |project, cx| project.open_buffer(buffer_path, cx))
            .await
            .unwrap();

        let context_handle = AgentContextHandle::File(FileContextHandle {
            buffer: buffer.clone(),
            context_id: ContextId::zero(),
        });

        cx.update(|cx| load_context(vec![context_handle], &project, &None, cx))
            .await
            .loaded_context
            .contexts
            .into_iter()
            .find_map(|ctx| {
                if let AgentContext::File(file_ctx) = ctx {
                    Some(file_ctx)
                } else {
                    None
                }
            })
            .expect("Should have found a file context")
    }
}
