/*
 * d-token sidebar, structurally adapted from AiMaMi at
 * 297c7af56f10fb371b77bc9b6b65aa320afcbe7e (Apache-2.0).
 * Insets, expand/collapse and theme-segmented structure derive from AiMaMi's
 * layout/ui sidebar; page set, metrics, motion and tokens are d-token's own.
 * See docs/legal/THIRD_PARTY_NOTICES.md (AiMaMi 适配范围) and shell.css header.
 */
import { NAV, SETTINGS_NAV, type PageId } from "../../lib/nav";
import { useT } from "../../lib/i18n";
import { useAppearance } from "../../lib/theme-context";
import { BrandMark } from "../brand/BrandMark";
import { cn } from "../ui/utils";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "../ui/tooltip";
import { AnimatedNavIcon } from "./AnimatedNavIcon";
import { AnimatedSegmentedControl } from "./AnimatedSegmentedControl";
import {
  forwardRef,
  memo,
  useCallback,
  useEffect,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type ReactNode,
} from "react";

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

  return (
    <TooltipProvider delayDuration={0}>
      <aside
        className="shell-sidebar relative z-20 flex min-h-0 min-w-0 flex-col"
        data-state={collapsed ? "collapsed" : "expanded"}
      >
        <div
          className="shell-sidebar-header flex h-12 shrink-0 items-center"
          data-tauri-drag-region
          aria-label={t("app.name")}
        >
          <span className="flex size-[35px] shrink-0 items-center justify-center overflow-hidden rounded-[10px] bg-white/90 shadow-sm ring-1 ring-black/10">
            <BrandMark className="size-full object-cover" />
          </span>
          {!collapsed && (
            <div className="ml-2.5 flex min-w-0 items-center leading-tight">
              <span className="shell-brand-label truncate text-[14px] font-semibold tracking-[-0.018em]">
                d-token
              </span>
            </div>
          )}
        </div>

        <nav
          className="relative z-10 flex flex-1 flex-col px-2 pt-[18px]"
          style={{ gap: ITEM_GAP }}
          aria-label={t("app.name")}
        >
          {NAV.map((item) => (
            <SidebarItem
              key={item.id}
              id={item.id}
              labelKey={item.labelKey}
              badge={item.badge}
              active={page === item.id}
              setPage={setPage}
              collapsed={collapsed}
              reducedMotion={reducedMotion}
            />
          ))}
        </nav>

        <div
          className="relative z-10 flex flex-col border-t border-sidebar-border/70 px-2 pb-3 pt-2"
          style={{ gap: ITEM_GAP }}
        >
          <SidebarItem
            id={SETTINGS_NAV.id}
            labelKey={SETTINGS_NAV.labelKey}
            badge={SETTINGS_NAV.badge}
            active={page === SETTINGS_NAV.id}
            setPage={setPage}
            collapsed={collapsed}
            reducedMotion={reducedMotion}
          />

          <div
            className={cn(
              "mt-1 flex items-center gap-1",
              collapsed && "flex-col",
            )}
          >
            {collapsed ? (
              <CollapsedTip
                label={isDark ? t("set.themeLight") : t("set.themeDark")}
              >
                <ThemeToggleButton
                  isDark={isDark}
                  reducedMotion={reducedMotion}
                  onToggle={() => setTheme(isDark ? "light" : "dark")}
                />
              </CollapsedTip>
            ) : (
              <AnimatedSegmentedControl
                ariaLabel={t("set.theme")}
                items={
                  [
                    {
                      value: "light",
                      label: t("set.themeLight"),
                      icon: (
                        <AnimatedNavIcon
                          id="theme"
                          active={!isDark}
                          reducedMotion={reducedMotion}
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
                          className="size-[14px]"
                        />
                      ),
                    },
                  ] as const
                }
                value={isDark ? "dark" : "light"}
                onValueChange={setTheme}
                reducedMotion={reducedMotion}
                className="min-w-0 flex-1"
              />
            )}

            <CollapsedTip
              label={
                collapsed ? t("app.expandSidebar") : t("app.collapseSidebar")
              }
              enabled={collapsed}
            >
              <CollapseButton
                collapsed={collapsed}
                reducedMotion={reducedMotion}
                onToggle={() => setCollapsed(!collapsed)}
              />
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
      <TooltipContent
        side="right"
        align="center"
        sideOffset={10}
        className="shell-nav-tooltip"
      >
        {label}
      </TooltipContent>
    </Tooltip>
  );
}

/**
 * Engagement and click-pulse state live inside each item so hovering or
 * clicking one nav entry never re-renders the rest of the sidebar.
 */
const SidebarItem = memo(function SidebarItem({
  id,
  labelKey,
  badge,
  active,
  setPage,
  collapsed,
  reducedMotion,
}: {
  id: PageId;
  labelKey: string;
  badge?: "beta";
  active: boolean;
  setPage: (page: PageId) => void;
  collapsed: boolean;
  reducedMotion: boolean;
}) {
  const t = useT();
  const [motionToken, setMotionToken] = useState(0);
  // Hover/focus engagement is written imperatively onto the icon element
  // (data-motion-engaged / data-motion-play) instead of React state, so
  // hovering one item never re-renders it or its siblings. Only the click
  // pulse uses state (motionToken), and it is local to this item.
  const buttonRef = useRef<HTMLButtonElement>(null);
  const engagedRef = useRef(false);

  const applyEngagement = useCallback(() => {
    const icon = buttonRef.current?.querySelector("[data-motion-id]");
    if (!icon) return;
    const engaged = engagedRef.current && !reducedMotion;
    icon.setAttribute("data-motion-engaged", engaged ? "true" : "false");
    // Only override play while engaged — the click-pulse timer owns it otherwise.
    if (engaged) icon.setAttribute("data-motion-play", "true");
  }, [reducedMotion]);

  // Click-token bumps and active flips re-render this item and reconcile the
  // icon's data attributes from props, so re-apply the imperative engagement
  // after every commit to keep data-motion-engaged true while the
  // pointer/keyboard is still on the item.
  useEffect(() => {
    applyEngagement();
  });

  const label = t(labelKey);
  const accessibleLabel = badge === "beta" ? `${label} Beta` : label;
  const button = (
    <button
      ref={buttonRef}
      type="button"
      onClick={() => {
        setMotionToken((token) => token + 1);
        setPage(id);
      }}
      onPointerEnter={() => {
        engagedRef.current = true;
        applyEngagement();
      }}
      onPointerLeave={() => {
        engagedRef.current = false;
        applyEngagement();
      }}
      onFocus={() => {
        engagedRef.current = true;
        applyEngagement();
      }}
      onBlur={() => {
        engagedRef.current = false;
        applyEngagement();
      }}
      aria-label={accessibleLabel}
      aria-current={active ? "page" : undefined}
      data-reduced-motion={reducedMotion ? "true" : "false"}
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
      <span
        className={cn(
          "relative flex shrink-0 items-center justify-center",
          collapsed ? "size-8" : "size-[18px]",
        )}
      >
        <AnimatedNavIcon
          id={id}
          active={active}
          reducedMotion={reducedMotion}
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
  return (
    <CollapsedTip label={accessibleLabel} enabled={collapsed}>
      {button}
    </CollapsedTip>
  );
});

type MotionButtonProps = ButtonHTMLAttributes<HTMLButtonElement>;

const ThemeToggleButton = forwardRef<
  HTMLButtonElement,
  MotionButtonProps & {
    isDark: boolean;
    reducedMotion: boolean;
    onToggle: () => void;
  }
>(function ThemeToggleButton(
  { isDark, reducedMotion, onToggle, className, onClick, type, ...props },
  ref,
) {
  const t = useT();
  const [motionToken, setMotionToken] = useState(0);
  return (
    <button
      {...props}
      ref={ref}
      type={type ?? "button"}
      onClick={(event) => {
        setMotionToken((token) => token + 1);
        onToggle();
        onClick?.(event);
      }}
      aria-label={
        props["aria-label"] ??
        (isDark ? t("set.themeLight") : t("set.themeDark"))
      }
      data-reduced-motion={reducedMotion ? "true" : "false"}
      className={cn(
        "nav-motion-item shell-theme-icon-button group flex h-[40px] w-[40px] items-center justify-center rounded-lg text-muted-foreground",
        className,
      )}
    >
      <AnimatedNavIcon
        id="theme"
        active={false}
        reducedMotion={reducedMotion}
        dark={isDark}
        motionToken={motionToken}
        className="size-[18px]"
      />
    </button>
  );
});

const CollapseButton = forwardRef<
  HTMLButtonElement,
  MotionButtonProps & {
    collapsed: boolean;
    reducedMotion: boolean;
    onToggle: () => void;
  }
>(function CollapseButton(
  { collapsed, reducedMotion, onToggle, className, onClick, type, ...props },
  ref,
) {
  const t = useT();
  const [motionToken, setMotionToken] = useState(0);
  return (
    <button
      {...props}
      ref={ref}
      type={type ?? "button"}
      onClick={(event) => {
        setMotionToken((token) => token + 1);
        onToggle();
        onClick?.(event);
      }}
      aria-label={
        props["aria-label"] ??
        (collapsed ? t("app.expandSidebar") : t("app.collapseSidebar"))
      }
      data-reduced-motion={reducedMotion ? "true" : "false"}
      className={cn(
        "nav-motion-item group flex shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-[var(--chip-hover-bg)]",
        collapsed ? "h-[40px] w-[40px]" : "size-7",
        className,
      )}
    >
      <AnimatedNavIcon
        id="collapse"
        active={false}
        reducedMotion={reducedMotion}
        expanded={!collapsed}
        motionToken={motionToken}
        className="size-[17px]"
      />
    </button>
  );
});
