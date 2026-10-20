use std::{fs, io, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const THEME_FILE: &str = "theme-settings.json";
const INPUT_FILE: &str = "input-settings.json";
const DEFAULT_THEME_MODE: &str = "system";
const DEFAULT_THEME_PRESET: &str = "crystal";
const DEFAULT_DOWNLOAD_DIRECTORY: &str = "~/Downloads";
const DEFAULT_COOKIES_PATH: &str = "~/Downloads/cookies.txt";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePreferences {
    pub mode: String,
    pub preset: String,
}

impl Default for ThemePreferences {
    fn default() -> Self {
        Self {
            mode: DEFAULT_THEME_MODE.to_string(),
            preset: DEFAULT_THEME_PRESET.to_string(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInputSettings {
    pub directory: String,
    pub cookies_path: String,
}

impl Default for DownloadInputSettings {
    fn default() -> Self {
        Self {
            directory: DEFAULT_DOWNLOAD_DIRECTORY.to_string(),
            cookies_path: DEFAULT_COOKIES_PATH.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("failed to save settings: {0}")]
    Save(#[from] io::Error),
    #[error("failed to serialize settings: {0}")]
    Serialize(#[from] serde_json::Error),
}

pub struct SettingsStore {
    directory: PathBuf,
    lock: Mutex<()>,
}

impl SettingsStore {
    #[must_use]
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            lock: Mutex::new(()),
        }
    }

    #[must_use]
    pub fn theme(&self) -> ThemePreferences {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.read(THEME_FILE).unwrap_or_default()
    }

    /// Save a validated theme mode while preserving the preset.
    ///
    /// # Errors
    /// Returns an error when settings cannot be written.
    pub fn set_theme_mode(&self, mode: String) -> Result<String, SettingsError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let next_mode = match mode.as_str() {
            "light" | "dark" | "system" => mode,
            _ => DEFAULT_THEME_MODE.to_string(),
        };
        let mut preferences: ThemePreferences = self.read(THEME_FILE).unwrap_or_default();
        preferences.mode.clone_from(&next_mode);
        self.write(THEME_FILE, &preferences)?;
        Ok(next_mode)
    }

    /// Save a validated theme preset while preserving the mode.
    ///
    /// # Errors
    /// Returns an error when settings cannot be written.
    pub fn set_theme_preset(
        &self,
        preset: String,
        supported: &[String],
        default: &str,
    ) -> Result<String, SettingsError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fallback = theme_preset_fallback(supported, default);
        let next_preset = if supported.iter().any(|item| item == &preset) {
            preset
        } else {
            fallback
        };
        let mut preferences: ThemePreferences = self.read(THEME_FILE).unwrap_or_default();
        preferences.preset.clone_from(&next_preset);
        self.write(THEME_FILE, &preferences)?;
        Ok(next_preset)
    }

    #[must_use]
    pub fn theme_preset(&self, supported: &[String], default: &str) -> String {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let preferences: ThemePreferences = self.read(THEME_FILE).unwrap_or_default();
        if supported.iter().any(|item| item == &preferences.preset) {
            preferences.preset
        } else {
            theme_preset_fallback(supported, default)
        }
    }

    #[must_use]
    pub fn download_inputs(&self) -> DownloadInputSettings {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.read(INPUT_FILE).unwrap_or_default()
    }

    /// Save download defaults, replacing blank values with the existing defaults.
    ///
    /// # Errors
    /// Returns an error when settings cannot be written.
    pub fn set_download_inputs(
        &self,
        inputs: DownloadInputSettings,
    ) -> Result<DownloadInputSettings, SettingsError> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let next = DownloadInputSettings {
            directory: if inputs.directory.trim().is_empty() {
                DEFAULT_DOWNLOAD_DIRECTORY.to_string()
            } else {
                inputs.directory
            },
            cookies_path: if inputs.cookies_path.trim().is_empty() {
                DEFAULT_COOKIES_PATH.to_string()
            } else {
                inputs.cookies_path
            },
        };
        self.write(INPUT_FILE, &next)?;
        Ok(next)
    }

    fn read<T: for<'de> Deserialize<'de>>(&self, file: &str) -> Option<T> {
        let content = fs::read_to_string(self.directory.join(file)).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn write<T: Serialize>(&self, file: &str, value: &T) -> Result<(), SettingsError> {
        fs::create_dir_all(&self.directory)?;
        let content = serde_json::to_string_pretty(value)?;
        fs::write(self.directory.join(file), content)?;
        Ok(())
    }
}

fn theme_preset_fallback(supported: &[String], default: &str) -> String {
    if supported.iter().any(|preset| preset == default) {
        default.to_string()
    } else {
        supported
            .first()
            .cloned()
            .unwrap_or_else(|| DEFAULT_THEME_PRESET.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{DownloadInputSettings, SettingsStore};

    #[test]
    fn reads_existing_tauri_json_and_preserves_other_settings() {
        let directory = tempfile::tempdir().expect("temporary settings directory");
        std::fs::write(
            directory.path().join("theme-settings.json"),
            r#"{"mode":"dark","preset":"crystal"}"#,
        )
        .expect("write old theme settings");
        let settings = SettingsStore::new(directory.path().to_path_buf());
        assert_eq!(settings.theme().mode, "dark");
        settings
            .set_theme_mode("light".into())
            .expect("save theme mode");
        assert_eq!(settings.theme().preset, "crystal");
        let inputs = settings
            .set_download_inputs(DownloadInputSettings {
                directory: String::new(),
                cookies_path: String::new(),
            })
            .expect("save input settings");
        assert_eq!(inputs.directory, "~/Downloads");
        assert_eq!(inputs.cookies_path, "~/Downloads/cookies.txt");
    }
}
