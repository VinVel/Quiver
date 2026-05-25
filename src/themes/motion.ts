export const motion = {
  durationFast: "0.16s",
  durationBase: "0.2s",
  easingStandard: "ease",
  nudgeUpSmall: "translateY(-1px)",
  nudgeUpMedium: "translateY(-2px)",
  nudgeInlineSmall: "translateX(2px)",
} as const;

export type MotionTokens = typeof motion;
