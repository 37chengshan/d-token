import { AppProviders } from "./lib/theme-context";
import { AppShell } from "./components/shell/AppShell";
import { RuntimeEvidenceProvider } from "./lib/useRuntimeDashboard";

export default function App() {
  return (
    <AppProviders>
      <RuntimeEvidenceProvider>
        <AppShell />
      </RuntimeEvidenceProvider>
    </AppProviders>
  );
}
