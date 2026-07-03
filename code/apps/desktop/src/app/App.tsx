import { AppProviders } from "./lib/theme-context";
import { AppShell } from "./components/shell/AppShell";

export default function App() {
  return (
    <AppProviders>
      <AppShell />
    </AppProviders>
  );
}
