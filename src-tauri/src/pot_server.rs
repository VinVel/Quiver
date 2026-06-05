use std::{
    error::Error,
    fmt,
    net::{SocketAddr, TcpStream},
    sync::Mutex,
    time::Duration,
};

use tauri::{AppHandle, Manager};
use tauri_plugin_shell::{
    ShellExt,
    process::{CommandChild, CommandEvent},
};

const POT_SERVER_HOST: &str = "127.0.0.1";
const POT_SERVER_PORT: u16 = 4416;
const POT_SERVER_PORT_TEXT: &str = "4416";

#[derive(Default)]
pub struct PotServer {
    child: Mutex<Option<CommandChild>>,
}

#[derive(Debug)]
pub enum PotServerError {
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

    let (mut rx, child) = app
        .shell()
        .sidecar("bgutil-pot")
        .map_err(|error| PotServerError::SpawnFailed(error.to_string()))?
        .args([
            "server",
            "--host",
            POT_SERVER_HOST,
            "--port",
            POT_SERVER_PORT_TEXT,
        ])
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
