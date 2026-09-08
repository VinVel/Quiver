import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import {
  AppErrorBoundary,
  AppWindowFrame,
  ThemeProvider,
  ToastProvider,
} from "./components";
import {
  createTauriThemePreferences,
  createTauriWindowController,
  reportUiError,
} from "./tauriUiAdapters";

const themePreferences = createTauriThemePreferences();
const windowController = createTauriWindowController();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppErrorBoundary onError={reportUiError}>
      <ThemeProvider
        onStorageError={(error) => reportUiError(error, "theme storage")}
        preferences={themePreferences}
      >
        <ToastProvider>
          <AppWindowFrame windowController={windowController}>
            <App />
          </AppWindowFrame>
        </ToastProvider>
      </ThemeProvider>
    </AppErrorBoundary>
  </React.StrictMode>,
);
