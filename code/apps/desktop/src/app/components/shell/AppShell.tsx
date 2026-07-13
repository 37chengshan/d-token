import { lazy, Suspense, useEffect, useState } from "react";
import { AnimatePresence, MotionConfig } from "motion/react";
import { Sidebar } from "./Sidebar";
import { Topbar } from "./Topbar";
import { StatusBar } from "./StatusBar";
import type { PageId } from "../../lib/nav";
import { useI18n } from "../../lib/i18n";
import { PageStage } from "./PageStage";
import { NotificationBanner, NotificationLiveRegion } from "../common/NotificationCenter";

const OverviewPage = lazy(() => import("../pages/Overview").then((module) => ({ default: module.Overview })));
const InstallWorkbenchPage = lazy(() => import("../pages/InstallWorkbench").then((module) => ({ default: module.InstallWorkbench })));
const AnalyticsPage = lazy(() => import("../pages/Analytics").then((module) => ({ default: module.Analytics })));
const DoctorPage = lazy(() => import("../pages/Doctor").then((module) => ({ default: module.Doctor })));
const ChangesPage = lazy(() => import("../pages/Changes").then((module) => ({ default: module.Changes })));
const SettingsPage = lazy(() => import("../pages/Settings").then((module) => ({ default: module.Settings })));

export function AppShell() {
  const [page, setPage] = useState<PageId>("overview");
  const [collapsed, setCollapsed] = useState(() => typeof window !== "undefined" && window.innerWidth <= 980);
  const [reduceMotion, setReduceMotion] = useState(
    () => typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const narrowWindow = window.matchMedia("(max-width: 980px)");
    const collapseForNarrowWindow = (event: MediaQueryListEvent | MediaQueryList) => {
      if (event.matches) setCollapsed(true);
    };
    collapseForNarrowWindow(narrowWindow);
    narrowWindow.addEventListener("change", collapseForNarrowWindow);
    return () => narrowWindow.removeEventListener("change", collapseForNarrowWindow);
  }, []);

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const updatePreference = (event: MediaQueryListEvent | MediaQueryList) => setReduceMotion(event.matches);
    updatePreference(preference);
    preference.addEventListener("change", updatePreference);
    return () => preference.removeEventListener("change", updatePreference);
  }, []);

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
    <MotionConfig reducedMotion="user">
      <div className={`app-shell ${collapsed ? "is-sidebar-collapsed" : ""}`}>
        <Sidebar page={page} setPage={setPage} collapsed={collapsed} setCollapsed={setCollapsed} reducedMotion={reduceMotion} />
        <section className="shell-workspace">
          <Topbar page={page} collapsed={collapsed} setCollapsed={setCollapsed} reducedMotion={reduceMotion} />
          <div className="shell-notification-slot"><NotificationBanner /></div>
          <div className="shell-page-content relative flex min-w-0 flex-1 flex-col">
            <main className="shell-content-scroll min-h-0 flex-1 overflow-y-auto">
              <AnimatePresence mode="wait" initial={false}>
                <PageStage key={page} reducedMotion={reduceMotion}>
                  <Suspense fallback={<PageFallback />}>{render()}</Suspense>
                </PageStage>
              </AnimatePresence>
            </main>
            <StatusBar />
          </div>
          <NotificationLiveRegion />
        </section>
      </div>
    </MotionConfig>
  );
}

function PageFallback() {
  const { lang } = useI18n();
  return (
    <div className="shell-page-fallback flex min-h-[280px] items-center justify-center rounded-lg border border-border/40 text-sm text-muted-foreground">
      {lang === "zh" ? "正在加载页面…" : "Loading view…"}
    </div>
  );
}
