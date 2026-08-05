import { NAV, SETTINGS_NAV, type PageId } from "../../lib/nav";
import { useT } from "../../lib/i18n";
import { useAppearance } from "../../lib/theme-context";
import { BrandMark } from "../brand/BrandMark";
import { cn } from "../ui/utils";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "../ui/tooltip";
import { AnimatedNavIcon } from "./AnimatedNavIcon";
import { AnimatedSegmentedControl } from "./AnimatedSegmentedControl";
import { useState, type ReactNode } from "react";

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
  const [engagedId, setEngagedId] = useState<string | null>(null);
  const triggerMotion = (id: string) => setMotionPulse((current) => ({ id, token: current.token + 1 }));
  const engage = (id: string) => setEngagedId(id);
  const disengage = (id: string) => setEngagedId((current) => (current === id ? null : current));

  return (
    <TooltipProvider delayDuration={0}>
      <aside className="shell-sidebar relative z-20 flex min-h-0 min-w-0 flex-col" data-state={collapsed ? "collapsed" : "expanded"}>
        <div className="shell-sidebar-header flex h-12 shrink-0 items-center" data-tauri-drag-region aria-label={t("app.name")}>
          <span className="flex size-[35px] shrink-0 items-center justify-center overflow-hidden rounded-[10px] bg-white/90 shadow-sm ring-1 ring-black/10">
            <BrandMark className="size-full object-cover" />
          </span>
          {!collapsed && (
            <div className="ml-2.5 flex min-w-0 items-center leading-tight">
              <span className="shell-brand-label truncate text-[14px] font-semibold tracking-[-0.018em]">d-token</span>
            </div>
          )}
        </div>

        <nav className="relative z-10 flex flex-1 flex-col px-2 pt-[18px]" style={{ gap: ITEM_GAP }} aria-label={t("app.name")}>
          {NAV.map((item) => (
            <SidebarItem
              key={item.id}
              id={item.id}
              labelKey={item.labelKey}
              badge={item.badge}
              page={page}
              setPage={setPage}
              collapsed={collapsed}
              reducedMotion={reducedMotion}
              engaged={engagedId === item.id}
              onEngage={() => engage(item.id)}
              onDisengage={() => disengage(item.id)}
              motionToken={motionPulse.id === item.id ? motionPulse.token : 0}
              triggerMotion={triggerMotion}
            />
          ))}
        </nav>

        <div className="relative z-10 flex flex-col border-t border-sidebar-border/70 px-2 pb-3 pt-2" style={{ gap: ITEM_GAP }}>
          <SidebarItem
            id={SETTINGS_NAV.id}
            labelKey={SETTINGS_NAV.labelKey}
            badge={SETTINGS_NAV.badge}
            page={page}
            setPage={setPage}
            collapsed={collapsed}
            reducedMotion={reducedMotion}
            engaged={engagedId === SETTINGS_NAV.id}
            onEngage={() => engage(SETTINGS_NAV.id)}
            onDisengage={() => disengage(SETTINGS_NAV.id)}
            motionToken={motionPulse.id === SETTINGS_NAV.id ? motionPulse.token : 0}
            triggerMotion={triggerMotion}
          />

          <div className={cn("mt-1 flex items-center gap-1", collapsed && "flex-col")}>
            {collapsed ? (
              <CollapsedTip label={isDark ? t("set.themeLight") : t("set.themeDark")}>
                <button
                  type="button"
                  onClick={() => {
                    triggerMotion("theme");
                    setTheme(isDark ? "light" : "dark");
                  }}
                  onPointerEnter={() => engage("theme")}
                  onPointerLeave={() => disengage("theme")}
                  onFocus={() => engage("theme")}
                  onBlur={() => disengage("theme")}
                  aria-label={isDark ? t("set.themeLight") : t("set.themeDark")}
                  className="nav-motion-item shell-theme-icon-button group flex h-[40px] w-[40px] items-center justify-center rounded-lg text-muted-foreground"
                  data-nav-engaged={engagedId === "theme" ? "true" : "false"}
                >
                  <AnimatedNavIcon
                    id="theme"
                    active={false}
                    reducedMotion={reducedMotion}
                    dark={isDark}
                    engaged={engagedId === "theme"}
                    motionToken={motionPulse.id === "theme" ? motionPulse.token : 0}
                    className="size-[18px]"
                  />
                </button>
              </CollapsedTip>
            ) : (
              <AnimatedSegmentedControl
                ariaLabel={t("set.theme")}
                items={[
                  {
                    value: "light",
                    label: t("set.themeLight"),
                    icon: (
                      <AnimatedNavIcon
                        id="theme"
                        active={!isDark}
                        reducedMotion={reducedMotion}
                        engaged={engagedId === "theme-light"}
                        className="size-[14px]"
                      />
                    ),
                  },
                  {
                    value: "dark",
                    label: t("set.themeDark"),
                    icon: (
                      <AnimatedNavIcon
                        id="theme"
                        active={isDark}
                        reducedMotion={reducedMotion}
                        dark
                        engaged={engagedId === "theme-dark"}
                        className="size-[14px]"
                      />
                    ),
                  },
                ] as const}
                value={isDark ? "dark" : "light"}
                onValueChange={setTheme}
                reducedMotion={reducedMotion}
                className="min-w-0 flex-1"
              />
            )}

            <CollapsedTip label={collapsed ? t("app.expandSidebar") : t("app.collapseSidebar")} enabled={collapsed}>
              <button
                type="button"
                onClick={() => {
                  triggerMotion("collapse");
                  setCollapsed(!collapsed);
                }}
                onPointerEnter={() => engage("collapse")}
                onPointerLeave={() => disengage("collapse")}
                onFocus={() => engage("collapse")}
                onBlur={() => disengage("collapse")}
                aria-label={collapsed ? t("app.expandSidebar") : t("app.collapseSidebar")}
                className={cn(
                  "nav-motion-item group flex items-center justify-center rounded-md text-muted-foreground hover:bg-[var(--chip-hover-bg)]",
                  collapsed ? "h-[40px] w-[40px]" : "size-7",
                )}
                data-nav-engaged={engagedId === "collapse" ? "true" : "false"}
              >
                <AnimatedNavIcon
                  id="collapse"
                  active={false}
                  reducedMotion={reducedMotion}
                  expanded={!collapsed}
                  engaged={engagedId === "collapse"}
                  motionToken={motionPulse.id === "collapse" ? motionPulse.token : 0}
                  className="size-[17px]"
                />
              </button>
            </CollapsedTip>
          </div>
        </div>
      </aside>
    </TooltipProvider>
  );
}

function CollapsedTip({
  label,
  enabled = true,
  children,
}: {
  label: string;
  enabled?: boolean;
  children: ReactNode;
}) {
  if (!enabled) return children;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent side="right" align="center" sideOffset={10} className="shell-nav-tooltip">
        {label}
      </TooltipContent>
    </Tooltip>
  );
}

function SidebarItem({
  id,
  labelKey,
  badge,
  page,
  setPage,
  collapsed,
  reducedMotion,
  engaged,
  onEngage,
  onDisengage,
  motionToken,
  triggerMotion,
}: {
  id: PageId;
  labelKey: string;
  badge?: "beta";
  page: PageId;
  setPage: (page: PageId) => void;
  collapsed: boolean;
  reducedMotion: boolean;
  engaged: boolean;
  onEngage: () => void;
  onDisengage: () => void;
  motionToken: number;
  triggerMotion: (id: string) => void;
}) {
  const t = useT();
  const active = page === id;
  const label = t(labelKey);
  const accessibleLabel = badge === "beta" ? `${label} Beta` : label;
  const button = (
    <button
      type="button"
      onClick={() => {
        triggerMotion(id);
        setPage(id);
      }}
      onPointerEnter={onEngage}
      onPointerLeave={onDisengage}
      onFocus={onEngage}
      onBlur={onDisengage}
      aria-label={accessibleLabel}
      aria-current={active ? "page" : undefined}
      data-nav-engaged={engaged ? "true" : "false"}
      className={cn(
        "nav-motion-item group relative z-10 flex items-center text-[13px] transition-colors duration-200",
        collapsed
          ? "mx-auto h-[40px] w-[40px] justify-center rounded-lg p-0"
          : "h-[34px] w-full gap-2.5 rounded-lg px-3",
        active
          ? "sidebar-item-selected text-[color:var(--interaction-accent-strong)]"
          : "text-sidebar-foreground/70 hover:bg-[var(--chip-hover-bg)] hover:text-sidebar-foreground",
      )}
    >
      <span className={cn("relative flex shrink-0 items-center justify-center", collapsed ? "size-8" : "size-[18px]")}>
        <AnimatedNavIcon
          id={id}
          active={active}
          reducedMotion={reducedMotion}
          engaged={engaged}
          motionToken={motionToken}
          className="size-[18px]"
        />
        {collapsed && badge === "beta" ? (
          <span
            className="absolute -right-0.5 -top-0.5 size-1.5 rounded-full bg-[color:var(--interaction-accent-strong)]"
            aria-hidden
          />
        ) : null}
      </span>
      {!collapsed && (
        <>
          <span className="min-w-0 flex-1 truncate">{label}</span>
          {badge === "beta" ? (
            <span className="shrink-0 rounded px-1 py-0.5 text-[10px] font-medium leading-none text-[color:var(--interaction-accent-strong)] ring-1 ring-[color:var(--interaction-accent-strong)]/30">
              Beta
            </span>
          ) : null}
        </>
      )}
    </button>
  );
  return <CollapsedTip label={accessibleLabel} enabled={collapsed}>{button}</CollapsedTip>;
}
