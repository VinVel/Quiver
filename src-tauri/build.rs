use std::{
    env, fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};

use sequoia_openpgp::{
    Cert, KeyHandle, Result as OpenPgpResult,
    parse::{
        Parse,
        stream::{DetachedVerifierBuilder, MessageLayer, MessageStructure, VerificationHelper},
    },
    policy::StandardPolicy,
};
use sha2::{Digest, Sha256, Sha512};
use zip::ZipArchive;

const DENO_VERSION: &str = "2.8.0";
const YT_DLP_RELEASE_BASE_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const YT_DLP_PUBLIC_KEY_URL: &str =
    "https://raw.githubusercontent.com/yt-dlp/yt-dlp/master/public.key";
const YT_DLP_SHA512_SUMS: &str = "SHA2-512SUMS";
const YT_DLP_SHA512_SIGNATURE: &str = "SHA2-512SUMS.sig";
const BGUTIL_POT_PROVIDER_SIDECAR_NAME: &str = "bgutil-pot";
const DENO_SIDECAR_NAME: &str = "deno";
const YT_DLP_SIDECAR_NAME: &str = "yt-dlp";
const YT_SUB_CONVERTER_SIDECAR_NAME: &str = "ytsubconverter";

fn main() {
    configure_build_tracking();
    stage_yt_dlp_sidecar();
    stage_yt_sub_converter_sidecar();
    stage_deno_sidecar();
    stage_bgutil_pot_provider_sidecar();
    prepare_pot_provider_plugin_resource();
    stop_running_copied_sidecars();

    let mut attributes = tauri_build::Attributes::new();
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        add_manifest();
    }

    tauri_build::try_build(attributes).expect("failed to build Tauri application metadata");
}

#[cfg(windows)]
fn stop_running_copied_sidecars() {
    let Some(target_profile_dir) = target_profile_dir() else {
        warn("Could not resolve Cargo target profile directory; skipping copied sidecar cleanup.");
        return;
    };

    let sidecars = [
        target_profile_dir.join(executable_name(BGUTIL_POT_PROVIDER_SIDECAR_NAME)),
        target_profile_dir.join(executable_name(DENO_SIDECAR_NAME)),
        target_profile_dir.join(executable_name(YT_DLP_SIDECAR_NAME)),
        target_profile_dir.join(executable_name(YT_SUB_CONVERTER_SIDECAR_NAME)),
    ];
    let running_sidecars = sidecars
        .iter()
        .filter(|path| path.is_file())
        .map(|path| path_to_str(path))
        .collect::<Vec<_>>();

    if running_sidecars.is_empty() {
        return;
    }

    let sidecar_paths = running_sidecars.join("|");
    let script = r#"
$paths = @($env:QUIVER_COPIED_SIDECARS -split '\|' | ForEach-Object {
    [System.IO.Path]::GetFullPath($_).ToLowerInvariant()
})
Get-CimInstance Win32_Process |
    Where-Object {
        $_.ExecutablePath -and
        $paths -contains ([System.IO.Path]::GetFullPath($_.ExecutablePath).ToLowerInvariant())
    } |
    ForEach-Object {
        Write-Host "Stopping stale copied sidecar process $($_.ProcessId): $($_.ExecutablePath)"
        Stop-Process -Id $_.ProcessId -Force
    }
"#;
    let status = Command::new("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .env("QUIVER_COPIED_SIDECARS", sidecar_paths)
        .status()
        .expect("failed to start PowerShell while checking copied sidecar processes");

    assert!(
        status.success(),
        "PowerShell failed while checking copied sidecar processes with status {status}"
    );
}

#[cfg(not(windows))]
fn stop_running_copied_sidecars() {}

fn target_profile_dir() -> Option<PathBuf> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR")?);
    out_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

fn configure_build_tracking() {
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-changed=../YTSubConverter/YTSubConverter.Shared");
    println!("cargo:rerun-if-changed=../YTSubConverter/YTSubConverter.UI.Linux");
    println!("cargo:rerun-if-changed=../YTSubConverter/YTSubConverter.UI.Win");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider-rs/Cargo.toml");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider-rs/src");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider-rs/plugin/yt_dlp_plugins");
}

fn stage_yt_dlp_sidecar() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let expected_sidecar = manifest_dir
        .join("binaries")
        .join(sidecar_file_name_for_host(YT_DLP_SIDECAR_NAME));

    if expected_sidecar.is_file() {
        return;
    }

    let asset_name = yt_dlp_asset_name(&build_target_triple());
    let checksums = download_text(&format!("{YT_DLP_RELEASE_BASE_URL}/{YT_DLP_SHA512_SUMS}"));
    let signature = download(&format!(
        "{YT_DLP_RELEASE_BASE_URL}/{YT_DLP_SHA512_SIGNATURE}"
    ));
    let public_key = download(YT_DLP_PUBLIC_KEY_URL);
    verify_gpg_signature(
        checksums.as_bytes(),
        &signature,
        &public_key,
        YT_DLP_SHA512_SUMS,
    );

    let binary = download(&format!("{YT_DLP_RELEASE_BASE_URL}/{asset_name}"));
    verify_sha512(&binary, &checksums, asset_name);
    write_executable(
        &mut Cursor::new(binary),
        &expected_sidecar,
        YT_DLP_SIDECAR_NAME,
    );
}

fn stage_yt_sub_converter_sidecar() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let expected_sidecar = manifest_dir
        .join("binaries")
        .join(sidecar_file_name_for_host(YT_SUB_CONVERTER_SIDECAR_NAME));

    if expected_sidecar.is_file() {
        return;
    }

    let Some(workspace_root) = manifest_dir.parent() else {
        warn("Could not resolve workspace root; skipping YTSubConverter sidecar build.");
        return;
    };
    let yt_sub_converter_dir = workspace_root.join("YTSubConverter");

    if !yt_sub_converter_dir.join("YTSubConverter.sln").is_file() {
        warn(format!(
            "YTSubConverter submodule is missing at {}; skipping sidecar build.",
            yt_sub_converter_dir.display()
        ));
        return;
    }

    let Some(built_binary) = build_yt_sub_converter(&yt_sub_converter_dir, &manifest_dir) else {
        return;
    };

    fs::create_dir_all(
        expected_sidecar
            .parent()
            .expect("sidecar path should have a parent directory"),
    )
    .expect("failed to create YTSubConverter sidecar output directory");
    fs::copy(&built_binary, &expected_sidecar).unwrap_or_else(|error| {
        panic!(
            "failed to copy {} to {}: {error}",
            built_binary.display(),
            expected_sidecar.display()
        );
    });

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(&expected_sidecar, fs::Permissions::from_mode(0o755))
            .expect("failed to mark YTSubConverter sidecar executable");
    }
}

fn build_yt_sub_converter(yt_sub_converter_dir: &Path, _manifest_dir: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        build_yt_sub_converter_windows(yt_sub_converter_dir)
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        Some(build_yt_sub_converter_self_contained(
            yt_sub_converter_dir,
            _manifest_dir,
        ))
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        panic!(
            "YTSubConverter sidecar build is not configured for target {}",
            build_target_triple()
        );
    }
}

#[cfg(target_os = "windows")]
fn build_yt_sub_converter_windows(yt_sub_converter_dir: &Path) -> Option<PathBuf> {
    let project = yt_sub_converter_dir
        .join("YTSubConverter.UI.Win")
        .join("YTSubConverter.UI.Win.csproj");
    let Some(msbuild) = find_msbuild() else {
        warn(
            "Visual Studio MSBuild was not found; skipping YTSubConverter Windows sidecar build. \
             Install Visual Studio Build Tools with MSBuild to generate the net48 sidecar.",
        );
        return None;
    };

    let status = Command::new(&msbuild)
        .arg(&project)
        .args([
            "/restore",
            "/m",
            "/p:Configuration=Release",
            "/p:Platform=AnyCPU",
        ])
        .current_dir(yt_sub_converter_dir)
        .status()
        .unwrap_or_else(|error| {
            panic!(
                "failed to start MSBuild at {} while building YTSubConverter sidecar: {error}",
                msbuild.display()
            )
        });

    assert!(
        status.success(),
        "MSBuild failed while building YTSubConverter sidecar with status {status}"
    );

    let built_binary = yt_sub_converter_dir
        .join("YTSubConverter.UI.Win")
        .join("bin")
        .join("Release")
        .join("YTSubConverter.exe");

    assert!(
        built_binary.is_file(),
        "YTSubConverter Windows build finished but no binary was found at {}",
        built_binary.display()
    );

    Some(built_binary)
}

#[cfg(target_os = "windows")]
fn find_msbuild() -> Option<PathBuf> {
    if let Some(msbuild) = find_msbuild_with_vswhere() {
        return Some(msbuild);
    }

    let program_files_x86 = PathBuf::from(env::var_os("ProgramFiles(x86)")?);
    [
        program_files_x86
            .join("Microsoft Visual Studio")
            .join("2022")
            .join("BuildTools")
            .join("MSBuild")
            .join("Current")
            .join("Bin")
            .join("amd64")
            .join("MSBuild.exe"),
        program_files_x86
            .join("Microsoft Visual Studio")
            .join("2022")
            .join("BuildTools")
            .join("MSBuild")
            .join("Current")
            .join("Bin")
            .join("MSBuild.exe"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

#[cfg(target_os = "windows")]
fn find_msbuild_with_vswhere() -> Option<PathBuf> {
    let program_files_x86 = env::var_os("ProgramFiles(x86)")?;
    let vswhere = PathBuf::from(program_files_x86)
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");

    if !vswhere.is_file() {
        return None;
    }

    let output = Command::new(vswhere)
        .args([
            "-latest",
            "-requires",
            "Microsoft.Component.MSBuild",
            "-find",
            "MSBuild\\**\\Bin\\MSBuild.exe",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn build_yt_sub_converter_self_contained(
    yt_sub_converter_dir: &Path,
    manifest_dir: &Path,
) -> PathBuf {
    let project = yt_sub_converter_dir
        .join("YTSubConverter.UI.Linux")
        .join("YTSubConverter.UI.Linux.csproj");
    let target = build_target_triple();
    let runtime_identifier = yt_sub_converter_runtime_identifier(&target);
    let publish_dir = manifest_dir
        .join("target")
        .join("ytsubconverter")
        .join(&target);

    run_dotnet(
        yt_sub_converter_dir,
        [
            "publish",
            path_to_str(&project),
            "--configuration",
            "Release",
            "--runtime",
            runtime_identifier,
            "--self-contained",
            "true",
            "--output",
            path_to_str(&publish_dir),
            "-p:PublishSingleFile=true",
            "-p:IncludeNativeLibrariesForSelfExtract=true",
            "-p:DebugType=None",
            "-p:DebugSymbols=false",
        ],
    );

    let built_binary = publish_dir.join(executable_name(YT_SUB_CONVERTER_SIDECAR_NAME));
    assert!(
        built_binary.is_file(),
        "YTSubConverter publish finished but no binary was found at {}",
        built_binary.display()
    );

    built_binary
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn yt_sub_converter_runtime_identifier(target: &str) -> &'static str {
    match target {
        "aarch64-apple-darwin" => "osx-arm64",
        "aarch64-unknown-linux-gnu" => "linux-arm64",
        "x86_64-apple-darwin" => "osx-x64",
        "x86_64-unknown-linux-gnu" => "linux-x64",
        unsupported => panic!("YTSubConverter publish is not configured for target {unsupported}"),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run_dotnet<const N: usize>(working_directory: &Path, args: [&str; N]) {
    let status = Command::new("dotnet")
        .args(args)
        .current_dir(working_directory)
        .status()
        .expect("failed to start dotnet while building YTSubConverter sidecar");

    assert!(
        status.success(),
        "dotnet failed while building YTSubConverter sidecar with status {status}"
    );
}

fn path_to_str(path: &Path) -> &str {
    path.to_str().unwrap_or_else(|| {
        panic!(
            "path contains non-Unicode characters and cannot be passed to a build tool: {}",
            path.display()
        )
    })
}

fn yt_dlp_asset_name(target: &str) -> &'static str {
    match target {
        "aarch64-apple-darwin" | "x86_64-apple-darwin" => "yt-dlp_macos",
        "aarch64-pc-windows-msvc" => "yt-dlp_arm64.exe",
        "x86_64-pc-windows-msvc" => "yt-dlp.exe",
        "aarch64-unknown-linux-gnu" => "yt-dlp_linux_aarch64",
        "x86_64-unknown-linux-gnu" => "yt-dlp_linux",
        unsupported => panic!("yt-dlp release asset is not configured for target {unsupported}"),
    }
}

fn write_executable(reader: &mut impl std::io::Read, destination: &Path, name: &str) {
    let parent = destination
        .parent()
        .unwrap_or_else(|| panic!("{name} binary path should have a parent directory"));
    fs::create_dir_all(parent).unwrap_or_else(|error| {
        panic!(
            "failed to create {name} binary output directory at {}: {error}",
            parent.display()
        );
    });

    let mut output = fs::File::create(destination).unwrap_or_else(|error| {
        panic!(
            "failed to create {name} binary at {}: {error}",
            destination.display()
        );
    });

    std::io::copy(reader, &mut output).unwrap_or_else(|error| {
        panic!(
            "failed to extract {name} binary to {}: {error}",
            destination.display()
        );
    });

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(destination, fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("failed to mark {name} binary executable: {error}"));
    }
}

fn stage_bgutil_pot_provider_sidecar() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let Some(workspace_root) = manifest_dir.parent() else {
        warn("Could not resolve workspace root; skipping bgutil POT provider sidecar build.");
        return;
    };
    let provider_dir = workspace_root.join("bgutil-ytdlp-pot-provider-rs");

    if !provider_dir.join("Cargo.toml").is_file() {
        warn(format!(
            "bgutil-ytdlp-pot-provider-rs submodule is missing at {}; skipping POT provider sidecar build.",
            provider_dir.display()
        ));
        return;
    }

    let target = build_target_triple();
    let expected_sidecar = manifest_dir
        .join("binaries")
        .join(sidecar_file_name_for_host(BGUTIL_POT_PROVIDER_SIDECAR_NAME));
    let built_binary = provider_dir
        .join("target")
        .join(&target)
        .join("release")
        .join(executable_name(BGUTIL_POT_PROVIDER_SIDECAR_NAME));

    if !built_binary.is_file()
        || is_source_newer_than(&provider_dir.join("Cargo.toml"), &built_binary)
        || is_source_newer_than(&provider_dir.join("src"), &built_binary)
    {
        let status = Command::new("cargo")
            .args([
                "build",
                "--release",
                "--bin",
                BGUTIL_POT_PROVIDER_SIDECAR_NAME,
                "--target",
                &target,
            ])
            .current_dir(&provider_dir)
            .status()
            .unwrap_or_else(|error| {
                panic!(
                    "failed to start cargo while building bgutil POT provider sidecar from {}: {error}",
                    provider_dir.display()
                )
            });

        assert!(
            status.success(),
            "cargo failed while building bgutil POT provider sidecar with status {status}"
        );
    }

    fs::create_dir_all(
        expected_sidecar
            .parent()
            .expect("sidecar path should have a parent directory"),
    )
    .expect("failed to create bgutil POT provider sidecar output directory");
    fs::copy(&built_binary, &expected_sidecar).unwrap_or_else(|error| {
        panic!(
            "failed to copy {} to {}: {error}",
            built_binary.display(),
            expected_sidecar.display()
        );
    });

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(&expected_sidecar, fs::Permissions::from_mode(0o755))
            .expect("failed to mark bgutil POT provider sidecar executable");
    }
}

fn prepare_pot_provider_plugin_resource() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let Some(workspace_root) = manifest_dir.parent() else {
        warn("Could not resolve workspace root; skipping POT provider plugin preparation.");
        return;
    };
    let source_plugin = workspace_root
        .join("bgutil-ytdlp-pot-provider-rs")
        .join("plugin");

    if !source_plugin.join("yt_dlp_plugins").is_dir() {
        warn(format!(
            "bgutil-ytdlp-pot-provider-rs plugin is missing at {}; skipping yt-dlp plugin resource preparation.",
            source_plugin.display()
        ));
        return;
    }

    let plugin_resource = manifest_dir
        .join("resources")
        .join("yt-dlp-plugins")
        .join("bgutil");

    remove_generated_resource_directory(&manifest_dir.join("resources"), &plugin_resource);
    if let Some(target_profile_dir) = target_profile_dir() {
        let copied_plugin_resource = target_profile_dir
            .join("resources")
            .join("yt-dlp-plugins")
            .join("bgutil");
        remove_generated_resource_directory(
            &target_profile_dir.join("resources"),
            &copied_plugin_resource,
        );
    }

    copy_file_if_exists(
        &source_plugin.join("pyproject.toml"),
        &plugin_resource.join("pyproject.toml"),
    );
    copy_directory(
        &source_plugin.join("yt_dlp_plugins"),
        &plugin_resource.join("yt_dlp_plugins"),
    );
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

fn remove_generated_resource_directory(resources_dir: &Path, path: &Path) {
    if !path.exists() {
        return;
    }

    assert!(
        path.starts_with(resources_dir),
        "refusing to remove generated directory outside resources: {}",
        path.display()
    );

    fs::remove_dir_all(path)
        .unwrap_or_else(|error| panic!("failed to remove {}: {error}", path.display()));
}

fn is_source_newer_than(source: &Path, destination: &Path) -> bool {
    let Ok(destination_modified) = destination
        .metadata()
        .and_then(|metadata| metadata.modified())
    else {
        return true;
    };

    source_modified_after(source, destination_modified)
}

fn source_modified_after(source: &Path, destination_modified: std::time::SystemTime) -> bool {
    if source.is_dir() {
        return fs::read_dir(source).is_ok_and(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|entry| source_modified_after(&entry.path(), destination_modified))
        });
    }

    source
        .metadata()
        .and_then(|metadata| metadata.modified())
        .is_ok_and(|modified| modified > destination_modified)
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
        "SHA-256 mismatch for downloaded archive {archive_name}"
    );
}

fn verify_sha512(bytes: &[u8], checksums: &str, asset_name: &str) {
    let expected = checksums
        .lines()
        .filter_map(parse_gnu_checksum_line)
        .find_map(|(hash, file_name)| (file_name == asset_name).then_some(hash))
        .unwrap_or_else(|| panic!("signed SHA-512 sums did not contain an entry for {asset_name}"))
        .to_ascii_lowercase();
    let actual = format!("{:x}", Sha512::digest(bytes));

    assert_eq!(
        expected, actual,
        "SHA-512 mismatch for downloaded yt-dlp asset {asset_name}"
    );
}

fn parse_gnu_checksum_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let (hash, file_name) = line.split_once(char::is_whitespace)?;
    let file_name = file_name.trim_start();
    let file_name = file_name.strip_prefix('*').unwrap_or(file_name);

    (hash.len() == 128 && hash.chars().all(|character| character.is_ascii_hexdigit()))
        .then_some((hash, file_name))
}

fn verify_gpg_signature(data: &[u8], signature: &[u8], public_key: &[u8], signed_file_name: &str) {
    let cert = Cert::from_bytes(public_key)
        .unwrap_or_else(|error| panic!("failed to parse yt-dlp public GPG key: {error}"));
    let policy = StandardPolicy::new();
    let helper = GpgVerificationHelper { cert };
    let mut verifier = DetachedVerifierBuilder::from_bytes(signature)
        .unwrap_or_else(|error| {
            panic!("failed to parse detached GPG signature for {signed_file_name}: {error}")
        })
        .with_policy(&policy, None, helper)
        .unwrap_or_else(|error| {
            panic!("failed to create GPG verifier for {signed_file_name}: {error}")
        });

    verifier
        .verify_bytes(data)
        .unwrap_or_else(|error| panic!("GPG verification failed for {signed_file_name}: {error}"));
}

struct GpgVerificationHelper {
    cert: Cert,
}

impl VerificationHelper for GpgVerificationHelper {
    fn get_certs(&mut self, _ids: &[KeyHandle]) -> OpenPgpResult<Vec<Cert>> {
        Ok(vec![self.cert.clone()])
    }

    fn check(&mut self, structure: MessageStructure<'_>) -> OpenPgpResult<()> {
        for layer in structure {
            if let MessageLayer::SignatureGroup { results } = layer
                && results.iter().any(Result::is_ok)
            {
                return Ok(());
            }
        }

        Err(anyhow::anyhow!(
            "detached signature was not made by the yt-dlp release key"
        ))
    }
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
