use serde::Serialize;
use std::{
    env,
    error::Error,
    ffi::OsStr,
    fmt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const YT_DLP_BINARY_ENV: &str = "QUIVER_YT_DLP_BINARY";
const YT_DLP_BINARY_NAME: &str = "yt-dlp";

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
    SpawnFailed(std::io::Error),
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
        match self {
            Self::MissingBinary(_) => None,
            Self::SpawnFailed(error) => Some(error),
        }
    }
}

pub struct YtDlpRunner {
    binary_path: PathBuf,
}

impl YtDlpRunner {
    pub fn from_environment_or_bundle() -> Result<Self, YtDlpError> {
        let candidate_paths = candidate_binary_paths();

        for path in &candidate_paths {
            if path.is_file() {
                return Ok(Self {
                    binary_path: path.clone(),
                });
            }
        }

        Err(YtDlpError::MissingBinary(candidate_paths))
    }

    pub fn run<I, S>(&self, args: I) -> Result<YtDlpCommandOutput, YtDlpError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = Command::new(&self.binary_path)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(YtDlpError::SpawnFailed)?;

        Ok(YtDlpCommandOutput {
            exit_code: output.status.code(),
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

fn candidate_binary_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(configured_path) = env::var_os(YT_DLP_BINARY_ENV) {
        paths.push(PathBuf::from(configured_path));
    }

    if let Ok(current_exe) = env::current_exe()
        && let Some(executable_directory) = current_exe.parent()
    {
        paths.push(executable_directory.join(sidecar_binary_name()));
        paths.push(executable_directory.join(platform_binary_name()));
    }

    if let Some(workspace_root) = workspace_root() {
        paths.push(
            workspace_root
                .join("src-tauri")
                .join("binaries")
                .join(sidecar_binary_name()),
        );
        paths.push(
            workspace_root
                .join("src-tauri")
                .join("binaries")
                .join(platform_binary_name()),
        );
    }

    paths
}

fn workspace_root() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().map(Path::to_path_buf)
}

fn platform_binary_name() -> String {
    if cfg!(windows) {
        format!("{YT_DLP_BINARY_NAME}.exe")
    } else {
        YT_DLP_BINARY_NAME.to_string()
    }
}

fn sidecar_binary_name() -> String {
    let target = option_env!("TARGET").unwrap_or_default();
    if target.is_empty() {
        return platform_binary_name();
    }

    if cfg!(windows) {
        format!("{YT_DLP_BINARY_NAME}-{target}.exe")
    } else {
        format!("{YT_DLP_BINARY_NAME}-{target}")
    }
}

#[cfg(test)]
mod tests {
    use super::{YtDlpRunner, platform_binary_name, sidecar_binary_name, workspace_root};
    use std::path::PathBuf;

    impl YtDlpRunner {
        fn from_path_for_test(binary_path: PathBuf) -> Self {
            Self { binary_path }
        }

        fn binary_path(&self) -> &std::path::Path {
            &self.binary_path
        }
    }

    #[test]
    fn platform_binary_name_uses_exe_extension_on_windows() {
        let binary_name = platform_binary_name();

        if cfg!(windows) {
            assert_eq!(binary_name, "yt-dlp.exe");
        } else {
            assert_eq!(binary_name, "yt-dlp");
        }
    }

    #[test]
    fn workspace_root_points_at_project_root() {
        let root = workspace_root().expect("workspace root should resolve from Cargo manifest");

        assert!(root.ends_with("Quiver"));
    }

    #[test]
    fn sidecar_binary_name_contains_base_name() {
        assert!(sidecar_binary_name().starts_with("yt-dlp"));
    }

    #[test]
    fn runner_exposes_binary_path() {
        let runner = YtDlpRunner::from_path_for_test(PathBuf::from("yt-dlp"));

        assert!(runner.binary_path().ends_with("yt-dlp"));
    }
}
