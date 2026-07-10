import { Activity, Brain, Database, FileText, Gauge, Route, ShieldCheck, Sparkles } from "lucide-react";
import { GlassCard } from "./GlassCard";
import { StatusBadge } from "./StatusBadge";
import { Badge } from "../ui/badge";
import { useI18n } from "../../lib/i18n";

type CapabilitySummaryProps = {
  payload: Record<string, any> | null;
  status: string;
  compact?: boolean;
};

export function CapabilitySummary({ payload, status, compact = false }: CapabilitySummaryProps) {
  const { lang } = useI18n();
  const isRuntime = status === "runtime";
  const isPreview = status === "browser-preview";
  const stats = isRuntime ? payload?.stats ?? {} : {};
  const metrics = isRuntime ? payload?.metrics ?? {} : {};
  const cache = isRuntime ? payload?.cache_summary ?? {} : {};
  const memory = isRuntime ? payload?.memory ?? {} : {};
  const runtime = payload?.runtime ?? {};
  const sections = Array.isArray(payload?.dashboard_sections) ? payload.dashboard_sections : [];
  const requestTotal = numberAt(stats, ["requests", "total"]);
  const tokensSaved = numberAt(stats, ["tokens", "saved"]);
  const ccrEntries = number(cache.entries);
  const memoryEntries = number(memory.entries);
  const transforms = Array.isArray(metrics?.compression_performance?.transforms)
    ? metrics.compression_performance.transforms.length
    : 0;
  const routeRows = Array.isArray(metrics?.route_performance?.routes)
    ? metrics.route_performance.routes.length
    : 0;
  const outputShapeAvailable = isRuntime || isPreview;
  const rtkMode = typeof runtime.rtk === "string" ? runtime.rtk : isRuntime ? "integrated" : "waiting";

  const rows = [
    {
      key: "compression",
      icon: Gauge,
      title: lang === "zh" ? "压缩 pipeline" : "Compression pipeline",
      value: isRuntime ? `${tokensSaved.toLocaleString("en-US")} saved` : isPreview ? "preview" : "waiting",
      detail: isRuntime ? `${requestTotal} requests · ${transforms} transforms` : previewDetail(lang),
      ok: isRuntime ? requestTotal >= 0 : isPreview,
    },
    {
      key: "ccr",
      icon: Database,
      title: "CCR metadata",
      value: isRuntime ? `${ccrEntries} refs` : isPreview ? "preview" : "waiting",
      detail: rawAvailable(cache) ? "raw enabled" : lang === "zh" ? "默认只返回 metadata" : "metadata only by default",
      ok: isRuntime ? rawAvailable(cache) === false : isPreview,
    },
    {
      key: "memory",
      icon: Brain,
      title: lang === "zh" ? "Memory / Learn" : "Memory / Learn",
      value: isRuntime ? `${memoryEntries} records` : isPreview ? "preview" : "waiting",
      detail: lang === "zh" ? "只保存显式摘要和偏好 metadata" : "explicit summaries and preference metadata only",
      ok: isRuntime ? rawAvailable(memory) === false : isPreview,
    },
    {
      key: "output",
      icon: Sparkles,
      title: lang === "zh" ? "输出 shaping" : "Output shaping",
      value: outputShapeAvailable ? "available" : "waiting",
      detail: lang === "zh" ? "由项目策略和 learn metadata 驱动" : "driven by project policy and learn metadata",
      ok: outputShapeAvailable,
    },
    {
      key: "rtk",
      icon: Activity,
      title: "Integrated RTK",
      value: rtkMode,
      detail: lang === "zh" ? "作为 d-token runtime 内部能力展示" : "shown as internal runtime capability",
      ok: isRuntime ? rtkMode === "integrated" : isPreview,
    },
    {
      key: "route",
      icon: Route,
      title: lang === "zh" ? "Route adapter" : "Route adapter",
      value: isRuntime ? `${routeRows} route rows` : isPreview ? "preview" : "waiting",
      detail: lang === "zh" ? "官方 / 自定义 / cc-switch / local provider" : "official / custom / cc-switch / local provider",
      ok: isRuntime ? routeRows >= 0 : isPreview,
    },
    {
      key: "dashboard",
      icon: FileText,
      title: lang === "zh" ? "Dashboard primitives" : "Dashboard primitives",
      value: isRuntime ? `${sections.length} sections` : isPreview ? "preview" : "waiting",
      detail: lang === "zh" ? "stats / history / metrics / logs / export" : "stats / history / metrics / logs / export",
      ok: isRuntime ? sections.length > 0 : isPreview,
    },
    {
      key: "privacy",
      icon: ShieldCheck,
      title: lang === "zh" ? "隐私边界" : "Privacy boundary",
      value: rawAvailable(cache) || rawAvailable(memory) ? "check" : "redacted",
      detail: lang === "zh" ? "不显示 raw prompt / response / key / cookie" : "no raw prompt / response / key / cookie shown",
      ok: !rawAvailable(cache) && !rawAvailable(memory),
    },
  ];

  return (
    <GlassCard className={compact ? "p-4" : "p-5"}>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div>
          <h3 className="text-sm font-semibold">
            {lang === "zh" ? "Headroom-derived 能力状态" : "Headroom-derived capability status"}
          </h3>
          <div className="text-xs text-muted-foreground">
            {isRuntime
              ? lang === "zh" ? "来自 d-token-owned runtime payload，实时轮询更新。" : "From d-token-owned runtime payload with live polling."
              : isPreview
                ? lang === "zh" ? "浏览器预览，仅用于界面检查。" : "Browser preview for UI inspection only."
                : lang === "zh" ? "等待 d-token 本地服务数据。" : "Waiting for d-token local service data."}
          </div>
        </div>
        <Badge variant={isRuntime ? "default" : "secondary"}>{isRuntime ? "d-token-owned-runtime" : status}</Badge>
      </div>
      <div className={`grid gap-2 ${compact ? "md:grid-cols-2" : "sm:grid-cols-2 xl:grid-cols-4"}`}>
        {rows.map((row) => (
          <div key={row.key} className="glass-inset p-3">
            <div className="flex items-start justify-between gap-2">
              <div className="flex min-w-0 items-center gap-2">
                <row.icon className="size-4 shrink-0 text-primary" />
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium">{row.title}</div>
                  <div className="truncate text-xs text-muted-foreground">{row.detail}</div>
                </div>
              </div>
              <StatusBadge status={row.ok ? "healthy" : "warning"} label={row.ok ? "ok" : "pending"} />
            </div>
            <div className="mt-2 font-mono text-xs text-muted-foreground">{row.value}</div>
          </div>
        ))}
      </div>
    </GlassCard>
  );
}

function number(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function numberAt(root: any, path: string[]) {
  let value = root;
  for (const key of path) value = value?.[key];
  return number(value);
}

function rawAvailable(value: any) {
  return value?.raw_available === true;
}

function previewDetail(lang: string) {
  return lang === "zh" ? "预览不代表真实接入" : "preview is not integration proof";
}
