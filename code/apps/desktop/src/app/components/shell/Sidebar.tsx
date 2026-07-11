import {
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  Sun,
  type LucideIcon,
} from "lucide-react";
import { NAV, SETTINGS_NAV, type PageId } from "../../lib/nav";
import { useT } from "../../lib/i18n";
import { useTheme } from "../../lib/theme-context";
import { BrandMark } from "../brand/BrandMark";
import { cn } from "../ui/utils";

const ITEM_GAP = 6;

export function Sidebar({
  page,
  setPage,
  collapsed,
  setCollapsed,
}: {
  page: PageId;
  setPage: (p: PageId) => void;
  collapsed: boolean;
  setCollapsed: (c: boolean) => void;
}) {
  const t = useT();
  const { theme, setTheme } = useTheme();
  const isDark = theme === "dark";

  const Item = ({
    id,
    icon: Icon,
    labelKey,
  }: {
    id: PageId;
    icon: LucideIcon;
    labelKey: string;
  }) => {
    const active = page === id;

    return (
      <button
        type="button"
        onClick={() => setPage(id)}
        title={collapsed ? t(labelKey) : undefined}
        aria-label={t(labelKey)}
        aria-current={active ? "page" : undefined}
        className={cn(
          "nav-motion-item group relative z-10 flex items-center text-[13px] transition-colors duration-150",
          collapsed
            ? "mx-auto h-[40px] w-[40px] justify-center rounded-lg p-0"
            : "h-[34px] w-full gap-2 rounded-md px-2",
          active
            ? "sidebar-item-selected text-[color:var(--accent-500)]"
            : "text-sidebar-foreground/65 hover:bg-[var(--chip-hover-bg)] hover:text-sidebar-foreground",
        )}
      >
        <span
          className={cn(
            "flex shrink-0 items-center justify-center rounded-lg",
            collapsed ? "size-8" : "size-[18px]",
          )}
        >
          <Icon className="size-[15px]" />
        </span>
        {!collapsed && <span className="truncate">{t(labelKey)}</span>}
      </button>
    );
  };

  return (
    <aside className="shell-sidebar relative z-20 flex min-h-0 min-w-0 flex-col">
      <div className="shell-sidebar-header flex h-12 shrink-0 items-center" data-tauri-drag-region>
        <span className="flex size-8 shrink-0 items-center justify-center overflow-hidden rounded-[10px] bg-white/90 shadow-sm ring-1 ring-black/10">
          <BrandMark className="size-full object-cover" />
        </span>
        {!collapsed && <span className="ml-2.5 truncate text-[13px] font-semibold tracking-[-0.012em]">d-token</span>}
      </div>

      <nav className="relative z-10 flex flex-1 flex-col px-2 pt-2" style={{ gap: ITEM_GAP }}>
        {NAV.map((item) => (
          <Item key={item.id} {...item} />
        ))}
      </nav>

      <div className="relative z-10 flex flex-col px-2 pb-3" style={{ gap: ITEM_GAP }}>
        <Item {...SETTINGS_NAV} />

        <div className={cn("mt-1 flex items-center gap-1", collapsed && "flex-col")}>
          <button
            type="button"
            onClick={() => setTheme(isDark ? "light" : "dark")}
            aria-label={isDark ? t("set.themeLight") : t("set.themeDark")}
            className={cn(
              "flex items-center justify-center gap-1.5 rounded-lg text-[13px] text-muted-foreground hover:bg-[var(--chip-hover-bg)]",
              collapsed ? "h-[40px] w-[40px]" : "flex-1 py-1",
            )}
            title={isDark ? t("set.themeLight") : t("set.themeDark")}
          >
            {isDark ? <Sun className="size-[14px]" /> : <Moon className="size-[14px]" />}
            {!collapsed && (
              <span className="text-xs">{isDark ? t("set.themeLight") : t("set.themeDark")}</span>
            )}
          </button>

          <button
            type="button"
            onClick={() => setCollapsed(!collapsed)}
            aria-label={collapsed ? t("app.expandSidebar") : t("app.collapseSidebar")}
            className={cn(
              "flex items-center justify-center rounded-md text-muted-foreground hover:bg-[var(--chip-hover-bg)]",
              collapsed ? "h-[40px] w-[40px]" : "size-7",
            )}
            title={collapsed ? t("app.expandSidebar") : t("app.collapseSidebar")}
          >
            {collapsed ? (
              <PanelLeftOpen className="size-[14px]" />
            ) : (
              <PanelLeftClose className="size-[14px]" />
            )}
          </button>
        </div>
      </div>
    </aside>
  );
}
