import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import { Sidebar } from "./Sidebar";
import { Topbar } from "./Topbar";
import { StatusBar } from "./StatusBar";
import { migratePageId, type PageId } from "../../lib/nav";
import { useI18n, translateCopy } from "../../lib/i18n";
import { PageStage } from "./PageStage";
import { NotificationBanner, NotificationLiveRegion } from "../common/NotificationCenter";
import { useNotifications } from "../../lib/notifications";
import { useOnboardingGate } from "../../lib/onboarding";
import { invokeNative, type NativeActionResult } from "../../lib/native";
import { useConnectionSetup } from "../../lib/connectionSetup";
import { useRuntimeDashboardSelector } from "../../lib/useRuntimeDashboard";
import { desktopInstallRoot } from "../../lib/install/presentation";
import type { OnboardingServiceStartResult } from "../pages/Onboarding";

const OnboardingPage = lazy(() => import("../pages/Onboarding").then((module) => ({ default: module.OnboardingPage })));
const ConfigurationPage = lazy(() => import("../pages/Configuration").then((module) => ({ default: module.Configuration })));
const AdvancedConfigurationPage = lazy(() => import("../pages/AdvancedConfiguration").then((module) => ({ default: module.AdvancedConfiguration })));
const AnalysisLogsPage = lazy(() => import("../pages/AnalysisLogs").then((module) => ({ default: module.AnalysisLogs })));
const DiagnosticsPage = lazy(() => import("../pages/Diagnostics").then((module) => ({ default: module.Diagnostics })));
const SettingsPage = lazy(() => import("../pages/Settings").then((module) => ({ default: module.Settings })));

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
  if (typeof window === "undefined") return "configuration";
  try {
    const saved = window.sessionStorage.getItem(PAGE_SESSION_KEY);
    const migrated = migratePageId(saved);
    // Persist a migrated legacy/deep-link value once so subsequent launches
    // enter the shell with a current PageId and no repeated translation.
    if (saved !== null && saved !== migrated) {
      try {
        window.sessionStorage.setItem(PAGE_SESSION_KEY, migrated);
      } catch {
        // A read-only store must not discard the restored destination.
      }
    }
    return migrated;
  } catch {
    /* ignore */
  }
  return "configuration";
}

export function AppShell() {
  const [page, setPageState] = useState<PageId>(readSessionPage);
  const [collapsed, setCollapsedState] = useState(readCollapsedPreference);
  const [reduceMotion, setReduceMotion] = useState(
    () => typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  // First-run onboarding gate: the persisted first-config marker decides
  // whether the wizard opens on launch. The top-right entry re-opens it at
  // any time; "later" only dismisses it for this session and never persists
  // a false completion.
  const gate = useOnboardingGate();
  const { lang } = useI18n();
  const { addNotification } = useNotifications();
  const { refresh: refreshDiscovery } = useConnectionSetup();
  const runtimeRefresh = useRuntimeDashboardSelector((state) => state.refresh, Object.is);
  const [onboardingOpen, setOnboardingOpen] = useState(false);
  const [onboardingDismissed, setOnboardingDismissed] = useState(false);

  const openOnboarding = useCallback(() => setOnboardingOpen(true), []);
  const closeOnboarding = useCallback(() => setOnboardingOpen(false), []);

  // Stable identities keep the sidebar and topbar out of every shell repaint.
  const setPage = useCallback((next: PageId) => {
    setPageState(next);
    try {
      window.sessionStorage.setItem(PAGE_SESSION_KEY, next);
    } catch {
      /* ignore */
    }
  }, []);

  // Navigating while the wizard is forced counts as an explicit "later" for
  // this session: the gate re-enters on next launch until the marker is set.
  const handleSetPage = useCallback(
    (next: PageId) => {
      setPage(next);
      closeOnboarding();
      if (gate.state === "pending") setOnboardingDismissed(true);
    },
    [closeOnboarding, gate.state, setPage],
  );

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

  // The wizard is forced on first open until the first-config marker is set;
  // once dismissed or completed, it only reopens through the top-right entry.
  const showOnboarding = gate.state === "pending"
    ? onboardingOpen || !onboardingDismissed
    : onboardingOpen;

  const handleCompleteOnboarding = useCallback(async (): Promise<boolean> => {
    const ok = await gate.complete();
    if (ok) {
      closeOnboarding();
      setPage("configuration");
    } else {
      // Persistence failures surface as a top notification (PRD P0.2.1 #7);
      // the wizard stays open so the user can retry. No configuration was
      // modified by the failed marker write.
      addNotification({
        dedupeKey: "onboarding-complete-failed",
        level: "error",
        title: translateCopy("operation.failureTitle", lang, { operation: translateCopy("onb.title", lang) }),
        message: translateCopy("onb.completeFailed", lang),
      });
    }
    return ok;
  }, [addNotification, closeOnboarding, gate, lang, setPage]);

  const handleDismissOnboarding = useCallback(() => {
    closeOnboarding();
    setOnboardingDismissed(true);
  }, [closeOnboarding]);

  // The wizard hands the whole automatic batch to Configuration: no Agent
  // preselection, no per-Agent choice carried over from onboarding.
  const handleStartSetup = useCallback(() => {
    closeOnboarding();
    setOnboardingDismissed(true);
    setPage("configuration");
  }, [closeOnboarding, setPage]);

  const handleOpenOverview = useCallback(() => {
    closeOnboarding();
    setPage("configuration");
  }, [closeOnboarding, setPage]);

  // The shell owns the real service-start transaction (target dtoken on
  // 127.0.0.1:18001) so the wizard never re-implements install business.
  // Success reports both the native receipt and the post-transaction
  // discovery/runtime refresh evidence; either missing ⇒ failure.
  const handleStartService = useCallback(async (): Promise<OnboardingServiceStartResult> => {
    try {
      const result = await invokeNative<NativeActionResult>("run_install_transaction", {
        request: {
          target: "dtoken",
          install_root: desktopInstallRoot(typeof navigator === "undefined" ? "" : navigator.userAgent),
          proxy_port: 18001,
          auto_backup: true,
          run_doctor_after: true,
          rollback_point: true,
          mode: "connect",
        },
      });
      if (!result?.ok) {
        return { ok: false, summary: result?.summary || "install_transaction_failed", refreshed: false };
      }
      const [discovery] = await Promise.all([refreshDiscovery(), runtimeRefresh()]);
      return { ok: true, summary: result.summary, refreshed: discovery !== null };
    } catch (reason) {
      return { ok: false, summary: reason instanceof Error ? reason.message : String(reason), refreshed: false };
    }
  }, [refreshDiscovery, runtimeRefresh]);

  const render = () => {
    switch (page) {
      case "configuration": return <ConfigurationPage setPage={handleSetPage} />;
      case "advanced-configuration": return <AdvancedConfigurationPage setPage={handleSetPage} />;
      case "analysis-logs": return <AnalysisLogsPage setPage={handleSetPage} />;
      case "diagnostics": return <DiagnosticsPage setPage={handleSetPage} />;
      case "settings": return <SettingsPage />;
    }
  };

  return (
    <div className={`app-shell ${collapsed ? "is-sidebar-collapsed" : ""}`}>
        <Sidebar page={page} setPage={handleSetPage} collapsed={collapsed} setCollapsed={setCollapsed} reducedMotion={reduceMotion} />
        <section className="shell-workspace">
          <Topbar page={page} collapsed={collapsed} setCollapsed={setCollapsed} reducedMotion={reduceMotion} onOpenOnboarding={openOnboarding} onboardingActive={showOnboarding} />
          <div className="shell-notification-slot"><NotificationBanner /></div>
          <div className="shell-page-content relative flex min-w-0 flex-1 flex-col">
            <main className="shell-content-scroll min-h-0 flex-1 overflow-y-auto">
              {gate.state === "checking" ? (
                <PageFallback />
              ) : showOnboarding ? (
                <Suspense fallback={<PageFallback />}>
                  <PageStage key="onboarding" reducedMotion={reduceMotion}>
                    <OnboardingPage
                      gateState={gate.state}
                      reducedMotion={reduceMotion}
                      onComplete={handleCompleteOnboarding}
                      onDismiss={handleDismissOnboarding}
                      onStartSetup={handleStartSetup}
                      onOpenOverview={handleOpenOverview}
                      onStartService={handleStartService}
                    />
                  </PageStage>
                </Suspense>
              ) : (
                // Suspense sits outside the stage so the enter animation runs once,
                // on real content, instead of playing against the fallback and
                // again when the lazy chunk resolves.
                <Suspense fallback={<PageFallback />}>
                  <PageStage key={page} reducedMotion={reduceMotion}>{render()}</PageStage>
                </Suspense>
              )}
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
