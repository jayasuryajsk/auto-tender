use gpui::{
    Action, AppContext, Context, EventEmitter, FocusHandle, Focusable, 
    ParentElement, Render, Styled, View, ViewContext, WindowContext, 
    div, h_flex, v_flex, prelude::*,
};
use settings::{Settings, SettingsStore, SettingsLocation};
use std::sync::Arc;
use ui::{
    prelude::*,
    Button, Checkbox, Label, TextInput, ToggleButton,
    ScrollView, TabBar, TabPosition,
};

use crate::WritingAppSettings;

pub struct WritingAppSettingsPanel {
    focus_handle: FocusHandle,
    settings: Arc<WritingAppSettings>,
    active_tab: SettingsTab,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum SettingsTab {
    General,
    Interface,
    Advanced,
}

impl SettingsTab {
    fn label(&self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Interface => "Interface",
            SettingsTab::Advanced => "Advanced",
        }
    }

    fn all() -> Vec<SettingsTab> {
        vec![
            SettingsTab::General,
            SettingsTab::Interface,
            SettingsTab::Advanced,
        ]
    }
}

pub fn toggle_settings_panel(cx: &mut WindowContext) {
    cx.dispatch_action(ToggleSettingsPanel.boxed_clone());
}

#[derive(Clone, Debug)]
pub struct ToggleSettingsPanel;

impl Action for ToggleSettingsPanel {}

impl WritingAppSettingsPanel {
    pub fn new(cx: &mut ViewContext<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let settings = WritingAppSettings::get_global(cx);
        
        Self {
            focus_handle,
            settings,
            active_tab: SettingsTab::General,
        }
    }

    fn set_tab(&mut self, tab: SettingsTab, cx: &mut ViewContext<Self>) {
        self.active_tab = tab;
        cx.notify();
    }

    fn update_setting<F>(&mut self, update_fn: F, cx: &mut ViewContext<Self>)
    where
        F: FnOnce(&mut WritingAppSettings),
    {
        let fs = cx.global::<Arc<dyn fs::Fs>>();
        settings::update_settings_file::<WritingAppSettings>(fs.clone(), cx, |settings, _| {
            update_fn(settings);
        });
        
        // Refresh the settings after update
        self.settings = WritingAppSettings::get_global(cx);
        cx.notify();
    }
    
    fn render_general_tab(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .gap_4()
            .p_4()
            .child(
                div()
                    .text_lg()
                    .font_weight_semibold()
                    .child("Writing Settings")
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(Label::new("Auto-save (seconds)"))
                            .child(
                                TextInput::new("auto_save_input")
                                    .text(
                                        self.settings.auto_save_interval_ms
                                            .map(|ms| (ms / 1000).to_string())
                                            .unwrap_or_else(|| "Off".to_string())
                                    )
                                    .on_submit(cx.listener(|this, text, cx| {
                                        let seconds = text.parse::<u64>().ok();
                                        this.update_setting(|settings| {
                                            settings.auto_save_interval_ms = seconds.map(|s| s * 1000);
                                        }, cx);
                                    }))
                            )
                    )
                    .child(
                        Checkbox::new("spelling_check")
                            .checked(self.settings.spelling_check_enabled)
                            .label("Enable spell checking")
                            .on_toggle(cx.listener(|this, checked, cx| {
                                this.update_setting(|settings| {
                                    settings.spelling_check_enabled = checked;
                                }, cx);
                            }))
                    )
                    .child(
                        Checkbox::new("grammar_check")
                            .checked(self.settings.grammar_check_enabled)
                            .label("Enable grammar checking")
                            .on_toggle(cx.listener(|this, checked, cx| {
                                this.update_setting(|settings| {
                                    settings.grammar_check_enabled = checked;
                                }, cx);
                            }))
                    )
                    .child(
                        Checkbox::new("word_count")
                            .checked(self.settings.word_count_visible)
                            .label("Show word count")
                            .on_toggle(cx.listener(|this, checked, cx| {
                                this.update_setting(|settings| {
                                    settings.word_count_visible = checked;
                                }, cx);
                            }))
                    )
                    .child(
                        Checkbox::new("reading_time")
                            .checked(self.settings.reading_time_visible)
                            .label("Show reading time")
                            .on_toggle(cx.listener(|this, checked, cx| {
                                this.update_setting(|settings| {
                                    settings.reading_time_visible = checked;
                                }, cx);
                            }))
                    )
                    .child(
                        Checkbox::new("focus_mode")
                            .checked(self.settings.focus_mode_enabled)
                            .label("Enable focus mode")
                            .on_toggle(cx.listener(|this, checked, cx| {
                                this.update_setting(|settings| {
                                    settings.focus_mode_enabled = checked;
                                }, cx);
                            }))
                    )
            )
    }
    
    fn render_interface_tab(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .gap_4()
            .p_4()
            .child(
                div()
                    .text_lg()
                    .font_weight_semibold()
                    .child("Interface Settings")
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        Checkbox::new("simplified_interface")
                            .checked(self.settings.simplified_interface)
                            .label("Use simplified interface")
                            .on_toggle(cx.listener(|this, checked, cx| {
                                this.update_setting(|settings| {
                                    settings.simplified_interface = checked;
                                }, cx);
                            }))
                    )
                    .child(
                        Checkbox::new("show_git")
                            .checked(self.settings.show_git_panel)
                            .label("Show Git features")
                            .on_toggle(cx.listener(|this, checked, cx| {
                                this.update_setting(|settings| {
                                    settings.show_git_panel = checked;
                                }, cx);
                            }))
                    )
                    .child(
                        Checkbox::new("show_themes")
                            .checked(self.settings.show_themes)
                            .label("Show theme selector")
                            .on_toggle(cx.listener(|this, checked, cx| {
                                this.update_setting(|settings| {
                                    settings.show_themes = checked;
                                }, cx);
                            }))
                    )
                    .child(
                        Checkbox::new("show_icon_themes")
                            .checked(self.settings.show_icon_themes)
                            .label("Show icon theme selector")
                            .on_toggle(cx.listener(|this, checked, cx| {
                                this.update_setting(|settings| {
                                    settings.show_icon_themes = checked;
                                }, cx);
                            }))
                    )
                    .child(
                        Checkbox::new("show_extensions")
                            .checked(self.settings.show_extensions)
                            .label("Show extensions manager")
                            .on_toggle(cx.listener(|this, checked, cx| {
                                this.update_setting(|settings| {
                                    settings.show_extensions = checked;
                                }, cx);
                            }))
                    )
            )
    }
    
    fn render_advanced_tab(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .gap_4()
            .p_4()
            .child(
                div()
                    .text_lg()
                    .font_weight_semibold()
                    .child("Advanced Settings")
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        Checkbox::new("show_api_settings")
                            .checked(self.settings.show_api_settings)
                            .label("Show API provider settings")
                            .on_toggle(cx.listener(|this, checked, cx| {
                                this.update_setting(|settings| {
                                    settings.show_api_settings = checked;
                                }, cx);
                            }))
                    )
                    .child(
                        Button::new("reset", "Reset to Defaults")
                            .on_click(cx.listener(|this, _, cx| {
                                this.update_setting(|settings| {
                                    *settings = WritingAppSettings::default();
                                    settings.show_git_panel = false;
                                    settings.show_themes = false;
                                    settings.show_icon_themes = false;
                                    settings.show_extensions = false;
                                    settings.simplified_interface = true;
                                    settings.show_api_settings = false;
                                    settings.auto_save_interval_ms = Some(30000);
                                    settings.spelling_check_enabled = true;
                                    settings.grammar_check_enabled = true;
                                    settings.word_count_visible = true;
                                    settings.reading_time_visible = true;
                                    settings.focus_mode_enabled = false;
                                }, cx);
                            }))
                    )
            )
    }
}

impl Focusable for WritingAppSettingsPanel {
    fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }
}

impl Render for WritingAppSettingsPanel {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .h_full()
            .child(
                TabBar::new("settings_tabs")
                    .tab_position(TabPosition::Top)
                    .selected_tab_id(self.active_tab as usize)
                    .on_change(cx.listener(|this, tab_id, cx| {
                        let tab = SettingsTab::all()[tab_id];
                        this.set_tab(tab, cx);
                    }))
                    .tabs(
                        SettingsTab::all()
                            .into_iter()
                            .map(|tab| (tab.label().to_string(), tab as usize))
                            .collect()
                    )
            )
            .child(
                ScrollView::new("settings_content")
                    .child(match self.active_tab {
                        SettingsTab::General => self.render_general_tab(cx),
                        SettingsTab::Interface => self.render_interface_tab(cx),
                        SettingsTab::Advanced => self.render_advanced_tab(cx),
                    })
            )
    }
}

impl EventEmitter<()> for WritingAppSettingsPanel {}

impl View for WritingAppSettingsPanel {
    fn receive_focus(&mut self, cx: &mut ViewContext<Self>, focus: bool) {
        if focus {
            cx.notify();
        }
    }
}