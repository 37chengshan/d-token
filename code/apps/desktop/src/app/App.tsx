import { AppProviders } from "./lib/theme-context";
import { AppShell } from "./components/shell/AppShell";
import { RuntimeEvidenceProvider } from "./lib/useRuntimeDashboard";
import { NotificationProvider } from "./lib/notifications";
import { RuntimeNotificationBridge } from "./components/common/RuntimeNotificationBridge";

export default function App() {
  return (
    <AppProviders>
      <RuntimeEvidenceProvider>
        <NotificationProvider>
          <RuntimeNotificationBridge />
          <AppShell />
        </NotificationProvider>
      </RuntimeEvidenceProvider>
    </AppProviders>
  );
}
