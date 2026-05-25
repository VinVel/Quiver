// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod yt_dlp;

use std::sync::Mutex;
use yt_dlp::{YtDlpCommandOutput, YtDlpRunner};

#[derive(Default)]
struct ThemeSettings {
    mode: Mutex<String>,
    preset: Mutex<String>,
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {name}! You've been greeted from Rust!")
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command state is extracted by value."
)]
fn get_theme_mode(settings: tauri::State<'_, ThemeSettings>) -> String {
    settings.mode.lock().map_or_else(
        |_| "system".to_string(),
        |mode| {
            if mode.is_empty() {
                "system".to_string()
            } else {
                mode.clone()
            }
        },
    )
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command state is extracted by value."
)]
fn set_theme_mode(mode: String, settings: tauri::State<'_, ThemeSettings>) -> String {
    let next_mode = match mode.as_str() {
        "light" | "dark" | "system" => mode,
        _ => "system".to_string(),
    };

    if let Ok(mut saved_mode) = settings.mode.lock() {
        saved_mode.clone_from(&next_mode);
    }

    next_mode
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command arguments are extracted by value."
)]
fn get_theme_preset(
    supported_presets: Vec<String>,
    default_preset: String,
    settings: tauri::State<'_, ThemeSettings>,
) -> String {
    let fallback = if supported_presets
        .iter()
        .any(|preset| preset == &default_preset)
    {
        default_preset
    } else {
        supported_presets
            .first()
            .cloned()
            .unwrap_or_else(|| "crystal".to_string())
    };

    settings
        .preset
        .lock()
        .map(|preset| {
            if supported_presets
                .iter()
                .any(|supported| supported == &*preset)
            {
                preset.clone()
            } else {
                fallback.clone()
            }
        })
        .unwrap_or(fallback)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command arguments are extracted by value."
)]
fn set_theme_preset(
    preset: String,
    supported_presets: Vec<String>,
    default_preset: String,
    settings: tauri::State<'_, ThemeSettings>,
) -> String {
    let fallback = if supported_presets
        .iter()
        .any(|supported| supported == &default_preset)
    {
        default_preset
    } else {
        supported_presets
            .first()
            .cloned()
            .unwrap_or_else(|| "crystal".to_string())
    };
    let next_preset = if supported_presets
        .iter()
        .any(|supported| supported == &preset)
    {
        preset
    } else {
        fallback
    };

    if let Ok(mut saved_preset) = settings.preset.lock() {
        saved_preset.clone_from(&next_preset);
    }

    next_preset
}

#[tauri::command]
fn yt_dlp_version() -> Result<YtDlpCommandOutput, String> {
    YtDlpRunner::from_environment_or_bundle()
        .map_err(|error| error.to_string())?
        .run(["--version"])
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn run_yt_dlp(args: Vec<String>) -> Result<YtDlpCommandOutput, String> {
    YtDlpRunner::from_environment_or_bundle()
        .map_err(|error| error.to_string())?
        .run(args)
        .map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Runs the Tauri application.
///
/// # Panics
///
/// Panics if Tauri fails to initialize or run the application.
pub fn run() {
    tauri::Builder::default()
        .manage(ThemeSettings::default())
        .plugin(tauri_plugin_os::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            get_theme_mode,
            set_theme_mode,
            get_theme_preset,
            set_theme_preset,
            yt_dlp_version,
            run_yt_dlp
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
