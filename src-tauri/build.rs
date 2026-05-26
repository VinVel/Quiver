use std::{
    env, fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};

use sha2::{Digest, Sha256};
use zip::ZipArchive;

const DENO_VERSION: &str = "2.8.0";
const PYTHON_VERSION: &str = "3.12";
const DENO_SIDECAR_NAME: &str = "deno";
const YT_DLP_SIDECAR_NAME: &str = "yt-dlp";

fn main() {
    configure_build_tracking();
    stage_yt_dlp_sidecar();
    stage_deno_sidecar();
    prepare_pot_provider_resource();

    let mut attributes = tauri_build::Attributes::new();
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        add_manifest();
    }

    tauri_build::try_build(attributes).expect("failed to build Tauri application metadata");
}

fn configure_build_tracking() {
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-changed=../yt-dlp/pyproject.toml");
    println!("cargo:rerun-if-changed=../yt-dlp/yt_dlp/version.py");
    println!("cargo:rerun-if-changed=../yt-dlp/bundle/pyinstaller.py");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider/server/package.json");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider/server/deno.lock");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider/server/src/main.ts");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider/server/src/session_manager.ts");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider/server/src/utils.ts");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider/plugin/yt_dlp_plugins");
}

fn stage_yt_dlp_sidecar() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let expected_sidecar = manifest_dir
        .join("binaries")
        .join(sidecar_file_name_for_host(YT_DLP_SIDECAR_NAME));

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

    let uv_binary = install_uv(&manifest_dir);
    build_yt_dlp(&yt_dlp_dir, &uv_binary);

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

fn install_uv(manifest_dir: &Path) -> PathBuf {
    install_cargo_binary("uv", manifest_dir)
}

fn install_cargo_binary(name: &str, manifest_dir: &Path) -> PathBuf {
    let install_root = manifest_dir.join("binaries");
    let binary = install_root.join("bin").join(executable_name(name));

    if binary.is_file() {
        return binary;
    }

    let status = Command::new("cargo")
        .args(["install", name, "--locked", "--root"])
        .arg(&install_root)
        .status()
        .unwrap_or_else(|error| panic!("failed to start cargo while installing {name}: {error}"));

    assert!(
        status.success(),
        "cargo install {name} --locked failed with status {status}"
    );

    binary
}

fn prepare_pot_provider_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let Some(workspace_root) = manifest_dir.parent() else {
        warn("Could not resolve workspace root; skipping POT provider resource preparation.");
        return;
    };
    let source_server = workspace_root
        .join("bgutil-ytdlp-pot-provider")
        .join("server");

    if !source_server.join("package.json").is_file() {
        warn(format!(
            "bgutil-ytdlp-pot-provider submodule is missing at {}; skipping POT provider resource preparation.",
            source_server.display()
        ));
        return;
    }

    let resource_server = manifest_dir
        .join("resources")
        .join("bgutil-ytdlp-pot-provider")
        .join("server");

    copy_pot_provider_sources(&source_server, &resource_server);
    copy_pot_provider_plugin(workspace_root, &manifest_dir);

    let commander_package = resource_server.join("node_modules").join("commander");
    let express_package = resource_server.join("node_modules").join("express");
    let node_modules = resource_server.join("node_modules");

    if !commander_package.is_dir() || !express_package.is_dir() || !is_symlink(&commander_package) {
        if node_modules.exists() {
            fs::remove_dir_all(&node_modules).unwrap_or_else(|error| {
                panic!(
                    "failed to remove stale POT provider dependencies at {}: {error}",
                    node_modules.display()
                )
            });
        }
        install_pot_provider_dependencies(&manifest_dir, &resource_server);
    }
}

fn copy_pot_provider_plugin(workspace_root: &Path, manifest_dir: &Path) {
    let source_plugin = workspace_root
        .join("bgutil-ytdlp-pot-provider")
        .join("plugin");

    if !source_plugin.join("yt_dlp_plugins").is_dir() {
        warn(format!(
            "bgutil-ytdlp-pot-provider plugin is missing at {}; skipping yt-dlp plugin resource preparation.",
            source_plugin.display()
        ));
        return;
    }

    let plugin_resource = manifest_dir
        .join("resources")
        .join("yt-dlp-plugins")
        .join("bgutil");

    copy_file_if_exists(
        &source_plugin.join("pyproject.toml"),
        &plugin_resource.join("pyproject.toml"),
    );
    copy_directory(
        &source_plugin.join("yt_dlp_plugins"),
        &plugin_resource.join("yt_dlp_plugins"),
    );
}

fn copy_pot_provider_sources(source_server: &Path, resource_server: &Path) {
    fs::create_dir_all(resource_server)
        .expect("failed to create POT provider resource output directory");

    for file_name in [
        ".gitattributes",
        ".prettierrc.json",
        "deno.lock",
        "package-lock.json",
        "package.json",
        "README.md",
        "tsconfig.json",
    ] {
        copy_file_if_exists(
            &source_server.join(file_name),
            &resource_server.join(file_name),
        );
    }

    copy_directory(&source_server.join("src"), &resource_server.join("src"));
    copy_directory(&source_server.join("types"), &resource_server.join("types"));
}

fn install_pot_provider_dependencies(manifest_dir: &Path, resource_server: &Path) {
    let deno_binary = manifest_dir
        .join("binaries")
        .join(sidecar_file_name_for_host(DENO_SIDECAR_NAME));
    let status = Command::new(&deno_binary)
        .args([
            "install",
            "--node-modules-dir=auto",
            "--allow-scripts=npm:canvas",
            "--frozen",
        ])
        .current_dir(resource_server)
        .status()
        .unwrap_or_else(|error| {
            panic!(
                "failed to start Deno while installing POT provider dependencies from {}: {error}",
                deno_binary.display()
            )
        });

    assert!(
        status.success(),
        "Deno failed while installing POT provider dependencies with status {status}"
    );
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

fn copy_directory(source: &Path, destination: &Path) {
    if !source.is_dir() {
        return;
    }

    fs::create_dir_all(destination).unwrap_or_else(|error| {
        panic!(
            "failed to create directory {} while preparing POT provider resource: {error}",
            destination.display()
        )
    });

    for entry in fs::read_dir(source).unwrap_or_else(|error| {
        panic!(
            "failed to read directory {} while preparing POT provider resource: {error}",
            source.display()
        )
    }) {
        let entry = entry.expect("failed to read POT provider source directory entry");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if source_path.is_dir() {
            copy_directory(&source_path, &destination_path);
        } else {
            copy_file_if_exists(&source_path, &destination_path);
        }
    }
}

fn copy_file_if_exists(source: &Path, destination: &Path) {
    if !source.is_file() {
        return;
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|error| {
            panic!(
                "failed to create directory {} while preparing POT provider resource: {error}",
                parent.display()
            )
        });
    }

    fs::copy(source, destination).unwrap_or_else(|error| {
        panic!(
            "failed to copy {} to {} while preparing POT provider resource: {error}",
            source.display(),
            destination.display()
        )
    });
}

fn build_yt_dlp(yt_dlp_dir: &Path, uv_binary: &Path) {
    run_uv(
        uv_binary,
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
        uv_binary,
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

fn run_uv<const N: usize>(uv_binary: &Path, working_directory: &Path, args: [&str; N]) {
    let status = Command::new(uv_binary)
        .args(args)
        .current_dir(working_directory)
        .status()
        .expect("failed to start uv while building yt-dlp sidecar");

    assert!(
        status.success(),
        "uv failed while building yt-dlp sidecar with status {status}"
    );
}

fn executable_name(name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("{name}.exe")
    }

    #[cfg(not(target_os = "windows"))]
    {
        name.to_string()
    }
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
        vec![YT_DLP_SIDECAR_NAME]
    }
}

fn stage_deno_sidecar() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let expected_sidecar = manifest_dir
        .join("binaries")
        .join(sidecar_file_name_for_host(DENO_SIDECAR_NAME));

    if expected_sidecar.is_file() {
        return;
    }

    let target = build_target_triple();
    let archive_name = deno_archive_name(&target);
    let archive_url = format!(
        "https://github.com/denoland/deno/releases/download/v{DENO_VERSION}/{archive_name}"
    );
    let checksum_url = format!("{archive_url}.sha256sum");
    let archive = download(&archive_url);
    let checksum = download_text(&checksum_url);

    verify_sha256(&archive, &checksum, &archive_name);
    extract_deno_binary(&archive, &expected_sidecar);
}

fn deno_archive_name(target: &str) -> String {
    match target {
        "aarch64-apple-darwin"
        | "aarch64-unknown-linux-gnu"
        | "aarch64-pc-windows-msvc"
        | "x86_64-apple-darwin"
        | "x86_64-pc-windows-msvc"
        | "x86_64-unknown-linux-gnu" => format!("{DENO_SIDECAR_NAME}-{target}.zip"),
        unsupported => panic!("Deno release asset is not configured for target {unsupported}"),
    }
}

fn download(url: &str) -> Vec<u8> {
    let response = reqwest::blocking::get(url)
        .unwrap_or_else(|error| panic!("failed to download {url}: {error}"))
        .error_for_status()
        .unwrap_or_else(|error| panic!("failed to download {url}: {error}"));

    response
        .bytes()
        .unwrap_or_else(|error| panic!("failed to read response body from {url}: {error}"))
        .to_vec()
}

fn download_text(url: &str) -> String {
    String::from_utf8(download(url))
        .unwrap_or_else(|error| panic!("downloaded checksum from {url} is not UTF-8: {error}"))
}

fn verify_sha256(bytes: &[u8], checksum: &str, archive_name: &str) {
    let expected = checksum
        .split_whitespace()
        .find(|part| {
            part.len() == 64 && part.chars().all(|character| character.is_ascii_hexdigit())
        })
        .unwrap_or_else(|| panic!("checksum for {archive_name} did not contain a SHA-256 digest"))
        .to_ascii_lowercase();
    let actual = format!("{:x}", Sha256::digest(bytes));

    assert_eq!(
        expected, actual,
        "SHA-256 mismatch for downloaded Deno archive {archive_name}"
    );
}

fn extract_deno_binary(archive: &[u8], expected_sidecar: &Path) {
    let mut archive = ZipArchive::new(Cursor::new(archive))
        .unwrap_or_else(|error| panic!("failed to open downloaded Deno zip archive: {error}"));
    let mut deno = archive
        .by_name(executable_name(DENO_SIDECAR_NAME).as_str())
        .unwrap_or_else(|error| {
            panic!("downloaded Deno zip archive did not contain deno: {error}")
        });

    let parent = expected_sidecar
        .parent()
        .expect("Deno sidecar path should have a parent directory");
    fs::create_dir_all(parent).expect("failed to create Deno sidecar output directory");

    let mut output = fs::File::create(expected_sidecar).unwrap_or_else(|error| {
        panic!(
            "failed to create Deno sidecar at {}: {error}",
            expected_sidecar.display()
        );
    });

    std::io::copy(&mut deno, &mut output).unwrap_or_else(|error| {
        panic!(
            "failed to extract Deno sidecar to {}: {error}",
            expected_sidecar.display()
        );
    });

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(expected_sidecar, fs::Permissions::from_mode(0o755))
            .expect("failed to mark Deno sidecar executable");
    }
}

fn sidecar_file_name_for_host(sidecar_name: &str) -> String {
    let target = build_target_triple();

    #[cfg(target_os = "windows")]
    {
        format!("{sidecar_name}-{target}.exe")
    }

    #[cfg(not(target_os = "windows"))]
    {
        format!("{sidecar_name}-{target}")
    }
}

fn build_target_triple() -> String {
    env::var("TARGET").unwrap_or_else(|_| host_target_triple().to_string())
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

#[cfg(all(target_os = "windows", target_env = "msvc"))]
fn add_manifest() {
    static WINDOWS_MANIFEST_FILE: &str = "windows-app-manifest.xml";

    let manifest = env::current_dir()
        .expect("failed to resolve current directory")
        .join(WINDOWS_MANIFEST_FILE);

    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
        manifest
            .to_str()
            .expect("manifest path should be valid Unicode")
    );
    println!("cargo:rustc-link-arg=/WX");
}
