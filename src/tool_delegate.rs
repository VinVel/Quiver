//! Source-build delegates for the `FFmpeg` pair. Release archives replace these
//! targets with the packaged native tools on Windows and Linux.

use std::{env, path::PathBuf, process::Command};

/// Forward to a Homebrew tool on macOS or the development-staged tool elsewhere.
#[must_use]
pub fn run(tool: &str) -> i32 {
    let Some(binary) = tool_path(tool) else {
        eprintln!(
            "{tool} is unavailable; install FFmpeg with Homebrew on macOS or build the native sidecar for this platform"
        );
        return 127;
    };
    match Command::new(&binary).args(env::args_os().skip(1)).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("could not run {}: {error}", binary.display());
            1
        }
    }
}

#[cfg(target_os = "macos")]
fn tool_path(tool: &str) -> Option<PathBuf> {
    ["/opt/homebrew/bin", "/usr/local/bin"]
        .into_iter()
        .map(|directory| PathBuf::from(directory).join(tool))
        .find(|path| path.is_file())
}

#[cfg(not(target_os = "macos"))]
fn tool_path(tool: &str) -> Option<PathBuf> {
    let staged = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src-tauri/binaries");
    let prefix = format!("{tool}-{}-", env::consts::ARCH);
    let extension = if cfg!(windows) { ".exe" } else { "" };
    let platform = if cfg!(windows) { "windows" } else { "linux" };
    std::fs::read_dir(staged)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with(&prefix)
                            && name.contains(platform)
                            && name.ends_with(extension)
                    })
        })
}
