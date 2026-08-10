import { useState } from "react";
import { useT } from "../../lib/i18n";
import { NAV, SETTINGS_NAV, type PageId } from "../../lib/nav";
import { AnimatedNavIcon } from "./AnimatedNavIcon";
import { NotificationCenter } from "../common/NotificationCenter";
import { Wand2 } from "lucide-react";

export function Topbar({
  page = "overview",
  collapsed,
  setCollapsed,
  reducedMotion = false,
  onOpenOnboarding,
  onboardingActive = false,
}: {
  page?: PageId;
  collapsed: boolean;
  setCollapsed: (c: boolean) => void;
  reducedMotion?: boolean;
  /** Opens the repeatable first-run setup entry (never displaces NotificationCenter). */
  onOpenOnboarding?: () => void;
  onboardingActive?: boolean;
}) {
  const t = useT();
  const current = [...NAV, SETTINGS_NAV].find((item) => item.id === page) ?? NAV[0];
  const [engaged, setEngaged] = useState(false);
  const [motionToken, setMotionToken] = useState(0);

  return (
    <header
      className="shell-topbar relative z-30 flex h-12 min-w-0 shrink-0 items-center gap-2 px-4"
      data-tauri-drag-region
    >
      <button
        type="button"
        onClick={() => {
          setMotionToken((token) => token + 1);
          setCollapsed(!collapsed);
        }}
        onPointerEnter={() => setEngaged(true)}
        onPointerLeave={() => setEngaged(false)}
        onFocus={() => setEngaged(true)}
        onBlur={() => setEngaged(false)}
        aria-label={collapsed ? t("app.expandSidebar") : t("app.collapseSidebar")}
        title={collapsed ? t("app.expandSidebar") : t("app.collapseSidebar")}
        data-reduced-motion={reducedMotion ? "true" : "false"}
        className="shell-window-control nav-motion-item flex size-8 shrink-0 items-center justify-center rounded-lg text-muted-foreground hover:bg-[var(--chip-hover-bg)] hover:text-foreground"
      >
        <AnimatedNavIcon
          id="collapse"
          active={false}
          reducedMotion={reducedMotion}
          expanded={!collapsed}
          engaged={engaged}
          motionToken={motionToken}
          className="size-4"
        />
      </button>
      <span className="shell-topbar-separator" aria-hidden="true" />
      {collapsed && (
        <span className="shell-topbar-context truncate text-[12px] font-medium text-muted-foreground" data-tauri-drag-region>
          {t(current.labelKey)}
        </span>
      )}
      <span className="sr-only">{t("app.name")}</span>
      <div className="ml-auto flex items-center gap-2">
        {onOpenOnboarding && (
          <button
            type="button"
            onClick={onOpenOnboarding}
            aria-label={t("onb.entryTitle")}
            title={t("onb.entryTitle")}
            aria-pressed={onboardingActive}
            data-onboarding-active={onboardingActive ? "true" : "false"}
            className="flex h-8 shrink-0 items-center gap-1.5 rounded-lg px-2 text-muted-foreground hover:bg-[var(--chip-hover-bg)] hover:text-foreground"
          >
            <Wand2 aria-hidden="true" className="size-4 shrink-0" />
            <span className="hidden whitespace-nowrap text-[12px] font-medium min-[420px]:inline">{t("onb.entry")}</span>
          </button>
        )}
        <NotificationCenter />
      </div>
    </header>
  );
}
