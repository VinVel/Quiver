export const elevation = {
  outlineSubtle:
    "inset 0 0 0 1px color-mix(in srgb, var(--outline-variant) 74%, transparent)",
  outlineStrong:
    "inset 0 0 0 1px color-mix(in srgb, var(--outline) 52%, transparent)",
  surfaceRaised:
    "0 12px 24px color-mix(in srgb, var(--shadow) 8%, transparent)",
  panelRaised: "0 28px 72px color-mix(in srgb, var(--shadow) 10%, transparent)",
  primaryAction:
    "0 18px 32px color-mix(in srgb, var(--primary) 22%, transparent)",
  primaryActionHover:
    "0 24px 40px color-mix(in srgb, var(--primary) 28%, transparent)",
  secondaryAction:
    "0 12px 24px color-mix(in srgb, var(--shadow) 8%, transparent), inset 0 0 0 1px color-mix(in srgb, var(--outline) 45%, transparent)",
  fieldInvalid:
    "inset 0 0 0 1px var(--error), 0 12px 24px color-mix(in srgb, var(--shadow) 8%, transparent)",
  fieldFocus:
    "inset 0 0 0 1px var(--primary), 0 0 0 4px color-mix(in srgb, var(--primary) 18%, transparent), 0 12px 24px color-mix(in srgb, var(--shadow) 8%, transparent)",
  avatarRaised:
    "0 16px 32px color-mix(in srgb, var(--shadow) 10%, transparent), inset 0 0 0 1px color-mix(in srgb, var(--outline) 46%, transparent)",
  interactivePrimaryOutline:
    "inset 0 0 0 1px color-mix(in srgb, var(--primary) 42%, transparent)",
  interactivePrimaryOutlineActive:
    "inset 0 0 0 1px color-mix(in srgb, var(--primary) 38%, transparent)",
  serverCardHover:
    "inset 0 0 0 1px color-mix(in srgb, var(--primary) 40%, transparent), 0 18px 36px color-mix(in srgb, var(--shadow) 10%, transparent)",
  webviewHostRaised:
    "inset 0 0 0 1px color-mix(in srgb, var(--outline-variant) 82%, transparent), 0 18px 36px color-mix(in srgb, var(--shadow) 8%, transparent)",
} as const;

export type ElevationTokens = typeof elevation;
