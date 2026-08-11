import type { ReactNode } from "react";
import { AppProviders } from "./lib/theme-context";
import { AppShell } from "./components/shell/AppShell";
import { RuntimeEvidenceProvider } from "./lib/useRuntimeDashboard";
import { MockPreviewProvider } from "./lib/useRuntimeDashboard.mock";
import { NotificationProvider } from "./lib/notifications";
import { RuntimeNotificationBridge } from "./components/common/RuntimeNotificationBridge";
import { ConnectionSetupProvider } from "./lib/connectionSetup";

// DEV-only web-preview mock data layer.
//
// `useMock` is true exactly when we run as a plain web preview (not the Tauri
// shell) in a DEV build. In `vite build` this ternary flattens to `RuntimeEvidenceProvider`
// as the only live reference and Vite tree-shakes the mock module out of the shipped
// bundle. Plain static imports only — no require(), no runtime casts.
const useMock =
  import.meta.env.DEV &&
  typeof window !== "undefined" &&
  typeof (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ === "undefined";

const EvidenceRoot = useMock ? MockPreviewProvider : RuntimeEvidenceProvider;

export default function App() {
  return (
    <AppProviders>
      <EvidenceRoot>
        <ConnectionSetupProvider>
          <NotificationProvider>
            <RuntimeNotificationBridge />
            <AppShell />
          </NotificationProvider>
        </ConnectionSetupProvider>
      </EvidenceRoot>
    </AppProviders>
  );
}
