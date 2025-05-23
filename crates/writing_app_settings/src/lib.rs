use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use settings::Settings;
use std::sync::Arc;
use schemars::JsonSchema;

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct WritingAppSettings {
    // Settings for controlling feature visibility
    pub show_git_panel: bool,
    pub show_themes: bool,
    pub show_icon_themes: bool,
    pub show_extensions: bool,
    pub simplified_interface: bool,
    pub show_api_settings: bool,
    
    // Writing-focused settings
    pub auto_save_interval_ms: Option<u64>,
    pub spelling_check_enabled: bool,
    pub grammar_check_enabled: bool,
    pub word_count_visible: bool,
    pub reading_time_visible: bool,
    pub focus_mode_enabled: bool,
}

impl Global for WritingAppSettings {}

impl Settings for WritingAppSettings {
    const KEY: Option<&'static str> = Some("writing_app");

    type FileContent = Self;
    
    fn load(
        sources: settings::SettingsSources<'_, Self::FileContent>,
        _: &mut App,
    ) -> anyhow::Result<Self> {
        if let Some(user_settings) = sources.user {
            Ok(user_settings.clone())
        } else {
            Ok(Self::default())
        }
    }

    fn import_from_vscode(_vscode_settings: &settings::VsCodeSettings, _settings: &mut Self::FileContent) {
        // No implementation needed
    }
}

pub fn init(cx: &mut App) {
    // Initialize with default settings
    let settings = WritingAppSettings {
        // Default to hiding developer-focused features
        show_git_panel: false,
        show_themes: false,
        show_icon_themes: false,
        show_extensions: false,
        simplified_interface: true,
        show_api_settings: false,
        
        // Enable writing-focused features by default
        auto_save_interval_ms: Some(30000), // 30 seconds
        spelling_check_enabled: true,
        grammar_check_enabled: true,
        word_count_visible: true,
        reading_time_visible: true,
        focus_mode_enabled: false,
    };

    cx.set_global(settings);
}

pub fn get_global(cx: &App) -> Arc<WritingAppSettings> {
    let settings = cx.global::<WritingAppSettings>();
    Arc::new(settings.clone())
}