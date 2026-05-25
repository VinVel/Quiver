export const sizing = {
  iconSmall: "1rem",
  iconMedium: "1.2rem",
  iconLarge: "1.4rem",
  iconButtonSize: "3.55rem",
  iconButtonCompactMinHeight: "3.4rem",
  titlebarButtonWidth: "2.75rem",
  titlebarIconSize: "1.65rem",
  brandLogoSize: "2.85rem",
  appNavLogoSize: "2.3rem",
  loginAvatarSize: "4.75rem",
  loginAvatarIconSize: "2.25rem",
  loginSignUpButtonMinWidth: "7rem",
  appRailButtonSize: "2.9rem",
  accountAvatarSize: "2.7rem",
  accountAvatarLargeSize: "3.35rem",
  roomListAvatarSize: "2.45rem",
  spaceTileIconSize: "2.9rem",
} as const;

export type SizingTokens = typeof sizing;
