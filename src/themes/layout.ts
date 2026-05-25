export const layout = {
  viewportMinHeight: "100dvh",
  centeredScreenMinHeight: "calc(100dvh - 5.5rem)",
  contentWidth: "78rem",
  contentWidthWide: "82rem",
  authenticatedShellWidth: "min(100%, 94rem)",
  formWidth: "58rem",
  compactFormWidth: "42rem",
  loginPanelWidth: "clamp(22rem, 50vw, 56rem)",
  appRailWidth: "4.75rem",
  mobileBottomBarHeight: "5.5rem",
  shellSidebarWidth: "22rem",
  compactShellSidebarWidth: "19rem",
  globalSearchWidth: "min(100%, 44rem)",
  accountCenterPopoverWidth: "min(100vw - 2.5rem, 22rem)",
  directoryCardMinWidth: "18rem",
  spaceTileMinWidth: "15rem",
  webviewHostMinHeight: "clamp(24rem, 72dvh, 52rem)",
  mobileBreakpoint: "640px",
  tabletBreakpoint: "760px",
  desktopBreakpoint: "960px",
} as const;

export type LayoutTokens = typeof layout;
