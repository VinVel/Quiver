//! Source-build delegates for native sidecars staged by the build script.
//! Release archives replace these targets with the packaged native tools.

use std::{env, path::PathBuf, process::Command};

/// Forward to a Homebrew `FFmpeg` tool on macOS or a staged native sidecar.
#[must_use]
pub fn run(tool: &str) -> i32 {
    let Some(binary) = tool_path(tool) else {
        eprintln!(
            "{tool} is unavailable; build Quiver's native sidecars or install FFmpeg with Homebrew on macOS"
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

fn tool_path(tool: &str) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    if matches!(tool, "ffmpeg" | "ffprobe") {
        return ["/opt/homebrew/bin", "/usr/local/bin"]
            .into_iter()
            .map(|directory| PathBuf::from(directory).join(tool))
            .find(|path| path.is_file());
    }

    let manifest = PathBuf::from(option_env!("QUIVER_SIDECAR_MANIFEST")?);
    let filename = if cfg!(windows) {
        format!("quiver-{tool}.exe")
    } else {
        format!("quiver-{tool}")
    };
    let path = manifest.parent()?.join("sidecars").join(filename);
    path.is_file().then_some(path)
}
