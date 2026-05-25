export const effects = {
  // Shared overlay blur for modal scrims that should soften the inactive app.
  overlayBackdropBlur: "blur(14px)",
} as const;

export type EffectsTokens = typeof effects;
