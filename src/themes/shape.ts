export const shape = {
  borderWidthThin: "1px",
  radiusExtraSmall: "4px",
  radiusSmall: "8px",
  radiusMedium: "12px",
  radiusLarge: "16px",
  radiusRound: "999px",
  radiusCircle: "50%",
} as const;

export type ShapeTokens = typeof shape;
