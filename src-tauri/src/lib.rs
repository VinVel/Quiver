#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri commands extract owned arguments and state handles from IPC."
)]

use std::path::PathBuf;

use quiver_core::{
    download::{DownloadEvent, DownloadRequest, DownloadService},
    presets::{self, DownloadPreset, DownloadPresetInput, PresetCommandPreview, PresetId},
    settings::{DownloadInputSettings, SettingsStore},
    yt_dlp::{self, YtDlpCommandOutput, YtDlpOutputStream, YtDlpRunner},
};
use serde::Serialize;
use tauri::{Emitter, Manager};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct YtDlpOutputChunk {
    run_id: String,
    stream: YtDlpOutputStream,
    chunk: String,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn update(app: tauri::AppHandle) -> tauri_plugin_updater::Result<()> {
    if let Some(update) = app.updater()?.check().await? {
        let mut downloaded = 0;
        update
            .download_and_install(
                |chunk_length, content_length| {
                    downloaded += chunk_length;
                    println!("downloaded {downloaded} from {content_length:?}");
                },
                || println!("download finished"),
            )
            .await?;
        println!("update installed");
        app.restart();
    }
    Ok(())
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {name}! You've been greeted from Rust!")
}

#[tauri::command]
fn get_theme_mode(settings: tauri::State<'_, SettingsStore>) -> String {
    match settings.theme().mode.as_str() {
        "light" => "light",
        "dark" => "dark",
        _ => "system",
    }
    .to_string()
}

#[tauri::command]
fn set_theme_mode(
    mode: String,
    settings: tauri::State<'_, SettingsStore>,
) -> Result<String, String> {
    settings
        .set_theme_mode(mode)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_theme_preset(
    supported_presets: Vec<String>,
    default_preset: String,
    settings: tauri::State<'_, SettingsStore>,
) -> String {
    settings.theme_preset(&supported_presets, &default_preset)
}

#[tauri::command]
fn set_theme_preset(
    preset: String,
    supported_presets: Vec<String>,
    default_preset: String,
    settings: tauri::State<'_, SettingsStore>,
) -> Result<String, String> {
    settings
        .set_theme_preset(preset, &supported_presets, &default_preset)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_license_html() -> String {
    include_str!("../license.html").to_string()
}

#[tauri::command]
fn get_download_input_settings(settings: tauri::State<'_, SettingsStore>) -> DownloadInputSettings {
    settings.download_inputs()
}

#[tauri::command]
fn set_download_input_settings(
    input_settings: DownloadInputSettings,
    settings: tauri::State<'_, SettingsStore>,
) -> Result<DownloadInputSettings, String> {
    settings
        .set_download_inputs(input_settings)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn yt_dlp_version() -> Result<YtDlpCommandOutput, String> {
    YtDlpRunner::from_environment_or_bundle()
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
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let output_app = app.clone();
    let output_task = tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            if let DownloadEvent::Output { stream, chunk } = event {
                let _ = output_app.emit(
                    "yt-dlp-output",
                    YtDlpOutputChunk {
                        run_id: run_id.clone(),
                        stream,
                        chunk,
                    },
                );
            }
        }
    });
    let request = DownloadRequest {
        preset_id,
        link,
        args,
        advanced_subtitle_pipeline,
        plugin_dirs: yt_dlp_plugin_dirs(&app),
    };
    let result = app
        .state::<DownloadService>()
        .run(request, sender)
        .await
        .map_err(|error| error.to_string());
    let _ = output_task.await;
    result
}

#[tauri::command]
fn cancel_yt_dlp(service: tauri::State<'_, DownloadService>) {
    service.cancel();
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

#[cfg(desktop)]
fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Run the compatibility Tauri interface during the terminal migration.
///
/// # Panics
/// Panics if Tauri fails to initialize or run.
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main_window(app);
        }));
    }

    builder
        .manage(DownloadService::default())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let directory = app.path().app_config_dir()?;
            app.manage(SettingsStore::new(directory));
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = update(handle).await {
                        eprintln!("failed to check for updates: {error}");
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_theme_mode,
            set_theme_mode,
            get_theme_preset,
            set_theme_preset,
            get_license_html,
            get_download_input_settings,
            set_download_input_settings,
            yt_dlp_version,
            run_yt_dlp,
            cancel_yt_dlp,
            download_presets,
            download_preset_input_fields,
            preview_download_preset
        ])
        .build(tauri::generate_context!())
        .expect("error while building Tauri application")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { .. }
            | tauri::RunEvent::Exit
            | tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::CloseRequested { .. },
                ..
            } => app.state::<DownloadService>().stop(),
            _ => {}
        });
}
