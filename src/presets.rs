use serde::{Deserialize, Serialize};

const COOKIE_PATH: &str = "~/Downloads/cookies.txt";
const POT_BASE_URL: &str = "http://127.0.0.1:4416";
const DEFAULT_DOWNLOAD_DIRECTORY: &str = "~/Downloads";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PresetId {
    YouTubeCookiesVideo,
    YouTubeCookiesAudio,
    YouTubePlainVideo,
    YouTubePlainAudio,
    GeneralCookiesVideo,
    GeneralCookiesAudio,
    GeneralPlainVideo,
    GeneralPlainAudio,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPreset {
    pub id: PresetId,
    pub label: &'static str,
    pub source: PresetSource,
    pub media: PresetMedia,
    pub uses_cookies: bool,
    pub requires_pot_server: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PresetSource {
    YouTube,
    General,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PresetMedia {
    Video,
    Audio,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPresetInput {
    pub link: String,
    pub directory: Option<String>,
    pub cookies_path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetInputField {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: PresetInputKind,
    pub required: bool,
    pub default_value: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PresetInputKind {
    Url,
    Directory,
    File,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetCommandPreview {
    pub preset: DownloadPreset,
    pub args: Vec<String>,
    pub requires_pot_server: bool,
    pub pending_integrations: Vec<&'static str>,
}

pub fn all_presets() -> [DownloadPreset; 8] {
    [
        preset(
            PresetId::YouTubeCookiesVideo,
            "YouTube Video, with Cookies",
            PresetSource::YouTube,
            PresetMedia::Video,
            true,
        ),
        preset(
            PresetId::YouTubeCookiesAudio,
            "YouTube Audio, with Cookies",
            PresetSource::YouTube,
            PresetMedia::Audio,
            true,
        ),
        preset(
            PresetId::YouTubePlainVideo,
            "YouTube Video, no Cookies",
            PresetSource::YouTube,
            PresetMedia::Video,
            false,
        ),
        preset(
            PresetId::YouTubePlainAudio,
            "YouTube Audio, no Cookies",
            PresetSource::YouTube,
            PresetMedia::Audio,
            false,
        ),
        preset(
            PresetId::GeneralCookiesVideo,
            "General Video, with Cookies",
            PresetSource::General,
            PresetMedia::Video,
            true,
        ),
        preset(
            PresetId::GeneralCookiesAudio,
            "General Audio, with Cookies",
            PresetSource::General,
            PresetMedia::Audio,
            true,
        ),
        preset(
            PresetId::GeneralPlainVideo,
            "General Video, no Cookies",
            PresetSource::General,
            PresetMedia::Video,
            false,
        ),
        preset(
            PresetId::GeneralPlainAudio,
            "General Audio, no Cookies",
            PresetSource::General,
            PresetMedia::Audio,
            false,
        ),
    ]
}

pub fn interactive_input_fields() -> [PresetInputField; 3] {
    [
        PresetInputField {
            id: "link",
            label: "Link",
            kind: PresetInputKind::Url,
            required: true,
            default_value: None,
        },
        PresetInputField {
            id: "directory",
            label: "Save directory",
            kind: PresetInputKind::Directory,
            required: false,
            default_value: Some(DEFAULT_DOWNLOAD_DIRECTORY),
        },
        PresetInputField {
            id: "cookiesPath",
            label: "Cookies file",
            kind: PresetInputKind::File,
            required: false,
            default_value: Some(COOKIE_PATH),
        },
    ]
}

pub fn command_preview(
    preset_id: PresetId,
    input: &DownloadPresetInput,
) -> Result<PresetCommandPreview, String> {
    let preset = find_preset(preset_id)?;
    let args = command_args(&preset, input)?;
    let pending_integrations = if preset.requires_pot_server {
        vec!["pot-server"]
    } else {
        Vec::new()
    };

    Ok(PresetCommandPreview {
        requires_pot_server: preset.requires_pot_server,
        preset,
        args,
        pending_integrations,
    })
}

pub const fn is_youtube_video_preset(preset_id: PresetId) -> bool {
    matches!(
        preset_id,
        PresetId::YouTubeCookiesVideo | PresetId::YouTubePlainVideo
    )
}

fn preset(
    id: PresetId,
    label: &'static str,
    source: PresetSource,
    media: PresetMedia,
    uses_cookies: bool,
) -> DownloadPreset {
    DownloadPreset {
        id,
        label,
        source,
        media,
        uses_cookies,
        requires_pot_server: matches!(source, PresetSource::YouTube),
    }
}

fn find_preset(preset_id: PresetId) -> Result<DownloadPreset, String> {
    all_presets()
        .into_iter()
        .find(|preset| preset.id == preset_id)
        .ok_or_else(|| format!("Unsupported preset: {preset_id:?}"))
}

fn command_args(
    preset: &DownloadPreset,
    input: &DownloadPresetInput,
) -> Result<Vec<String>, String> {
    let link = input.link.trim();
    if link.is_empty() {
        return Err("Link is required.".to_string());
    }

    let directory = input
        .directory
        .as_deref()
        .filter(|directory| !directory.trim().is_empty())
        .unwrap_or(DEFAULT_DOWNLOAD_DIRECTORY);
    let cookies_path = input
        .cookies_path
        .as_deref()
        .filter(|cookies_path| !cookies_path.trim().is_empty())
        .unwrap_or(COOKIE_PATH);

    let mut args = vec![
        "-v".to_string(),
        "--add-metadata".to_string(),
        "-P".to_string(),
        directory.to_string(),
        link.to_string(),
    ];

    if preset.uses_cookies {
        args.extend(["--cookies".to_string(), cookies_path.to_string()]);
    }

    args.extend(["--windows-filenames".to_string(), "--continue".to_string()]);

    match (preset.source, preset.media, preset.uses_cookies) {
        (PresetSource::YouTube, PresetMedia::Video, true) => {
            args.extend(video_args("bv*+mergeall[format_id*='251']/bv*+ba"));
            args.extend(youtube_args());
        }
        (PresetSource::YouTube, PresetMedia::Video, false) => {
            args.extend(video_args(
                "bestvideo[protocol!=m3u8]+mergeall[format_id='251'][format_id!*='drc']/bv*+ba",
            ));
            args.extend(youtube_args());
        }
        (PresetSource::YouTube, PresetMedia::Audio, _) => {
            args.extend(audio_args());
            args.extend(youtube_args());
        }
        (PresetSource::General, PresetMedia::Video, _) => {
            args.extend(video_args("bv*+mergeall/bv*+ba"));
        }
        (PresetSource::General, PresetMedia::Audio, _) => {
            args.extend(audio_args());
        }
    }

    Ok(args)
}

fn video_args(format_selector: &str) -> Vec<String> {
    vec![
        "-f".to_string(),
        format_selector.to_string(),
        "--audio-multistreams".to_string(),
        "-S".to_string(),
        "quality,vcodec:av1:vp9:h264,acodec:opus:aac".to_string(),
        "--embed-chapters".to_string(),
        "--embed-thumbnail".to_string(),
        "--convert-thumbnails".to_string(),
        "png".to_string(),
        "--embed-subs".to_string(),
        "--compat-options".to_string(),
        "no-live-chat".to_string(),
        "--concat-playlist".to_string(),
        "never".to_string(),
        "--sub-lang".to_string(),
        "all".to_string(),
        "--remux-video".to_string(),
        "mkv".to_string(),
    ]
}

fn audio_args() -> Vec<String> {
    vec![
        "-x".to_string(),
        "--embed-thumbnail".to_string(),
        "--embed-thumbnail".to_string(),
        "--concat-playlist".to_string(),
        "never".to_string(),
        "--embed-chapters".to_string(),
    ]
}

fn youtube_args() -> Vec<String> {
    vec![
        "--impersonate".to_string(),
        "chrome".to_string(),
        "--extractor-args".to_string(),
        "youtube:player-client=default,mweb,web_safari,tv,ios".to_string(),
        "--extractor-args".to_string(),
        format!("youtubepot-bgutilhttp:base_url={POT_BASE_URL}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::{DownloadPresetInput, PresetId, all_presets, command_preview};

    #[test]
    fn exposes_eight_script_combinations() {
        assert_eq!(all_presets().len(), 8);
    }

    #[test]
    fn youtube_presets_report_pending_pot_server() {
        let preview = command_preview(
            PresetId::YouTubePlainVideo,
            &DownloadPresetInput {
                link: "https://example.com/video".to_string(),
                directory: None,
                cookies_path: None,
            },
        )
        .expect("preview should be created");

        assert!(preview.requires_pot_server);
        assert!(preview.pending_integrations.contains(&"pot-server"));
    }

    #[test]
    fn rejects_empty_link() {
        let error = command_preview(
            PresetId::GeneralPlainAudio,
            &DownloadPresetInput {
                link: String::new(),
                directory: None,
                cookies_path: None,
            },
        )
        .expect_err("empty link should be rejected");

        assert_eq!(error, "Link is required.");
    }

    #[test]
    fn video_presets_remux_to_mkv_and_preserve_standard_subtitles() {
        for preset_id in [
            PresetId::YouTubeCookiesVideo,
            PresetId::YouTubePlainVideo,
            PresetId::GeneralCookiesVideo,
            PresetId::GeneralPlainVideo,
        ] {
            let preview = command_preview(
                preset_id,
                &DownloadPresetInput {
                    link: "https://example.com/video".to_string(),
                    directory: None,
                    cookies_path: None,
                },
            )
            .expect("preview should be created");

            assert!(has_option_value(&preview.args, "--remux-video", "mkv"));
            assert!(preview.args.iter().any(|arg| arg == "--embed-thumbnail"));
            assert!(has_option_value(
                &preview.args,
                "--convert-thumbnails",
                "png"
            ));
            assert!(!preview.args.iter().any(|arg| arg == "--convert-subs"));
        }
    }

    fn has_option_value(args: &[String], option: &str, value: &str) -> bool {
        args.windows(2)
            .any(|window| window[0] == option && window[1] == value)
    }
}
