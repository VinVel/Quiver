use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const PYTHON_VERSION: &str = "3.12";
const SIDECAR_NAME: &str = "yt-dlp";

fn main() {
    configure_build_tracking();
    stage_yt_dlp_sidecar();
    tauri_build::build();
}

fn configure_build_tracking() {
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-changed=../yt-dlp/pyproject.toml");
    println!("cargo:rerun-if-changed=../yt-dlp/yt_dlp/version.py");
    println!("cargo:rerun-if-changed=../yt-dlp/bundle/pyinstaller.py");
}

fn stage_yt_dlp_sidecar() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let expected_sidecar = manifest_dir
        .join("binaries")
        .join(sidecar_file_name_for_host());

    if expected_sidecar.is_file() {
        return;
    }

    let Some(workspace_root) = manifest_dir.parent() else {
        warn("Could not resolve workspace root; skipping yt-dlp sidecar build.");
        return;
    };
    let yt_dlp_dir = workspace_root.join("yt-dlp");

    if !yt_dlp_dir.join("pyproject.toml").is_file() {
        warn(format!(
            "yt-dlp submodule is missing at {}; skipping sidecar build.",
            yt_dlp_dir.display()
        ));
        return;
    }

    build_yt_dlp(&yt_dlp_dir);

    let Some(built_binary) = find_built_binary(&yt_dlp_dir) else {
        panic!(
            "yt-dlp build finished but no matching binary was found in {}",
            yt_dlp_dir.join("dist").display()
        );
    };

    fs::create_dir_all(
        expected_sidecar
            .parent()
            .expect("sidecar path should have a parent directory"),
    )
    .expect("failed to create sidecar output directory");
    fs::copy(&built_binary, &expected_sidecar).unwrap_or_else(|error| {
        panic!(
            "failed to copy {} to {}: {error}",
            built_binary.display(),
            expected_sidecar.display()
        );
    });
}

fn build_yt_dlp(yt_dlp_dir: &Path) {
    run_uv(
        yt_dlp_dir,
        [
            "run",
            "--project",
            ".",
            "--python",
            PYTHON_VERSION,
            "--extra",
            "default",
            "--group",
            "pyinstaller",
            "python",
            "devscripts/make_lazy_extractors.py",
        ],
    );
    run_uv(
        yt_dlp_dir,
        [
            "run",
            "--project",
            ".",
            "--python",
            PYTHON_VERSION,
            "--extra",
            "default",
            "--group",
            "pyinstaller",
            "python",
            "-m",
            "bundle.pyinstaller",
        ],
    );
}

fn run_uv<const N: usize>(working_directory: &Path, args: [&str; N]) {
    let status = Command::new("uv")
        .args(args)
        .current_dir(working_directory)
        .status()
        .expect("failed to start uv while building yt-dlp sidecar");

    assert!(
        status.success(),
        "uv failed while building yt-dlp sidecar with status {status}"
    );
}

fn find_built_binary(yt_dlp_dir: &Path) -> Option<PathBuf> {
    let dist = yt_dlp_dir.join("dist");

    built_binary_candidates()
        .iter()
        .map(|candidate| dist.join(candidate))
        .find(|candidate| candidate.is_file())
}

fn built_binary_candidates() -> Vec<&'static str> {
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        vec!["yt-dlp_arm64.exe", "yt-dlp.exe"]
    }

    #[cfg(all(target_os = "windows", not(target_arch = "aarch64")))]
    {
        vec!["yt-dlp.exe"]
    }

    #[cfg(target_os = "macos")]
    {
        vec!["yt-dlp_macos"]
    }

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        vec!["yt-dlp_linux_aarch64", "yt-dlp_linux"]
    }

    #[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
    {
        vec!["yt-dlp_linux"]
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        vec![SIDECAR_NAME]
    }
}

fn sidecar_file_name_for_host() -> String {
    let target = env::var("TARGET").unwrap_or_else(|_| host_target_triple().to_string());

    #[cfg(target_os = "windows")]
    {
        format!("{SIDECAR_NAME}-{target}.exe")
    }

    #[cfg(not(target_os = "windows"))]
    {
        format!("{SIDECAR_NAME}-{target}")
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn host_target_triple() -> &'static str {
    "x86_64-pc-windows-msvc"
}

#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
fn host_target_triple() -> &'static str {
    "aarch64-pc-windows-msvc"
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn host_target_triple() -> &'static str {
    "x86_64-apple-darwin"
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn host_target_triple() -> &'static str {
    "aarch64-apple-darwin"
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn host_target_triple() -> &'static str {
    "x86_64-unknown-linux-gnu"
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn host_target_triple() -> &'static str {
    "aarch64-unknown-linux-gnu"
}

#[cfg(not(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64")
)))]
fn host_target_triple() -> &'static str {
    "unknown"
}

fn warn(message: impl AsRef<str>) {
    println!("cargo:warning={}", message.as_ref());
}
