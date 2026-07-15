import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ThemeProvider, ToastProvider } from "./components";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <ToastProvider>
        <App />
      </ToastProvider>
    </ThemeProvider>
  </React.StrictMode>,
);
