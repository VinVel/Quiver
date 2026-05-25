export type ThemeMode = "light" | "dark" | "system";
export type ThemePresetName = "crystal" | "ocean" | "forest" | "sun";

type ColorSchemeName = Exclude<ThemeMode, "system">;

export const colorTokens = {
  // Primary colors
  primary: "", // Main accent used across screens and components.
  onPrimary: "", // Text and icons shown against the primary color.
  primaryContainer: "", // Standout container color for key components.
  onPrimaryContainer: "", // Contrast-passing color shown against the primary container.

  // Secondary colors
  secondary: "", // Supporting accent used across screens and components.
  onSecondary: "", // Text and icons shown against the secondary color.
  secondaryContainer: "", // Less prominent container color for components like tonal buttons.
  onSecondaryContainer: "", // Contrast-passing color shown against the secondary container.

  // Tertiary colors
  tertiary: "", // Contrasting accent used across screens and components.
  onTertiary: "", // Text and icons shown against the tertiary color.
  tertiaryContainer: "", // Contrasting container color for components like input fields.
  onTertiaryContainer: "", // Contrast-passing color shown against the tertiary container.

  // Error colors
  error: "", // Error state color for invalid input, destructive feedback, and blocking failures.
  onError: "", // Text and icons shown against the error color.
  errorContainer: "", // Container color for error messages and badges.
  onErrorContainer: "", // Text and icons shown against the error container color.

  // Surface colors
  surface: "", // Base surface color for cards, sheets, panels, and menus.
  onSurface: "", // Main text and icon color shown against the surface color.
  surfaceVariant: "", // Alternate surface color for differentiated or active states.
  onSurfaceVariant: "", // Supporting text and icon color used on surfaceVariant.
  surfaceContainerHighest: "", // Strongest elevated container surface.
  surfaceContainerHigh: "", // High-emphasis elevated container surface.
  surfaceContainer: "", // Standard elevated container surface.
  surfaceContainerLow: "", // Low-emphasis elevated container surface.
  surfaceContainerLowest: "", // Lowest elevated container surface, often near the page background.
  inverseSurface: "", // Opposite surface color used when surrounding UI needs reversed emphasis.
  inverseOnSurface: "", // Text and icons shown against the inverse surface color.
  surfaceTint: "",
  shadow: "", // Base shadow color used by elevation tokens and focus rings.

  // Outline colors
  outline: "", // Subtle boundary color for borders and dividers.
  outlineVariant: "", // Softer outline color for low-emphasis borders, containers, or separators.
} satisfies Record<string, string>;

export type ThemeColors = typeof colorTokens;

const crystalLightColors: ThemeColors = {
  primary: "#65558F",
  surfaceTint: "#65558F",
  onPrimary: "#FFFFFF",
  primaryContainer: "#E9DDFF",
  onPrimaryContainer: "#4D3D75",
  secondary: "#65558F",
  onSecondary: "#FFFFFF",
  secondaryContainer: "#E9DDFF",
  onSecondaryContainer: "#4D3D75",
  tertiary: "#8B4A61",
  onTertiary: "#FFFFFF",
  tertiaryContainer: "#FFD9E3",
  onTertiaryContainer: "#6F334A",
  error: "#BA1A1A",
  onError: "#FFFFFF",
  errorContainer: "#FFDAD6",
  onErrorContainer: "#93000A",
  surface: "#FDF7FF",
  onSurface: "#1D1B20",
  surfaceVariant: "#E7E0EB",
  onSurfaceVariant: "#49454E",
  outline: "#7A757F",
  outlineVariant: "#CAC4CF",
  shadow: "#000000",
  inverseSurface: "#322F35",
  inverseOnSurface: "#F5EFF7",
  surfaceContainerLowest: "#FFFFFF",
  surfaceContainerLow: "#F8F2FA",
  surfaceContainer: "#F2ECF4",
  surfaceContainerHigh: "#ECE6EE",
  surfaceContainerHighest: "#E6E0E9",
};

const crystalDarkColors: ThemeColors = {
  primary: "#CFBDFE",
  surfaceTint: "#CFBDFE",
  onPrimary: "#36275D",
  primaryContainer: "#4D3D75",
  onPrimaryContainer: "#E9DDFF",
  secondary: "#D0BCFE",
  onSecondary: "#36265D",
  secondaryContainer: "#4D3D75",
  onSecondaryContainer: "#E9DDFF",
  tertiary: "#FFB0CA",
  onTertiary: "#541D33",
  tertiaryContainer: "#6F334A",
  onTertiaryContainer: "#FFD9E3",
  error: "#FFB4AB",
  onError: "#690005",
  errorContainer: "#93000A",
  onErrorContainer: "#FFDAD6",
  surface: "#141218",
  onSurface: "#E6E0E9",
  surfaceVariant: "#49454E",
  onSurfaceVariant: "#CAC4CF",
  outline: "#948F99",
  outlineVariant: "#49454E",
  shadow: "#000000",
  inverseSurface: "#E6E0E9",
  inverseOnSurface: "#322F35",
  surfaceContainerLowest: "#0F0D13",
  surfaceContainerLow: "#1D1B20",
  surfaceContainer: "#211F24",
  surfaceContainerHigh: "#2B292F",
  surfaceContainerHighest: "#36343A",
};

const oceanLightColors: ThemeColors = {
  primary: "#415F91",
  surfaceTint: "#415F91",
  onPrimary: "#FFFFFF",
  primaryContainer: "#D6E3FF",
  onPrimaryContainer: "#284777",
  secondary: "#565F71",
  onSecondary: "#FFFFFF",
  secondaryContainer: "#DAE2F9",
  onSecondaryContainer: "#3E4759",
  tertiary: "#705575",
  onTertiary: "#FFFFFF",
  tertiaryContainer: "#FAD8FD",
  onTertiaryContainer: "#573E5C",
  error: "#BA1A1A",
  onError: "#FFFFFF",
  errorContainer: "#FFDAD6",
  onErrorContainer: "#93000A",
  surface: "#F9F9FF",
  onSurface: "#191C20",
  surfaceVariant: "#E0E2EC",
  onSurfaceVariant: "#44474E",
  outline: "#74777F",
  outlineVariant: "#C4C6D0",
  shadow: "#000000",
  inverseSurface: "#2E3036",
  inverseOnSurface: "#F0F0F7",
  surfaceContainerLowest: "#FFFFFF",
  surfaceContainerLow: "#F3F3FA",
  surfaceContainer: "#EDEDF4",
  surfaceContainerHigh: "#E7E8EE",
  surfaceContainerHighest: "#E2E2E9",
};

const oceanDarkColors: ThemeColors = {
  primary: "#AAC7FF",
  surfaceTint: "#AAC7FF",
  onPrimary: "#0A305F",
  primaryContainer: "#284777",
  onPrimaryContainer: "#D6E3FF",
  secondary: "#BEC6DC",
  onSecondary: "#283141",
  secondaryContainer: "#3E4759",
  onSecondaryContainer: "#DAE2F9",
  tertiary: "#DDBCE0",
  onTertiary: "#3F2844",
  tertiaryContainer: "#573E5C",
  onTertiaryContainer: "#FAD8FD",
  error: "#FFB4AB",
  onError: "#690005",
  errorContainer: "#93000A",
  onErrorContainer: "#FFDAD6",
  surface: "#111318",
  onSurface: "#E2E2E9",
  surfaceVariant: "#44474E",
  onSurfaceVariant: "#C4C6D0",
  outline: "#8E9099",
  outlineVariant: "#44474E",
  shadow: "#000000",
  inverseSurface: "#E2E2E9",
  inverseOnSurface: "#2E3036",
  surfaceContainerLowest: "#0C0E13",
  surfaceContainerLow: "#191C20",
  surfaceContainer: "#1D2024",
  surfaceContainerHigh: "#282A2F",
  surfaceContainerHighest: "#33353A",
};

const forestLightColors: ThemeColors = {
  primary: "#4C662B",
  surfaceTint: "#4C662B",
  onPrimary: "#FFFFFF",
  primaryContainer: "#CDEDA3",
  onPrimaryContainer: "#354E16",
  secondary: "#586249",
  onSecondary: "#FFFFFF",
  secondaryContainer: "#DCE7C8",
  onSecondaryContainer: "#404A33",
  tertiary: "#386663",
  onTertiary: "#FFFFFF",
  tertiaryContainer: "#BCECE7",
  onTertiaryContainer: "#1F4E4B",
  error: "#BA1A1A",
  onError: "#FFFFFF",
  errorContainer: "#FFDAD6",
  onErrorContainer: "#93000A",
  surface: "#F9FAEF",
  onSurface: "#1A1C16",
  surfaceVariant: "#E1E4D5",
  onSurfaceVariant: "#44483D",
  outline: "#75796C",
  outlineVariant: "#C5C8BA",
  shadow: "#000000",
  inverseSurface: "#2F312A",
  inverseOnSurface: "#F1F2E6",
  surfaceContainerLowest: "#FFFFFF",
  surfaceContainerLow: "#F3F4E9",
  surfaceContainer: "#EEEFE3",
  surfaceContainerHigh: "#E8E9DE",
  surfaceContainerHighest: "#E2E3D8",
};

const forestDarkColors: ThemeColors = {
  primary: "#B1D18A",
  surfaceTint: "#B1D18A",
  onPrimary: "#1F3701",
  primaryContainer: "#354E16",
  onPrimaryContainer: "#CDEDA3",
  secondary: "#BFCBAD",
  onSecondary: "#2A331E",
  secondaryContainer: "#404A33",
  onSecondaryContainer: "#DCE7C8",
  tertiary: "#A0D0CB",
  onTertiary: "#003735",
  tertiaryContainer: "#1F4E4B",
  onTertiaryContainer: "#BCECE7",
  error: "#FFB4AB",
  onError: "#690005",
  errorContainer: "#93000A",
  onErrorContainer: "#FFDAD6",
  surface: "#12140E",
  onSurface: "#E2E3D8",
  surfaceVariant: "#44483D",
  onSurfaceVariant: "#C5C8BA",
  outline: "#8F9285",
  outlineVariant: "#44483D",
  shadow: "#000000",
  inverseSurface: "#E2E3D8",
  inverseOnSurface: "#2F312A",
  surfaceContainerLowest: "#0C0F09",
  surfaceContainerLow: "#1A1C16",
  surfaceContainer: "#1E201A",
  surfaceContainerHigh: "#282B24",
  surfaceContainerHighest: "#33362E",
};

const sunLightColors: ThemeColors = {
  primary: "#6D5E0F",
  surfaceTint: "#6D5E0F",
  onPrimary: "#FFFFFF",
  primaryContainer: "#F8E287",
  onPrimaryContainer: "#534600",
  secondary: "#665E40",
  onSecondary: "#FFFFFF",
  secondaryContainer: "#EEE2BC",
  onSecondaryContainer: "#4E472A",
  tertiary: "#43664E",
  onTertiary: "#FFFFFF",
  tertiaryContainer: "#C5ECCE",
  onTertiaryContainer: "#2C4E38",
  error: "#BA1A1A",
  onError: "#FFFFFF",
  errorContainer: "#FFDAD6",
  onErrorContainer: "#93000A",
  surface: "#FFF9EE",
  onSurface: "#1E1B13",
  surfaceVariant: "#EAE2D0",
  onSurfaceVariant: "#4B4739",
  outline: "#7C7767",
  outlineVariant: "#CDC6B4",
  shadow: "#000000",
  inverseSurface: "#333027",
  inverseOnSurface: "#F7F0E2",
  surfaceContainerLowest: "#FFFFFF",
  surfaceContainerLow: "#FAF3E5",
  surfaceContainer: "#F4EDDF",
  surfaceContainerHigh: "#EEE8DA",
  surfaceContainerHighest: "#E8E2D4",
};

const sunDarkColors: ThemeColors = {
  primary: "#DBC66E",
  surfaceTint: "#DBC66E",
  onPrimary: "#3A3000",
  primaryContainer: "#534600",
  onPrimaryContainer: "#F8E287",
  secondary: "#D1C6A1",
  onSecondary: "#363016",
  secondaryContainer: "#4E472A",
  onSecondaryContainer: "#EEE2BC",
  tertiary: "#A9D0B3",
  onTertiary: "#143723",
  tertiaryContainer: "#2C4E38",
  onTertiaryContainer: "#C5ECCE",
  error: "#FFB4AB",
  onError: "#690005",
  errorContainer: "#93000A",
  onErrorContainer: "#FFDAD6",
  surface: "#15130B",
  onSurface: "#E8E2D4",
  surfaceVariant: "#4B4739",
  onSurfaceVariant: "#CDC6B4",
  outline: "#969080",
  outlineVariant: "#4B4739",
  shadow: "#000000",
  inverseSurface: "#E8E2D4",
  inverseOnSurface: "#333027",
  surfaceContainerLowest: "#100E07",
  surfaceContainerLow: "#1E1B13",
  surfaceContainer: "#222017",
  surfaceContainerHigh: "#2D2A21",
  surfaceContainerHighest: "#38352B",
};

type ThemePalette = Record<ColorSchemeName, ThemeColors>;

export const themePresetDetails: Record<ThemePresetName, { label: string }> = {
  crystal: { label: "Crystal" },
  ocean: { label: "Ocean" },
  forest: { label: "Forest" },
  sun: { label: "Sun" },
};

// Keep the startup palette explicit so apps and tests have one stable fallback.
export const DEFAULT_THEME_PRESET: ThemePresetName = "crystal";

// Every preset owns both light and dark branches so mode switching stays orthogonal.
export const themePalettes: Record<ThemePresetName, ThemePalette> = {
  crystal: {
    light: crystalLightColors,
    dark: crystalDarkColors,
  },
  ocean: {
    light: oceanLightColors,
    dark: oceanDarkColors,
  },
  forest: {
    light: forestLightColors,
    dark: forestDarkColors,
  },
  sun: {
    light: sunLightColors,
    dark: sunDarkColors,
  },
};

export function isThemePresetName(value: string): value is ThemePresetName {
  return Object.prototype.hasOwnProperty.call(themePalettes, value);
}
