import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { platform } from "@tauri-apps/plugin-os";
import type {
  ThemePreferences,
  WindowController,
} from "./components";

const appWindow = getCurrentWindow();
const desktopPlatforms = new Set(["linux", "macos", "windows"]);

export function createTauriThemePreferences(): ThemePreferences {
  return {
    getThemeMode: () => invoke<string>("get_theme_mode"),
    setThemeMode: (mode) => invoke<string>("set_theme_mode", { mode }),
    getThemePreset: (supportedPresets, defaultPreset) =>
      invoke<string>("get_theme_preset", {
        supportedPresets,
        defaultPreset,
      }),
    setThemePreset: (preset, supportedPresets, defaultPreset) =>
      invoke<string>("set_theme_preset", {
        preset,
        supportedPresets,
        defaultPreset,
      }),
  };
}

export function createTauriWindowController(): WindowController {
  return {
    isDesktopPlatform: () => {
      try {
        return desktopPlatforms.has(platform());
      } catch {
        return false;
      }
    },
    startDragging: () => appWindow.startDragging(),
    toggleMaximize: () => appWindow.toggleMaximize(),
    minimize: () => appWindow.minimize(),
    close: () => appWindow.close(),
  };
}

export function reportUiError(error: unknown, errorInfo?: unknown): void {
  console.error("Quiver UI failure", error, errorInfo);
}
