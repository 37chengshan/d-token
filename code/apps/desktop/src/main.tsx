import { createRoot } from "react-dom/client";
import App from "./app/App";
import { initializePerformanceCapture, markPerformancePhase } from "./app/lib/performance";
import "./styles/index.css";

initializePerformanceCapture();

const rootEl = document.getElementById("root");
if (!rootEl) throw new Error("Root element #root not found");

const root = createRoot(rootEl);
const isTraySurface = new URLSearchParams(window.location.search).get("surface") === "tray";

if (isTraySurface) {
  // The tray panel runs in its own window and needs none of the control plane,
  // so it loads on demand. The control plane itself stays a static import:
  // splitting it only moved bytes off the initial chunk into extra round trips
  // without making the shell usable any sooner.
  document.documentElement.dataset.surface = "tray";
  void import("./app/TrayPanelApp").then(({ default: TrayPanelApp }) => {
    root.render(<TrayPanelApp />);
    requestAnimationFrame(() => {
      requestAnimationFrame(() => markPerformancePhase("tray_panel", "interactive"));
    });
  });
} else {
  root.render(<App />);
  requestAnimationFrame(() => {
    requestAnimationFrame(() => markPerformancePhase("app_boot", "shell_interactive"));
  });
}
