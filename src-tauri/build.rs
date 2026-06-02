use std::{
    env, fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;
use zip::ZipArchive;

#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

const DENO_VERSION: &str = "2.8.0";
const PYTHON_VERSION: &str = "3.12";
const UV_RELEASE_BASE_URL: &str = "https://github.com/astral-sh/uv/releases/latest/download";
const DENO_SIDECAR_NAME: &str = "deno";
const YT_DLP_SIDECAR_NAME: &str = "yt-dlp";
const YT_SUB_CONVERTER_SIDECAR_NAME: &str = "ytsubconverter";

#[derive(Clone, Copy)]
enum UvArchiveFormat {
    TarGz,
    Zip,
}

fn main() {
    configure_build_tracking();
    stage_yt_dlp_sidecar();
    stage_yt_sub_converter_sidecar();
    stage_deno_sidecar();
    prepare_pot_provider_resource();
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

#[cfg(windows)]
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
    println!("cargo:rerun-if-changed=../yt-dlp/pyproject.toml");
    println!("cargo:rerun-if-changed=../yt-dlp/yt_dlp/version.py");
    println!("cargo:rerun-if-changed=../yt-dlp/bundle/pyinstaller.py");
    println!("cargo:rerun-if-changed=../YTSubConverter/YTSubConverter.Shared");
    println!("cargo:rerun-if-changed=../YTSubConverter/YTSubConverter.UI.Linux");
    println!("cargo:rerun-if-changed=../YTSubConverter/YTSubConverter.UI.Win");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider/server/package.json");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider/server/deno.lock");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider/server/src/main.ts");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider/server/src/session_manager.ts");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider/server/src/utils.ts");
    println!("cargo:rerun-if-changed=../bgutil-ytdlp-pot-provider/plugin/yt_dlp_plugins");
}

fn stage_yt_dlp_sidecar() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let uv_binary = install_uv(&manifest_dir);
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

fn install_uv(manifest_dir: &Path) -> PathBuf {
    let install_root = manifest_dir.join("binaries").join("build");
    let binary = install_root.join(executable_name("uv"));

    if binary.is_file() {
        return binary;
    }

    let host = host_target_triple();
    let (archive_name, archive_format) = uv_archive(host);
    let archive_url = format!("{UV_RELEASE_BASE_URL}/{archive_name}");
    let checksum_url = format!("{archive_url}.sha256");
    let archive = download(&archive_url);
    let checksum = download_text(&checksum_url);

    verify_sha256(&archive, &checksum, &archive_name);
    extract_uv_binary(&archive, archive_format, &binary);

    binary
}

fn uv_archive(target: &str) -> (String, UvArchiveFormat) {
    match target {
        "aarch64-apple-darwin"
        | "aarch64-unknown-linux-gnu"
        | "x86_64-apple-darwin"
        | "x86_64-unknown-linux-gnu" => (format!("uv-{target}.tar.gz"), UvArchiveFormat::TarGz),
        "aarch64-pc-windows-msvc" | "x86_64-pc-windows-msvc" => {
            (format!("uv-{target}.zip"), UvArchiveFormat::Zip)
        }
        unsupported => panic!("uv release asset is not configured for target {unsupported}"),
    }
}

fn extract_uv_binary(archive: &[u8], archive_format: UvArchiveFormat, binary: &Path) {
    match archive_format {
        UvArchiveFormat::TarGz => extract_uv_binary_from_tar_gz(archive, binary),
        UvArchiveFormat::Zip => extract_uv_binary_from_zip(archive, binary),
    }
}

fn extract_uv_binary_from_zip(archive: &[u8], binary: &Path) {
    let mut archive = ZipArchive::new(Cursor::new(archive))
        .unwrap_or_else(|error| panic!("failed to open downloaded uv zip archive: {error}"));

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .unwrap_or_else(|error| panic!("failed to read uv zip archive entry: {error}"));
        let Some(file_name) = file.enclosed_name().and_then(|path| {
            path.file_name()
                .and_then(|file_name| file_name.to_str())
                .map(str::to_owned)
        }) else {
            continue;
        };

        if file_name == executable_name("uv") {
            write_executable(&mut file, binary, "uv");
            return;
        }
    }

    panic!("downloaded uv zip archive did not contain uv");
}

fn extract_uv_binary_from_tar_gz(archive: &[u8], binary: &Path) {
    let decoder = GzDecoder::new(Cursor::new(archive));
    let mut archive = Archive::new(decoder);
    let entries = archive
        .entries()
        .unwrap_or_else(|error| panic!("failed to read downloaded uv tar archive: {error}"));

    for entry in entries {
        let mut entry =
            entry.unwrap_or_else(|error| panic!("failed to read uv tar archive entry: {error}"));
        let path = entry
            .path()
            .unwrap_or_else(|error| panic!("failed to read uv tar archive entry path: {error}"));
        let Some(file_name) = path.file_name().and_then(|file_name| file_name.to_str()) else {
            continue;
        };

        if file_name == executable_name("uv") {
            write_executable(&mut entry, binary, "uv");
            return;
        }
    }

    panic!("downloaded uv tar archive did not contain uv");
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
    let dev_dependency_package = node_modules
        .join(".deno")
        .join("@typescript-eslint+eslint-plugin@8.54.0");
    let hoisted_dev_dependency_package = node_modules.join("@typescript-eslint");

    if !commander_package.is_dir()
        || !express_package.is_dir()
        || is_symlink(&commander_package)
        || dev_dependency_package.exists()
        || hoisted_dev_dependency_package.exists()
    {
        if node_modules.exists() {
            remove_directory_tree(&node_modules).unwrap_or_else(|error| {
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
        "README.md",
        "tsconfig.json",
    ] {
        copy_file_if_exists(
            &source_server.join(file_name),
            &resource_server.join(file_name),
        );
    }

    copy_runtime_package_json(&source_server.join("package.json"), resource_server);
    remove_file_if_exists(&resource_server.join("package-lock.json"));
    copy_directory(&source_server.join("src"), &resource_server.join("src"));
    copy_directory(&source_server.join("types"), &resource_server.join("types"));
}

fn copy_runtime_package_json(source: &Path, resource_server: &Path) {
    let package_json = fs::read_to_string(source).unwrap_or_else(|error| {
        panic!(
            "failed to read POT provider package.json at {}: {error}",
            source.display()
        )
    });
    let mut package_json: serde_json::Value =
        serde_json::from_str(&package_json).unwrap_or_else(|error| {
            panic!(
                "failed to parse POT provider package.json at {}: {error}",
                source.display()
            )
        });

    if let Some(package) = package_json.as_object_mut() {
        package.remove("devDependencies");
        package.remove("scripts");
    }

    let destination = resource_server.join("package.json");
    fs::write(
        &destination,
        serde_json::to_string_pretty(&package_json)
            .expect("failed to serialize runtime POT provider package.json"),
    )
    .unwrap_or_else(|error| {
        panic!(
            "failed to write runtime POT provider package.json at {}: {error}",
            destination.display()
        )
    });
}

fn install_pot_provider_dependencies(manifest_dir: &Path, resource_server: &Path) {
    let deno_binary = manifest_dir
        .join("binaries")
        .join(sidecar_file_name_for_host(DENO_SIDECAR_NAME));
    let status = Command::new(&deno_binary)
        .args([
            "install",
            "--node-modules-dir=manual",
            "--node-modules-linker=hoisted",
            "--allow-scripts=npm:canvas",
            "--frozen=false",
            "--prod",
            "--skip-types",
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

fn remove_directory_tree(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| remove_error(path, &error))?;

    if metadata.file_type().is_symlink() {
        if is_directory_symlink(&metadata) {
            remove_dir_entry(path)
        } else {
            remove_file_entry(path)
        }
    } else if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| remove_error(path, &error))? {
            let entry = entry.map_err(|error| remove_error(path, &error))?;
            remove_directory_tree(&entry.path())?;
        }
        remove_dir_entry(path)
    } else {
        remove_file_entry(path)
    }
}

fn is_directory_symlink(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
        metadata.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0
    }

    #[cfg(not(windows))]
    {
        metadata.is_dir()
    }
}

fn remove_file_entry(path: &Path) -> std::io::Result<()> {
    fs::remove_file(path).or_else(|error| {
        make_writable(path)?;
        fs::remove_file(path).map_err(|retry_error| {
            let combined_error =
                std::io::Error::new(retry_error.kind(), format!("{error}; retry: {retry_error}"));
            remove_error(path, &combined_error)
        })
    })
}

fn remove_dir_entry(path: &Path) -> std::io::Result<()> {
    fs::remove_dir(path).or_else(|error| {
        make_writable(path)?;
        fs::remove_dir(path).map_err(|retry_error| {
            let combined_error =
                std::io::Error::new(retry_error.kind(), format!("{error}; retry: {retry_error}"));
            remove_error(path, &combined_error)
        })
    })
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
fn make_writable(path: &Path) -> std::io::Result<()> {
    let mut permissions = fs::symlink_metadata(path)?.permissions();
    if permissions.readonly() {
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(unix)]
fn make_writable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::symlink_metadata(path)?.permissions();
    let mode = permissions.mode();
    if mode & 0o200 == 0 {
        permissions.set_mode(mode | 0o200);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(not(any(windows, unix)))]
fn make_writable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn remove_error(path: &Path, error: &std::io::Error) -> std::io::Error {
    std::io::Error::new(error.kind(), format!("{}: {error}", path.display()))
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

fn remove_file_if_exists(path: &Path) {
    if path.is_file() {
        fs::remove_file(path)
            .unwrap_or_else(|error| panic!("failed to remove {}: {error}", path.display()));
    }
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
        "SHA-256 mismatch for downloaded archive {archive_name}"
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
