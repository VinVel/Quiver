use std::{path::PathBuf, sync::Mutex};

use thiserror::Error;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    pot_server::{PotServer, PotServerError},
    presets::{self, PresetId},
    subtitle_pipeline,
    yt_dlp::{YtDlpCommandOutput, YtDlpError, YtDlpOutputStream, YtDlpRunner},
};

pub struct DownloadRequest {
    pub preset_id: PresetId,
    pub link: String,
    pub args: Vec<String>,
    pub advanced_subtitle_pipeline: bool,
    pub plugin_dirs: Vec<PathBuf>,
}

#[derive(Debug)]
pub enum DownloadEvent {
    Output {
        stream: YtDlpOutputStream,
        chunk: String,
    },
    Finished {
        success: bool,
        exit_code: Option<i32>,
    },
    Failed(String),
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("a download is already running")]
    Busy,
    #[error(transparent)]
    YtDlp(#[from] YtDlpError),
    #[error(transparent)]
    PotServer(#[from] PotServerError),
    #[error("subtitle pipeline failed: {0}")]
    SubtitlePipeline(String),
}

#[derive(Default)]
pub struct DownloadService {
    pot_server: PotServer,
    active: Mutex<Option<CancellationToken>>,
}

struct ActiveDownload<'a>(&'a DownloadService);

impl Drop for ActiveDownload<'_> {
    fn drop(&mut self) {
        self.0.pot_server.stop();
        if let Ok(mut active) = self.0.active.lock() {
            active.take();
        }
    }
}

impl DownloadService {
    /// Execute one download and send progress to the interface.
    ///
    /// # Errors
    /// Returns an error if another download is active or a required process fails.
    pub async fn run(
        &self,
        request: DownloadRequest,
        events: mpsc::UnboundedSender<DownloadEvent>,
    ) -> Result<YtDlpCommandOutput, DownloadError> {
        let cancellation = {
            let mut active = self
                .active
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if active.is_some() {
                return Err(DownloadError::Busy);
            }
            let cancellation = CancellationToken::new();
            *active = Some(cancellation.clone());
            cancellation
        };
        let _active_guard = ActiveDownload(self);
        let result = self.run_active(request, cancellation, &events).await;
        match &result {
            Ok(output) => {
                let _ = events.send(DownloadEvent::Finished {
                    success: output.success,
                    exit_code: output.exit_code,
                });
            }
            Err(error) => {
                let _ = events.send(DownloadEvent::Failed(error.to_string()));
            }
        }
        result
    }

    async fn run_active(
        &self,
        request: DownloadRequest,
        cancellation: CancellationToken,
        events: &mpsc::UnboundedSender<DownloadEvent>,
    ) -> Result<YtDlpCommandOutput, DownloadError> {
        let requires_pot = presets::all_presets()
            .iter()
            .find(|preset| preset.id == request.preset_id)
            .is_some_and(|preset| preset.requires_pot_server);
        if requires_pot {
            self.pot_server.ensure_started().await?;
        }
        let runner = YtDlpRunner::from_environment_or_bundle()?
            .with_plugin_dirs(request.plugin_dirs)
            .with_cancellation(cancellation);
        let event_sender = events.clone();
        let on_chunk = move |stream, chunk: &str| {
            let _ = event_sender.send(DownloadEvent::Output {
                stream,
                chunk: chunk.to_string(),
            });
        };
        if request.advanced_subtitle_pipeline && presets::is_youtube_video_preset(request.preset_id)
        {
            subtitle_pipeline::run_youtube_video_download(
                runner,
                request.args,
                request.link,
                on_chunk,
            )
            .await
            .map_err(DownloadError::SubtitlePipeline)
        } else {
            runner
                .run_streaming(request.args, on_chunk)
                .await
                .map_err(Into::into)
        }
    }

    /// Request cancellation of the active yt-dlp process.
    pub fn cancel(&self) {
        if let Ok(active) = self.active.lock()
            && let Some(cancellation) = active.as_ref()
        {
            cancellation.cancel();
        }
    }

    /// Cancel a download and stop a provider owned by Quiver.
    pub fn stop(&self) {
        self.cancel();
        self.pot_server.stop();
    }
}
