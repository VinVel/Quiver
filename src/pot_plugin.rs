use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

use directories::BaseDirs;
use thiserror::Error;

const PLUGIN_ROOT: &str = "bgutil/yt_dlp_plugins/extractor";
static EXTRACTION_LOCK: Mutex<()> = Mutex::new(());
const FILES: [(&str, &[u8]); 3] = [
    (
        "getpot_bgutil.py",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/bgutil-ytdlp-pot-provider/plugin/yt_dlp_plugins/extractor/getpot_bgutil.py"
        )),
    ),
    (
        "getpot_bgutil_http.py",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/bgutil-ytdlp-pot-provider/plugin/yt_dlp_plugins/extractor/getpot_bgutil_http.py"
        )),
    ),
    (
        "getpot_bgutil_script.py",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/bgutil-ytdlp-pot-provider/plugin/yt_dlp_plugins/extractor/getpot_bgutil_script.py"
        )),
    ),
];

#[derive(Debug, Error)]
pub enum PotPluginError {
    #[error("could not find a cache directory for the POT plugin")]
    MissingCacheDirectory,
    #[error("could not extract the POT plugin: {0}")]
    Io(#[from] io::Error),
}

/// Extract the embedded POT plugin to a versioned cache directory.
///
/// # Errors
/// Returns an error if the cache is unavailable or extraction fails.
pub fn ensure_extracted() -> Result<PathBuf, PotPluginError> {
    let cache = BaseDirs::new().ok_or(PotPluginError::MissingCacheDirectory)?;
    extract_to(&cache.cache_dir().join("quiver").join("yt-dlp-plugins"))
}

fn extract_to(parent: &Path) -> Result<PathBuf, PotPluginError> {
    let _guard = EXTRACTION_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let destination = parent.join(format!("v{}", env!("CARGO_PKG_VERSION")));
    if is_current(&destination) {
        return Ok(destination);
    }
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".pot-plugin-")
        .tempdir_in(parent)?;
    let extractor = staging.path().join(PLUGIN_ROOT);
    fs::create_dir_all(&extractor)?;
    for (name, contents) in FILES {
        fs::write(extractor.join(name), contents)?;
    }
    let stale = if destination.exists() {
        let quarantine = parent.join(format!(".stale-pot-plugin-{}", std::process::id()));
        if quarantine.exists() {
            fs::remove_dir_all(&quarantine)?;
        }
        fs::rename(&destination, &quarantine)?;
        Some(quarantine)
    } else {
        None
    };
    match fs::rename(staging.path(), &destination) {
        Ok(()) => {
            if let Some(stale) = stale {
                let _ = fs::remove_dir_all(stale);
            }
            Ok(destination)
        }
        Err(_error) if is_current(&destination) => {
            if let Some(stale) = stale {
                let _ = fs::remove_dir_all(stale);
            }
            Ok(destination)
        }
        Err(error) => Err(error.into()),
    }
}

fn is_current(root: &Path) -> bool {
    let extractor = root.join(PLUGIN_ROOT);
    FILES.iter().all(|(name, contents)| {
        fs::read(extractor.join(name)).is_ok_and(|actual| actual == *contents)
    }) && !extractor.join("getpot_bgutil_cli.py").exists()
}

#[cfg(test)]
mod tests {
    use super::{FILES, PLUGIN_ROOT, extract_to};

    #[test]
    fn extraction_is_complete_and_repairs_stale_files() {
        let cache = tempfile::tempdir().expect("temporary cache");
        let first = extract_to(cache.path()).expect("extract plugin");
        for (name, expected) in FILES {
            assert_eq!(
                std::fs::read(first.join(PLUGIN_ROOT).join(name)).unwrap(),
                expected
            );
        }
        let stale = first.join(PLUGIN_ROOT).join("getpot_bgutil_cli.py");
        std::fs::write(&stale, "obsolete").expect("create stale plugin");
        let second = extract_to(cache.path()).expect("repair plugin");
        assert_eq!(first, second);
        assert!(!stale.exists());
    }
}
