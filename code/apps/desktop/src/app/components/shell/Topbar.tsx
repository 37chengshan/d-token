import { PanelLeft } from "lucide-react";
import { useT } from "../../lib/i18n";
import type { PageId } from "../../lib/nav";

const TITLE_KEY: Record<PageId, string> = {
  overview: "ov.title",
  install: "in.title",
  advanced: "adv.title",
  analytics: "an.title",
  doctor: "dr.title",
  changes: "ch.title",
  logs: "ls.title",
  compatibility: "cp.title",
  settings: "set.title",
};

export function Topbar({ page, collapsed, setCollapsed }: { page: PageId; collapsed: boolean; setCollapsed: (c: boolean) => void }) {
  const t = useT();
  return (
    <header className="glass-light sticky top-0 z-10 flex h-14 items-center border-b border-border/40 px-4">
      <div className="glass-backdrop" />
      <div className="flex items-center gap-3">
        {collapsed && (
          <button
            type="button"
            onClick={() => setCollapsed(false)}
            aria-label="Expand sidebar"
            title="Expand sidebar"
            className="rounded-lg p-2 text-muted-foreground hover:bg-accent"
          >
            <PanelLeft className="size-4" />
          </button>
        )}
        <div className="h-4 w-px bg-border" />
        <h2 className="text-base">{t(TITLE_KEY[page])}</h2>
      </div>
    </header>
  );
}
