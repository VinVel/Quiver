use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};

use serde::Serialize;
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};
use tokio_util::sync::CancellationToken;

const YT_DLP_BINARY_ENV: &str = "QUIVER_YT_DLP_BINARY";
const YT_DLP_BINARY_NAME: &str = "quiver_yt-dlp";

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
            .args(self.command_args(args))
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

    fn command_args<I, S>(&self, args: I) -> Vec<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let mut command_args = Vec::new();
        if !has_option(&args, "--js-runtimes")
            && let Some(path) = sidecar_path("deno")
        {
            command_args.extend([
                "--js-runtimes".to_string(),
                format!("deno:{}", path.display()),
            ]);
        }
        if !has_option(&args, "--ffmpeg-location")
            && let Some(location) = bundled_ffmpeg_location()
        {
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
        command_args
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
        paths.push(directory.join(executable_name(name)));
    }
    #[cfg(debug_assertions)]
    if let Some(path) = workspace_binaries_sidecar_path(name) {
        paths.push(path);
    }
    paths
}

/// Locate a sidecar beside Quiver or in the development staging directory.
#[must_use]
pub fn sidecar_path(name: &str) -> Option<PathBuf> {
    candidate_sidecar_paths(name)
        .into_iter()
        .find(|path| path.is_file())
}

fn bundled_ffmpeg_location() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        let ffmpeg = sidecar_path("ffmpeg")?;
        let ffprobe = ffmpeg.parent()?.join(executable_name("ffprobe"));
        ffprobe.is_file().then_some(ffmpeg.parent()?.to_path_buf())
    }
}

#[must_use]
pub fn ffmpeg_path() -> PathBuf {
    sidecar_path("ffmpeg").unwrap_or_else(|| "ffmpeg".into())
}

#[must_use]
pub fn ffprobe_path() -> PathBuf {
    sidecar_path("ffprobe").unwrap_or_else(|| "ffprobe".into())
}

#[must_use]
pub fn candidate_plugin_dirs(resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(debug_assertions)]
    paths.push(workspace_root().join("src-tauri/resources/yt-dlp-plugins"));
    if let Some(resource_dir) = resource_dir {
        paths.push(resource_dir.join("yt-dlp-plugins"));
        paths.push(resource_dir.join("resources/yt-dlp-plugins"));
    }
    paths
        .into_iter()
        .filter(|path| is_supported_plugin_dir(path))
        .collect()
}

fn is_supported_plugin_dir(path: &Path) -> bool {
    let root = path.join("bgutil/yt_dlp_plugins");
    root.is_dir()
        && root.join("extractor/getpot_bgutil_http.py").is_file()
        && !root.join("extractor/getpot_bgutil_cli.py").exists()
}

#[cfg(debug_assertions)]
fn workspace_binaries_sidecar_path(name: &str) -> Option<PathBuf> {
    let binaries_dir = workspace_root().join("src-tauri/binaries");
    let prefix = format!("{name}-");
    let extension = if cfg!(windows) { ".exe" } else { "" };
    fs::read_dir(binaries_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|file_name| {
                        file_name.starts_with(&prefix) && file_name.ends_with(extension)
                    })
        })
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
    use super::{YtDlpError, YtDlpRunner, has_option, is_supported_plugin_dir, workspace_root};
    #[cfg(unix)]
    use std::{os::unix::fs::PermissionsExt, time::Duration};
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    };
    #[cfg(unix)]
    use tokio_util::sync::CancellationToken;

    #[test]
    fn workspace_root_points_at_project_root() {
        assert!(workspace_root().ends_with("Quiver"));
    }

    #[test]
    fn override_ignores_plugin_dirs() {
        let args = YtDlpRunner::from_binary(PathBuf::from("yt-dlp"), false)
            .with_plugin_dirs(vec![PathBuf::from("/tmp/yt-dlp-plugins")])
            .command_args(["--version"]);
        assert!(!has_option(&args, "--plugin-dirs"));
    }

    #[test]
    fn bundled_runner_uses_plugin_dirs() {
        let args = YtDlpRunner::from_binary(PathBuf::from("quiver_yt-dlp"), true)
            .with_plugin_dirs(vec![PathBuf::from("/tmp/yt-dlp-plugins")])
            .command_args(["--version"]);
        assert!(has_option(&args, "--plugin-dirs"));
    }

    #[test]
    fn stale_cli_plugin_is_rejected() {
        let root =
            std::env::temp_dir().join(format!("quiver-stale-plugin-test-{}", std::process::id()));
        let extractor = root.join("bgutil/yt_dlp_plugins/extractor");
        std::fs::create_dir_all(&extractor).expect("create plugin directory");
        std::fs::write(extractor.join("getpot_bgutil_cli.py"), "# stale").expect("write plugin");
        std::fs::write(extractor.join("getpot_bgutil_http.py"), "# http").expect("write plugin");
        assert!(!is_supported_plugin_dir(&root));
        std::fs::remove_dir_all(root).expect("remove plugin directory");
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
