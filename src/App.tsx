import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Check,
  Download,
  FileAudio,
  FileVideo,
  Globe,
  Link,
  Play,
  Server,
  Settings2,
  Terminal,
  Video,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Button,
  Card,
  FeedbackMessage,
  Panel,
  Pill,
  TextField,
  ToolbarField,
  Typography,
} from "./components/ui";
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

function App() {
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
  const [activeSource, setActiveSource] = useState<PresetSource>("youTube");
  const [error, setError] = useState<string | null>(null);
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

  const visiblePresets = useMemo(
    () => presets.filter((preset) => preset.source === activeSource),
    [activeSource, presets],
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
      });
      setDownloadResult(result);
      if (!result.success) {
        setError(result.stderr || "yt-dlp exited with an error.");
      }
    } catch (downloadError) {
      setError(errorToMessage(downloadError));
    } finally {
      unlisten?.();
      setIsDownloading(false);
    }
  }

  const commandText = preview ? ["yt-dlp", ...preview.args].join(" ") : "";
  const completedOutput = downloadResult
    ? [downloadResult.stdout, downloadResult.stderr].filter(Boolean).join("\n")
    : "";
  const outputText = commandOutput || completedOutput;
  const isActionBusy = isPreviewLoading || isDownloading;
  const primaryActionLabel = preview ? "Download" : "Prepare";

  return (
    <div className="quiver-shell">
      <main className="quiver-main">
        <section className="quiver-command-row" aria-label="Download input">
          <div className="quiver-url-panel">
            <ToolbarField
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
          </div>
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

        <section className="quiver-toolbar" aria-label="Preset filters">
          <div className="quiver-icon-group">
            <Button
              variant={activeSource === "youTube" ? "primary" : "ghost"}
              iconOnly
              aria-label="YouTube presets"
              onClick={() => selectSource("youTube")}
            >
              <Video aria-hidden="true" />
            </Button>
            <Button
              variant={activeSource === "general" ? "primary" : "ghost"}
              iconOnly
              aria-label="General presets"
              onClick={() => selectSource("general")}
            >
              <Globe aria-hidden="true" />
            </Button>
          </div>

          <div className="quiver-toolbar__spacer" />

          <Pill tone="primary">
            {selectedPreset?.requiresPotServer ? "POT server" : "No server"}
          </Pill>
          <Pill tone="secondary">
            {selectedPreset?.usesCookies ? "Cookies" : "No cookies"}
          </Pill>
          <div className="quiver-status" aria-label="System status">
            <Check aria-hidden="true" />
            <Settings2 aria-hidden="true" />
          </div>
        </section>

        <div className="quiver-workspace">
          <Panel className="quiver-presets" aria-label="Preset selection">
            <div className="quiver-section-heading">
              <Typography variant="h2">Presets</Typography>
              <Typography variant="bodySmall" muted>
                Choose the workflow that matches the source and media type.
              </Typography>
            </div>

            <div className="quiver-preset-grid">
              {visiblePresets.map((preset) => (
                <button
                  className={
                    preset.id === selectedPresetId
                      ? "quiver-preset quiver-preset--active"
                      : "quiver-preset"
                  }
                  key={preset.id}
                  type="button"
                  onClick={() => setSelectedPresetId(preset.id)}
                >
                  <span className="quiver-preset__icon" aria-hidden="true">
                    {preset.media === "video" ? <FileVideo /> : <FileAudio />}
                  </span>
                  <span className="quiver-preset__body">
                    <span className="quiver-preset__label">{preset.label}</span>
                    <span className="quiver-preset__meta">
                      {preset.source === "youTube" ? "YouTube" : "General"} ·{" "}
                      {preset.media}
                    </span>
                  </span>
                </button>
              ))}
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
                  <div className="quiver-preview__meta">
                    <Pill tone="primary">{preview.preset.label}</Pill>
                    {preview.requiresPotServer ? (
                      <Pill tone="secondary">
                        <Server aria-hidden="true" />
                        POT server
                      </Pill>
                    ) : null}
                  </div>
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

              {downloadResult ? (
                <div className="quiver-output__meta">
                  <Pill tone={downloadResult.success ? "primary" : "secondary"}>
                    {downloadResult.success ? "Success" : "Failed"}
                  </Pill>
                  <Pill tone="secondary">
                    Exit {downloadResult.exitCode ?? "unknown"}
                  </Pill>
                </div>
              ) : null}
            </Card>
          </div>
        </div>
      </main>
    </div>
  );

  function selectSource(source: PresetSource) {
    setActiveSource(source);
    setDownloadResult(null);
    setCommandOutput("");

    const nextPreset = presets.find((preset) => preset.source === source);
    if (nextPreset) {
      setSelectedPresetId(nextPreset.id);
    }
  }
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
