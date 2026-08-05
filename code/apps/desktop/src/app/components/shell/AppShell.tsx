import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import { Sidebar } from "./Sidebar";
import { Topbar } from "./Topbar";
import { StatusBar } from "./StatusBar";
import { NAV, SETTINGS_NAV, type PageId } from "../../lib/nav";
import { useI18n } from "../../lib/i18n";
import { PageStage } from "./PageStage";
import { NotificationBanner, NotificationLiveRegion } from "../common/NotificationCenter";

const OverviewPage = lazy(() => import("../pages/Overview").then((module) => ({ default: module.Overview })));
const InstallWorkbenchPage = lazy(() => import("../pages/InstallWorkbench").then((module) => ({ default: module.InstallWorkbench })));
const CodebaseMemoryPage = lazy(() => import("../pages/CodebaseMemory").then((module) => ({ default: module.CodebaseMemory })));
const AnalyticsPage = lazy(() => import("../pages/Analytics").then((module) => ({ default: module.Analytics })));
const VisualCompressionPage = lazy(() => import("../pages/VisualCompression").then((module) => ({ default: module.VisualCompression })));
const DoctorPage = lazy(() => import("../pages/Doctor").then((module) => ({ default: module.Doctor })));
const ChangesPage = lazy(() => import("../pages/Changes").then((module) => ({ default: module.Changes })));
const SettingsPage = lazy(() => import("../pages/Settings").then((module) => ({ default: module.Settings })));

const PAGE_IDS = new Set<PageId>([...NAV, SETTINGS_NAV].map((item) => item.id));
const SIDEBAR_COLLAPSED_KEY = "d-token.shell.sidebar-collapsed";
const PAGE_SESSION_KEY = "d-token.shell.page";

function readCollapsedPreference(): boolean {
  // Product default: icon rail collapsed; only an explicit "0" expands.
  if (typeof window === "undefined") return true;
  try {
    const saved = window.localStorage.getItem(SIDEBAR_COLLAPSED_KEY);
    if (saved === "1") return true;
    if (saved === "0") return false;
  } catch {
    /* session-only */
  }
  return true;
}

function readSessionPage(): PageId {
  if (typeof window === "undefined") return "overview";
  try {
    const saved = window.sessionStorage.getItem(PAGE_SESSION_KEY);
    if (saved && PAGE_IDS.has(saved as PageId)) return saved as PageId;
  } catch {
    /* ignore */
  }
  return "overview";
}

export function AppShell() {
  const [page, setPageState] = useState<PageId>(readSessionPage);
  const [collapsed, setCollapsedState] = useState(readCollapsedPreference);
  const [reduceMotion, setReduceMotion] = useState(
    () => typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );

  // Stable identities keep the sidebar and topbar out of every shell repaint.
  const setPage = useCallback((next: PageId) => {
    setPageState(next);
    try {
      window.sessionStorage.setItem(PAGE_SESSION_KEY, next);
    } catch {
      /* ignore */
    }
  }, []);

  const setCollapsed = useCallback((next: boolean) => {
    setCollapsedState(next);
    try {
      window.localStorage.setItem(SIDEBAR_COLLAPSED_KEY, next ? "1" : "0");
    } catch {
      /* ignore */
    }
  }, []);

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const narrowWindow = window.matchMedia("(max-width: 980px)");
    const collapseForNarrowWindow = (event: MediaQueryListEvent | MediaQueryList) => {
      // Only force-collapse when entering a narrow layout; do not auto-expand.
      if (event.matches) setCollapsedState(true);
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
      case "codebase-memory": return <CodebaseMemoryPage />;
      case "analytics": return <AnalyticsPage />;
      case "visual": return <VisualCompressionPage />;
      case "doctor": return <DoctorPage setPage={setPage} />;
      case "changes": return <ChangesPage setPage={setPage} />;
      case "settings": return <SettingsPage />;
    }
  };

  return (
    <div className={`app-shell ${collapsed ? "is-sidebar-collapsed" : ""}`}>
        <Sidebar page={page} setPage={setPage} collapsed={collapsed} setCollapsed={setCollapsed} reducedMotion={reduceMotion} />
        <section className="shell-workspace">
          <Topbar page={page} collapsed={collapsed} setCollapsed={setCollapsed} reducedMotion={reduceMotion} />
          <div className="shell-notification-slot"><NotificationBanner /></div>
          <div className="shell-page-content relative flex min-w-0 flex-1 flex-col">
            <main className="shell-content-scroll min-h-0 flex-1 overflow-y-auto">
              {/* Suspense sits outside the stage so the enter animation runs once,
                * on real content, instead of playing against the fallback and
                * again when the lazy chunk resolves. */}
              <Suspense fallback={<PageFallback />}>
                <PageStage key={page} reducedMotion={reduceMotion}>{render()}</PageStage>
              </Suspense>
            </main>
            <StatusBar />
          </div>
          <NotificationLiveRegion />
        </section>
    </div>
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
