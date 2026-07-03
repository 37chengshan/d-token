import { motion } from "motion/react";
import { Sun, Moon, Zap, PanelLeftClose } from "lucide-react";
import { NAV, SETTINGS_NAV, type PageId } from "../../lib/nav";
import { useT } from "../../lib/i18n";
import { useTheme } from "../../lib/theme-context";
import { cn } from "../ui/utils";

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

  const activeIndex = NAV.findIndex((n) => n.id === page);
  const isSettings = page === "settings";
  const indicatorIndex = isSettings ? NAV.length : activeIndex;
  const ITEM_HEIGHT = 34;
  const ITEM_GAP = 2;
  const STEP = ITEM_HEIGHT + ITEM_GAP;
  const settingsOffset = ITEM_GAP + 4;
  const indicatorY = indicatorIndex * STEP + (isSettings ? settingsOffset : 0);

  const Item = ({ id, icon: Icon, labelKey }: { id: PageId; icon: typeof Zap; labelKey: string }) => {
    const active = page === id;
    return (
      <button
        onClick={() => setPage(id)}
        title={collapsed ? t(labelKey) : undefined}
        className={cn(
          "group relative z-10 flex w-full items-center gap-2 rounded-md px-2 py-[7px] text-[13px] transition-colors duration-150",
          active
            ? "text-[color:var(--accent-500)]"
            : "text-sidebar-foreground/65 hover:text-sidebar-foreground",
          !active && "hover:bg-[var(--chip-hover-bg)]",
        )}
      >
        <Icon className="size-[14px] shrink-0" />
        {!collapsed && <span className="truncate">{t(labelKey)}</span>}
      </button>
    );
  };

  return (
    <aside
      className={cn(
        "glass-strong relative z-20 flex h-full flex-col border-r border-sidebar-border/60 transition-all duration-300",
        collapsed ? "w-[56px]" : "w-[124px]",
      )}
    >
      {/* Extended backdrop child (required by new glass system) */}
      <div className="glass-backdrop" />

      {/* Logo */}
      <div className="flex items-center gap-2 px-2 py-3">
        <div
          className="flex size-6 shrink-0 items-center justify-center rounded-md text-white shadow-sm"
          style={{ background: "var(--accent-500, #f97316)" }}
        >
          <Zap className="size-[13px]" />
        </div>
        {!collapsed && (
          <div className="min-w-0">
            <div className="truncate text-[13px]" style={{ fontWeight: 600 }}>
              {t("app.name")}
            </div>
          </div>
        )}
      </div>

      {/* Navigation with single sliding glass indicator */}
      <nav className="relative flex flex-1 flex-col px-1" style={{ gap: ITEM_GAP }}>
        {/* Single persistent glass indicator — always rendered, slides vertically */}
        <motion.div
          className="absolute left-1 right-1 z-0 rounded-md border backdrop-blur-md"
          style={{
            height: ITEM_HEIGHT,
            background: "color-mix(in srgb, var(--accent-500) 12%, transparent)",
            borderColor: "color-mix(in srgb, var(--accent-500) 35%, transparent)",
            boxShadow: "inset 0 1px 0 rgba(255,255,255,0.30)",
          }}
          initial={false}
          animate={{ y: indicatorY }}
          transition={{ type: "spring", stiffness: 400, damping: 30 }}
        />
        {NAV.map((n) => (
          <Item key={n.id} {...n} />
        ))}
      </nav>

      {/* Footer: settings + theme toggle */}
      <div className="flex flex-col px-1 pb-3" style={{ gap: ITEM_GAP }}>
        <Item {...SETTINGS_NAV} />
        <div className="mt-1 flex items-center gap-1">
          <button
            onClick={() => setTheme(isDark ? "light" : "dark")}
            className="flex flex-1 items-center justify-center gap-1.5 rounded-md py-1 text-[13px] text-muted-foreground hover:bg-[var(--chip-hover-bg)]"
            title="Toggle theme"
          >
            {isDark ? <Sun className="size-[14px]" /> : <Moon className="size-[14px]" />}
            {!collapsed && <span className="text-xs">{isDark ? t("set.themeLight") : t("set.themeDark")}</span>}
          </button>
          {!collapsed && (
            <button
              onClick={() => setCollapsed(true)}
              className="rounded-md p-1 text-muted-foreground hover:bg-[var(--chip-hover-bg)]"
              title="Collapse"
            >
              <PanelLeftClose className="size-[14px]" />
            </button>
          )}
        </div>
      </div>
    </aside>
  );
}
