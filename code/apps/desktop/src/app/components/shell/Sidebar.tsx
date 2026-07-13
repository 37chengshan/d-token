import { NAV, SETTINGS_NAV, type PageId } from "../../lib/nav";
import { useT } from "../../lib/i18n";
import { useAppearance } from "../../lib/theme-context";
import { BrandMark } from "../brand/BrandMark";
import { cn } from "../ui/utils";
import { AnimatedNavIcon } from "./AnimatedNavIcon";
import { AnimatedSegmentedControl } from "./AnimatedSegmentedControl";
import { useState } from "react";

const ITEM_GAP = 6;

export function Sidebar({
  page,
  setPage,
  collapsed,
  setCollapsed,
  reducedMotion,
}: {
  page: PageId;
  setPage: (p: PageId) => void;
  collapsed: boolean;
  setCollapsed: (c: boolean) => void;
  reducedMotion: boolean;
}) {
  const t = useT();
  const { theme, resolvedTheme, setTheme } = useAppearance();
  const isDark = (resolvedTheme ?? theme) === "dark";
  const [motionPulse, setMotionPulse] = useState<{ id: string; token: number }>({ id: "", token: 0 });
  const triggerMotion = (id: string) => setMotionPulse((current) => ({ id, token: current.token + 1 }));

  return (
    <aside className="shell-sidebar relative z-20 flex min-h-0 min-w-0 flex-col" data-state={collapsed ? "collapsed" : "expanded"}>
      <div className="shell-sidebar-header flex h-12 shrink-0 items-center" data-tauri-drag-region aria-label={t("app.name")}>
        <span className="flex size-[35px] shrink-0 items-center justify-center overflow-hidden rounded-[10px] bg-white/90 shadow-sm ring-1 ring-black/10">
          <BrandMark className="size-full object-cover" />
        </span>
        {!collapsed && (
          <div className="ml-2.5 flex min-w-0 items-center leading-tight">
            <span className="truncate text-[14px] font-semibold tracking-[-0.012em]">d-token</span>
          </div>
        )}
      </div>

      <nav className="relative z-10 flex flex-1 flex-col px-2 pt-[18px]" style={{ gap: ITEM_GAP }} aria-label={t("app.name")}>
        {NAV.map((item) => <SidebarItem key={item.id} {...item} page={page} setPage={setPage} collapsed={collapsed} reducedMotion={reducedMotion} motionToken={motionPulse.id === item.id ? motionPulse.token : 0} triggerMotion={triggerMotion} />)}
      </nav>

      <div className="relative z-10 flex flex-col border-t border-sidebar-border/70 px-2 pb-3 pt-2" style={{ gap: ITEM_GAP }}>
        <SidebarItem {...SETTINGS_NAV} page={page} setPage={setPage} collapsed={collapsed} reducedMotion={reducedMotion} motionToken={motionPulse.id === SETTINGS_NAV.id ? motionPulse.token : 0} triggerMotion={triggerMotion} />

        <div className={cn("mt-1 flex items-center gap-1", collapsed && "flex-col")}>
          {collapsed ? (
            <button
              type="button"
              onClick={() => setTheme(isDark ? "light" : "dark")}
              aria-label={isDark ? t("set.themeLight") : t("set.themeDark")}
              className="nav-motion-item shell-theme-icon-button group flex h-[40px] w-[40px] items-center justify-center rounded-lg text-muted-foreground"
              title={isDark ? t("set.themeLight") : t("set.themeDark")}
            >
              <AnimatedNavIcon key={`theme-${motionPulse.id === "theme" ? motionPulse.token : 0}`} id="theme" active={false} reducedMotion={reducedMotion} dark={isDark} motionToken={motionPulse.id === "theme" ? motionPulse.token : 0} className="size-[18px]" />
            </button>
          ) : (
            <AnimatedSegmentedControl
              ariaLabel={t("set.theme")}
              items={[
                { value: "light", label: t("set.themeLight"), icon: <AnimatedNavIcon id="theme" active={!isDark} reducedMotion={reducedMotion} className="size-[14px]" /> },
                { value: "dark", label: t("set.themeDark"), icon: <AnimatedNavIcon id="theme" active={isDark} reducedMotion={reducedMotion} dark className="size-[14px]" /> },
              ] as const}
              value={isDark ? "dark" : "light"}
              onValueChange={setTheme}
              reducedMotion={reducedMotion}
              className="min-w-0 flex-1"
            />
          )}

          <button
            type="button"
            onClick={() => setCollapsed(!collapsed)}
            aria-label={collapsed ? t("app.expandSidebar") : t("app.collapseSidebar")}
            className={cn(
              "nav-motion-item group flex items-center justify-center rounded-md text-muted-foreground hover:bg-[var(--chip-hover-bg)]",
              collapsed ? "h-[40px] w-[40px]" : "size-7",
            )}
            title={collapsed ? t("app.expandSidebar") : t("app.collapseSidebar")}
          >
            <AnimatedNavIcon key={`collapse-${motionPulse.id === "collapse" ? motionPulse.token : 0}`} id="collapse" active={false} reducedMotion={reducedMotion} expanded={!collapsed} motionToken={motionPulse.id === "collapse" ? motionPulse.token : 0} className="size-[17px]" />
          </button>
        </div>
      </div>
    </aside>
  );
}

function SidebarItem({
  id,
  labelKey,
  page,
  setPage,
  collapsed,
  reducedMotion,
  motionToken,
  triggerMotion,
}: {
  id: PageId;
  labelKey: string;
  page: PageId;
  setPage: (page: PageId) => void;
  collapsed: boolean;
  reducedMotion: boolean;
  motionToken: number;
  triggerMotion: (id: string) => void;
}) {
  const t = useT();
  const active = page === id;
  return (
    <button
      type="button"
      onClick={() => {
        triggerMotion(id);
        setPage(id);
      }}
      title={collapsed ? t(labelKey) : undefined}
      aria-label={t(labelKey)}
      aria-current={active ? "page" : undefined}
      className={cn(
        "nav-motion-item group relative z-10 flex items-center text-[13px] transition-colors duration-200",
        collapsed
          ? "mx-auto h-[40px] w-[40px] justify-center rounded-lg p-0"
          : "h-[34px] w-full gap-2.5 rounded-lg px-3",
        active
          ? "sidebar-item-selected text-[color:var(--accent-500)]"
          : "text-sidebar-foreground/70 hover:bg-[var(--chip-hover-bg)] hover:text-sidebar-foreground",
      )}
    >
      <span className={cn("flex shrink-0 items-center justify-center", collapsed ? "size-8" : "size-[18px]")}>
        <AnimatedNavIcon key={`${id}-${motionToken}`} id={id} active={active} reducedMotion={reducedMotion} motionToken={motionToken} className="size-[18px]" />
      </span>
      {!collapsed && <span className="truncate">{t(labelKey)}</span>}
    </button>
  );
}
