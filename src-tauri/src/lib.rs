// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod pot_server;
mod presets;
mod subtitle_pipeline;
mod yt_dlp;

use presets::{DownloadPreset, DownloadPresetInput, PresetCommandPreview, PresetId};
use serde::Serialize;
use std::{path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};
use yt_dlp::{YtDlpCommandOutput, YtDlpOutputStream, YtDlpRunner};

#[derive(Default)]
struct ThemeSettings {
    mode: Mutex<String>,
    preset: Mutex<String>,
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
    let args = resolve_pot_server_home_args(args, &app);
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

fn resolve_pot_server_home_args(args: Vec<String>, app: &tauri::AppHandle) -> Vec<String> {
    let Some(server_home) = pot_server::provider_server_path(app) else {
        return args;
    };
    let server_home = server_home.display().to_string();

    args.into_iter()
        .map(|arg| arg.replace("__QUIVER_POT_SERVER_HOME__", &server_home))
        .collect()
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
    let mut builder = tauri::Builder::default();

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
