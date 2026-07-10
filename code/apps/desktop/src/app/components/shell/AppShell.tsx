import { lazy, Suspense, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Sidebar } from "./Sidebar";
import { Topbar } from "./Topbar";
import { StatusBar } from "./StatusBar";
import { BackgroundBlobs } from "./BackgroundBlobs";
import type { PageId } from "../../lib/nav";

const OverviewPage = lazy(() => import("../pages/Overview").then((module) => ({ default: module.Overview })));
const InstallWorkbenchPage = lazy(() => import("../pages/InstallWorkbench").then((module) => ({ default: module.InstallWorkbench })));
const AnalyticsPage = lazy(() => import("../pages/Analytics").then((module) => ({ default: module.Analytics })));
const DoctorPage = lazy(() => import("../pages/Doctor").then((module) => ({ default: module.Doctor })));
const ChangesPage = lazy(() => import("../pages/Changes").then((module) => ({ default: module.Changes })));
const SettingsPage = lazy(() => import("../pages/Settings").then((module) => ({ default: module.Settings })));

export function AppShell() {
  const [page, setPage] = useState<PageId>("overview");
  const [collapsed, setCollapsed] = useState(false);

  const render = () => {
    switch (page) {
      case "overview": return <OverviewPage setPage={setPage} />;
      case "install": return <InstallWorkbenchPage />;
      case "analytics": return <AnalyticsPage />;
      case "doctor": return <DoctorPage setPage={setPage} />;
      case "changes": return <ChangesPage />;
      case "settings": return <SettingsPage />;
    }
  };

  return (
    <div className="app-bg relative flex h-screen w-full overflow-hidden text-foreground">
      <BackgroundBlobs />
      <Sidebar page={page} setPage={setPage} collapsed={collapsed} setCollapsed={setCollapsed} />
      {/* GlassEffectContainer: shared backdrop context for all glass surfaces.
           Does NOT set backdrop-filter itself (would become a backdrop root). */}
      <div className="glass-stage relative z-10 flex min-w-0 flex-1 flex-col">
        <Topbar page={page} collapsed={collapsed} setCollapsed={setCollapsed} />
        <main className="min-h-0 flex-1 overflow-y-auto">
          <AnimatePresence mode="wait">
            <motion.div
              key={page}
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.2 }}
              className="mx-auto max-w-[1200px] p-6"
            >
              <Suspense fallback={<PageFallback />}>
                {render()}
              </Suspense>
            </motion.div>
          </AnimatePresence>
        </main>
        <StatusBar />
      </div>
    </div>
  );
}

function PageFallback() {
  return (
    <div className="glass-light flex min-h-[280px] items-center justify-center rounded-lg border border-border/40 text-sm text-muted-foreground">
      Loading d-token view...
    </div>
  );
}
