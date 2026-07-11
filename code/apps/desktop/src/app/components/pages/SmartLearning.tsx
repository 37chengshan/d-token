import { Search, ShieldCheck } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { EmptyState } from "../common/EmptyState";
import { Button } from "../ui/button";
import { useT } from "../../lib/i18n";

export function SmartLearning() {
  const t = useT();

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="text-base font-semibold">{t("sl.title")}</h2>
          <p className="mt-1 text-sm text-muted-foreground">{t("sl.subtitle")}</p>
        </div>
        <Button size="sm" disabled>
          <Search className="size-3.5" />
          {t("sl.nativeUnavailableShort")}
        </Button>
      </div>

      <div className="flex items-center gap-2 rounded-lg border border-amber-500/25 bg-amber-500/8 px-3 py-2 text-xs text-amber-800 dark:text-amber-200">
        <ShieldCheck className="size-3.5 shrink-0" />
        <span>{t("sl.nativeUnavailable")}</span>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <GlassCard className="p-4" hover={false}>
          <h3 className="mb-2 text-sm font-medium">{t("sl.candidates")}</h3>
          <EmptyState status="empty" title={t("sl.nativeUnavailableShort")} />
        </GlassCard>
        <GlassCard className="p-4" hover={false}>
          <h3 className="mb-2 text-sm font-medium">{t("sl.output")}</h3>
          <EmptyState status="empty" title={t("sl.nativeUnavailableShort")} />
        </GlassCard>
      </div>
    </div>
  );
}
