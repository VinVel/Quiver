// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod pot_server;
mod presets;
mod subtitle_pipeline;
mod yt_dlp;

use presets::{DownloadPreset, DownloadPresetInput, PresetCommandPreview, PresetId};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};
use yt_dlp::{YtDlpCommandOutput, YtDlpOutputStream, YtDlpRunner};

const THEME_SETTINGS_FILE_NAME: &str = "theme-settings.json";
const DEFAULT_THEME_MODE: &str = "system";
const DEFAULT_THEME_PRESET: &str = "crystal";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ThemePreferences {
    mode: String,
    preset: String,
}

impl Default for ThemePreferences {
    fn default() -> Self {
        Self {
            mode: DEFAULT_THEME_MODE.to_string(),
            preset: DEFAULT_THEME_PRESET.to_string(),
        }
    }
}

#[derive(Default)]
struct ThemeSettings {
    preferences: Mutex<ThemePreferences>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct YtDlpOutputChunk {
    run_id: String,
    stream: YtDlpOutputStream,
    chunk: String,
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {name}! You've been greeted from Rust!")
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command state and app handles are extracted by value."
)]
fn get_theme_mode(app: tauri::AppHandle, settings: tauri::State<'_, ThemeSettings>) -> String {
    let preferences = load_theme_preferences(&app, &settings);
    match preferences.mode.as_str() {
        "light" | "dark" | "system" => preferences.mode,
        _ => DEFAULT_THEME_MODE.to_string(),
    }
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command state and app handles are extracted by value."
)]
fn set_theme_mode(
    app: tauri::AppHandle,
    mode: String,
    settings: tauri::State<'_, ThemeSettings>,
) -> Result<String, String> {
    let next_mode = match mode.as_str() {
        "light" | "dark" | "system" => mode,
        _ => DEFAULT_THEME_MODE.to_string(),
    };
    let mut preferences = load_theme_preferences(&app, &settings);
    preferences.mode.clone_from(&next_mode);

    save_theme_preferences(&app, &settings, &preferences)?;

    Ok(next_mode)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command arguments, state, and app handles are extracted by value."
)]
fn get_theme_preset(
    app: tauri::AppHandle,
    supported_presets: Vec<String>,
    default_preset: String,
    settings: tauri::State<'_, ThemeSettings>,
) -> String {
    let preferences = load_theme_preferences(&app, &settings);
    let fallback = if supported_presets
        .iter()
        .any(|preset| preset == &default_preset)
    {
        default_preset
    } else {
        supported_presets
            .first()
            .cloned()
            .unwrap_or_else(|| DEFAULT_THEME_PRESET.to_string())
    };

    if supported_presets
        .iter()
        .any(|supported| supported == &preferences.preset)
    {
        preferences.preset
    } else {
        fallback
    }
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command arguments are extracted by value."
)]
fn set_theme_preset(
    app: tauri::AppHandle,
    preset: String,
    supported_presets: Vec<String>,
    default_preset: String,
    settings: tauri::State<'_, ThemeSettings>,
) -> Result<String, String> {
    let fallback = if supported_presets
        .iter()
        .any(|supported| supported == &default_preset)
    {
        default_preset
    } else {
        supported_presets
            .first()
            .cloned()
            .unwrap_or_else(|| DEFAULT_THEME_PRESET.to_string())
    };
    let next_preset = if supported_presets
        .iter()
        .any(|supported| supported == &preset)
    {
        preset
    } else {
        fallback
    };
    let mut preferences = load_theme_preferences(&app, &settings);
    preferences.preset.clone_from(&next_preset);

    save_theme_preferences(&app, &settings, &preferences)?;

    Ok(next_preset)
}

#[tauri::command]
fn get_license_html() -> String {
    include_str!("../license.html").to_string()
}

#[tauri::command]
async fn yt_dlp_version(app: tauri::AppHandle) -> Result<YtDlpCommandOutput, String> {
    YtDlpRunner::from_environment_or_bundle(app)
        .map_err(|error| error.to_string())?
        .run(["--version"])
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn run_yt_dlp(
    app: tauri::AppHandle,
    run_id: String,
    preset_id: PresetId,
    link: String,
    args: Vec<String>,
    advanced_subtitle_pipeline: bool,
) -> Result<YtDlpCommandOutput, String> {
    let emit_app = app.clone();
    let emit_run_id = run_id.clone();
    let plugin_dirs = yt_dlp_plugin_dirs(&app);
    let runner = YtDlpRunner::from_environment_or_bundle(app.clone())
        .map_err(|error| error.to_string())?
        .with_plugin_dirs(plugin_dirs);
    let on_chunk = move |stream, chunk: &str| {
        let _ = emit_app.emit(
            "yt-dlp-output",
            YtDlpOutputChunk {
                run_id: emit_run_id.clone(),
                stream,
                chunk: chunk.to_string(),
            },
        );
    };

    if advanced_subtitle_pipeline && presets::is_youtube_video_preset(preset_id) {
        subtitle_pipeline::run_youtube_video_download(app, runner, args, link, on_chunk).await
    } else {
        runner
            .run_streaming(args, on_chunk)
            .await
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn download_presets() -> Vec<DownloadPreset> {
    presets::all_presets().to_vec()
}

#[tauri::command]
fn download_preset_input_fields() -> Vec<presets::PresetInputField> {
    presets::interactive_input_fields().to_vec()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command arguments are extracted by value."
)]
fn preview_download_preset(
    preset_id: PresetId,
    input: DownloadPresetInput,
) -> Result<PresetCommandPreview, String> {
    presets::command_preview(preset_id, &input)
}

fn yt_dlp_plugin_dirs(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let resource_dir = app.path().resource_dir().ok();
    yt_dlp::candidate_plugin_dirs(resource_dir.as_deref())
}

fn theme_settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let settings_directory = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("failed to resolve app settings directory: {error}"))?;
    fs::create_dir_all(&settings_directory).map_err(|error| {
        format!(
            "failed to create app settings directory at {}: {error}",
            settings_directory.display()
        )
    })?;

    Ok(settings_directory.join(THEME_SETTINGS_FILE_NAME))
}

fn load_theme_preferences(
    app: &tauri::AppHandle,
    settings: &tauri::State<'_, ThemeSettings>,
) -> ThemePreferences {
    let preferences = theme_settings_path(app)
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|settings_json| serde_json::from_str::<ThemePreferences>(&settings_json).ok())
        .unwrap_or_else(|| {
            settings.preferences.lock().map_or_else(
                |_| ThemePreferences::default(),
                |preferences| preferences.clone(),
            )
        });

    if let Ok(mut saved_preferences) = settings.preferences.lock() {
        saved_preferences.clone_from(&preferences);
    }

    preferences
}

fn save_theme_preferences(
    app: &tauri::AppHandle,
    settings: &tauri::State<'_, ThemeSettings>,
    preferences: &ThemePreferences,
) -> Result<(), String> {
    let settings_path = theme_settings_path(app)?;
    let settings_json = serde_json::to_string_pretty(&preferences)
        .map_err(|error| format!("failed to serialize theme settings: {error}"))?;
    fs::write(&settings_path, settings_json).map_err(|error| {
        format!(
            "failed to write theme settings to {}: {error}",
            settings_path.display()
        )
    })?;

    if let Ok(mut saved_preferences) = settings.preferences.lock() {
        saved_preferences.clone_from(preferences);
    }

    Ok(())
}

#[cfg(desktop)]
fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn stop_pot_server(app: &tauri::AppHandle) {
    app.state::<pot_server::PotServer>().stop();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Runs the Tauri application.
///
/// # Panics
///
/// Panics if Tauri fails to initialize or run the application.
pub fn run() {
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main_window(app);
        }));
    }

    builder
        .manage(ThemeSettings::default())
        .manage(pot_server::PotServer::default())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            pot_server::PotServer::start_async(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_theme_mode,
            set_theme_mode,
            get_theme_preset,
            set_theme_preset,
            get_license_html,
            yt_dlp_version,
            run_yt_dlp,
            download_presets,
            download_preset_input_fields,
            preview_download_preset
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { .. }
            | tauri::RunEvent::Exit
            | tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::CloseRequested { .. },
                ..
            } => {
                stop_pot_server(app);
            }
            _ => {}
        });
}
