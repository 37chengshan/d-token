import { Database } from "lucide-react";
import { useT } from "../../lib/i18n";

export function DataSourceBadge() {
  const t = useT();
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full border border-border/60 bg-muted/50 px-2 py-0.5 text-xs text-muted-foreground">
      <Database className="size-3" />
      {t("common.fixture")}
    </span>
  );
}
