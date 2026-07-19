import { createRoot } from "react-dom/client";
import App from "./app/App";
import { initializePerformanceCapture, markPerformancePhase } from "./app/lib/performance";
import "./styles/index.css";

initializePerformanceCapture();

const rootEl = document.getElementById("root");
if (!rootEl) throw new Error("Root element #root not found");

createRoot(rootEl).render(<App />);
requestAnimationFrame(() => {
  requestAnimationFrame(() => markPerformancePhase("app_boot", "shell_interactive"));
});
