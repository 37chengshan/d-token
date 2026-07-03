import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Sidebar } from "./Sidebar";
import { Topbar } from "./Topbar";
import { StatusBar } from "./StatusBar";
import { BackgroundBlobs } from "./BackgroundBlobs";
import type { PageId } from "../../lib/nav";
import { Overview } from "../pages/Overview";
import { Install } from "../pages/Install";
import { Advanced } from "../pages/Advanced";
import { Analytics } from "../pages/Analytics";
import { Doctor } from "../pages/Doctor";
import { Changes } from "../pages/Changes";
import { LogsSecurity } from "../pages/LogsSecurity";
import { Settings } from "../pages/Settings";

export function AppShell() {
  const [page, setPage] = useState<PageId>("overview");
  const [collapsed, setCollapsed] = useState(false);

  const render = () => {
    switch (page) {
      case "overview": return <Overview setPage={setPage} />;
      case "install": return <Install />;
      case "advanced": return <Advanced />;
      case "analytics": return <Analytics />;
      case "doctor": return <Doctor setPage={setPage} />;
      case "changes": return <Changes />;
      case "logs": return <LogsSecurity />;
      case "settings": return <Settings />;
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
              {render()}
            </motion.div>
          </AnimatePresence>
        </main>
        <StatusBar />
      </div>
    </div>
  );
}
