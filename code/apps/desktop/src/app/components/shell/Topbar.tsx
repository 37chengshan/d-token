import { PanelLeft, Bell, Coffee, Github, Wrench, Cast } from "lucide-react";
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
  settings: "set.title",
};

export function Topbar({ page, collapsed, setCollapsed }: { page: PageId; collapsed: boolean; setCollapsed: (c: boolean) => void }) {
  const t = useT();
  const icons = [Coffee, Github, Wrench, Cast, Bell];
  return (
    <header className="glass-light sticky top-0 z-10 flex h-14 items-center justify-between border-b border-border/40 px-4">
      <div className="glass-backdrop" />
      <div className="flex items-center gap-3">
        {collapsed && (
          <button onClick={() => setCollapsed(false)} className="rounded-lg p-2 text-muted-foreground hover:bg-accent">
            <PanelLeft className="size-4" />
          </button>
        )}
        <div className="h-4 w-px bg-border" />
        <h2 className="text-base">{t(TITLE_KEY[page])}</h2>
      </div>
      <div className="flex items-center gap-1 text-muted-foreground">
        {icons.map((Icon, i) => (
          <button key={i} className="rounded-lg p-2 hover:bg-accent hover:text-foreground">
            <Icon className="size-[18px]" />
          </button>
        ))}
      </div>
    </header>
  );
}
