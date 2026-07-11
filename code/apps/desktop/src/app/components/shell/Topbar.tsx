import { PanelLeft } from "lucide-react";
import { useT } from "../../lib/i18n";
import { BrandMark } from "../brand/BrandMark";
import { cn } from "../ui/utils";

export function Topbar({ collapsed, setCollapsed }: { collapsed: boolean; setCollapsed: (c: boolean) => void }) {
  const t = useT();

  return (
    <header
      className="shell-topbar glass-light relative z-30 flex h-12 min-w-0 shrink-0 items-center"
      data-tauri-drag-region
    >
      <div className="glass-backdrop" />
      <div
        className={cn(
          "shell-topbar-brand flex h-full shrink-0 items-center border-r border-sidebar-border/60 transition-[width,padding] duration-300",
          collapsed ? "w-[60px] justify-center" : "w-[148px] gap-2.5 px-3",
        )}
        data-tauri-drag-region
      >
        <span className="flex size-7 shrink-0 items-center justify-center overflow-hidden rounded-[9px] bg-white/90 shadow-sm ring-1 ring-black/10">
          <BrandMark className="size-full object-cover" />
        </span>
        {!collapsed && <span className="truncate text-[13px] font-semibold tracking-[-0.01em]">{t("app.name")}</span>}
      </div>

      <div className="flex h-full min-w-0 flex-1 items-center px-3" data-tauri-drag-region>
        {collapsed && (
          <button
            type="button"
            onClick={() => setCollapsed(false)}
            aria-label="Expand sidebar"
            title="Expand sidebar"
            className="shell-window-control flex size-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-[var(--chip-hover-bg)] hover:text-foreground"
          >
            <PanelLeft className="size-4" />
          </button>
        )}
        <span className="sr-only">{t("app.name")}</span>
      </div>
    </header>
  );
}
