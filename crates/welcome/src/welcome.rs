use client::{TelemetrySettings, telemetry::Telemetry};
use db::kvp::KEY_VALUE_STORE;
use gpui::{
    Action, App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    ParentElement, Render, Styled, Subscription, Task, WeakEntity, Window, actions, svg,
};
use language::language_settings::{EditPredictionProvider, all_language_settings};
use settings::{Settings, SettingsStore};
use std::sync::Arc;
use ui::{CheckboxWithLabel, ElevationIndex, Tooltip, prelude::*};
use util::ResultExt;
use vim_mode_setting::VimModeSetting;
use workspace::{
    AppState, Welcome, Workspace, WorkspaceId,
    dock::DockPosition,
    item::{Item, ItemEvent},
    open_new,
};

pub use base_keymap_setting::BaseKeymap;
pub use multibuffer_hint::*;

mod base_keymap_picker;
mod base_keymap_setting;
mod multibuffer_hint;
mod welcome_ui;

actions!(welcome, [ResetHints]);

pub const FIRST_OPEN: &str = "first_open";
pub const DOCS_URL: &str = "https://autotender.dev/docs/";
const BOOK_ONBOARDING: &str = "https://autotender.dev/onboarding";

pub fn init(cx: &mut App) {
    BaseKeymap::register(cx);

    cx.observe_new(|workspace: &mut Workspace, _, _cx| {
        workspace.register_action(|workspace, _: &Welcome, window, cx| {
            let welcome_page = WelcomePage::new(workspace, cx);
            workspace.add_item_to_active_pane(Box::new(welcome_page), None, true, window, cx)
        });
        workspace
            .register_action(|_workspace, _: &ResetHints, _, cx| MultibufferHint::set_count(0, cx));
    })
    .detach();

    base_keymap_picker::init(cx);
}

pub fn show_welcome_view(app_state: Arc<AppState>, cx: &mut App) -> Task<anyhow::Result<()>> {
    open_new(
        Default::default(),
        app_state,
        cx,
        |workspace, window, cx| {
            workspace.toggle_dock(DockPosition::Left, window, cx);
            let welcome_page = WelcomePage::new(workspace, cx);
            workspace.add_item_to_center(Box::new(welcome_page.clone()), window, cx);

            window.focus(&welcome_page.focus_handle(cx));

            cx.notify();

            db::write_and_log(cx, || {
                KEY_VALUE_STORE.write_kvp(FIRST_OPEN.to_string(), "false".to_string())
            });
        },
    )
}

pub struct WelcomePage {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    telemetry: Arc<Telemetry>,
    _settings_subscription: Subscription,
}

impl Render for WelcomePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let edit_prediction_provider_is_zed =
            all_language_settings(None, cx).edit_predictions.provider
                == EditPredictionProvider::Zed;

        let edit_prediction_label = if edit_prediction_provider_is_zed {
            "Edit Prediction Enabled"
        } else {
            "Try Edit Prediction"
        };

        h_flex()
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .key_context("Welcome")
            .track_focus(&self.focus_handle(cx))
            .child(
                v_flex()
                    .gap_8()
                    .mx_auto()
                    .child(
                        v_flex()
                            .w_full()
                            .child(
                                svg()
                                    .path("icons/logo_96.svg")
                                    .text_color(cx.theme().colors().icon_disabled)
                                    .w(px(40.))
                                    .h(px(40.))
                                    .mx_auto()
                                    .mb_4(),
                            )
                            .child(
                                h_flex()
                                    .w_full()
                                    .justify_center()
                                    .child(Headline::new("Welcome to Auto Tender")),
                            )
                            .child(
                                h_flex().w_full().justify_center().child(
                                    Label::new("The editor for your Contracts.")
                                        .color(Color::Muted)
                                        .italic(),
                                ),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_start()
                            .gap_8()
                            .child(
                                v_flex()
                                    .gap_2()
                                    .pr_8()
                                    .border_r_1()
                                    .border_color(cx.theme().colors().border_variant)
                                    .child(
                                        self.section_label( cx).child(
                                            Label::new("Get Started")
                                                .size(LabelSize::XSmall)
                                                .color(Color::Muted),
                                        ),
                                    )
                                    .child(
                                        Button::new("choose-theme", "Choose a Theme")
                                            .icon(IconName::SwatchBook)
                                            .icon_size(IconSize::XSmall)
                                            .icon_color(Color::Muted)
                                            .icon_position(IconPosition::Start)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                telemetry::event!("Welcome Theme Changed");
                                                this.workspace
                                                    .update(cx, |_workspace, cx| {
                                                        window.dispatch_action(zed_actions::theme_selector::Toggle::default().boxed_clone(), cx);
                                                    })
                                                    .ok();
                                            })),
                                    )
                                    .child(
                                        Button::new("choose-keymap", "Choose a Keymap")
                                            .icon(IconName::Keyboard)
                                            .icon_size(IconSize::XSmall)
                                            .icon_color(Color::Muted)
                                            .icon_position(IconPosition::Start)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                telemetry::event!("Welcome Keymap Changed");
                                                this.workspace
                                                    .update(cx, |workspace, cx| {
                                                        base_keymap_picker::toggle(
                                                            workspace,
                                                            &Default::default(),
                                                            window, cx,
                                                        )
                                                    })
                                                    .ok();
                                            })),
                                    )
                                    .child(
                                        Button::new(
                                            "try-zed-edit-prediction",
                                            edit_prediction_label,
                                        )
                                        .disabled(edit_prediction_provider_is_zed)
                                        .icon(IconName::ZedPredict)
                                        .icon_size(IconSize::XSmall)
                                        .icon_color(Color::Muted)
                                        .icon_position(IconPosition::Start)
                                        .on_click(
                                            cx.listener(|_, _, window, cx| {
                                                telemetry::event!("Welcome Screen Try Edit Prediction clicked");
                                                window.dispatch_action(zed_actions::OpenZedPredictOnboarding.boxed_clone(), cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        Button::new("edit settings", "Edit Settings")
                                            .icon(IconName::Settings)
                                            .icon_size(IconSize::XSmall)
                                            .icon_color(Color::Muted)
                                            .icon_position(IconPosition::Start)
                                            .on_click(cx.listener(|_, _, window, cx| {
                                                telemetry::event!("Welcome Settings Edited");
                                                window.dispatch_action(Box::new(
                                                    zed_actions::OpenSettings,
                                                ), cx);
                                            })),
                                    )

                            )
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(
                                        self.section_label(cx).child(
                                            Label::new("Tender Resources")
                                                .size(LabelSize::XSmall)
                                                .color(Color::Muted),
                                        ),
                                    )
                                    .child(
                                        Button::new("view-templates", "Browse Templates")
                                            .icon(IconName::FileText)
                                            .icon_size(IconSize::XSmall)
                                            .icon_color(Color::Muted)
                                            .icon_position(IconPosition::Start)
                                            .on_click(cx.listener(|_, _, _, cx| {
                                                telemetry::event!("Tender Templates Viewed");
                                                cx.open_url("https://autotender.dev/templates");
                                            })),
                                    )
                                    .child(
                                        Button::new("view-docs", "Tender Writing Guide")
                                            .icon(IconName::Book)
                                            .icon_size(IconSize::XSmall)
                                            .icon_color(Color::Muted)
                                            .icon_position(IconPosition::Start)
                                            .on_click(cx.listener(|_, _, _, cx| {
                                                telemetry::event!("Tender Documentation Viewed");
                                                cx.open_url("https://autotender.dev/guide");
                                            })),
                                    )
                                    .child(
                                        Button::new("schedule-demo", "Schedule a Demo")
                                            .icon(IconName::PhoneIncoming)
                                            .icon_size(IconSize::XSmall)
                                            .icon_color(Color::Muted)
                                            .icon_position(IconPosition::Start)
                                            .on_click(cx.listener(|_, _, _, cx| {
                                                telemetry::event!("Tender Demo Scheduled");
                                                cx.open_url("https://autotender.dev/schedule");
                                            })),
                                    )
                                    .child(
                                        Button::new("tender-support", "Get Support")
                                            .icon(IconName::Info)
                                            .icon_size(IconSize::XSmall)
                                            .icon_color(Color::Muted)
                                            .icon_position(IconPosition::Start)
                                            .on_click(cx.listener(|_, _, _, cx| {
                                                telemetry::event!("Tender Support Accessed");
                                                cx.open_url("https://autotender.dev/support");
                                            })),
                                    ),
                            ),
                    ),
            )
    }
}

impl WelcomePage {
    pub fn new(workspace: &Workspace, cx: &mut Context<Workspace>) -> Entity<Self> {
        let this = cx.new(|cx| {
            cx.on_release(|_: &mut Self, _| {
                telemetry::event!("Welcome Page Closed");
            })
            .detach();

            WelcomePage {
                focus_handle: cx.focus_handle(),
                workspace: workspace.weak_handle(),
                telemetry: workspace.client().telemetry().clone(),
                _settings_subscription: cx
                    .observe_global::<SettingsStore>(move |_, cx| cx.notify()),
            }
        });

        this
    }

    fn section_label(&self, cx: &mut App) -> Div {
        div()
            .pl_1()
            .font_buffer(cx)
            .text_color(Color::Muted.color(cx))
    }

    fn update_settings<T: Settings>(
        &mut self,
        selection: &ToggleState,
        cx: &mut Context<Self>,
        callback: impl 'static + Send + Fn(&mut T::FileContent, bool),
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            let fs = workspace.read(cx).app_state().fs.clone();
            let selection = *selection;
            settings::update_settings_file::<T>(fs, cx, move |settings, _| {
                let value = match selection {
                    ToggleState::Unselected => false,
                    ToggleState::Selected => true,
                    _ => return,
                };

                callback(settings, value)
            });
        }
    }
}

impl EventEmitter<ItemEvent> for WelcomePage {}

impl Focusable for WelcomePage {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for WelcomePage {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Welcome".into()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("Welcome Page Opened")
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn clone_on_split(
        &self,
        _workspace_id: Option<WorkspaceId>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<Self>> {
        Some(cx.new(|cx| WelcomePage {
            focus_handle: cx.focus_handle(),
            workspace: self.workspace.clone(),
            telemetry: self.telemetry.clone(),
            _settings_subscription: cx.observe_global::<SettingsStore>(move |_, cx| cx.notify()),
        }))
    }

    fn to_item_events(event: &Self::Event, mut f: impl FnMut(workspace::item::ItemEvent)) {
        f(*event)
    }
}
