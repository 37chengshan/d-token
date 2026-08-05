import { useState } from "react";
import { useT } from "../../lib/i18n";
import { NAV, SETTINGS_NAV, type PageId } from "../../lib/nav";
import { AnimatedNavIcon } from "./AnimatedNavIcon";
import { NotificationCenter } from "../common/NotificationCenter";

export function Topbar({
  page = "overview",
  collapsed,
  setCollapsed,
  reducedMotion = false,
}: {
  page?: PageId;
  collapsed: boolean;
  setCollapsed: (c: boolean) => void;
  reducedMotion?: boolean;
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
      <span className="shell-topbar-context truncate text-[12px] font-medium text-muted-foreground" data-tauri-drag-region>
        {t(current.labelKey)}
      </span>
      <span className="sr-only">{t("app.name")}</span>
      <div className="ml-auto"><NotificationCenter /></div>
    </header>
  );
}
