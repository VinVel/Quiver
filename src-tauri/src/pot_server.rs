use std::{
    env,
    error::Error,
    fmt,
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use tauri::{AppHandle, Manager};
use tauri_plugin_shell::{
    ShellExt,
    process::{CommandChild, CommandEvent},
};

const POT_SERVER_PORT: u16 = 4416;
const POT_SERVER_RESOURCE_PATH: &[&str] = &["bgutil-ytdlp-pot-provider", "server"];

#[derive(Default)]
pub struct PotServer {
    child: Mutex<Option<CommandChild>>,
}

#[derive(Debug)]
pub enum PotServerError {
    MissingProvider(Vec<PathBuf>),
    MissingNodeModules(PathBuf),
    SpawnFailed(String),
    AlreadyRunning,
}

impl PotServer {
    pub fn start_async(app: AppHandle) {
        tauri::async_runtime::spawn(async move {
            if let Err(error) = start_and_store(&app) {
                eprintln!("Failed to start POT server: {error}");
            }
        });
    }

    fn set_child(&self, child: CommandChild) {
        if let Ok(mut existing_child) = self.child.lock() {
            if let Some(existing_child) = existing_child.take() {
                let _ = existing_child.kill();
            }

            *existing_child = Some(child);
        }
    }

    pub fn stop(&self) {
        if let Ok(mut child) = self.child.lock()
            && let Some(child) = child.take()
        {
            let _ = child.kill();
        }
    }
}

fn start_and_store(app: &AppHandle) -> Result<(), PotServerError> {
    if is_server_reachable() {
        return Ok(());
    }

    let child = spawn_pot_server(app)?;
    app.state::<PotServer>().set_child(child);

    Ok(())
}

fn spawn_pot_server(app: &AppHandle) -> Result<CommandChild, PotServerError> {
    if is_server_reachable() {
        return Err(PotServerError::AlreadyRunning);
    }

    let provider_server = find_provider_server(app)?;
    let node_modules = provider_server.join("node_modules");

    if !node_modules.is_dir() {
        return Err(PotServerError::MissingNodeModules(node_modules));
    }

    let (mut rx, child) = app
        .shell()
        .sidecar("deno")
        .map_err(|error| PotServerError::SpawnFailed(error.to_string()))?
        .args([
            "run",
            "--allow-env",
            "--allow-net",
            "--allow-ffi=.",
            "--allow-read=.",
            "../src/main.ts",
        ])
        .current_dir(node_modules)
        .spawn()
        .map_err(|error| PotServerError::SpawnFailed(error.to_string()))?;

    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stderr(bytes) => {
                    eprintln!(
                        "POT server stderr: {}",
                        String::from_utf8_lossy(&bytes).trim_end()
                    );
                }
                CommandEvent::Error(error) => eprintln!("POT server process error: {error}"),
                _ => {}
            }
        }
    });

    Ok(child)
}

impl Drop for PotServer {
    fn drop(&mut self) {
        self.stop();
    }
}

impl fmt::Display for PotServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingProvider(paths) => write!(
                formatter,
                "POT provider server resource not found; searched {}",
                display_paths(paths)
            ),
            Self::MissingNodeModules(path) => write!(
                formatter,
                "POT provider dependencies are missing at {}",
                path.display()
            ),
            Self::SpawnFailed(error) => write!(formatter, "failed to start POT server: {error}"),
            Self::AlreadyRunning => write!(formatter, "POT server is already reachable"),
        }
    }
}

impl Error for PotServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

fn is_server_reachable() -> bool {
    let addresses = [
        SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], POT_SERVER_PORT)),
        SocketAddr::from(([127, 0, 0, 1], POT_SERVER_PORT)),
    ];

    addresses
        .iter()
        .any(|address| TcpStream::connect_timeout(address, Duration::from_millis(100)).is_ok())
}

fn find_provider_server(app: &AppHandle) -> Result<PathBuf, PotServerError> {
    provider_server_path(app)
        .ok_or_else(|| PotServerError::MissingProvider(provider_candidate_paths(app)))
}

pub fn provider_server_path(app: &AppHandle) -> Option<PathBuf> {
    let candidate_paths = provider_candidate_paths(app);

    for path in &candidate_paths {
        if is_prepared_provider_server(path) {
            return Some(path.clone());
        }
    }

    None
}

fn provider_candidate_paths(app: &AppHandle) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(workspace_root) = workspace_root() {
        paths.push(
            workspace_root
                .join("src-tauri")
                .join("resources")
                .join(provider_relative_path()),
        );
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        paths.push(resource_dir.join(provider_relative_path()));
        paths.push(
            resource_dir
                .join("resources")
                .join(provider_relative_path()),
        );
    }

    paths
}

fn is_prepared_provider_server(path: &Path) -> bool {
    path.join("src").join("main.ts").is_file()
        && path.join("node_modules").join("commander").is_dir()
}

fn provider_relative_path() -> PathBuf {
    POT_SERVER_RESOURCE_PATH.iter().collect()
}

fn workspace_root() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().map(Path::to_path_buf)
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
