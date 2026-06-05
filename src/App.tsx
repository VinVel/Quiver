import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Download,
  FileText,
  FileAudio,
  FileVideo,
  Info,
  Link,
  Moon,
  Palette,
  Play,
  SunMedium,
  Terminal,
  X,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import {
  Button,
  Card,
  FeedbackMessage,
  Panel,
  TextField,
  ToolbarField,
  Toggle,
  Typography,
} from "./components/ui";
import { useTheme } from "./context/ThemeContext";
import {
  themePalettes,
  themePresetDetails,
  type ThemePresetName,
} from "./themes/colorpalette";
import "./App.css";

type PresetSource = "youTube" | "general";
type PresetMedia = "video" | "audio";

type DownloadPreset = {
  id: string;
  label: string;
  source: PresetSource;
  media: PresetMedia;
  usesCookies: boolean;
  requiresPotServer: boolean;
};

type PresetCommandPreview = {
  preset: DownloadPreset;
  args: string[];
  requiresPotServer: boolean;
  pendingIntegrations: string[];
};

type YtDlpCommandOutput = {
  exitCode?: number;
  success: boolean;
  stdout: string;
  stderr: string;
};

type YtDlpOutputChunk = {
  runId: string;
  stream: "stdout" | "stderr";
  chunk: string;
};

type DownloadPresetInput = {
  link: string;
  directory?: string;
  cookiesPath?: string;
};

const defaultDirectory = "~/Downloads";
const defaultCookiesPath = "~/Downloads/cookies.txt";
const ytDlpCommandFailedMessage =
  "Something went wrong. The download may still have succeeded, so check the logs.";
const themePresetOptions = Object.keys(themePresetDetails) as ThemePresetName[];

function App() {
  const { theme, setTheme, themePreset, setThemePreset } = useTheme();
  const [presets, setPresets] = useState<DownloadPreset[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<string>("");
  const [link, setLink] = useState("");
  const [directory, setDirectory] = useState(defaultDirectory);
  const [cookiesPath, setCookiesPath] = useState(defaultCookiesPath);
  const [preview, setPreview] = useState<PresetCommandPreview | null>(null);
  const [downloadResult, setDownloadResult] = useState<YtDlpCommandOutput | null>(
    null,
  );
  const [commandOutput, setCommandOutput] = useState("");
  const [isAdvancedSubtitlePipelineEnabled, setIsAdvancedSubtitlePipelineEnabled] =
    useState(false);
  const [error, setError] = useState<string | null>(null);
  const [licenseHtml, setLicenseHtml] = useState<string | null>(null);
  const [isLicenseOpen, setIsLicenseOpen] = useState(false);
  const [isLicenseLoading, setIsLicenseLoading] = useState(false);
  const [isPreviewLoading, setIsPreviewLoading] = useState(false);
  const [isDownloading, setIsDownloading] = useState(false);
  const hasPreparedPreviewRef = useRef(false);
  const commandOutputRef = useRef<HTMLPreElement | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadPresets() {
      try {
        const loadedPresets =
          await invoke<DownloadPreset[]>("download_presets");
        if (cancelled) {
          return;
        }

        setPresets(loadedPresets);
        setSelectedPresetId((currentPresetId) =>
          currentPresetId || loadedPresets[0]?.id || "",
        );
      } catch (loadError) {
        if (!cancelled) {
          setError(errorToMessage(loadError));
        }
      }
    }

    void loadPresets();

    return () => {
      cancelled = true;
    };
  }, []);

  const selectedPreset = useMemo(
    () => presets.find((preset) => preset.id === selectedPresetId) ?? null,
    [presets, selectedPresetId],
  );

  const youTubePresets = useMemo(
    () => presets.filter((preset) => preset.source === "youTube"),
    [presets],
  );

  const generalPresets = useMemo(
    () => presets.filter((preset) => preset.source === "general"),
    [presets],
  );

  const buildPreview = useCallback(
    async (presetId = selectedPresetId) => {
      if (!presetId) {
        setError("Choose a preset before building the command.");
        return;
      }

      setIsPreviewLoading(true);
      setError(null);
      setDownloadResult(null);
      setCommandOutput("");

      const input: DownloadPresetInput = {
        link,
        directory,
        cookiesPath,
      };

      try {
        const nextPreview = await invoke<PresetCommandPreview>(
          "preview_download_preset",
          {
            presetId,
            input,
          },
        );
        setPreview(nextPreview);
        hasPreparedPreviewRef.current = true;
      } catch (previewError) {
        setPreview(null);
        setError(errorToMessage(previewError));
      } finally {
        setIsPreviewLoading(false);
      }
    },
    [cookiesPath, directory, link, selectedPresetId],
  );

  useEffect(() => {
    if (!hasPreparedPreviewRef.current || !selectedPresetId || !link.trim()) {
      return;
    }

    void buildPreview(selectedPresetId);
  }, [buildPreview, link, selectedPresetId]);

  useEffect(() => {
    if (!commandOutputRef.current) {
      return;
    }

    commandOutputRef.current.scrollTop = commandOutputRef.current.scrollHeight;
  }, [commandOutput, isDownloading]);

  async function runDownload() {
    if (!preview) {
      await buildPreview();
      return;
    }

    setIsDownloading(true);
    setError(null);
    setDownloadResult(null);
    setCommandOutput("");

    const runId = createRunId();
    let unlisten: UnlistenFn | null = null;

    try {
      unlisten = await listen<YtDlpOutputChunk>(
        "yt-dlp-output",
        (event) => {
          if (event.payload.runId !== runId) {
            return;
          }

          setCommandOutput(
            (currentOutput) => currentOutput + event.payload.chunk,
          );
        },
      );

      const result = await invoke<YtDlpCommandOutput>("run_yt_dlp", {
        runId,
        presetId: preview.preset.id,
        link,
        args: preview.args,
        advancedSubtitlePipeline: shouldRunAdvancedSubtitlePipeline,
      });
      setDownloadResult(result);
      if (!result.success) {
        setError(ytDlpCommandFailedMessage);
      }
    } catch (downloadError) {
      setError(errorToMessage(downloadError));
    } finally {
      unlisten?.();
      setIsDownloading(false);
    }
  }

  async function handleThemePresetChange(nextPreset: ThemePresetName) {
    try {
      await setThemePreset(nextPreset);
    } catch (themeError) {
      setError(errorToMessage(themeError));
    }
  }

  async function handleThemeModeToggle() {
    try {
      await setTheme(theme === "dark" ? "light" : "dark");
    } catch (themeError) {
      setError(errorToMessage(themeError));
    }
  }

  async function openLicenses() {
    setIsLicenseOpen(true);

    if (licenseHtml) {
      return;
    }

    setIsLicenseLoading(true);
    try {
      setLicenseHtml(await invoke<string>("get_license_html"));
    } catch (licenseError) {
      setIsLicenseOpen(false);
      setError(errorToMessage(licenseError));
    } finally {
      setIsLicenseLoading(false);
    }
  }

  const commandText = preview ? ["yt-dlp", ...preview.args].join(" ") : "";
  const completedOutput = downloadResult
    ? [downloadResult.stdout, downloadResult.stderr].filter(Boolean).join("\n")
    : "";
  const outputText = commandOutput || completedOutput;
  const isActionBusy = isPreviewLoading || isDownloading;
  const primaryActionLabel = preview ? "Download" : "Prepare";
  const isAdvancedSubtitlePipelineAvailable =
    selectedPreset?.source === "youTube";
  const shouldRunAdvancedSubtitlePipeline =
    isAdvancedSubtitlePipelineAvailable && isAdvancedSubtitlePipelineEnabled;
  const licenseDocument = licenseHtml ? buildLicenseDocument(licenseHtml) : "";

  return (
    <div className="quiver-shell">
      <main className="quiver-main">
        <section className="quiver-command-row" aria-label="Download input">
          <ToolbarField
            className="quiver-url-field"
            icon={<Link aria-hidden="true" />}
            placeholder="Paste a URL and prepare a download"
            value={link}
            onChange={(event) => setLink(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                void buildPreview();
              }
            }}
          />
          <div className="quiver-command-actions">
            <Button
              variant="primary"
              className="quiver-primary-action"
              onClick={() => void (preview ? runDownload() : buildPreview())}
              disabled={isActionBusy}
            >
              {preview ? (
                <Download aria-hidden="true" />
              ) : (
                <Play aria-hidden="true" />
              )}
              {isActionBusy
                ? isDownloading
                  ? "Downloading"
                  : "Preparing"
                : primaryActionLabel}
            </Button>
          </div>
        </section>

        <div className="quiver-divider" aria-hidden="true" />

        <div className="quiver-workspace">
          <Panel className="quiver-presets" aria-label="Preset selection">
            <div className="quiver-section-heading">
              <Typography variant="h2">Presets</Typography>
              <Typography variant="bodySmall" muted>
                Choose the workflow that matches the source and media type.
              </Typography>
            </div>

            <div className="quiver-preset-grid">
              {youTubePresets.map((preset) => (
                <PresetButton
                  key={preset.id}
                  preset={preset}
                  isSelected={preset.id === selectedPresetId}
                  onSelectPreset={setSelectedPresetId}
                />
              ))}
              {youTubePresets.length > 0 && generalPresets.length > 0 ? (
                <div className="quiver-preset-divider" aria-hidden="true" />
              ) : null}
              {generalPresets.map((preset) => (
                <PresetButton
                  key={preset.id}
                  preset={preset}
                  isSelected={preset.id === selectedPresetId}
                  onSelectPreset={setSelectedPresetId}
                />
              ))}
              <div className="quiver-preset-divider" aria-hidden="true" />
              <div className="quiver-advanced-subtitles">
                <Toggle
                  checked={shouldRunAdvancedSubtitlePipeline}
                  disabled={!isAdvancedSubtitlePipelineAvailable}
                  label="Advanced Subtitle Pipeline"
                  onClick={() =>
                    setIsAdvancedSubtitlePipelineEnabled((current) => !current)
                  }
                />
                <Typography
                  className="quiver-advanced-subtitles__label"
                  variant="bodySmall"
                >
                  Advanced Subtitle Pipeline
                </Typography>
                <span
                  className="quiver-advanced-subtitles__info"
                  tabIndex={0}
                >
                  <Info aria-hidden="true" />
                  <span className="quiver-advanced-subtitles__tooltip">
                    Makes subtitles appear closer to YouTube, especially useful
                    for lyrics videos.
                  </span>
                </span>
              </div>
            </div>
          </Panel>

          <Panel className="quiver-inputs" aria-label="Download settings">
            <div className="quiver-section-heading">
              <Typography variant="h2">Inputs</Typography>
              <Typography variant="bodySmall" muted>
                These replace the interactive prompts from the script.
              </Typography>
            </div>

            <div className="quiver-field-grid">
              <TextField
                label="Save directory"
                value={directory}
                onChange={(event) => setDirectory(event.currentTarget.value)}
                helperText="Defaults to ~/Downloads."
              />
              <TextField
                label="Cookies file"
                value={cookiesPath}
                onChange={(event) => setCookiesPath(event.currentTarget.value)}
                disabled={!selectedPreset?.usesCookies}
                helperText={
                  selectedPreset?.usesCookies
                    ? "Used for cookie-enabled presets."
                    : "Disabled for this preset."
                }
              />
            </div>

            {error ? <FeedbackMessage tone="error">{error}</FeedbackMessage> : null}
            {downloadResult?.success ? (
              <FeedbackMessage tone="success">
                yt-dlp finished successfully.
              </FeedbackMessage>
            ) : null}
          </Panel>

          <Panel className="quiver-settings" aria-label="Appearance and about">
            <section className="quiver-settings-section">
              <div className="quiver-section-heading">
                <Typography variant="h2">Appearance</Typography>
                <Typography variant="bodySmall" muted>
                  Choose a color scheme and display mode.
                </Typography>
              </div>

              <div className="quiver-theme-options" aria-label="Color schemes">
                {themePresetOptions.map((presetName) => {
                  const colors = themePalettes[presetName].light;
                  const swatchStyle = {
                    "--quiver-theme-swatch": colors.primary,
                    "--quiver-theme-swatch-container": colors.primaryContainer,
                  } as CSSProperties;

                  return (
                    <button
                      key={presetName}
                      aria-pressed={themePreset === presetName}
                      className={
                        themePreset === presetName
                          ? "quiver-theme-option quiver-theme-option--active"
                          : "quiver-theme-option"
                      }
                      style={swatchStyle}
                      type="button"
                      onClick={() => void handleThemePresetChange(presetName)}
                    >
                      <span
                        className="quiver-theme-option__swatch"
                        aria-hidden="true"
                      />
                      <span className="quiver-theme-option__label">
                        {themePresetDetails[presetName].label}
                      </span>
                    </button>
                  );
                })}
              </div>

              <div className="quiver-theme-mode">
                <SunMedium aria-hidden="true" />
                <Toggle
                  checked={theme === "dark"}
                  label="Dark mode"
                  onClick={() => void handleThemeModeToggle()}
                />
                <Moon aria-hidden="true" />
              </div>
            </section>

            <div className="quiver-settings-divider" aria-hidden="true" />

            <section className="quiver-settings-section quiver-about-section">
              <div className="quiver-section-heading">
                <Typography variant="h2">About</Typography>
                <Typography variant="bodySmall" muted>
                  Quiver includes third-party open-source components.
                </Typography>
              </div>

              <Button
                className="quiver-license-button"
                variant="secondary"
                onClick={() => void openLicenses()}
              >
                <FileText aria-hidden="true" />
                Licenses
              </Button>
            </section>
          </Panel>

          <div className="quiver-preview-stack">
            <Card className="quiver-preview">
                <div className="quiver-section-heading">
                  <Typography variant="h2">Command Preview</Typography>
                  <Typography variant="bodySmall" muted>
                  Review the generated yt-dlp command before running it.
                </Typography>
              </div>

              {preview ? (
                <>
                  <pre className="quiver-command-preview">
                    <code>{commandText}</code>
                  </pre>
                </>
              ) : (
                <div className="quiver-empty-preview">
                  <Terminal aria-hidden="true" />
                  <Typography variant="bodySmall" muted>
                    Paste a URL, choose a preset, then prepare the command.
                  </Typography>
                </div>
              )}
            </Card>

            <Card className="quiver-output">
              <div className="quiver-section-heading">
                <Typography variant="h2">Command Output</Typography>
                <Typography variant="bodySmall" muted>
                  Latest yt-dlp output from the current command.
                </Typography>
              </div>

              {isDownloading && !outputText ? (
                <pre
                  ref={commandOutputRef}
                  className="quiver-command-preview quiver-command-preview--output"
                >
                  <code>Running yt-dlp...</code>
                </pre>
              ) : outputText ? (
                <pre
                  ref={commandOutputRef}
                  className="quiver-command-preview quiver-command-preview--output"
                >
                  <code>{outputText}</code>
                </pre>
              ) : (
                <div className="quiver-empty-preview quiver-empty-preview--output">
                  <Terminal aria-hidden="true" />
                  <Typography variant="bodySmall" muted>
                    Run the command to see its output here.
                  </Typography>
                </div>
              )}

            </Card>
          </div>
        </div>
      </main>

      {isLicenseOpen ? (
        <div
          className="quiver-license-overlay"
          role="dialog"
          aria-modal="true"
          aria-label="Third-party licenses"
        >
          <button
            className="quiver-license-scrim"
            type="button"
            aria-label="Close licenses"
            onClick={() => setIsLicenseOpen(false)}
          />
          <section className="quiver-license-dialog">
            <header className="quiver-license-dialog__header">
              <div>
                <Typography variant="h2">Third-party Licenses</Typography>
              </div>
              <Button
                variant="icon"
                iconOnly
                aria-label="Close licenses"
                onClick={() => setIsLicenseOpen(false)}
              >
                <X aria-hidden="true" />
              </Button>
            </header>

            {isLicenseLoading ? (
              <div className="quiver-license-loading">
                <Palette aria-hidden="true" />
                <Typography variant="bodySmall" muted>
                  Loading licenses...
                </Typography>
              </div>
            ) : (
              <iframe
                className="quiver-license-frame"
                title="Third-party licenses"
                srcDoc={licenseDocument}
                sandbox=""
              />
            )}
          </section>
        </div>
      ) : null}
    </div>
  );
}

type PresetButtonProps = {
  preset: DownloadPreset;
  isSelected: boolean;
  onSelectPreset: (presetId: string) => void;
};

function PresetButton({
  preset,
  isSelected,
  onSelectPreset,
}: PresetButtonProps) {
  return (
    <button
      className={
        isSelected ? "quiver-preset quiver-preset--active" : "quiver-preset"
      }
      type="button"
      onClick={() => onSelectPreset(preset.id)}
    >
      <span className="quiver-preset__icon" aria-hidden="true">
        {preset.media === "video" ? <FileVideo /> : <FileAudio />}
      </span>
      <span className="quiver-preset__body">
        <span className="quiver-preset__label">{preset.label}</span>
        <span className="quiver-preset__meta">{preset.media}</span>
      </span>
    </button>
  );
}

function buildLicenseDocument(html: string): string {
  const themeStyles = `
    <style>
      :root {
        color-scheme: light dark;
        font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      }

      body {
        margin: 0;
        padding: 1.25rem;
        background: transparent;
      }

      .container {
        max-width: none;
      }

      .intro {
        padding: 1rem;
        border-radius: 8px;
        background: rgba(120, 120, 120, 0.1);
      }

      a {
        text-underline-offset: 0.18em;
      }

      .licenses-overview {
        display: grid;
        gap: 0.45rem;
        padding-left: 1.25rem;
      }

      .licenses-list {
        display: grid;
        gap: 1rem;
      }

      .license {
        padding: 1rem;
        border-radius: 8px;
        background: rgba(120, 120, 120, 0.1);
      }

      .license-text {
        max-height: 18rem;
        padding: 1rem;
        border-radius: 8px;
        background: rgba(0, 0, 0, 0.08);
      }
    </style>
  `;

  return html.includes("</head>")
    ? html.replace("</head>", `${themeStyles}</head>`)
    : `${themeStyles}${html}`;
}

function createRunId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function errorToMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  return String(error);
}

export default App;
