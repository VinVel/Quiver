use std::{
    env, fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::json;
use sha2::{Digest, Sha256};
use tar::Archive;
use xz2::read::XzDecoder;
use zip::ZipArchive;

use utils::{
    build_target_triple, copy_directory, copy_file_if_exists, download, download_text,
    executable_name, host_target_triple, mark_sidecar_executable, out_dir, path_to_str,
    sidecar_dir, sidecar_file_name_for_host, warn,
};

const SIDECARS: &[&str] = &[
    "yt-dlp",
    "ytsubconverter",
    "deno",
    "bgutil-pot",
    "ffmpeg",
    "ffprobe",
];

fn main() {
    configure_build_tracking();
    fs::create_dir_all(sidecar_dir()).expect("failed to create OUT_DIR sidecar directory");
    yt_dlp::stage();
    yt_sub_converter::stage();
    deno::stage();
    bgutil_pot_provider::stage();
    ffmpeg::stage();
    write_sidecar_manifest();
}

fn configure_build_tracking() {
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-changed=src/build.rs");
    println!("cargo:rerun-if-changed=.python-version");
    for source in [
        "YTSubConverter/YTSubConverter.Shared",
        "YTSubConverter/YTSubConverter.UI.Linux",
        "YTSubConverter/YTSubConverter.UI.Win",
        "bgutil-ytdlp-pot-provider/server/package.json",
        "bgutil-ytdlp-pot-provider/server/package-lock.json",
        "bgutil-ytdlp-pot-provider/server/src",
        "bgutil-ytdlp-pot-provider/plugin/yt_dlp_plugins",
        "yt-dlp/pyproject.toml",
        "yt-dlp/THIRD_PARTY_LICENSES.txt",
        "yt-dlp/bundle",
        "yt-dlp/devscripts",
        "yt-dlp/yt_dlp",
    ] {
        println!("cargo:rerun-if-changed={source}");
    }
}

fn write_sidecar_manifest() {
    let release = env::var("PROFILE").as_deref() != Ok("debug");
    let mut entries = Vec::new();
    for &name in SIDECARS {
        if cfg!(target_os = "macos") && matches!(name, "ffmpeg" | "ffprobe") {
            entries.push(json!({ "bin": format!("quiver-{name}"), "source": "cargo-bin" }));
            continue;
        }
        let path = sidecar_dir().join(sidecar_file_name_for_host(name));
        if !path.is_file() {
            assert!(
                !release,
                "required release sidecar is missing: {}",
                path.display()
            );
            warn(format!(
                "sidecar unavailable in this development build: {}",
                path.display()
            ));
            continue;
        }
        let digest = format!(
            "{:x}",
            Sha256::digest(
                fs::read(&path)
                    .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
            )
        );
        entries.push(json!({
            "bin": format!("quiver-{name}"),
            "source": "staged",
            "path": path,
            "sha256": digest,
        }));
    }
    let manifest = out_dir().join("sidecar-manifest.json");
    let contents = json!({ "target": build_target_triple(), "sidecars": entries });
    fs::write(
        &manifest,
        serde_json::to_vec_pretty(&contents).expect("serialize sidecar manifest"),
    )
    .expect("write sidecar manifest");
    println!(
        "cargo:rustc-env=QUIVER_SIDECAR_MANIFEST={}",
        manifest.display()
    );
}

mod utils {
    use super::{Path, PathBuf, env, fs};

    pub fn out_dir() -> PathBuf {
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"))
    }

    pub fn sidecar_dir() -> PathBuf {
        out_dir().join("sidecars")
    }

    pub fn path_to_str(path: &Path) -> &str {
        path.to_str().unwrap_or_else(|| {
            panic!(
                "path contains non-Unicode characters and cannot be passed to a build tool: {}",
                path.display()
            )
        })
    }

    pub fn copy_directory(source: &Path, destination: &Path) {
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

    pub fn copy_file_if_exists(source: &Path, destination: &Path) {
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

    pub fn mark_sidecar_executable(path: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap_or_else(|error| {
                panic!(
                    "failed to mark sidecar executable at {}: {error}",
                    path.display()
                )
            });
        }

        #[cfg(not(unix))]
        let _ = path;
    }

    pub fn executable_name(name: &str) -> String {
        #[cfg(target_os = "windows")]
        {
            format!("{name}.exe")
        }

        #[cfg(not(target_os = "windows"))]
        {
            name.to_string()
        }
    }

    pub fn sidecar_file_name_for_host(sidecar_name: &str) -> String {
        executable_name(&format!("quiver-{sidecar_name}"))
    }

    pub fn build_target_triple() -> String {
        env::var("TARGET").unwrap_or_else(|_| host_target_triple().to_string())
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    pub fn host_target_triple() -> &'static str {
        "x86_64-pc-windows-msvc"
    }

    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    pub fn host_target_triple() -> &'static str {
        "aarch64-pc-windows-msvc"
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    pub fn host_target_triple() -> &'static str {
        "x86_64-apple-darwin"
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    pub fn host_target_triple() -> &'static str {
        "aarch64-apple-darwin"
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    pub fn host_target_triple() -> &'static str {
        "x86_64-unknown-linux-gnu"
    }

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    pub fn host_target_triple() -> &'static str {
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
    pub fn host_target_triple() -> &'static str {
        "unknown"
    }

    pub fn download(url: &str) -> Vec<u8> {
        let response = reqwest::blocking::get(url)
            .unwrap_or_else(|error| panic!("failed to download {url}: {error}"))
            .error_for_status()
            .unwrap_or_else(|error| panic!("failed to download {url}: {error}"));

        response
            .bytes()
            .unwrap_or_else(|error| panic!("failed to read response body from {url}: {error}"))
            .to_vec()
    }

    pub fn download_text(url: &str) -> String {
        String::from_utf8(download(url))
            .unwrap_or_else(|error| panic!("downloaded checksum from {url} is not UTF-8: {error}"))
    }

    pub fn warn(message: impl AsRef<str>) {
        println!("cargo:warning={}", message.as_ref());
    }
}

mod yt_dlp {
    use super::{
        Command, Digest, Output, Path, PathBuf, Sha256, build_target_triple, copy_directory,
        copy_file_if_exists, env, fs, host_target_triple, mark_sidecar_executable, out_dir,
        sidecar_dir, sidecar_file_name_for_host,
    };

    pub const SIDECAR_NAME: &str = "yt-dlp";
    const BUILD_RECIPE: &str = "pyinstaller-default-curl-cffi-v2";
    const OPTIONAL_PYTHON_IMPORTS: &[&str] = &[
        "brotli",
        "certifi",
        "Cryptodome",
        "curl_cffi",
        "mutagen",
        "requests",
        "urllib3",
        "websockets",
        "yt_dlp_ejs",
    ];
    const REQUIRED_DEPENDENCIES: &[&str] = &[
        "brotli",
        "certifi",
        "cryptodome",
        "curl-cffi",
        "mutagen",
        "requests",
        "urllib3",
        "websockets",
        "yt-dlp-ejs",
    ];

    pub fn stage() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.as_path();
        let yt_dlp_source_dir = workspace_root.join("yt-dlp");
        assert!(
            yt_dlp_source_dir.join("pyproject.toml").is_file(),
            "yt-dlp submodule is missing at {}; initialize it before building Quiver",
            yt_dlp_source_dir.display()
        );

        let target = build_target_triple();
        let host = env::var("HOST").unwrap_or_else(|_| host_target_triple().to_string());
        assert_eq!(
            target, host,
            "yt-dlp must be built natively because PyInstaller freezes the host Python interpreter (host: {host}, target: {target})"
        );

        let expected_sidecar = sidecar_dir().join(sidecar_file_name_for_host(SIDECAR_NAME));
        let build_stamp_path = sidecar_dir().join(format!("{SIDECAR_NAME}-{target}.build-stamp"));
        let source_fingerprint = yt_dlp_source_fingerprint(&yt_dlp_source_dir);
        let python_request = yt_dlp_python_request(&target);
        let expected_build_stamp =
            format!("{BUILD_RECIPE}:{python_request}:{target}:{source_fingerprint}\n");

        if expected_sidecar.is_file()
            && fs::read_to_string(&build_stamp_path)
                .is_ok_and(|stamp| stamp == expected_build_stamp)
        {
            mark_sidecar_executable(&expected_sidecar);
            return;
        }

        let out_dir =
            PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR should be set by Cargo"));
        let build_dir = prepare_yt_dlp_build_directory(&yt_dlp_source_dir, &out_dir);
        let python = prepare_yt_dlp_build_environment(&build_dir, &target, &python_request);
        generate_yt_dlp_lazy_extractors(&build_dir, &python);
        let built_binary = build_yt_dlp_executable(&build_dir, &python);
        verify_yt_dlp_executable(&built_binary);

        fs::create_dir_all(
            expected_sidecar
                .parent()
                .expect("yt-dlp sidecar path should have a parent directory"),
        )
        .expect("failed to create yt-dlp sidecar output directory");
        fs::copy(&built_binary, &expected_sidecar).unwrap_or_else(|error| {
            panic!(
                "failed to copy built yt-dlp executable from {} to {}: {error}",
                built_binary.display(),
                expected_sidecar.display()
            )
        });

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&expected_sidecar, fs::Permissions::from_mode(0o755))
                .expect("failed to mark yt-dlp sidecar executable");
        }

        fs::write(&build_stamp_path, expected_build_stamp).unwrap_or_else(|error| {
            panic!(
                "failed to write yt-dlp build stamp at {}: {error}",
                build_stamp_path.display()
            )
        });
    }

    fn yt_dlp_source_fingerprint(source_dir: &Path) -> String {
        let mut hasher = Sha256::new();
        for relative_path in [
            "pyproject.toml",
            "THIRD_PARTY_LICENSES.txt",
            "bundle",
            "devscripts",
            "yt_dlp",
        ] {
            hash_source_path(source_dir, &source_dir.join(relative_path), &mut hasher);
        }

        format!("{:x}", hasher.finalize())
    }

    fn hash_source_path(source_root: &Path, path: &Path, hasher: &mut Sha256) {
        assert!(
            path.exists(),
            "required yt-dlp source path is missing: {}",
            path.display()
        );

        if path.is_dir() {
            let mut entries = fs::read_dir(path)
                .unwrap_or_else(|error| {
                    panic!(
                        "failed to read yt-dlp source directory {}: {error}",
                        path.display()
                    )
                })
                .map(|entry| entry.expect("failed to read yt-dlp source directory entry"))
                .collect::<Vec<_>>();
            entries.sort_by_key(fs::DirEntry::file_name);

            for entry in entries {
                hash_source_path(source_root, &entry.path(), hasher);
            }
            return;
        }

        let relative_path = path.strip_prefix(source_root).unwrap_or_else(|_| {
            panic!(
                "yt-dlp source path {} is outside {}",
                path.display(),
                source_root.display()
            )
        });
        hasher.update(relative_path.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(path).unwrap_or_else(|error| {
            panic!(
                "failed to read yt-dlp source file {}: {error}",
                path.display()
            )
        }));
        hasher.update([0]);
    }

    fn prepare_yt_dlp_build_directory(source_dir: &Path, out_dir: &Path) -> PathBuf {
        let build_dir = out_dir.join("yt-dlp-source");
        if build_dir.exists() {
            fs::remove_dir_all(&build_dir).unwrap_or_else(|error| {
                panic!(
                    "failed to clear yt-dlp build directory at {}: {error}",
                    build_dir.display()
                )
            });
        }

        for directory in ["bundle", "devscripts", "yt_dlp"] {
            copy_directory(&source_dir.join(directory), &build_dir.join(directory));
        }
        for file in ["pyproject.toml", "THIRD_PARTY_LICENSES.txt"] {
            copy_file_if_exists(&source_dir.join(file), &build_dir.join(file));
        }

        build_dir
    }

    fn prepare_yt_dlp_build_environment(
        build_dir: &Path,
        target: &str,
        python_request: &str,
    ) -> PathBuf {
        let virtual_environment = build_dir.join(".venv");
        let mut create_environment = Command::new("uv");
        configure_uv(&mut create_environment);
        create_environment
            .args(["venv", "--managed-python", "--python", python_request])
            .arg(&virtual_environment)
            .current_dir(build_dir);
        run_yt_dlp_build_command(
            &mut create_environment,
            "create the Python environment; install uv and ensure it is available on PATH",
        );

        let python = yt_dlp_python_executable(&virtual_environment);
        assert!(
            python.is_file(),
            "uv created the yt-dlp environment but Python was not found at {}",
            python.display()
        );
        verify_yt_dlp_python_architecture(build_dir, &python, target);

        let requirements_dir = build_dir.join("bundle").join("requirements");
        let pyinstaller_requirements =
            requirements_dir.join(yt_dlp_pyinstaller_requirements(target));
        // Upstream generates curl-cffi.txt from the complete default extra plus curl-cffi. This
        // includes yt-dlp-ejs, Mutagen, PyCryptodome, and all recommended networking libraries.
        // SecretStorage is Linux-only, and Quiver supplies Deno as a separate sidecar.
        let feature_requirements = requirements_dir.join("curl-cffi.txt");
        let mut install_dependencies = Command::new("uv");
        configure_uv(&mut install_dependencies);
        install_dependencies
            .args(["pip", "install", "--python"])
            .arg(&python)
            .args(["--require-hashes", "--strict", "--requirements"])
            .arg(&pyinstaller_requirements)
            .arg("--requirements")
            .arg(&feature_requirements)
            .current_dir(build_dir);
        run_yt_dlp_build_command(
            &mut install_dependencies,
            "install yt-dlp's hash-pinned PyInstaller, default, yt-dlp-ejs, and curl-cffi dependencies",
        );

        verify_yt_dlp_python_dependencies(build_dir, &python);
        python
    }

    fn configure_uv(command: &mut Command) {
        command
            .env("UV_CACHE_DIR", out_dir().join("uv-cache"))
            .env("UV_PYTHON_INSTALL_DIR", out_dir().join("uv-python"));
    }

    fn yt_dlp_python_request(target: &str) -> String {
        let version = include_str!("../.python-version").trim();
        let platform = match target {
            "aarch64-apple-darwin" => "macos-aarch64-none",
            "aarch64-unknown-linux-gnu" => "linux-aarch64-gnu",
            "x86_64-apple-darwin" => "macos-x86_64-none",
            "x86_64-unknown-linux-gnu" => "linux-x86_64-gnu",
            "aarch64-pc-windows-msvc" => "windows-aarch64-none",
            "x86_64-pc-windows-msvc" => "windows-x86_64-none",
            unsupported => panic!("yt-dlp Python is not configured for target {unsupported}"),
        };
        format!("cpython-{version}-{platform}")
    }

    fn verify_yt_dlp_python_architecture(build_dir: &Path, python: &Path, target: &str) {
        let expected_machine = match target {
            "aarch64-apple-darwin" | "aarch64-pc-windows-msvc" => "arm64",
            "aarch64-unknown-linux-gnu" => "aarch64",
            "x86_64-apple-darwin" | "x86_64-unknown-linux-gnu" => "x86_64",
            "x86_64-pc-windows-msvc" => "amd64",
            unsupported => panic!("yt-dlp Python is not configured for target {unsupported}"),
        };
        let output = Command::new(python)
            .args(["-c", "import platform; print(platform.machine().lower())"])
            .current_dir(build_dir)
            .output()
            .unwrap_or_else(|error| {
                panic!(
                    "failed to inspect yt-dlp Python architecture at {}: {error}",
                    python.display()
                )
            });
        assert_command_output_success(&output, "inspect the yt-dlp Python architecture");

        let actual_machine = String::from_utf8(output.stdout)
            .expect("yt-dlp Python architecture output should be UTF-8")
            .trim()
            .to_ascii_lowercase();
        assert_eq!(
            actual_machine, expected_machine,
            "uv selected a Python interpreter for {actual_machine}, but yt-dlp target {target} requires {expected_machine}"
        );
    }

    fn yt_dlp_pyinstaller_requirements(target: &str) -> &'static str {
        match target {
            "aarch64-apple-darwin"
            | "aarch64-unknown-linux-gnu"
            | "x86_64-apple-darwin"
            | "x86_64-unknown-linux-gnu" => "pyinstaller.txt",
            "aarch64-pc-windows-msvc" => "win-arm64-pyinstaller.txt",
            "x86_64-pc-windows-msvc" => "win-x64-pyinstaller.txt",
            unsupported => panic!("yt-dlp source build is not configured for target {unsupported}"),
        }
    }

    fn yt_dlp_python_executable(virtual_environment: &Path) -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            virtual_environment.join("Scripts").join("python.exe")
        }

        #[cfg(not(target_os = "windows"))]
        {
            virtual_environment.join("bin").join("python")
        }
    }

    fn verify_yt_dlp_python_dependencies(build_dir: &Path, python: &Path) {
        let imports = OPTIONAL_PYTHON_IMPORTS.join(", ");
        let mut command = Command::new(python);
        command
            .args(["-c", &format!("import {imports}")])
            .current_dir(build_dir);
        run_yt_dlp_build_command(&mut command, "verify yt-dlp's optional Python dependencies");
    }

    fn generate_yt_dlp_lazy_extractors(build_dir: &Path, python: &Path) {
        let mut command = Command::new(python);
        command
            .arg("devscripts/make_lazy_extractors.py")
            .current_dir(build_dir);
        run_yt_dlp_build_command(&mut command, "generate yt-dlp's lazy extractors");
    }

    fn build_yt_dlp_executable(build_dir: &Path, python: &Path) -> PathBuf {
        let mut command = Command::new(python);
        command
            .args(["-m", "bundle.pyinstaller"])
            .current_dir(build_dir);
        run_yt_dlp_build_command(&mut command, "freeze yt-dlp with PyInstaller");

        let dist_dir = build_dir.join("dist");
        let mut candidates = fs::read_dir(&dist_dir)
            .unwrap_or_else(|error| {
                panic!(
                    "failed to read yt-dlp PyInstaller output directory {}: {error}",
                    dist_dir.display()
                )
            })
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("yt-dlp"))
            })
            .collect::<Vec<_>>();
        candidates.sort();

        assert_eq!(
            candidates.len(),
            1,
            "expected exactly one frozen yt-dlp executable in {}, found: {candidates:?}",
            dist_dir.display()
        );
        candidates.remove(0)
    }

    fn verify_yt_dlp_executable(executable: &Path) {
        let output = Command::new(executable)
            .args(["--ignore-config", "--verbose", "--list-impersonate-targets"])
            .output()
            .unwrap_or_else(|error| {
                panic!(
                    "failed to start built yt-dlp executable at {}: {error}",
                    executable.display()
                )
            });
        assert_command_output_success(&output, "verify the built yt-dlp executable");

        let diagnostic_output = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .to_ascii_lowercase()
        .replace('_', "-");
        for dependency in REQUIRED_DEPENDENCIES {
            assert!(
                diagnostic_output.contains(dependency),
                "built yt-dlp executable did not report required dependency {dependency}; output:\n{diagnostic_output}"
            );
        }
    }

    fn run_yt_dlp_build_command(command: &mut Command, description: &str) {
        let status = command
            .status()
            .unwrap_or_else(|error| panic!("failed to {description}: {error}"));
        assert!(status.success(), "failed to {description}: {status}");
    }

    fn assert_command_output_success(output: &Output, description: &str) {
        assert!(
            output.status.success(),
            "failed to {description}: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

mod yt_sub_converter {
    use super::{
        Command, Digest, Path, PathBuf, Sha256, build_target_triple, copy_directory, env,
        executable_name, fs, mark_sidecar_executable, out_dir, path_to_str, sidecar_dir,
        sidecar_file_name_for_host, warn,
    };

    pub const SIDECAR_NAME: &str = "ytsubconverter";

    pub fn stage() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let expected_sidecar = sidecar_dir().join(sidecar_file_name_for_host(SIDECAR_NAME));
        let workspace_root = manifest_dir.as_path();
        let yt_sub_converter_dir = workspace_root.join("YTSubConverter");

        if !yt_sub_converter_dir.join("YTSubConverter.sln").is_file() {
            warn(format!(
                "YTSubConverter submodule is missing at {}; skipping sidecar build.",
                yt_sub_converter_dir.display()
            ));
            return;
        }

        let stamp_path = sidecar_dir().join("ytsubconverter.build-stamp");
        let expected_stamp = format!(
            "{}:{}\n",
            build_target_triple(),
            source_fingerprint(&yt_sub_converter_dir)
        );
        if expected_sidecar.is_file()
            && fs::read_to_string(&stamp_path).is_ok_and(|stamp| stamp == expected_stamp)
        {
            mark_sidecar_executable(&expected_sidecar);
            return;
        }

        let build_source_dir = prepare_build_sources(&yt_sub_converter_dir);

        #[cfg(target_os = "windows")]
        let built_binary = {
            let Some(built_binary) = build_yt_sub_converter_windows(&build_source_dir) else {
                return;
            };
            built_binary
        };

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let built_binary = build_yt_sub_converter_self_contained(&build_source_dir);

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        let built_binary = {
            panic!(
                "YTSubConverter sidecar build is not configured for target {}",
                build_target_triple()
            );
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

        mark_sidecar_executable(&expected_sidecar);
        fs::write(stamp_path, expected_stamp).expect("failed to write YTSubConverter build stamp");
    }

    fn source_fingerprint(root: &Path) -> String {
        fn hash_path(root: &Path, path: &Path, hasher: &mut Sha256) {
            if path.is_dir() {
                let mut entries = fs::read_dir(path)
                    .expect("failed to read YTSubConverter source")
                    .map(|entry| entry.expect("failed to read YTSubConverter entry").path())
                    .filter(|entry| {
                        !matches!(
                            entry.file_name().and_then(|name| name.to_str()),
                            Some("bin" | "obj")
                        )
                    })
                    .collect::<Vec<_>>();
                entries.sort();
                for entry in entries {
                    hash_path(root, &entry, hasher);
                }
            } else {
                hasher.update(
                    path.strip_prefix(root)
                        .expect("source path within project")
                        .to_string_lossy()
                        .as_bytes(),
                );
                hasher.update([0]);
                hasher.update(fs::read(path).expect("failed to read YTSubConverter source"));
                hasher.update([0]);
            }
        }
        let mut hasher = Sha256::new();
        for project in [
            "YTSubConverter.Shared",
            "YTSubConverter.UI.Linux",
            "YTSubConverter.UI.Win",
        ] {
            hash_path(root, &root.join(project), &mut hasher);
        }
        format!("{:x}", hasher.finalize())
    }

    fn prepare_build_sources(source: &Path) -> PathBuf {
        let destination = out_dir().join("ytsubconverter-source");
        if destination.exists() {
            fs::remove_dir_all(&destination).expect("failed to clear YTSubConverter build sources");
        }
        for project in [
            "YTSubConverter.Shared",
            "YTSubConverter.UI.Linux",
            "YTSubConverter.UI.Win",
        ] {
            copy_directory(&source.join(project), &destination.join(project));
            for generated in ["bin", "obj"] {
                let path = destination.join(project).join(generated);
                if path.exists() {
                    fs::remove_dir_all(path).expect("failed to clear copied YTSubConverter output");
                }
            }
        }
        destination
    }

    #[cfg(target_os = "windows")]
    fn build_yt_sub_converter_windows(yt_sub_converter_dir: &Path) -> Option<PathBuf> {
        let project = yt_sub_converter_dir
            .join("YTSubConverter.UI.Win")
            .join("YTSubConverter.UI.Win.csproj");
        let publish_dir = out_dir().join("ytsubconverter-publish");
        fs::create_dir_all(&publish_dir).expect("failed to create YTSubConverter output directory");
        let Some(msbuild) = find_msbuild() else {
            warn(
                "Visual Studio MSBuild was not found; skipping YTSubConverter Windows sidecar build. \
             Install Visual Studio Build Tools with MSBuild to generate the net48 sidecar.",
            );
            return None;
        };

        let status = Command::new(&msbuild)
            .arg(&project)
            .env("NUGET_PACKAGES", out_dir().join("nuget-packages"))
            .args([
                "/restore",
                "/m",
                "/p:Configuration=Release",
                "/p:Platform=AnyCPU",
            ])
            .arg(format!("/p:OutDir={}\\", publish_dir.display()))
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

        let built_binary = publish_dir.join("YTSubConverter.exe");

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
    fn build_yt_sub_converter_self_contained(yt_sub_converter_dir: &Path) -> PathBuf {
        let project = yt_sub_converter_dir
            .join("YTSubConverter.UI.Linux")
            .join("YTSubConverter.UI.Linux.csproj");
        let target = build_target_triple();
        let runtime_identifier = yt_sub_converter_runtime_identifier(&target);
        let publish_dir = out_dir().join("ytsubconverter-publish").join(&target);

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

        let built_binary = publish_dir.join(executable_name(SIDECAR_NAME));
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
            unsupported => {
                panic!("YTSubConverter publish is not configured for target {unsupported}")
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn run_dotnet<const N: usize>(working_directory: &Path, args: [&str; N]) {
        let status = Command::new("dotnet")
            .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
            .env("DOTNET_CLI_HOME", out_dir().join("dotnet-home"))
            .env("NUGET_PACKAGES", out_dir().join("nuget-packages"))
            .args(args)
            .current_dir(working_directory)
            .status()
            .expect("failed to start dotnet while building YTSubConverter sidecar");

        assert!(
            status.success(),
            "dotnet failed while building YTSubConverter sidecar with status {status}"
        );
    }
}

mod bgutil_pot_provider {
    use super::{
        Command, Path, PathBuf, build_target_triple, copy_directory, copy_file_if_exists, deno,
        env, executable_name, fs, host_target_triple, sidecar_dir, sidecar_file_name_for_host,
        warn,
    };

    pub const SIDECAR_NAME: &str = "bgutil-pot";

    pub fn stage() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.as_path();
        let provider_dir = workspace_root.join("bgutil-ytdlp-pot-provider");
        let server_dir = provider_dir.join("server");

        if !server_dir.join("package.json").is_file()
            || !server_dir.join("package-lock.json").is_file()
            || !server_dir.join("src/main.ts").is_file()
        {
            warn(format!(
                "bgutil-ytdlp-pot-provider server sources are missing at {}; skipping POT provider sidecar build.",
                server_dir.display()
            ));
            return;
        }

        let target = build_target_triple();
        let host = env::var("HOST").unwrap_or_else(|_| host_target_triple().to_string());
        assert_eq!(
            target, host,
            "bgutil POT provider sidecar must be built natively because the build executes the target-specific Deno runtime (host: {host}, target: {target})"
        );

        let expected_sidecar = sidecar_dir().join(sidecar_file_name_for_host(SIDECAR_NAME));
        let build_stamp_path = sidecar_dir().join(format!("{SIDECAR_NAME}-{target}.build-stamp"));
        let expected_build_stamp = format!("{}:{target}\n", deno::VERSION);
        let sidecar_is_current = expected_sidecar.is_file()
            && fs::read_to_string(&build_stamp_path)
                .is_ok_and(|stamp| stamp == expected_build_stamp)
            && !is_source_newer_than(&server_dir.join("package.json"), &expected_sidecar)
            && !is_source_newer_than(&server_dir.join("package-lock.json"), &expected_sidecar)
            && !is_source_newer_than(&server_dir.join("src"), &expected_sidecar);

        if sidecar_is_current {
            return;
        }

        let out_dir =
            PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR should be set by Cargo"));
        let build_dir = prepare_bgutil_build_directory(&server_dir, &out_dir);
        install_bgutil_dependencies(&build_dir, &out_dir);
        let built_binary = compile_bgutil_sidecar(&build_dir, &target);

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

        fs::write(&build_stamp_path, expected_build_stamp).unwrap_or_else(|error| {
            panic!(
                "failed to write bgutil POT provider build stamp at {}: {error}",
                build_stamp_path.display()
            )
        });
    }

    fn prepare_bgutil_build_directory(server_dir: &Path, out_dir: &Path) -> PathBuf {
        let build_dir = out_dir.join("bgutil-pot-provider");
        if build_dir.exists() {
            fs::remove_dir_all(&build_dir).unwrap_or_else(|error| {
                panic!(
                    "failed to clear bgutil POT provider build directory at {}: {error}",
                    build_dir.display()
                )
            });
        }

        copy_file_if_exists(
            &server_dir.join("package.json"),
            &build_dir.join("package.json"),
        );
        copy_file_if_exists(
            &server_dir.join("package-lock.json"),
            &build_dir.join("package-lock.json"),
        );
        copy_directory(&server_dir.join("src"), &build_dir.join("src"));

        build_dir
    }

    fn install_bgutil_dependencies(build_dir: &Path, out_dir: &Path) {
        let npm_cache_dir = out_dir.join("npm-cache");
        let npm_status = Command::new(npm_executable())
        // canvas is an unused optional jsdom integration, and it has no Windows ARM64 prebuild.
        // Skipping dependency lifecycle scripts prevents npm from attempting a Cairo source build.
        .args([
            "ci",
            "--omit=dev",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
        ])
        .env("npm_config_cache", npm_cache_dir)
        .current_dir(build_dir)
        .status()
        .unwrap_or_else(|error| {
            panic!(
                "failed to start npm while installing bgutil POT provider dependencies in {}: {error}",
                build_dir.display()
            )
        });
        assert!(
            npm_status.success(),
            "npm ci failed while installing bgutil POT provider dependencies with status {npm_status}"
        );
    }

    fn compile_bgutil_sidecar(build_dir: &Path, target: &str) -> PathBuf {
        let deno_sidecar = sidecar_dir().join(sidecar_file_name_for_host(deno::SIDECAR_NAME));
        assert!(
            deno_sidecar.is_file(),
            "Deno sidecar is missing at {}; it must be staged before building the bgutil POT provider",
            deno_sidecar.display()
        );

        let built_binary = build_dir.join(executable_name(SIDECAR_NAME));
        let deno_status = Command::new(&deno_sidecar)
            .env("DENO_DIR", build_dir.join("deno-cache"))
            .args([
                "compile",
                "--no-check",
                "--no-lock",
                "--node-modules-dir=manual",
                "--self-extracting",
                "--allow-env",
                "--allow-net",
                "--allow-read",
                "--allow-sys",
                "--allow-ffi",
                "--target",
                target,
                "--output",
            ])
            .arg(&built_binary)
            .arg("src/main.ts")
            .current_dir(build_dir)
            .status()
            .unwrap_or_else(|error| {
                panic!(
                    "failed to start Deno while compiling bgutil POT provider from {}: {error}",
                    build_dir.display()
                )
            });
        assert!(
            deno_status.success(),
            "Deno failed while compiling bgutil POT provider with status {deno_status}"
        );

        built_binary
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

    fn npm_executable() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            "npm.cmd"
        }

        #[cfg(not(target_os = "windows"))]
        {
            "npm"
        }
    }
}

mod deno {
    use super::{
        Cursor, Digest, Path, Sha256, ZipArchive, build_target_triple, download, executable_name,
        fs, sidecar_dir, sidecar_file_name_for_host,
    };

    pub const VERSION: &str = "2.9.4";
    pub const SIDECAR_NAME: &str = "deno";
    const RELEASE_BASE_URL: &str = "https://github.com/denoland/deno/releases/download";

    pub fn stage() {
        let expected_sidecar = sidecar_dir().join(sidecar_file_name_for_host(SIDECAR_NAME));
        let target = build_target_triple();
        let expected_stamp = format!("{VERSION}:{target}:{}\n", deno_checksum(&target));
        let stamp_path = sidecar_dir().join("deno.build-stamp");
        if expected_sidecar.is_file()
            && fs::read_to_string(&stamp_path).is_ok_and(|stamp| stamp == expected_stamp)
        {
            return;
        }

        let archive_name = deno_archive_name(&target);
        let archive_url = format!("{RELEASE_BASE_URL}/v{VERSION}/{archive_name}");
        let archive = download(&archive_url);
        verify_sha256(&archive, deno_checksum(&target), &archive_name);
        extract_deno_binary(&archive, &expected_sidecar);
        fs::write(stamp_path, expected_stamp).expect("failed to write Deno build stamp");
    }

    fn deno_archive_name(target: &str) -> String {
        match target {
            "aarch64-apple-darwin"
            | "aarch64-unknown-linux-gnu"
            | "aarch64-pc-windows-msvc"
            | "x86_64-apple-darwin"
            | "x86_64-pc-windows-msvc"
            | "x86_64-unknown-linux-gnu" => format!("{SIDECAR_NAME}-{target}.zip"),
            unsupported => panic!("Deno release asset is not configured for target {unsupported}"),
        }
    }

    fn deno_checksum(target: &str) -> &'static str {
        match target {
            "aarch64-apple-darwin" => {
                "6d17647fdbf9c587a581dba205054c4ccf732dae0a196cc1e9b44c07589db412"
            }
            "aarch64-unknown-linux-gnu" => {
                "111da5c05c240cfdc4340f234a0e3539d39dbcb6755221f19dcd60bacc8be5aa"
            }
            "aarch64-pc-windows-msvc" => {
                "7c8c94a603c7de90a4c41fa9b87d31bc9d3c91fb84af65dadacc8cdd97d365d4"
            }
            "x86_64-apple-darwin" => {
                "f757df6d3991e37601c69fad56c22b37c4ea77b5dcfad3636a642c2ba4c9b19f"
            }
            "x86_64-pc-windows-msvc" => {
                "68ed08b05c56cf887e9aa509947dc3f468f7e12f47a13e5c1abd51d46d1453ef"
            }
            "x86_64-unknown-linux-gnu" => {
                "c24f955d9fbfe0ea5ae2b501c8e71ae76e31e4c9782390a54a284b3364fda725"
            }
            unsupported => panic!("Deno checksum is not configured for target {unsupported}"),
        }
    }

    fn verify_sha256(bytes: &[u8], expected: &str, archive_name: &str) {
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
            .by_name(executable_name(SIDECAR_NAME).as_str())
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
}

mod ffmpeg {
    use super::{
        Archive, Cursor, Digest, Path, Sha256, XzDecoder, ZipArchive, build_target_triple,
        download, download_text, executable_name, fs, mark_sidecar_executable, sidecar_dir,
        sidecar_file_name_for_host,
    };

    pub const FFMPEG_SIDECAR_NAME: &str = "ffmpeg";
    pub const FFPROBE_SIDECAR_NAME: &str = "ffprobe";
    const BUILDS_RELEASE_BASE_URL: &str =
        "https://github.com/yt-dlp/FFmpeg-Builds/releases/latest/download";
    const BUILDS_CHECKSUMS: &str = "checksums.sha256";

    pub fn stage() {
        #[cfg(not(target_os = "macos"))]
        {
            let expected_ffmpeg =
                sidecar_dir().join(sidecar_file_name_for_host(FFMPEG_SIDECAR_NAME));
            let expected_ffprobe =
                sidecar_dir().join(sidecar_file_name_for_host(FFPROBE_SIDECAR_NAME));

            if expected_ffmpeg.is_file() && expected_ffprobe.is_file() {
                mark_sidecar_executable(&expected_ffmpeg);
                mark_sidecar_executable(&expected_ffprobe);
                return;
            }

            stage_yt_dlp_ffmpeg_builds_sidecars(&expected_ffmpeg, &expected_ffprobe);
        }
    }

    fn stage_yt_dlp_ffmpeg_builds_sidecars(expected_ffmpeg: &Path, expected_ffprobe: &Path) {
        let asset_name = ffmpeg_builds_asset_name(&build_target_triple());
        let checksums = download_text(&format!("{BUILDS_RELEASE_BASE_URL}/{BUILDS_CHECKSUMS}"));
        let archive = download(&format!("{BUILDS_RELEASE_BASE_URL}/{asset_name}"));

        verify_sha256_for_file(&archive, &checksums, asset_name);

        if has_extension(asset_name, "zip") {
            extract_zip_binary(&archive, expected_ffmpeg, FFMPEG_SIDECAR_NAME);
            extract_zip_binary(&archive, expected_ffprobe, FFPROBE_SIDECAR_NAME);
        } else if has_compound_extensions(asset_name, "tar", "xz") {
            extract_tar_xz_binary(&archive, expected_ffmpeg, FFMPEG_SIDECAR_NAME);
            extract_tar_xz_binary(&archive, expected_ffprobe, FFPROBE_SIDECAR_NAME);
        } else {
            panic!("unsupported FFmpeg archive format for {asset_name}");
        }
    }

    fn ffmpeg_builds_asset_name(target: &str) -> &'static str {
        match target {
            "aarch64-pc-windows-msvc" => "ffmpeg-master-latest-winarm64-gpl.zip",
            "x86_64-pc-windows-msvc" => "ffmpeg-master-latest-win64-gpl.zip",
            "aarch64-unknown-linux-gnu" => "ffmpeg-master-latest-linuxarm64-gpl.tar.xz",
            "x86_64-unknown-linux-gnu" => "ffmpeg-master-latest-linux64-gpl.tar.xz",
            unsupported => {
                panic!("FFmpeg release asset is not configured for target {unsupported}")
            }
        }
    }

    fn has_extension(path: &str, extension: &str) -> bool {
        Path::new(path)
            .extension()
            .and_then(|actual_extension| actual_extension.to_str())
            .is_some_and(|actual_extension| actual_extension.eq_ignore_ascii_case(extension))
    }

    fn has_compound_extensions(path: &str, stem_extension: &str, extension: &str) -> bool {
        let path = Path::new(path);

        has_extension(path.to_string_lossy().as_ref(), extension)
            && path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| has_extension(stem, stem_extension))
    }

    fn verify_sha256_for_file(bytes: &[u8], checksums: &str, asset_name: &str) {
        let expected = checksums
            .lines()
            .filter_map(parse_sha256_checksum_line)
            .find_map(|(hash, file_name)| (file_name == asset_name).then_some(hash))
            .unwrap_or_else(|| panic!("SHA-256 sums did not contain an entry for {asset_name}"))
            .to_ascii_lowercase();
        let actual = format!("{:x}", Sha256::digest(bytes));

        assert_eq!(
            expected, actual,
            "SHA-256 mismatch for downloaded archive {asset_name}"
        );
    }

    fn parse_sha256_checksum_line(line: &str) -> Option<(&str, &str)> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }

        let (hash, file_name) = line.split_once(char::is_whitespace)?;
        let file_name = file_name.trim_start();
        let file_name = file_name.strip_prefix('*').unwrap_or(file_name);

        (hash.len() == 64 && hash.chars().all(|character| character.is_ascii_hexdigit()))
            .then_some((hash, file_name))
    }

    fn extract_zip_binary(archive: &[u8], expected_sidecar: &Path, binary_name: &str) {
        let mut archive = ZipArchive::new(Cursor::new(archive))
            .unwrap_or_else(|error| panic!("failed to open downloaded {binary_name} zip: {error}"));
        let expected_name = executable_name(binary_name);

        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap_or_else(|error| {
                panic!("failed to read {binary_name} zip entry {index}: {error}")
            });

            if !entry.is_file() || !archive_entry_matches_binary(entry.name(), &expected_name) {
                continue;
            }

            write_executable(&mut entry, expected_sidecar, binary_name);
            return;
        }

        panic!("downloaded {binary_name} zip did not contain {expected_name}");
    }

    fn extract_tar_xz_binary(archive: &[u8], expected_sidecar: &Path, binary_name: &str) {
        let decoder = XzDecoder::new(Cursor::new(archive));
        let mut archive = Archive::new(decoder);
        let expected_name = executable_name(binary_name);
        let entries = archive.entries().unwrap_or_else(|error| {
            panic!("failed to read downloaded {binary_name} tar.xz: {error}")
        });

        for entry in entries {
            let mut entry = entry
                .unwrap_or_else(|error| panic!("failed to read {binary_name} tar entry: {error}"));
            let entry_path = entry.path().unwrap_or_else(|error| {
                panic!("failed to read {binary_name} tar entry path: {error}")
            });
            let entry_name = entry_path.to_string_lossy().into_owned();

            if !archive_entry_matches_binary(&entry_name, &expected_name) {
                continue;
            }

            write_executable(&mut entry, expected_sidecar, binary_name);
            return;
        }

        panic!("downloaded {binary_name} tar.xz did not contain {expected_name}");
    }

    fn archive_entry_matches_binary(entry_name: &str, expected_name: &str) -> bool {
        let normalized_name = entry_name.replace('\\', "/");

        normalized_name == expected_name
            || normalized_name.ends_with(&format!("/{expected_name}"))
            || normalized_name.ends_with(&format!("/bin/{expected_name}"))
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
}
