import {
  createContext,
  useContext,
  useState,
  useEffect,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { primitives, type ThemePrimitives } from "../themes/primitives";
import {
  DEFAULT_THEME_PRESET,
  isThemePresetName,
  themePalettes,
  type ThemeColors,
  type ThemeMode,
  type ThemePresetName,
} from "../themes/colorpalette";
import { useColorScheme } from "../hooks/useColorScheme";

interface ThemeContextType {
  theme: ThemeMode;
  setTheme: (mode: ThemeMode) => Promise<void>;
  themePreset: ThemePresetName;
  setThemePreset: (preset: ThemePresetName) => Promise<void>;
  colors: ThemeColors;
  primitives: ThemePrimitives;
}

const ThemeContext = createContext<ThemeContextType | undefined>(undefined);
const supportedThemePresets = Object.keys(themePalettes) as ThemePresetName[];

function toCssVariableName(token: string): string {
  return token.replace(/[A-Z]/g, (match) => `-${match.toLowerCase()}`);
}

function exposeTokenGroup(
  root: HTMLElement,
  prefix: string,
  tokens: Record<string, string>,
) {
  for (const [token, value] of Object.entries(tokens)) {
    root.style.setProperty(`--${prefix}-${toCssVariableName(token)}`, value);
  }
}

export const ThemeProvider = ({ children }: { children: ReactNode }) => {
  const [theme, setTheme] = useState<ThemeMode>("system");
  const [themePreset, setThemePresetState] =
    useState<ThemePresetName>(DEFAULT_THEME_PRESET);
  const systemPrefersDark = useColorScheme() === "dark";
  const isDark = theme === "system" ? systemPrefersDark : theme === "dark";
  const currentColors = themePalettes[themePreset][isDark ? "dark" : "light"];

  useEffect(() => {
    let cancelled = false;

    async function loadThemePreferences() {
      try {
        const [savedMode, savedPreset] = await Promise.all([
          invoke<string>("get_theme_mode"),
          invoke<string>("get_theme_preset", {
            supportedPresets: supportedThemePresets,
            defaultPreset: DEFAULT_THEME_PRESET,
          }),
        ]);

        if (
          !cancelled &&
          (savedMode === "system" ||
            savedMode === "light" ||
            savedMode === "dark")
        ) {
          setTheme(savedMode);
        }

        if (!cancelled && isThemePresetName(savedPreset)) {
          setThemePresetState(savedPreset);
        }
      } catch (error) {
        console.error("Failed to load saved theme preferences.", error);
      }
    }

    void loadThemePreferences();

    return () => {
      cancelled = true;
    };
  }, [supportedThemePresets]);

  async function updateThemeMode(nextTheme: ThemeMode) {
    const previousTheme = theme;
    setTheme(nextTheme);

    try {
      const savedTheme = await invoke<string>("set_theme_mode", {
        mode: nextTheme,
      });

      if (
        savedTheme !== "system" &&
        savedTheme !== "light" &&
        savedTheme !== "dark"
      ) {
        throw new Error(
          `Unsupported theme mode returned from native settings: ${savedTheme}`,
        );
      }

      setTheme(savedTheme);
    } catch (error) {
      setTheme(previousTheme);
      throw error;
    }
  }

  async function setThemePreset(nextPreset: ThemePresetName) {
    const previousPreset = themePreset;
    setThemePresetState(nextPreset);

    try {
      const savedPreset = await invoke<string>("set_theme_preset", {
        preset: nextPreset,
        supportedPresets: supportedThemePresets,
        defaultPreset: DEFAULT_THEME_PRESET,
      });

      if (!isThemePresetName(savedPreset)) {
        throw new Error(
          `Unsupported theme preset returned from native settings: ${savedPreset}`,
        );
      }

      setThemePresetState(savedPreset);
    } catch (error) {
      setThemePresetState(previousPreset);
      throw error;
    }
  }

  // Apply active theme and expose palette and primitive values as CSS variables.
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = isDark ? "dark" : "light";
    root.dataset.themePreset = themePreset;

    for (const [token, value] of Object.entries(currentColors)) {
      const cssToken = toCssVariableName(token);
      root.style.setProperty(`--${cssToken}`, value);
    }

    exposeTokenGroup(root, "typography", primitives.typography);
    exposeTokenGroup(root, "spacing", primitives.spacing);
    exposeTokenGroup(root, "sizing", primitives.sizing);
    exposeTokenGroup(root, "shape", primitives.shape);
    exposeTokenGroup(root, "elevation", primitives.elevation);
    exposeTokenGroup(root, "effects", primitives.effects);
    exposeTokenGroup(root, "motion", primitives.motion);
    exposeTokenGroup(root, "layout", primitives.layout);
  }, [isDark, currentColors, themePreset]);

  return (
    <ThemeContext.Provider
      value={{
        theme,
        setTheme: updateThemeMode,
        themePreset,
        setThemePreset,
        colors: currentColors,
        primitives,
      }}
    >
      {children}
    </ThemeContext.Provider>
  );
};

export const useTheme = () => {
  const context = useContext(ThemeContext);
  if (!context) throw new Error("useTheme must be used within ThemeProvider");
  return context;
};
