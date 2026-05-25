export const typography = {
  fontFamilyBase: '"Aptos", "Segoe UI", "Noto Sans", sans-serif',
  // Code blocks need a stable monospace stack for Markdown rendering.
  fontFamilyCode: '"Cascadia Mono", "Consolas", "SFMono-Regular", monospace',
  fontWeightRegular: "400",
  fontWeightMedium: "600",
  fontWeightBold: "700",
  lineHeightTight: "1.12",
  lineHeightBase: "1.5",
  lineHeightRelaxed: "1.65",
  h1Size: "clamp(1.9rem, 4vw, 2.4rem)",
  h2Size: "clamp(1.7rem, 3.4vw, 2.2rem)",
  h3Size: "1rem",
  bodySize: "1rem",
  bodySmallSize: "0.9rem",
  metaSize: "0.86rem",
  labelSize: "0.74rem",
  eyebrowSize: "0.8rem",
  brandNameSize: "1.05rem",
  pillSize: "0.68rem",
  letterSpacingLabel: "0.12em",
  letterSpacingEyebrow: "0.14em",
  letterSpacingDetailLabel: "0.08em",
} as const;

export type TypographyTokens = typeof typography;
