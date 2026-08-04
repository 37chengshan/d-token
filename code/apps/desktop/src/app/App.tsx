import { AppProviders } from "./lib/theme-context";
import { AppShell } from "./components/shell/AppShell";
import { RuntimeEvidenceProvider } from "./lib/useRuntimeDashboard";
import { NotificationProvider } from "./lib/notifications";
import { RuntimeNotificationBridge } from "./components/common/RuntimeNotificationBridge";
import { ConnectionSetupProvider } from "./lib/connectionSetup";

export default function App() {
  return (
    <AppProviders>
      <RuntimeEvidenceProvider>
        <ConnectionSetupProvider>
          <NotificationProvider>
            <RuntimeNotificationBridge />
            <AppShell />
          </NotificationProvider>
        </ConnectionSetupProvider>
      </RuntimeEvidenceProvider>
    </AppProviders>
  );
}
