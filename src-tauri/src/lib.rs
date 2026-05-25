// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use std::sync::Mutex;

#[derive(Default)]
struct ThemeSettings {
    mode: Mutex<String>,
    preset: Mutex<String>,
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_theme_mode(settings: tauri::State<'_, ThemeSettings>) -> String {
    settings
        .mode
        .lock()
        .map(|mode| {
            if mode.is_empty() {
                "system".to_string()
            } else {
                mode.clone()
            }
        })
        .unwrap_or_else(|_| "system".to_string())
}

#[tauri::command]
fn set_theme_mode(mode: String, settings: tauri::State<'_, ThemeSettings>) -> String {
    let next_mode = match mode.as_str() {
        "light" | "dark" | "system" => mode,
        _ => "system".to_string(),
    };

    if let Ok(mut saved_mode) = settings.mode.lock() {
        *saved_mode = next_mode.clone();
    }

    next_mode
}

#[tauri::command]
fn get_theme_preset(
    supported_presets: Vec<String>,
    default_preset: String,
    settings: tauri::State<'_, ThemeSettings>,
) -> String {
    let fallback = if supported_presets.iter().any(|preset| preset == &default_preset) {
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
            if supported_presets.iter().any(|supported| supported == &*preset) {
                preset.clone()
            } else {
                fallback.clone()
            }
        })
        .unwrap_or(fallback)
}

#[tauri::command]
fn set_theme_preset(
    preset: String,
    supported_presets: Vec<String>,
    default_preset: String,
    settings: tauri::State<'_, ThemeSettings>,
) -> String {
    let fallback = if supported_presets.iter().any(|supported| supported == &default_preset) {
        default_preset
    } else {
        supported_presets
            .first()
            .cloned()
            .unwrap_or_else(|| "crystal".to_string())
    };
    let next_preset = if supported_presets.iter().any(|supported| supported == &preset) {
        preset
    } else {
        fallback
    };

    if let Ok(mut saved_preset) = settings.preset.lock() {
        *saved_preset = next_preset.clone();
    }

    next_preset
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ThemeSettings::default())
        .plugin(tauri_plugin_os::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            get_theme_mode,
            set_theme_mode,
            get_theme_preset,
            set_theme_preset
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
