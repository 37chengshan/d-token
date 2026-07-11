import { motion } from "motion/react";
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
      <motion.button
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
        whileHover={{ y: -1, scale: 1.035 }}
        whileTap={{ scale: 0.97 }}
        transition={{ type: "spring", stiffness: 420, damping: 24 }}
      >
        <motion.span
          className={cn(
            "flex shrink-0 items-center justify-center rounded-lg",
            collapsed ? "size-8" : "size-[18px]",
          )}
          whileHover={{ rotate: active ? 0 : -8, scale: 1.08 }}
          transition={{ type: "spring", stiffness: 360, damping: 18 }}
        >
          <Icon className="size-[15px]" />
        </motion.span>
        {!collapsed && <span className="truncate">{t(labelKey)}</span>}
      </motion.button>
    );
  };

  return (
    <aside
      className={cn(
        "shell-sidebar glass-strong relative z-20 flex h-full shrink-0 flex-col border-r border-sidebar-border/60 transition-all duration-300",
        collapsed ? "w-[60px]" : "w-[148px]",
      )}
    >
      <nav className="relative z-10 flex flex-1 flex-col px-2 pt-3" style={{ gap: ITEM_GAP }}>
        {NAV.map((item) => (
          <Item key={item.id} {...item} />
        ))}
      </nav>

      <div className="relative z-10 flex flex-col px-2 pb-3" style={{ gap: ITEM_GAP }}>
        <Item {...SETTINGS_NAV} />

        <div className={cn("mt-1 flex items-center gap-1", collapsed && "flex-col")}>
          <motion.button
            type="button"
            onClick={() => setTheme(isDark ? "light" : "dark")}
            aria-label={isDark ? t("set.themeLight") : t("set.themeDark")}
            className={cn(
              "flex items-center justify-center gap-1.5 rounded-lg text-[13px] text-muted-foreground hover:bg-[var(--chip-hover-bg)]",
              collapsed ? "h-[40px] w-[40px]" : "flex-1 py-1",
            )}
            title={isDark ? t("set.themeLight") : t("set.themeDark")}
            whileHover={{ y: -1, scale: 1.035 }}
            whileTap={{ scale: 0.97 }}
          >
            {isDark ? <Sun className="size-[14px]" /> : <Moon className="size-[14px]" />}
            {!collapsed && (
              <span className="text-xs">{isDark ? t("set.themeLight") : t("set.themeDark")}</span>
            )}
          </motion.button>

          <motion.button
            type="button"
            onClick={() => setCollapsed(!collapsed)}
            aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            className={cn(
              "flex items-center justify-center rounded-md text-muted-foreground hover:bg-[var(--chip-hover-bg)]",
              collapsed ? "h-[40px] w-[40px]" : "size-7",
            )}
            title={collapsed ? "Expand" : "Collapse"}
            whileHover={{ y: -1, scale: 1.04 }}
            whileTap={{ scale: 0.96 }}
          >
            {collapsed ? (
              <PanelLeftOpen className="size-[14px]" />
            ) : (
              <PanelLeftClose className="size-[14px]" />
            )}
          </motion.button>
        </div>
      </div>
    </aside>
  );
}
