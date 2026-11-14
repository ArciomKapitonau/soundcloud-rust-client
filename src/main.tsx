import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { AppProvider } from "./lib/store";
import { bootstrapTheme, ThemeProvider } from "./lib/theme";
import "./index.css";

// No browser context menu in a desktop app (keep it for inputs).
document.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (t.tagName !== "INPUT" && t.tagName !== "TEXTAREA") e.preventDefault();
});

bootstrapTheme().then((theme) => {
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <ThemeProvider initial={theme}>
        <AppProvider>
          <App />
        </AppProvider>
      </ThemeProvider>
    </React.StrictMode>,
  );
});
