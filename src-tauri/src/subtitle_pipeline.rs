use crate::yt_dlp::{
    YtDlpCommandOutput, YtDlpError, YtDlpOutputStream, YtDlpRunner, ffmpeg_path, ffprobe_path,
};
use std::{
    ffi::OsStr,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

const YT_SUB_CONVERTER_BINARY_NAME: &str = "ytsubconverter";

#[derive(Clone, Debug)]
struct SubtitleTrack {
    language: String,
    path: PathBuf,
}

pub async fn run_youtube_video_download<F>(
    app: AppHandle,
    runner: YtDlpRunner,
    args: Vec<String>,
    link: String,
    on_chunk: F,
) -> Result<YtDlpCommandOutput, String>
where
    F: Fn(YtDlpOutputStream, &str) + Send + Sync + 'static,
{
    let on_chunk = Arc::new(on_chunk);
    let subtitle_dir = create_subtitle_dir()?;
    emit_pipeline_log(
        &on_chunk,
        &format!(
            "Downloading YouTube srv3 subtitles to {}.\n",
            path_name(&subtitle_dir)
        ),
    );
    let subtitle_output = runner
        .run(subtitle_download_args(&args, &link, &subtitle_dir))
        .await
        .map_err(|error| error.to_string())?;
    emit_command_output(&on_chunk, &subtitle_output);

    if !subtitle_output.success {
        let mut output = subtitle_output;
        output.stderr.push_str("\nSubtitle download failed.");
        let _ = fs::remove_dir_all(&subtitle_dir);
        return Ok(output);
    }

    let srv3_files = collect_files_with_extension(&subtitle_dir, "srv3")?;
    emit_pipeline_log(
        &on_chunk,
        &format!("Found {} srv3 subtitle file(s).\n", srv3_files.len()),
    );
    let conversion_app = app.clone();
    let conversion_on_chunk = Arc::clone(&on_chunk);
    let conversion_task = tauri::async_runtime::spawn(async move {
        convert_subtitles(conversion_app, srv3_files, conversion_on_chunk).await
    });

    let downloaded_paths_file = subtitle_dir.join("downloaded-media-paths.txt");
    emit_pipeline_log(&on_chunk, "Recording final yt-dlp media path(s).\n");
    let streaming_on_chunk = Arc::clone(&on_chunk);
    let main_output = runner
        .run_streaming(
            main_download_args(args, &downloaded_paths_file),
            move |stream, chunk| {
                streaming_on_chunk(stream, chunk);
            },
        )
        .await
        .map_err(|error| error.to_string())?;

    let converted_subtitles = conversion_task
        .await
        .map_err(|error| format!("subtitle conversion task failed: {error}"))??;
    emit_pipeline_log(
        &on_chunk,
        &format!(
            "Converted {} ass subtitle file(s).\n",
            converted_subtitles.len()
        ),
    );

    if !main_output.success || converted_subtitles.is_empty() {
        let _ = fs::remove_dir_all(&subtitle_dir);
        return Ok(with_pipeline_output(main_output, &subtitle_output, ""));
    }

    let media_paths = downloaded_media_paths(&downloaded_paths_file);
    emit_pipeline_log(
        &on_chunk,
        &format!(
            "Found {} downloaded media file(s) for remux.\n",
            media_paths.len()
        ),
    );
    if media_paths.is_empty() {
        let _ = fs::remove_dir_all(&subtitle_dir);
        return Ok(with_pipeline_output(
            main_output,
            &subtitle_output,
            "\nCould not identify the final downloaded media path; skipping subtitle remux.",
        ));
    }

    let mut remux_log = String::new();
    for media_path in media_paths {
        remux_subtitles(&media_path, &converted_subtitles, &mut remux_log, &on_chunk).await?;
    }

    let _ = fs::remove_dir_all(&subtitle_dir);

    Ok(with_pipeline_output(
        main_output,
        &subtitle_output,
        &remux_log,
    ))
}

fn subtitle_download_args(args: &[String], link: &str, subtitle_dir: &Path) -> Vec<String> {
    let mut subtitle_args = vec![
        "-v".to_string(),
        "--write-subs".to_string(),
        "--sub-langs".to_string(),
        "all".to_string(),
        "--sub-format".to_string(),
        "srv3".to_string(),
        "--skip-download".to_string(),
        "--windows-filenames".to_string(),
        "-P".to_string(),
        subtitle_dir.display().to_string(),
    ];

    copy_option_with_value(args, "--cookies", &mut subtitle_args);
    copy_option_with_value(args, "--compat-options", &mut subtitle_args);
    copy_option_with_value(args, "--extractor-args", &mut subtitle_args);
    subtitle_args.push(link.trim().to_string());
    subtitle_args
}

fn copy_option_with_value(args: &[String], option: &str, output: &mut Vec<String>) {
    let mut index = 0;
    while index + 1 < args.len() {
        if args[index] == option {
            output.push(args[index].clone());
            output.push(args[index + 1].clone());
            index += 2;
        } else {
            index += 1;
        }
    }
}

fn main_download_args(args: Vec<String>, downloaded_paths_file: &Path) -> Vec<String> {
    let mut args = args;
    args.extend([
        "--print-to-file".to_string(),
        "after_move:filepath".to_string(),
        downloaded_paths_file.display().to_string(),
    ]);
    args
}

async fn convert_subtitles(
    app: AppHandle,
    srv3_files: Vec<PathBuf>,
    on_chunk: Arc<impl Fn(YtDlpOutputStream, &str) + Send + Sync + 'static>,
) -> Result<Vec<SubtitleTrack>, String> {
    let mut subtitles = Vec::new();

    for srv3_file in srv3_files {
        let Some((subtitle_name, language)) = subtitle_name_and_language(&srv3_file) else {
            continue;
        };
        let ass_file = srv3_file.with_file_name(format!("{subtitle_name}.{language}.ass"));
        emit_pipeline_log(
            &on_chunk,
            &format!(
                "Converting {} to {}.\n",
                path_name(&srv3_file),
                path_name(&ass_file)
            ),
        );
        run_ytsubconverter(&app, &srv3_file, &ass_file).await?;
        subtitles.push(SubtitleTrack {
            language,
            path: ass_file,
        });
    }

    Ok(subtitles)
}

async fn run_ytsubconverter(app: &AppHandle, input: &Path, output: &Path) -> Result<(), String> {
    let command_output = app
        .shell()
        .sidecar(YT_SUB_CONVERTER_BINARY_NAME)
        .map_err(|error| format!("failed to prepare YTSubConverter sidecar: {error}"))?
        .args([
            "--visual",
            &input.display().to_string(),
            &output.display().to_string(),
        ])
        .output()
        .await
        .map_err(|error| format!("failed to run YTSubConverter: {error}"))?;

    if command_output.status.success() && output.is_file() {
        Ok(())
    } else {
        Err(format!(
            "YTSubConverter failed for {}: {}{}",
            input.display(),
            String::from_utf8_lossy(&command_output.stdout),
            String::from_utf8_lossy(&command_output.stderr)
        ))
    }
}

async fn remux_subtitles(
    media_path: &Path,
    subtitles: &[SubtitleTrack],
    remux_log: &mut String,
    on_chunk: &Arc<impl Fn(YtDlpOutputStream, &str) + Send + Sync + 'static>,
) -> Result<(), String> {
    let temp_media_path = temp_media_path(media_path)?;
    emit_pipeline_log(
        on_chunk,
        &format!(
            "Renaming {} to {} before ffmpeg remux.\n",
            path_name(media_path),
            path_name(&temp_media_path)
        ),
    );
    fs::rename(media_path, &temp_media_path).map_err(|error| {
        format!(
            "failed to rename {} to {} before subtitle remux: {error}",
            media_path.display(),
            temp_media_path.display()
        )
    })?;

    emit_pipeline_log(on_chunk, "Running ffmpeg.\n");
    let existing_subtitle_count = subtitle_stream_count(&temp_media_path).await?;
    emit_pipeline_log(
        on_chunk,
        &format!(
            "Found {existing_subtitle_count} existing subtitle stream(s); naming added tracks after that offset.\n"
        ),
    );
    let ffmpeg_result = run_ffmpeg_remux(
        temp_media_path.clone(),
        media_path.to_path_buf(),
        subtitles.to_vec(),
        existing_subtitle_count,
    )
    .await;

    match ffmpeg_result {
        Ok(output) if output.status.success() => {
            emit_pipeline_log(
                on_chunk,
                &format!("ffmpeg remux completed for {}.\n", path_name(media_path)),
            );
            let _ = write!(
                remux_log,
                "\nAttached {} visual subtitle track(s) to {}.",
                subtitles.len(),
                path_name(media_path)
            );
            let _ = fs::remove_file(&temp_media_path);
            Ok(())
        }
        Ok(output) => {
            emit_process_output(on_chunk, "ffmpeg", &output);
            let _ = restore_original_media(media_path, &temp_media_path);
            Err(format!(
                "ffmpeg subtitle remux failed for {}: {}{}",
                media_path.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
        }
        Err(error) => {
            emit_pipeline_log(on_chunk, &format!("{error}\n"));
            let _ = restore_original_media(media_path, &temp_media_path);
            Err(error.to_string())
        }
    }
}

fn emit_command_output(
    on_chunk: &Arc<impl Fn(YtDlpOutputStream, &str) + Send + Sync + 'static>,
    output: &YtDlpCommandOutput,
) {
    if !output.stdout.is_empty() {
        on_chunk(YtDlpOutputStream::Stdout, &output.stdout);
    }
    if !output.stderr.is_empty() {
        on_chunk(YtDlpOutputStream::Stderr, &output.stderr);
    }
}

fn emit_process_output(
    on_chunk: &Arc<impl Fn(YtDlpOutputStream, &str) + Send + Sync + 'static>,
    process_name: &str,
    output: &std::process::Output,
) {
    emit_pipeline_log(
        on_chunk,
        &format!("{process_name} exited with status {}.\n", output.status),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.is_empty() {
        on_chunk(YtDlpOutputStream::Stdout, &stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        on_chunk(YtDlpOutputStream::Stderr, &stderr);
    }
}

fn emit_pipeline_log(
    on_chunk: &Arc<impl Fn(YtDlpOutputStream, &str) + Send + Sync + 'static>,
    message: &str,
) {
    on_chunk(YtDlpOutputStream::Stderr, &format!("[quiver] {message}"));
}

fn path_name(path: &Path) -> String {
    path.file_name()
        .and_then(OsStr::to_str)
        .map_or_else(|| path.display().to_string(), ToString::to_string)
}

async fn run_ffmpeg_remux(
    input_media: PathBuf,
    output_media: PathBuf,
    subtitles: Vec<SubtitleTrack>,
    existing_subtitle_count: usize,
) -> Result<std::process::Output, YtDlpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut command = Command::new(ffmpeg_path());
        command
            .arg("-y")
            .arg("-i")
            .arg(input_media)
            .args(
                subtitles
                    .iter()
                    .flat_map(|subtitle| [OsStr::new("-i"), subtitle.path.as_os_str()]),
            )
            .arg("-map")
            .arg("0");

        for index in 0..subtitles.len() {
            command.arg("-map").arg((index + 1).to_string());
        }

        command
            .arg("-c")
            .arg("copy")
            .arg("-max_interleave_delta")
            .arg("0");

        for (index, subtitle) in subtitles.iter().enumerate() {
            let output_subtitle_index = existing_subtitle_count + index;
            command
                .arg(format!("-metadata:s:s:{output_subtitle_index}"))
                .arg(format!("language={}", subtitle.language))
                .arg(format!("-metadata:s:s:{output_subtitle_index}"))
                .arg(format!("title={}", subtitle.language))
                .arg(format!("-metadata:s:s:{output_subtitle_index}"))
                .arg(format!("handler_name={}", subtitle.language));
        }

        command
            .arg(output_media)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| YtDlpError::SpawnFailed(format!("failed to run ffmpeg: {error}")))
    })
    .await
    .map_err(|error| YtDlpError::SpawnFailed(error.to_string()))?
}

async fn subtitle_stream_count(media_path: &Path) -> Result<usize, String> {
    let media_path = media_path.to_path_buf();
    let output = tauri::async_runtime::spawn_blocking(move || {
        Command::new(ffprobe_path())
            .arg("-v")
            .arg("error")
            .arg("-select_streams")
            .arg("s")
            .arg("-show_entries")
            .arg("stream=index")
            .arg("-of")
            .arg("csv=p=0")
            .arg(media_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    })
    .await
    .map_err(|error| format!("failed to join ffprobe task: {error}"))?
    .map_err(|error| format!("failed to run ffprobe: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe failed while counting existing subtitle streams: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count())
}

fn restore_original_media(media_path: &Path, temp_media_path: &Path) -> Result<(), std::io::Error> {
    if media_path.exists() {
        fs::remove_file(media_path)?;
    }
    fs::rename(temp_media_path, media_path)
}

fn downloaded_media_paths(downloaded_paths_file: &Path) -> Vec<PathBuf> {
    let Ok(paths) = fs::read_to_string(downloaded_paths_file) else {
        return Vec::new();
    };

    paths
        .lines()
        .map(str::trim)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .collect()
}

fn collect_files_with_extension(directory: &Path, extension: &str) -> Result<Vec<PathBuf>, String> {
    let mut files = fs::read_dir(directory)
        .map_err(|error| format!("failed to read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(OsStr::to_str)
                .is_some_and(|path_extension| path_extension.eq_ignore_ascii_case(extension))
        })
        .collect::<Vec<_>>();

    files.sort();
    Ok(files)
}

fn subtitle_name_and_language(path: &Path) -> Option<(String, String)> {
    let file_stem = path.file_stem()?.to_string_lossy();
    let (name, language) = file_stem.rsplit_once('.')?;
    Some((name.to_string(), language.to_string()))
}

fn temp_media_path(media_path: &Path) -> Result<PathBuf, String> {
    let parent = media_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = media_path
        .file_stem()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("{} has no valid file name", media_path.display()))?;
    let extension = media_path.extension().and_then(OsStr::to_str);
    let mut candidate = parent.join(match extension {
        Some(extension) => format!("{stem}_temp.{extension}"),
        None => format!("{stem}_temp"),
    });

    let mut suffix = 1;
    while candidate.exists() {
        candidate = parent.join(match extension {
            Some(extension) => format!("{stem}_temp_{suffix}.{extension}"),
            None => format!("{stem}_temp_{suffix}"),
        });
        suffix += 1;
    }

    Ok(candidate)
}

fn create_subtitle_dir() -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before UNIX epoch: {error}"))?
        .as_millis();
    let directory = std::env::temp_dir().join(format!("quiver-ytsubs-{timestamp}"));
    fs::create_dir_all(&directory)
        .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;
    Ok(directory)
}

fn with_pipeline_output(
    mut main_output: YtDlpCommandOutput,
    subtitle_output: &YtDlpCommandOutput,
    extra_stderr: &str,
) -> YtDlpCommandOutput {
    main_output.stdout = format!("{}{}", subtitle_output.stdout, main_output.stdout);
    main_output.stderr = format!(
        "{}{}{}",
        subtitle_output.stderr, main_output.stderr, extra_stderr
    );
    main_output
}
