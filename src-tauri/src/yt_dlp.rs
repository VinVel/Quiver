use serde::Serialize;
use std::{
    env,
    error::Error,
    ffi::OsStr,
    fmt, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

use tauri::AppHandle;
use tauri_plugin_shell::{ShellExt, process::CommandEvent};

const YT_DLP_BINARY_ENV: &str = "QUIVER_YT_DLP_BINARY";
const DENO_BINARY_NAME: &str = "deno";
const FFMPEG_BINARY_NAME: &str = "ffmpeg";
const FFPROBE_BINARY_NAME: &str = "ffprobe";
const YT_DLP_BINARY_NAME: &str = "yt-dlp";
const YT_DLP_PLUGINS_RESOURCE_PATH: &[&str] = &["yt-dlp-plugins"];

#[derive(Debug, Serialize)]
pub struct YtDlpCommandOutput {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub enum YtDlpError {
    MissingBinary(Vec<PathBuf>),
    SpawnFailed(String),
}

impl fmt::Display for YtDlpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBinary(paths) => {
                let searched_paths = paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    formatter,
                    "yt-dlp binary not found; searched {searched_paths}"
                )
            }
            Self::SpawnFailed(error) => write!(formatter, "failed to run yt-dlp: {error}"),
        }
    }
}

impl Error for YtDlpError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

pub struct YtDlpRunner {
    command: YtDlpCommand,
    plugin_dirs: Vec<PathBuf>,
}

enum YtDlpCommand {
    Override(PathBuf),
    Sidecar(AppHandle),
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum YtDlpOutputStream {
    Stdout,
    Stderr,
}

impl YtDlpRunner {
    pub fn from_environment_or_bundle(app: AppHandle) -> Result<Self, YtDlpError> {
        if let Some(configured_path) = env::var_os(YT_DLP_BINARY_ENV) {
            let path = PathBuf::from(configured_path);
            if path.is_file() {
                return Ok(Self {
                    command: YtDlpCommand::Override(path),
                    plugin_dirs: Vec::new(),
                });
            }

            return Err(YtDlpError::MissingBinary(vec![path]));
        }

        Ok(Self {
            command: YtDlpCommand::Sidecar(app),
            plugin_dirs: Vec::new(),
        })
    }

    pub async fn run<I, S>(&self, args: I) -> Result<YtDlpCommandOutput, YtDlpError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = self.command_args(args);

        match &self.command {
            YtDlpCommand::Override(binary_path) => {
                let binary_path = binary_path.clone();
                tauri::async_runtime::spawn_blocking(move || run_override(binary_path, args))
                    .await
                    .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?
            }
            YtDlpCommand::Sidecar(app) => {
                let output = app
                    .shell()
                    .sidecar(YT_DLP_BINARY_NAME)
                    .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?
                    .args(args)
                    .set_raw_out(true)
                    .output()
                    .await
                    .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?;

                Ok(YtDlpCommandOutput {
                    exit_code: output.status.code(),
                    success: output.status.success(),
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                })
            }
        }
    }

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
        let args = self.command_args(args);

        match &self.command {
            YtDlpCommand::Override(binary_path) => {
                let binary_path = binary_path.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    run_override_streaming(binary_path, args, on_chunk)
                })
                .await
                .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?
            }
            YtDlpCommand::Sidecar(app) => run_sidecar_streaming(app, args, on_chunk).await,
        }
    }

    pub fn with_plugin_dirs(mut self, plugin_dirs: Vec<PathBuf>) -> Self {
        self.plugin_dirs = plugin_dirs;
        self
    }

    fn command_args<I, S>(&self, args: I) -> Vec<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command_args = Vec::new();

        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        if !has_js_runtime_args(&args)
            && let Some(deno_path) = bundled_deno_path()
        {
            command_args.push("--js-runtimes".to_string());
            command_args.push(format!("deno:{}", deno_path.display()));
        }

        if !has_ffmpeg_location_args(&args)
            && let Some(ffmpeg_location) = bundled_ffmpeg_location()
        {
            command_args.push("--ffmpeg-location".to_string());
            command_args.push(ffmpeg_location.display().to_string());
        }

        for plugin_dir in &self.plugin_dirs {
            command_args.push("--plugin-dirs".to_string());
            command_args.push(plugin_dir.display().to_string());
        }

        command_args.extend(args);
        command_args
    }
}

fn has_js_runtime_args(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg == "--js-runtimes" || arg.starts_with("--js-runtimes="))
}

fn has_ffmpeg_location_args(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg == "--ffmpeg-location" || arg.starts_with("--ffmpeg-location="))
}

fn bundled_deno_path() -> Option<PathBuf> {
    current_exe_sidecar_path(DENO_BINARY_NAME)
        .filter(|path| path.is_file())
        .or_else(|| workspace_binaries_sidecar_path(DENO_BINARY_NAME))
}

pub fn bundled_ffmpeg_location() -> Option<PathBuf> {
    current_exe_sidecar_dir_with_tools([FFMPEG_BINARY_NAME, FFPROBE_BINARY_NAME])
}

pub fn bundled_ffmpeg_path() -> Option<PathBuf> {
    bundled_tool_path(FFMPEG_BINARY_NAME)
}

pub fn bundled_ffprobe_path() -> Option<PathBuf> {
    bundled_tool_path(FFPROBE_BINARY_NAME)
}

fn bundled_tool_path(name: &str) -> Option<PathBuf> {
    current_exe_sidecar_path(name)
        .filter(|path| path.is_file())
        .or_else(|| workspace_binaries_sidecar_path(name))
}

fn current_exe_sidecar_dir_with_tools<const N: usize>(names: [&str; N]) -> Option<PathBuf> {
    let current_exe = env::current_exe().ok()?;
    let sidecar_dir = current_exe.parent()?.to_path_buf();

    sidecar_dir_contains_tools(&sidecar_dir, names).then_some(sidecar_dir)
}

fn sidecar_dir_contains_tools<const N: usize>(directory: &Path, names: [&str; N]) -> bool {
    names
        .into_iter()
        .all(|name| directory.join(executable_name(name)).is_file())
}

fn current_exe_sidecar_path(name: &str) -> Option<PathBuf> {
    let current_exe = env::current_exe().ok()?;
    let sidecar_dir = current_exe.parent()?;

    Some(sidecar_dir.join(executable_name(name)))
}

fn workspace_binaries_sidecar_path(name: &str) -> Option<PathBuf> {
    let binaries_dir = workspace_root()?.join("src-tauri").join("binaries");
    let prefix = format!("{name}-");
    let extension = executable_extension();

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

fn run_override(binary_path: PathBuf, args: Vec<String>) -> Result<YtDlpCommandOutput, YtDlpError> {
    let output = Command::new(binary_path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?;

    Ok(YtDlpCommandOutput {
        exit_code: output.status.code(),
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn run_override_streaming<F>(
    binary_path: PathBuf,
    args: Vec<String>,
    on_chunk: F,
) -> Result<YtDlpCommandOutput, YtDlpError>
where
    F: Fn(YtDlpOutputStream, &str) + Send + Sync + 'static,
{
    let mut child = Command::new(binary_path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_buffer = Arc::new(Mutex::new(String::new()));
    let stderr_buffer = Arc::new(Mutex::new(String::new()));
    let on_chunk = Arc::new(on_chunk);

    let stdout_reader = stdout.map(|stream| {
        spawn_stream_reader(
            stream,
            YtDlpOutputStream::Stdout,
            Arc::clone(&stdout_buffer),
            Arc::clone(&on_chunk),
        )
    });
    let stderr_reader = stderr.map(|stream| {
        spawn_stream_reader(
            stream,
            YtDlpOutputStream::Stderr,
            Arc::clone(&stderr_buffer),
            Arc::clone(&on_chunk),
        )
    });

    let status = child
        .wait()
        .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?;

    if let Some(reader) = stdout_reader {
        let _ = reader.join();
    }
    if let Some(reader) = stderr_reader {
        let _ = reader.join();
    }

    Ok(YtDlpCommandOutput {
        exit_code: status.code(),
        success: status.success(),
        stdout: read_buffer(&stdout_buffer),
        stderr: read_buffer(&stderr_buffer),
    })
}

async fn run_sidecar_streaming<F>(
    app: &AppHandle,
    args: Vec<String>,
    on_chunk: F,
) -> Result<YtDlpCommandOutput, YtDlpError>
where
    F: Fn(YtDlpOutputStream, &str) + Send + Sync + 'static,
{
    let (mut rx, _child) = app
        .shell()
        .sidecar(YT_DLP_BINARY_NAME)
        .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?
        .args(args)
        .set_raw_out(true)
        .spawn()
        .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?;
    let mut exit_code = None;
    let mut stdout = String::new();
    let mut stderr = String::new();

    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(bytes) => {
                let chunk = String::from_utf8_lossy(&bytes);
                stdout.push_str(&chunk);
                on_chunk(YtDlpOutputStream::Stdout, &chunk);
            }
            CommandEvent::Stderr(bytes) => {
                let chunk = String::from_utf8_lossy(&bytes);
                stderr.push_str(&chunk);
                on_chunk(YtDlpOutputStream::Stderr, &chunk);
            }
            CommandEvent::Terminated(payload) => {
                exit_code = payload.code;
            }
            CommandEvent::Error(error) => {
                stderr.push_str(&error);
                on_chunk(YtDlpOutputStream::Stderr, &error);
            }
            _ => {}
        }
    }

    Ok(YtDlpCommandOutput {
        exit_code,
        success: exit_code == Some(0),
        stdout,
        stderr,
    })
}

fn spawn_stream_reader<R, F>(
    mut stream: R,
    output_stream: YtDlpOutputStream,
    output_buffer: Arc<Mutex<String>>,
    on_chunk: Arc<F>,
) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
    F: Fn(YtDlpOutputStream, &str) + Send + Sync + 'static,
{
    thread::spawn(move || {
        let mut buffer = [0; 4096];

        while let Ok(bytes_read) = stream.read(&mut buffer) {
            if bytes_read == 0 {
                break;
            }

            let chunk = String::from_utf8_lossy(&buffer[..bytes_read]).into_owned();
            if let Ok(mut collected_output) = output_buffer.lock() {
                collected_output.push_str(&chunk);
            }
            on_chunk(output_stream, &chunk);
        }
    })
}

fn read_buffer(output_buffer: &Arc<Mutex<String>>) -> String {
    output_buffer
        .lock()
        .map(|buffer| buffer.clone())
        .unwrap_or_default()
}

fn workspace_root() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().map(Path::to_path_buf)
}

pub fn candidate_plugin_dirs(resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(workspace_root) = workspace_root() {
        paths.push(
            workspace_root
                .join("src-tauri")
                .join("resources")
                .join(plugin_relative_path()),
        );
    }

    if let Some(resource_dir) = resource_dir {
        paths.push(resource_dir.join(plugin_relative_path()));
        paths.push(resource_dir.join("resources").join(plugin_relative_path()));
    }

    paths
        .into_iter()
        .filter(|path| path.join("bgutil").join("yt_dlp_plugins").is_dir())
        .collect()
}

fn plugin_relative_path() -> PathBuf {
    YT_DLP_PLUGINS_RESOURCE_PATH.iter().collect()
}

fn executable_name(name: &str) -> String {
    let extension = executable_extension();

    if extension.is_empty() {
        name.to_string()
    } else {
        format!("{name}{extension}")
    }
}

fn executable_extension() -> &'static str {
    if cfg!(windows) { ".exe" } else { "" }
}

#[cfg(test)]
mod tests {
    use super::{YtDlpCommand, YtDlpRunner, workspace_root};
    use std::path::PathBuf;

    impl YtDlpRunner {
        fn from_path_for_test(binary_path: PathBuf) -> Self {
            Self {
                command: YtDlpCommand::Override(binary_path),
                plugin_dirs: Vec::new(),
            }
        }

        fn binary_path(&self) -> Option<&std::path::Path> {
            match &self.command {
                YtDlpCommand::Override(path) => Some(path),
                YtDlpCommand::Sidecar(_) => None,
            }
        }
    }

    #[test]
    fn workspace_root_points_at_project_root() {
        let root = workspace_root().expect("workspace root should resolve from Cargo manifest");

        assert!(root.ends_with("Quiver"));
    }

    #[test]
    fn runner_exposes_binary_path() {
        let runner = YtDlpRunner::from_path_for_test(PathBuf::from("yt-dlp"));

        assert!(
            runner
                .binary_path()
                .is_some_and(|path| path.ends_with("yt-dlp"))
        );
    }
}
