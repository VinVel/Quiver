use std::{
    net::{SocketAddr, TcpStream},
    process::Stdio,
    sync::Mutex,
    time::Duration,
};

use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    time::sleep,
};

use crate::yt_dlp::sidecar_path;

const POT_SERVER_PORT: u16 = 4416;
const POT_START_ATTEMPTS: usize = 50;
const POT_START_DELAY: Duration = Duration::from_millis(100);

#[derive(Default)]
pub struct PotServer {
    child: Mutex<Option<Child>>,
}

#[derive(Debug, Error)]
pub enum PotServerError {
    #[error("POT provider sidecar is missing")]
    MissingBinary,
    #[error("failed to start POT server: {0}")]
    SpawnFailed(String),
    #[error("POT server did not become ready")]
    NotReady,
}

impl PotServer {
    /// Start the bundled provider only if no provider already listens on the configured port.
    ///
    /// # Errors
    /// Returns an error if the sidecar is missing, fails to launch, or never becomes ready.
    pub async fn ensure_started(&self) -> Result<(), PotServerError> {
        if is_server_reachable() {
            return Ok(());
        }
        let binary = sidecar_path("bgutil-pot").ok_or(PotServerError::MissingBinary)?;
        {
            let mut child_slot = self
                .child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(child) = child_slot.as_mut()
                && child
                    .try_wait()
                    .map_err(|error| PotServerError::SpawnFailed(error.to_string()))?
                    .is_none()
            {
                return Ok(());
            }
            let mut child = Command::new(binary)
                .args(["--port", "4416"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(|error| PotServerError::SpawnFailed(error.to_string()))?;
            if let Some(stderr) = child.stderr.take() {
                tokio::spawn(async move {
                    let mut lines = BufReader::new(stderr).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        eprintln!("POT server: {line}");
                    }
                });
            }
            *child_slot = Some(child);
        }
        for _ in 0..POT_START_ATTEMPTS {
            if is_server_reachable() {
                return Ok(());
            }
            sleep(POT_START_DELAY).await;
        }
        self.stop();
        Err(PotServerError::NotReady)
    }

    pub fn stop(&self) {
        if let Ok(mut child_slot) = self.child.lock()
            && let Some(mut child) = child_slot.take()
        {
            let _ = child.start_kill();
        }
    }
}

impl Drop for PotServer {
    fn drop(&mut self) {
        self.stop();
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
