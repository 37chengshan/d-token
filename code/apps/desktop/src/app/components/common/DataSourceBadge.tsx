import { Database } from "lucide-react";
import { useT } from "../../lib/i18n";

type DataSourceBadgeProps = {
  label?: string;
  mode?: "native" | "runtime" | "preview" | "unavailable";
};

export function DataSourceBadge({ label, mode = "preview" }: DataSourceBadgeProps) {
  const t = useT();
  const modeClass =
    mode === "runtime" || mode === "native"
      ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
      : mode === "unavailable"
        ? "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300"
        : "border-border/60 bg-muted/50 text-muted-foreground";

  return (
    <span className={`inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs ${modeClass}`}>
      <Database className="size-3" />
      {label ?? (mode === "runtime" || mode === "native" ? t("common.runtimeData") : t("common.previewData"))}
    </span>
  );
}
