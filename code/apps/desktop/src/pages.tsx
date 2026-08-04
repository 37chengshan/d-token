// Legacy compatibility shim.
// The active desktop UI lives under src/app/components/pages and is mounted by src/main.tsx.
// Keep this file tiny so stale imports do not resurrect the old 8787-era UI.

export { Overview } from "./app/components/pages/Overview";
export { Install } from "./app/components/pages/Install";
export { Advanced } from "./app/components/pages/Advanced";
export { Analytics } from "./app/components/pages/Analytics";
export { Doctor } from "./app/components/pages/Doctor";
export { Changes } from "./app/components/pages/Changes";
export { LogsSecurity } from "./app/components/pages/LogsSecurity";
export { Settings } from "./app/components/pages/Settings";
