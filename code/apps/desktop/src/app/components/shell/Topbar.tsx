import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { useT } from "../../lib/i18n";
import { NAV, SETTINGS_NAV, type PageId } from "../../lib/nav";

export function Topbar({
  page = "overview",
  collapsed,
  setCollapsed,
}: {
  page?: PageId;
  collapsed: boolean;
  setCollapsed: (c: boolean) => void;
}) {
  const t = useT();
  const current = [...NAV, SETTINGS_NAV].find((item) => item.id === page) ?? NAV[0];

  return (
    <header
      className="shell-topbar relative z-30 flex h-12 min-w-0 shrink-0 items-center gap-2 px-4"
      data-tauri-drag-region
    >
      <button
        type="button"
        onClick={() => setCollapsed(!collapsed)}
        aria-label={collapsed ? t("app.expandSidebar") : t("app.collapseSidebar")}
        title={collapsed ? t("app.expandSidebar") : t("app.collapseSidebar")}
        className="shell-window-control flex size-8 shrink-0 items-center justify-center rounded-lg text-muted-foreground hover:bg-[var(--chip-hover-bg)] hover:text-foreground"
      >
        {collapsed ? <PanelLeftOpen className="size-4" /> : <PanelLeftClose className="size-4" />}
      </button>
      <span className="shell-topbar-context truncate text-[12px] font-medium text-muted-foreground" data-tauri-drag-region>
        {t(current.labelKey)}
      </span>
      <span className="sr-only">{t("app.name")}</span>
    </header>
  );
}
