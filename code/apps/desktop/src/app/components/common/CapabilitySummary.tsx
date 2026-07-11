import { Activity, Brain, Database, FileText, Gauge, Route, ShieldCheck, Sparkles } from "lucide-react";
import { GlassCard } from "./GlassCard";
import { StatusBadge } from "./StatusBadge";
import { Badge } from "../ui/badge";
import { useI18n } from "../../lib/i18n";
import { classifyEvidence } from "../../lib/state/evidence";

type CapabilitySummaryProps = {
  payload: Record<string, any> | null;
  status: string;
  compact?: boolean;
};

type CapabilityRow = {
  key: string;
  icon: typeof Gauge;
  title: string;
  value: string;
  detail: string;
  status: "healthy" | "warning" | "error";
  statusLabel: string;
};

export function CapabilitySummary({ payload, status, compact = false }: CapabilitySummaryProps) {
  const { lang } = useI18n();
  const evidenceState = classifyEvidence({ payload, status, requireRuntimeIdentity: true });
  const verified = evidenceState === "verified";
  const stats = verified && isRecord(payload?.stats) ? payload.stats : {};
  const metrics = verified && isRecord(payload?.metrics) ? payload.metrics : {};
  const cache = verified && isRecord(payload?.cache_summary) ? payload.cache_summary : null;
  const memory = verified && isRecord(payload?.memory) ? payload.memory : null;
  const runtime = verified && isRecord(payload?.runtime) ? payload.runtime : {};
  const outputShape = verified && isRecord(payload?.output_shape) ? payload.output_shape : null;
  const sections = verified && Array.isArray(payload?.dashboard_sections) ? payload.dashboard_sections : [];
  const requestTotal = optionalNumberAt(stats, ["requests", "total"]);
  const tokensSaved = optionalNumberAt(stats, ["tokens", "saved"]);
  const ccrEntries = optionalNumber(cache?.entries);
  const memoryEntries = optionalNumber(memory?.entries);
  const transforms = Array.isArray(metrics?.compression_performance?.transforms)
    ? metrics.compression_performance.transforms.length
    : null;
  const routeRows = Array.isArray(metrics?.route_performance?.routes)
    ? metrics.route_performance.routes.length
    : null;
  const rtkMode = typeof runtime.rtk === "string" ? runtime.rtk : null;
  const noEvidence = unavailableCopy(evidenceState, lang);

  const rows: CapabilityRow[] = [
    {
      key: "compression",
      icon: Gauge,
      title: lang === "zh" ? "请求压缩" : "Request compression",
      value: verified && requestTotal !== null && requestTotal > 0 && tokensSaved !== null
        ? lang === "zh" ? `估算节省 ${tokensSaved.toLocaleString("en-US")} Token` : `Estimated ${tokensSaved.toLocaleString("en-US")} tokens saved`
        : "—",
      detail: verified && requestTotal !== null && requestTotal > 0
        ? lang === "zh"
          ? `${requestTotal} 条本地请求${transforms === null ? "" : ` · ${transforms} 类处理步骤`}`
          : `${requestTotal} local requests${transforms === null ? "" : ` · ${transforms} processing steps`}`
        : verified ? (lang === "zh" ? "暂无真实请求记录" : "No real request records") : noEvidence,
      status: verified && requestTotal !== null && requestTotal > 0 && tokensSaved !== null ? "healthy" : "warning",
      statusLabel: verified && requestTotal !== null && requestTotal > 0 && tokensSaved !== null
        ? lang === "zh" ? "有运行记录" : "Recorded"
        : lang === "zh" ? "待有请求" : "Waiting",
    },
    {
      key: "ccr",
      icon: Database,
      title: lang === "zh" ? "可恢复压缩索引" : "Recoverable compression index",
      value: ccrEntries === null ? "—" : lang === "zh" ? `${ccrEntries} 条索引` : `${ccrEntries} indexes`,
      detail: cache
        ? rawAvailabilityDetail(cache, lang)
        : verified ? (lang === "zh" ? "本地服务未返回索引状态" : "No local index status returned") : noEvidence,
      status: rawAvailable(cache) === true ? "error" : rawAvailable(cache) === false ? "healthy" : "warning",
      statusLabel: cache
        ? rawAvailable(cache) === true
          ? lang === "zh" ? "需检查" : "Review"
          : rawAvailable(cache) === false
            ? lang === "zh" ? "已验证" : "Verified"
            : lang === "zh" ? "待验证" : "Unverified"
        : lang === "zh" ? "无证据" : "No evidence",
    },
    {
      key: "memory",
      icon: Brain,
      title: lang === "zh" ? "项目记忆" : "Project memory",
      value: memoryEntries === null ? "—" : lang === "zh" ? `${memoryEntries} 条记录` : `${memoryEntries} records`,
      detail: memory
        ? rawAvailabilityDetail(memory, lang)
        : verified ? (lang === "zh" ? "本地服务未返回项目记忆状态" : "No project memory status returned") : noEvidence,
      status: rawAvailable(memory) === true ? "error" : rawAvailable(memory) === false ? "healthy" : "warning",
      statusLabel: memory
        ? rawAvailable(memory) === true
          ? lang === "zh" ? "需检查" : "Review"
          : rawAvailable(memory) === false
            ? lang === "zh" ? "已验证" : "Verified"
            : lang === "zh" ? "待验证" : "Unverified"
        : lang === "zh" ? "无证据" : "No evidence",
    },
    {
      key: "output",
      icon: Sparkles,
      title: lang === "zh" ? "输出精简" : "Output reduction",
      value: outputShape ? (lang === "zh" ? "已返回本地状态" : "Local status returned") : "—",
      detail: outputShape
        ? lang === "zh" ? "依据当前项目策略与本地偏好" : "Based on current project policy and local preferences"
        : verified ? (lang === "zh" ? "暂无可核验的输出精简状态" : "No verifiable output reduction status") : noEvidence,
      status: outputShape ? "healthy" : "warning",
      statusLabel: outputShape
        ? lang === "zh" ? "已读取" : "Loaded"
        : lang === "zh" ? "无证据" : "No evidence",
    },
    {
      key: "rtk",
      icon: Activity,
      title: lang === "zh" ? "命令输出精简" : "Command output reduction",
      value: rtkMode === "integrated" ? (lang === "zh" ? "产品内置能力" : "Built-in capability") : "—",
      detail: rtkMode === "integrated"
        ? lang === "zh" ? "这是实现能力声明，不代表本机已有运行记录" : "Implementation capability; not proof of local activity"
        : verified ? (lang === "zh" ? "本地服务未返回能力声明" : "No capability declaration returned") : noEvidence,
      status: "warning",
      statusLabel: rtkMode === "integrated"
        ? lang === "zh" ? "能力声明" : "Catalog"
        : lang === "zh" ? "无证据" : "No evidence",
    },
    {
      key: "route",
      icon: Route,
      title: lang === "zh" ? "连接路径" : "Connection paths",
      value: routeRows !== null && routeRows > 0
        ? lang === "zh" ? `${routeRows} 条有记录路径` : `${routeRows} recorded paths`
        : "—",
      detail: routeRows !== null && routeRows > 0
        ? lang === "zh" ? "来自本地请求的路由统计" : "Route statistics from local requests"
        : verified ? (lang === "zh" ? "暂无真实路由记录" : "No real route records") : noEvidence,
      status: routeRows !== null && routeRows > 0 ? "healthy" : "warning",
      statusLabel: routeRows !== null && routeRows > 0
        ? lang === "zh" ? "有运行记录" : "Recorded"
        : lang === "zh" ? "待有请求" : "Waiting",
    },
    {
      key: "dashboard",
      icon: FileText,
      title: lang === "zh" ? "本地数据面板" : "Local data surface",
      value: sections.length > 0
        ? lang === "zh" ? `${sections.length} 类数据` : `${sections.length} data groups`
        : "—",
      detail: sections.length > 0
        ? lang === "zh" ? "本地服务已返回可展示的数据范围" : "The local service returned its available data scope"
        : verified ? (lang === "zh" ? "本地服务未返回数据范围" : "No data scope returned") : noEvidence,
      status: sections.length > 0 ? "healthy" : "warning",
      statusLabel: sections.length > 0
        ? lang === "zh" ? "已读取" : "Loaded"
        : lang === "zh" ? "无证据" : "No evidence",
    },
    {
      key: "privacy",
      icon: ShieldCheck,
      title: lang === "zh" ? "隐私边界" : "Privacy boundary",
      value: privacyVerified(cache, memory)
        ? lang === "zh" ? "原始内容默认不可用" : "Raw content unavailable by default"
        : "—",
      detail: privacyVerified(cache, memory)
        ? lang === "zh" ? "界面不展示原始提示词、回复或凭据" : "Raw prompts, responses, and credentials are not shown"
        : verified ? (lang === "zh" ? "缺少完整的本地隐私状态" : "Complete local privacy state is missing") : noEvidence,
      status: privacyVerified(cache, memory) ? "healthy" : "warning",
      statusLabel: privacyVerified(cache, memory)
        ? lang === "zh" ? "已验证" : "Verified"
        : lang === "zh" ? "待验证" : "Unverified",
    },
  ];

  return (
    <GlassCard className={compact ? "p-4" : "p-5"} hover={false}>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div>
          <h3 className="text-sm font-semibold">{lang === "zh" ? "本地能力状态" : "Local capability status"}</h3>
          <div className="text-xs text-muted-foreground">{capabilitySourceDetail(evidenceState, lang)}</div>
        </div>
        <Badge variant={verified ? "default" : "secondary"}>{capabilitySourceLabel(evidenceState, lang)}</Badge>
      </div>
      <div className={`grid gap-2 ${compact ? "md:grid-cols-2" : "sm:grid-cols-2 xl:grid-cols-4"}`}>
        {rows.map((row) => (
          <div key={row.key} className="glass-inset p-3">
            <div className="flex items-start justify-between gap-2">
              <div className="flex min-w-0 items-center gap-2">
                <row.icon className="size-4 shrink-0 text-primary" />
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium">{row.title}</div>
                  <div className="truncate text-xs text-muted-foreground" title={row.detail}>{row.detail}</div>
                </div>
              </div>
              <StatusBadge status={row.status} label={row.statusLabel} />
            </div>
            <div className="mt-2 text-xs text-muted-foreground">{row.value}</div>
          </div>
        ))}
      </div>
    </GlassCard>
  );
}

function optionalNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

function optionalNumberAt(root: any, path: string[]) {
  let value = root;
  for (const key of path) value = value?.[key];
  return optionalNumber(value);
}

function rawAvailable(value: Record<string, any> | null): boolean | null {
  return typeof value?.raw_available === "boolean" ? value.raw_available : null;
}

function privacyVerified(cache: Record<string, any> | null, memory: Record<string, any> | null) {
  return rawAvailable(cache) === false && rawAvailable(memory) === false;
}

function rawAvailabilityDetail(value: Record<string, any>, lang: string) {
  if (rawAvailable(value) === false) return lang === "zh" ? "默认只返回脱敏索引与摘要" : "Redacted indexes and summaries only by default";
  if (rawAvailable(value) === true) return lang === "zh" ? "本地状态允许原始内容，需要检查隐私设置" : "Raw content is locally available; review privacy settings";
  return lang === "zh" ? "未说明原始内容是否可用" : "Raw-content availability was not reported";
}

function unavailableCopy(state: string, lang: string) {
  if (state === "preview") return lang === "zh" ? "浏览器预览不读取本机能力状态" : "Browser preview does not read local capability status";
  if (state === "loading") return lang === "zh" ? "正在读取本地服务状态" : "Loading local service status";
  if (state === "stale") return lang === "zh" ? "本地能力数据已过期" : "Local capability data is stale";
  if (state === "partial") return lang === "zh" ? "本地能力数据不完整" : "Local capability data is incomplete";
  return lang === "zh" ? "本地能力状态暂不可用" : "Local capability status unavailable";
}

function capabilitySourceDetail(state: string, lang: string) {
  if (state === "verified") return lang === "zh" ? "只展示 d-token 本地服务已经返回的运行证据。" : "Only runtime evidence returned by the local d-token service is shown.";
  return unavailableCopy(state, lang);
}

function capabilitySourceLabel(state: string, lang: string) {
  if (state === "verified") return lang === "zh" ? "实时本地数据" : "Live local data";
  if (state === "preview") return lang === "zh" ? "浏览器预览" : "Browser preview";
  if (state === "loading") return lang === "zh" ? "读取中" : "Loading";
  if (state === "stale") return lang === "zh" ? "数据已过期" : "Stale data";
  if (state === "partial") return lang === "zh" ? "数据不完整" : "Partial data";
  return lang === "zh" ? "数据不可用" : "Unavailable";
}

function isRecord(value: unknown): value is Record<string, any> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
