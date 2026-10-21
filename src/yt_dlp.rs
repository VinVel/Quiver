use std::{env, ffi::OsStr, path::PathBuf, process::Stdio, sync::Arc};

use serde::Serialize;
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};
use tokio_util::sync::CancellationToken;

#[cfg(target_os = "macos")]
use std::path::Path;

const YT_DLP_BINARY_ENV: &str = "QUIVER_YT_DLP_BINARY";
const YT_DLP_BINARY_NAME: &str = "yt-dlp";

#[derive(Debug, Serialize)]
pub struct YtDlpCommandOutput {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Error)]
pub enum YtDlpError {
    #[error("yt-dlp binary not found; searched {0:?}")]
    MissingBinary(Vec<PathBuf>),
    #[error("required {name} sidecar is missing; searched {paths:?}")]
    MissingSidecar {
        name: &'static str,
        paths: Vec<PathBuf>,
    },
    #[error("failed to run yt-dlp: {0}")]
    SpawnFailed(String),
    #[error("download cancelled")]
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum YtDlpOutputStream {
    Stdout,
    Stderr,
}

pub struct YtDlpRunner {
    binary_path: PathBuf,
    plugin_dirs: Vec<PathBuf>,
    uses_bundled_yt_dlp: bool,
    cancellation: CancellationToken,
}

impl YtDlpRunner {
    /// Locate the bundled executable, or use the development override.
    ///
    /// # Errors
    /// Returns the paths searched when no executable exists.
    pub fn from_environment_or_bundle() -> Result<Self, YtDlpError> {
        if let Some(configured_path) = env::var_os(YT_DLP_BINARY_ENV) {
            let path = PathBuf::from(configured_path);
            return if path.is_file() {
                Ok(Self::from_binary(path, false))
            } else {
                Err(YtDlpError::MissingBinary(vec![path]))
            };
        }
        let candidates = candidate_sidecar_paths(YT_DLP_BINARY_NAME);
        let path = candidates
            .iter()
            .find(|path| path.is_file())
            .cloned()
            .ok_or(YtDlpError::MissingBinary(candidates))?;
        Ok(Self::from_binary(path, true))
    }

    fn from_binary(binary_path: PathBuf, uses_bundled_yt_dlp: bool) -> Self {
        Self {
            binary_path,
            plugin_dirs: Vec::new(),
            uses_bundled_yt_dlp,
            cancellation: CancellationToken::new(),
        }
    }

    #[must_use]
    pub fn with_plugin_dirs(mut self, plugin_dirs: Vec<PathBuf>) -> Self {
        if self.uses_bundled_yt_dlp {
            self.plugin_dirs = plugin_dirs;
        }
        self
    }

    #[must_use]
    pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Run yt-dlp and collect its output.
    ///
    /// # Errors
    /// Returns an error if the process cannot start, fails while reading, or is cancelled.
    pub async fn run<I, S>(&self, args: I) -> Result<YtDlpCommandOutput, YtDlpError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.run_streaming(args, |_, _| {}).await
    }

    /// Run yt-dlp and forward stdout/stderr chunks to the caller.
    ///
    /// # Errors
    /// Returns an error if the process cannot start, fails while reading, or is cancelled.
    pub async fn run_streaming<I, S, F>(
        &self,
        args: I,
        on_chunk: F,
    ) -> Result<YtDlpCommandOutput, YtDlpError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        F: Fn(YtDlpOutputStream, &str) + Send + Sync + 'static,
    {
        if self.cancellation.is_cancelled() {
            return Err(YtDlpError::Cancelled);
        }
        let mut command = Command::new(&self.binary_path);
        command
            .args(self.command_args(args)?)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command
            .spawn()
            .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?;
        let on_chunk = Arc::new(on_chunk);
        let stdout_pipe = child.stdout.take().ok_or_else(|| {
            YtDlpError::SpawnFailed("yt-dlp stdout pipe was unavailable".to_string())
        })?;
        let stderr_pipe = child.stderr.take().ok_or_else(|| {
            YtDlpError::SpawnFailed("yt-dlp stderr pipe was unavailable".to_string())
        })?;
        let stdout = tokio::spawn(read_stream(
            stdout_pipe,
            YtDlpOutputStream::Stdout,
            Arc::clone(&on_chunk),
        ));
        let stderr = tokio::spawn(read_stream(
            stderr_pipe,
            YtDlpOutputStream::Stderr,
            on_chunk,
        ));
        let status = tokio::select! {
            biased;
            () = self.cancellation.cancelled() => {
                #[cfg(unix)]
                kill_process_tree(&mut child);
                #[cfg(windows)]
                kill_process_tree(&mut child).await;
                let _ = child.wait().await;
                stdout.abort();
                stderr.abort();
                return Err(YtDlpError::Cancelled);
            }
            status = child.wait() => status.map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?,
        };
        let stdout = stdout
            .await
            .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))??;
        let stderr = stderr
            .await
            .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))??;
        Ok(YtDlpCommandOutput {
            exit_code: status.code(),
            success: status.success(),
            stdout,
            stderr,
        })
    }

    fn command_args<I, S>(&self, args: I) -> Result<Vec<String>, YtDlpError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let mut command_args = Vec::new();
        if self.uses_bundled_yt_dlp && !has_option(&args, "--js-runtimes") {
            let path = required_sidecar_path("deno")?;
            command_args.extend([
                "--js-runtimes".to_string(),
                format!("deno:{}", path.display()),
            ]);
        }
        if self.uses_bundled_yt_dlp && !has_option(&args, "--ffmpeg-location") {
            let location = bundled_ffmpeg_location()?;
            command_args.extend([
                "--ffmpeg-location".to_string(),
                location.display().to_string(),
            ]);
        }
        for plugin_dir in &self.plugin_dirs {
            command_args.extend([
                "--plugin-dirs".to_string(),
                plugin_dir.display().to_string(),
            ]);
        }
        command_args.extend(args);
        Ok(command_args)
    }
}

#[cfg(unix)]
fn kill_process_tree(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        use nix::{
            sys::signal::{Signal, kill},
            unistd::Pid,
        };
        if let Ok(pid) = i32::try_from(pid) {
            let _ = kill(Pid::from_raw(-pid), Signal::SIGKILL);
        }
    }
    let _ = child.start_kill();
}

#[cfg(windows)]
async fn kill_process_tree(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .await;
    }
    let _ = child.start_kill();
}

async fn read_stream<R, F>(
    mut stream: R,
    output_stream: YtDlpOutputStream,
    on_chunk: Arc<F>,
) -> Result<String, YtDlpError>
where
    R: AsyncRead + Unpin,
    F: Fn(YtDlpOutputStream, &str),
{
    let mut output = String::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = stream
            .read(&mut buffer)
            .await
            .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?;
        if read == 0 {
            return Ok(output);
        }
        let chunk = String::from_utf8_lossy(&buffer[..read]);
        output.push_str(&chunk);
        on_chunk(output_stream, &chunk);
    }
}

fn has_option(args: &[String], option: &str) -> bool {
    args.iter()
        .any(|arg| arg == option || arg.starts_with(&format!("{option}=")))
}

fn candidate_sidecar_paths(name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(exe) = env::current_exe()
        && let Some(directory) = exe.parent()
    {
        paths.push(directory.join(executable_name(&format!("quiver-{name}"))));
    }
    #[cfg(debug_assertions)]
    if let Some(path) = staged_sidecar_path(name) {
        paths.push(path);
    }
    paths
}

/// Locate a sidecar beside Quiver or in Cargo's development staging directory.
#[must_use]
pub fn sidecar_path(name: &str) -> Option<PathBuf> {
    candidate_sidecar_paths(name)
        .into_iter()
        .find(|path| path.is_file())
}

fn required_sidecar_path(name: &'static str) -> Result<PathBuf, YtDlpError> {
    let candidates = candidate_sidecar_paths(name);
    candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .ok_or(YtDlpError::MissingSidecar {
            name,
            paths: candidates,
        })
}

fn bundled_ffmpeg_location() -> Result<PathBuf, YtDlpError> {
    #[cfg(target_os = "macos")]
    {
        let (ffmpeg, ffprobe) = if let (Some(ffmpeg), Some(ffprobe)) =
            (sidecar_path("ffmpeg"), sidecar_path("ffprobe"))
        {
            (ffmpeg, ffprobe)
        } else {
            (
                macos_homebrew_tool("ffmpeg")?,
                macos_homebrew_tool("ffprobe")?,
            )
        };
        if ffmpeg.parent() == ffprobe.parent() {
            Ok(ffmpeg)
        } else {
            Err(YtDlpError::SpawnFailed(
                "Homebrew ffmpeg and ffprobe must share a directory".into(),
            ))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let ffmpeg = required_sidecar_path("ffmpeg")?;
        let ffprobe = required_sidecar_path("ffprobe")?;
        if ffmpeg.parent() == ffprobe.parent() {
            Ok(ffmpeg)
        } else {
            Err(YtDlpError::SpawnFailed(
                "ffmpeg and ffprobe sidecars must share a directory".into(),
            ))
        }
    }
}

#[must_use]
pub fn ffmpeg_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    return sidecar_path("ffmpeg")
        .unwrap_or_else(|| macos_homebrew_tool("ffmpeg").unwrap_or_else(|_| "ffmpeg".into()));
    #[cfg(not(target_os = "macos"))]
    return sidecar_path("ffmpeg").unwrap_or_else(|| {
        candidate_sidecar_paths("ffmpeg")
            .into_iter()
            .next()
            .unwrap_or_else(|| "quiver-ffmpeg".into())
    });
}

#[must_use]
pub fn ffprobe_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    return sidecar_path("ffprobe")
        .unwrap_or_else(|| macos_homebrew_tool("ffprobe").unwrap_or_else(|_| "ffprobe".into()));
    #[cfg(not(target_os = "macos"))]
    return sidecar_path("ffprobe").unwrap_or_else(|| {
        candidate_sidecar_paths("ffprobe")
            .into_iter()
            .next()
            .unwrap_or_else(|| "quiver-ffprobe".into())
    });
}

#[cfg(target_os = "macos")]
fn macos_homebrew_tool(name: &'static str) -> Result<PathBuf, YtDlpError> {
    let candidates = ["/opt/homebrew/bin", "/usr/local/bin"]
        .into_iter()
        .map(|directory| Path::new(directory).join(name))
        .collect::<Vec<_>>();
    candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .ok_or(YtDlpError::MissingSidecar {
            name,
            paths: candidates,
        })
}

#[must_use]
#[cfg(debug_assertions)]
fn staged_sidecar_path(name: &str) -> Option<PathBuf> {
    let manifest = PathBuf::from(option_env!("QUIVER_SIDECAR_MANIFEST")?);
    Some(
        manifest
            .parent()?
            .join("sidecars")
            .join(executable_name(&format!("quiver-{name}"))),
    )
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        YtDlpError, YtDlpRunner, candidate_sidecar_paths, has_option, staged_sidecar_path,
    };
    #[cfg(unix)]
    use std::{os::unix::fs::PermissionsExt, time::Duration};
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    };
    #[cfg(unix)]
    use tokio_util::sync::CancellationToken;

    #[test]
    fn build_stages_yt_dlp_in_out_dir() {
        assert!(staged_sidecar_path("yt-dlp").is_some_and(|path| path.is_file()));
    }

    #[test]
    fn installed_sidecars_are_searched_beside_quiver_first() {
        let executable = std::env::current_exe().expect("current executable");
        assert_eq!(
            candidate_sidecar_paths("yt-dlp").first(),
            Some(&executable.parent().unwrap().join(if cfg!(windows) {
                "quiver-yt-dlp.exe"
            } else {
                "quiver-yt-dlp"
            }))
        );
    }

    #[test]
    fn override_ignores_plugin_dirs() {
        let args = YtDlpRunner::from_binary(PathBuf::from("yt-dlp"), false)
            .with_plugin_dirs(vec![PathBuf::from("/tmp/yt-dlp-plugins")])
            .command_args(["--version"])
            .expect("command args");
        assert!(!has_option(&args, "--plugin-dirs"));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn bundled_runner_uses_plugin_dirs() {
        let args = YtDlpRunner::from_binary(PathBuf::from("quiver-yt-dlp"), true)
            .with_plugin_dirs(vec![PathBuf::from("/tmp/yt-dlp-plugins")])
            .command_args(["--version"])
            .expect("command args");
        assert!(has_option(&args, "--plugin-dirs"));
        assert!(has_option(&args, "--js-runtimes"));
        assert!(has_option(&args, "--ffmpeg-location"));
        let ffmpeg_location = args
            .iter()
            .position(|arg| arg == "--ffmpeg-location")
            .and_then(|index| args.get(index + 1))
            .expect("explicit FFmpeg executable");
        assert!(ffmpeg_location.contains("ffmpeg"));
        assert!(!ffmpeg_location.ends_with("binaries"));
    }

    #[cfg(unix)]
    fn fake_sidecar(script: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("create temporary directory");
        let path = directory.path().join("fake-yt-dlp");
        std::fs::write(&path, script).expect("write fake sidecar");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("make fake sidecar executable");
        (directory, path)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn streams_both_process_outputs() {
        let (_directory, path) =
            fake_sidecar("#!/bin/sh\nprintf 'downloaded\\n'\nprintf 'warning\\n' >&2\n");
        let runner = YtDlpRunner::from_binary(path, false);
        let observed = Arc::new(Mutex::new(Vec::new()));
        let callback_observed = Arc::clone(&observed);
        let output = runner
            .run_streaming(["--version"], move |stream, chunk| {
                callback_observed
                    .lock()
                    .expect("callback mutex")
                    .push((stream, chunk.to_string()));
            })
            .await
            .expect("run fake sidecar");
        assert_eq!(output.stdout, "downloaded\n");
        assert_eq!(output.stderr, "warning\n");
        assert!(output.success);
        let observed = observed.lock().expect("callback mutex");
        let stdout = observed
            .iter()
            .filter(|(stream, _)| *stream == super::YtDlpOutputStream::Stdout)
            .map(|(_, chunk)| chunk.as_str())
            .collect::<String>();
        let stderr = observed
            .iter()
            .filter(|(stream, _)| *stream == super::YtDlpOutputStream::Stderr)
            .map(|(_, chunk)| chunk.as_str())
            .collect::<String>();
        drop(observed);
        assert_eq!(stdout, output.stdout);
        assert_eq!(stderr, output.stderr);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancellation_terminates_a_process_group() {
        let (_directory, path) = fake_sidecar("#!/bin/sh\nprintf 'ready\\n'\nsleep 30\n");
        let cancellation = CancellationToken::new();
        let runner = YtDlpRunner::from_binary(path, false).with_cancellation(cancellation.clone());
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancellation.cancel();
        });
        let result = tokio::time::timeout(Duration::from_secs(3), runner.run(["--version"]))
            .await
            .expect("cancellation should return promptly");
        assert!(matches!(result, Err(YtDlpError::Cancelled)));
    }
}
