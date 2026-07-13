import { useEffect, useMemo, useRef, useState } from "react";
import { motion } from "motion/react";
import { RefreshCw, Wrench, RotateCcw, ShieldCheck, PackageOpen, ChevronRight } from "lucide-react";
import { Surface } from "../common/Surface";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { CapabilitySummary } from "../common/CapabilitySummary";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Button } from "../ui/button";
import { useI18n, useT } from "../../lib/i18n";
import type { DoctorCheck } from "../../lib/doctor/types";
import type { PageId } from "../../lib/nav";
import { invokeNative, type NativeActionResult } from "../../lib/native";
import { classifyEvidence } from "../../lib/state/evidence";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";

const LAYERS: DoctorCheck["layer"][] = ["runtime", "port", "route", "provider", "security"];

type VerificationCheck = {
  id: string;
  label: string;
  ok: boolean;
  summary: string;
};

type VerificationResult = {
  ok: boolean;
  passed: number;
  total: number;
  summary: string;
  checks: VerificationCheck[];
  service_url?: string;
};

type ProductReadinessResult = {
  ok: boolean;
  mode: "product-readiness-check";
  passed: number;
  total: number;
  summary: string;
  privacy: string;
  external_services: string;
  checks: Array<{
    id: string;
    label: string;
    ok: boolean;
    summary: unknown;
    detail?: unknown;
  }>;
};

export const DOCTOR_READINESS_TITLE = "产品就绪检查";

type DoctorSource = "preview" | "native" | "unavailable" | "stale" | "partial";
type DoctorActionState = "idle" | "running" | "success" | "warning" | "error";

type ReadinessCountEvidence = {
  passed: number | null;
  total: number | null;
  complete: boolean;
};

type DoctorReadResult = {
  loaded: boolean;
  healthy: boolean;
};

type ActiveConfigEvidence = {
  ok: boolean;
  summary: string;
};

export function resolveDoctorDestination(target?: string): PageId {
  if (target === "advanced" || target === "install") return "install";
  return "changes";
}

export function doctorFailureSummary(error: unknown, lang: "zh" | "en" = "zh"): string {
  if (error instanceof Error && error.message === "native_unavailable") {
    return lang === "zh"
      ? "当前是浏览器预览，请在 d-token 桌面窗口中运行完整诊断。"
      : "Run the full diagnosis in the d-token desktop window; the browser is preview-only.";
  }
  return lang === "zh" ? "诊断失败，请稍后重试或查看连接日志。" : "Diagnosis failed. Try again or review connection logs.";
}

export function doctorFailureDetail(error: unknown, lang: "zh" | "en"): string {
  if (error instanceof Error && error.message === "native_unavailable") return "";
  const message = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  const normalized = message.trim().slice(0, 240);
  if (!normalized) return "";
  if (/authorization|bearer\s|api[_-]?key|cookie|secret|(?:^|\W)token(?:\W|$)|sk-[a-z0-9]|\/Users\/|[A-Za-z]:\\/i.test(normalized)) return "";
  const containsCjk = /[\u3400-\u9fff]/u.test(normalized);
  if ((lang === "zh" && !containsCjk) || (lang === "en" && containsCjk)) return "";
  return normalized;
}

export function readinessCountEvidence(result: Pick<ProductReadinessResult, "passed" | "total" | "checks">): ReadinessCountEvidence {
  const passed = finiteCountOrNull(result.passed);
  const total = finiteCountOrNull(result.total);
  const complete = passed !== null
    && total !== null
    && total > 0
    && passed <= total
    && Array.isArray(result.checks)
    && result.checks.length === total;
  return { passed, total, complete };
}

export function doctorSourceFromPayload(payload: Record<string, any> | null): DoctorSource {
  const evidence = classifyEvidence({ payload, status: "runtime", requireRuntimeIdentity: true });
  if (evidence === "verified") return "native";
  if (evidence === "preview") return "preview";
  if (evidence === "stale" || evidence === "partial") return evidence;
  return "unavailable";
}

export function hasCompressionEvidence(payload: Record<string, any> | null): boolean {
  if (!Array.isArray(payload?.request_logs)) return false;

  return payload.request_logs.some((entry: any) => {
    const explicitElapsed = entry?.compression_elapsed_us ?? entry?.compression?.elapsed_us;
    if (isNonNegativeFiniteNumber(explicitElapsed)) return true;

    const before = entry?.input_tokens_before;
    const after = entry?.input_tokens_after;
    const transforms = Array.isArray(entry?.transforms_applied)
      ? entry.transforms_applied.filter((item: unknown) => typeof item === "string" && item.trim().length > 0)
      : [];
    return isNonNegativeFiniteNumber(before)
      && isNonNegativeFiniteNumber(after)
      && transforms.some((item: string) => item !== "compression_error_fallback");
  });
}

export function resolveActiveConfigEvidence(
  payload: Record<string, any> | null,
  desktopStatus: Record<string, any> | null,
  lang: "zh" | "en" = "zh",
): ActiveConfigEvidence {
  const zh = lang === "zh";
  const runtimeEnvelope = payload?.runtime_active_config ?? payload?.active_config;
  const runtimeConfig = runtimeEnvelope?.config;
  const hasRuntimeContract = runtimeConfig && typeof runtimeConfig === "object";

  if (hasRuntimeContract) {
    const ownedRuntime = runtimeEnvelope?.source === "dtoken-owned-runtime";
    const loaded = runtimeConfig.loaded === true;
    const routeCount = finiteCountOrNull(runtimeConfig.route_count);
    const providerCount = finiteCountOrNull(runtimeConfig.provider_count);
    const fallbackDisabled = runtimeConfig.fallback_disabled === true;
    const configSource = safeStatusText(runtimeConfig.source, zh ? "来源未知" : "source unknown");
    return {
      ok: ownedRuntime && loaded && routeCount !== null && routeCount > 0 && providerCount !== null && providerCount > 0 && fallbackDisabled,
      summary: zh
        ? `来源=${configSource} · 路由=${routeCount ?? "—"} · 上游=${providerCount ?? "—"} · 回退=${fallbackDisabled ? "已禁用" : "未证明已禁用"}`
        : `source=${configSource} · routes=${routeCount ?? "—"} · providers=${providerCount ?? "—"} · fallback=${fallbackDisabled ? "disabled" : "not proven disabled"}`,
    };
  }

  const state = safeStatusText(desktopStatus?.state, zh ? "未知" : "unknown");
  const source = safeStatusText(desktopStatus?.source, zh ? "未知" : "unknown");
  const routeCount = finiteCountOrNull(desktopStatus?.route_count);
  const providerCount = finiteCountOrNull(desktopStatus?.provider_count);
  const fallbackDisabled = desktopStatus?.fallback_disabled === true;
  return {
    ok: source === "d-token-desktop"
      && state === "loaded"
      && routeCount !== null
      && routeCount > 0
      && providerCount !== null
      && providerCount > 0
      && fallbackDisabled,
    summary: zh
      ? `来源=${source} · 状态=${state} · 路由=${routeCount ?? "—"} · 上游=${providerCount ?? "—"} · 回退=${fallbackDisabled ? "已禁用" : "未证明已禁用"}`
      : `source=${source} · state=${state} · routes=${routeCount ?? "—"} · providers=${providerCount ?? "—"} · fallback=${fallbackDisabled ? "disabled" : "not proven disabled"}`,
  };
}

export function Doctor({ setPage }: { setPage: (p: PageId) => void }) {
  const t = useT();
  const { lang } = useI18n();
  const zh = lang === "zh";
  const { addNotification } = useNotifications();
  const { payload: sharedPayload, status: sharedStatus, refresh: refreshOperational } = useRuntimeDashboard();
  const [running, setRunning] = useState(false);
  const [agentSummary, setAgentSummary] = useState(() => zh ? "尚未检查 Agent 连接配置" : "Agent connection has not been checked");
  const [agentOk, setAgentOk] = useState<boolean | null>(null);
  const [runtimeSummary, setRuntimeSummary] = useState(() => zh ? "尚未读取 d-token 运行状态" : "d-token runtime status has not been read");
  const [runtimeOk, setRuntimeOk] = useState<boolean | null>(null);

  const [activeConfigSummary, setActiveConfigSummary] = useState(() => zh ? "尚未检查 d-token 活动路由配置" : "d-token active route configuration has not been checked");
  const [activeConfigOk, setActiveConfigOk] = useState<boolean | null>(null);
  const [verificationSummary, setVerificationSummary] = useState(() => zh ? "尚未读取 d-token 路由验证状态" : "d-token route verification has not been read");
  const [verificationOk, setVerificationOk] = useState<boolean | null>(null);
  const [verificationResult, setVerificationResult] = useState<VerificationResult | null>(null);
  const [doctorResult, setDoctorResult] = useState<NativeActionResult | null>(null);
  const doctorResultRef = useRef<NativeActionResult | null>(null);
  const [doctorSource, setDoctorSource] = useState<DoctorSource>("preview");
  const [runtimePayload, setRuntimePayload] = useState<Record<string, any> | null>(null);
  const [runtimePayloadStatus, setRuntimePayloadStatus] = useState("browser-preview");
  const [readinessResult, setReadinessResult] = useState<ProductReadinessResult | null>(null);
  const [actionState, setActionState] = useState<DoctorActionState>("idle");
  const [actionNotice, setActionNotice] = useState("");
  const [actionDetail, setActionDetail] = useState("");

  const readDoctorStatus = async (withDoctor = false) => {
    setRunning(true);
    try {
      const [doctor, payload, activeConfig, verification] = await Promise.all([
        withDoctor ? invokeNative<NativeActionResult>("run_doctor") : Promise.resolve(null),
        withDoctor
          ? invokeNative<Record<string, any>>("get_desktop_data")
          : Promise.resolve(sharedPayload ?? {}),
        invokeNative<Record<string, any>>("get_active_route_config_status"),
        invokeNative<VerificationResult>("verify_agent_route_config"),
      ]);
      const nextDoctorSource = doctorSourceFromPayload(payload);
      if (nextDoctorSource !== "native") {
        doctorResultRef.current = null;
        setDoctorResult(null);
        setDoctorSource(nextDoctorSource);
        setRuntimePayload(payload);
        setRuntimePayloadStatus(nextDoctorSource);
        setReadinessResult(null);
        setAgentOk(null);
        setAgentSummary(zh ? "缺少可验证的实时本地状态，未判断 Agent 连接是否正常。" : "No verifiable live local state is available; agent connectivity remains unknown.");
        setRuntimeOk(false);
        setRuntimeSummary(nextDoctorSource === "stale"
          ? (zh ? "d-token 运行数据已过期。" : "d-token runtime data is stale.")
          : (zh ? "d-token 运行数据不完整。" : "d-token runtime data is incomplete."));
        setActiveConfigOk(false);
        setActiveConfigSummary(zh ? "缺少当前活动路由配置的完整证据。" : "Complete active-route evidence is unavailable.");
        setVerificationOk(false);
        setVerificationResult(null);
        setVerificationSummary(zh ? "缺少实时路由验证证据。" : "Live route-verification evidence is unavailable.");
        return { loaded: false, healthy: false } satisfies DoctorReadResult;
      }

      if (doctor) {
        doctorResultRef.current = doctor;
        setDoctorResult(doctor);
      }
      if (withDoctor) void refreshOperational();
      setDoctorSource("native");
      const toolSource = doctor ?? doctorResultRef.current;
      const supportedAgent = toolSource?.tools.find((tool) => ["opencode", "claude", "codex", "cline", "cursor"].includes(tool.id));
      const routeCount = Array.isArray(payload.route_decisions) ? String(payload.route_decisions.length) : "—";
      const requestCount = isNonNegativeFiniteNumber(payload?.stats?.requests?.total)
        ? String(Math.floor(Number(payload.stats.requests.total)))
        : "—";
      const source = String(payload?.source ?? "unknown");
      const desktopSource = String(payload?.desktop_data_source ?? "");
      const isRuntimePayload = source === "dtoken-owned-runtime" && desktopSource === "dtoken-local-service";
      setRuntimePayload(payload);
      setRuntimePayloadStatus("runtime");
      if (!toolSource) {
        setAgentOk(null);
        setAgentSummary(zh ? "尚未运行完整诊断，暂不判断 Agent 连接状态。" : "A full diagnosis has not run; agent connectivity remains unknown.");
      } else if (!supportedAgent?.installed) {
        setAgentOk(false);
        setAgentSummary(zh ? "未检测到已支持的 Agent，请先在安装页连接或检查 PATH。" : "No supported agent was detected; connect one from Install or check PATH.");
      } else {
        setAgentOk(true);
        setAgentSummary(zh
          ? `${supportedAgent.label ?? "Agent"} 已检测到：${supportedAgent.version ?? supportedAgent.path ?? "已安装"} · 可继续验证 d-token 路由配置`
          : `${supportedAgent.label ?? "Agent"} detected: ${supportedAgent.version ?? supportedAgent.path ?? "installed"} · route verification can continue`);
      }
      setRuntimeOk(isRuntimePayload);
      const serviceUrl = typeof payload.desktop_service_url === "string" && payload.desktop_service_url.trim()
        ? payload.desktop_service_url
        : "—";
      setRuntimeSummary(zh
        ? `来源=${source} · 请求=${requestCount} · 路由决策=${routeCount} · 服务=${serviceUrl}`
        : `source=${source} · requests=${requestCount} · route decisions=${routeCount} · service=${serviceUrl}`);
      const activeEvidence = resolveActiveConfigEvidence(payload, activeConfig, lang);
      setActiveConfigOk(activeEvidence.ok);
      setActiveConfigSummary(activeEvidence.summary);
      setVerificationResult(verification);
      setVerificationOk(Boolean(verification.ok));
      setVerificationSummary(localizeVerificationOverall(verification, lang));
      return {
        loaded: true,
        healthy: isRuntimePayload
          && activeEvidence.ok
          && verification.ok === true
          && (!withDoctor || (doctor?.ok === true && supportedAgent?.installed === true)),
      } satisfies DoctorReadResult;
    } catch (error) {
      setAgentOk(false);
      setRuntimeOk(false);
      setActiveConfigOk(false);
      setVerificationOk(false);
      setVerificationResult(null);
      setAgentSummary(error instanceof Error && error.message === "native_unavailable"
        ? (zh ? "当前是浏览器预览，Doctor 需要在 Tauri 桌面窗口中运行。" : "Doctor requires the Tauri desktop window.")
        : (zh ? `Agent 诊断失败：${String(error)}` : `Agent diagnosis failed: ${String(error)}`));
      setRuntimeSummary(zh ? "无法读取 d-token 运行状态。" : "Unable to read d-token runtime status.");
      setActiveConfigSummary(zh ? "无法读取 d-token 活动路由配置状态。" : "Unable to read d-token active route configuration status.");
      setVerificationSummary(zh ? "无法读取 d-token 路由验证状态。" : "Unable to read d-token route-verification status.");
      if (withDoctor) setDoctorResult(null);
      setDoctorSource(error instanceof Error && error.message === "native_unavailable" ? "preview" : "unavailable");
      setRuntimePayload(null);
      setRuntimePayloadStatus(error instanceof Error && error.message === "native_unavailable" ? "browser-preview" : "unavailable");
      if (withDoctor) doctorResultRef.current = null;
      return { loaded: false, healthy: false } satisfies DoctorReadResult;
    } finally {
      setRunning(false);
    }
  };

  useEffect(() => {
    if (sharedStatus !== "loading") void readDoctorStatus(false);
  }, [lang, sharedPayload, sharedStatus]);

  const runAgentDoctor = async () => {
    const requestId = createOperationRequestId("doctor");
    setRunning(true);
    setActionState("running");
    setActionNotice(zh ? "正在检查本地服务、Agent、路由与安全边界…" : "Checking the local service, agents, routes, and safety boundaries…");
    setActionDetail("");
    try {
      const readiness = await invokeNative<ProductReadinessResult>("run_product_readiness_check");
      const counts = readinessCountEvidence(readiness);
      const countLabel = `${counts.passed ?? "—"}/${counts.total ?? "—"}`;
      setReadinessResult(readiness);
      setVerificationOk(readiness.ok === true && counts.complete && counts.passed === counts.total);
      setVerificationSummary(localizeReadinessOverall(readiness, lang));
      const details = await readDoctorStatus(true);
      const fullyPassed = readiness.ok === true
        && counts.complete
        && counts.passed === counts.total
        && details.loaded
        && details.healthy;
      setActionState(fullyPassed ? "success" : "warning");
      const completion = fullyPassed
        ? (zh
            ? `检查完成：${countLabel} 项通过。`
            : `Checks complete: ${countLabel} passed.`)
        : (zh
            ? `检查完成，但仍有问题：${countLabel} 项通过。${!counts.complete ? "检查计数证据不完整。" : !details.loaded ? "当前诊断明细不可用。" : "诊断明细仍有未通过项。"}`
            : `Checks complete with issues: ${countLabel} passed. ${!counts.complete ? "Check-count evidence is incomplete." : !details.loaded ? "Current diagnostic details are unavailable." : "Some diagnostic details did not pass."}`);
      setActionNotice(completion);
      addNotification({
        dedupeKey: `doctor-result-${fullyPassed ? "passed" : "attention"}`,
        level: fullyPassed ? "success" : "warning",
        title: fullyPassed ? (zh ? "Doctor 检查已通过" : "Doctor checks passed") : (zh ? "Doctor 检查需要处理" : "Doctor checks need attention"),
        message: completion,
        persistent: true,
        details: `request_id=${requestId} · passed=${counts.passed ?? "unavailable"} · total=${counts.total ?? "unavailable"} · evidence_complete=${counts.complete} · runtime_loaded=${details.loaded}`,
        receipt: { requestId, targetId: "doctor" },
      });
    } catch (error) {
      setReadinessResult(null);
      const isPreview = error instanceof Error && error.message === "native_unavailable";
      setDoctorSource(isPreview ? "preview" : "unavailable");
      setVerificationOk(false);
      const failure = doctorFailureSummary(error, lang);
      setVerificationSummary(failure);
      setActionState("error");
      setActionNotice(failure);
      setActionDetail(doctorFailureDetail(error, lang));
      addNotification({ dedupeKey: "doctor-run-failed", level: "error", title: zh ? "Doctor 检查失败" : "Doctor checks failed", message: failure, persistent: true, details: `request_id=${requestId}`, receipt: { requestId, targetId: "doctor" } });
    } finally {
      setRunning(false);
    }
  };

  const visibleChecks = useMemo(
    () => buildDoctorChecks(doctorResult, doctorSource, {
      agentOk,
      agentSummary,
      runtimeOk,
      runtimeSummary,
      activeConfigOk,
      activeConfigSummary,
      verificationOk,
      verificationSummary,
      verificationResult,
    }),
    [
      doctorResult,
      doctorSource,
      agentOk,
      agentSummary,
      runtimeOk,
      runtimeSummary,
      activeConfigOk,
      activeConfigSummary,
      verificationOk,
      verificationSummary,
      verificationResult,
      lang,
    ],
  );
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h1>{t("dr.title")}</h1>
          <DataSourceBadge
            mode={doctorSource === "native" ? "native" : doctorSource === "preview" ? "preview" : "unavailable"}
            label={doctorSource === "native"
              ? (zh ? "实时本地诊断" : "Live local diagnosis")
              : doctorSource === "preview"
                ? (zh ? "浏览器预览" : "Browser preview")
                : doctorSource === "stale"
                  ? (zh ? "诊断数据已过期" : "Diagnostic data is stale")
                  : doctorSource === "partial"
                    ? (zh ? "诊断数据不完整" : "Diagnostic data is incomplete")
                    : (zh ? "诊断数据不可用" : "Diagnostic data unavailable")}
          />
        </div>
        <Button variant="outline" onClick={runAgentDoctor} disabled={running}>
          <RefreshCw className={`size-4 ${running ? "animate-spin" : ""}`} />{t("dr.runAll")}
        </Button>
      </div>

      {actionState !== "idle" && (
        <div
          data-doctor-action-result
          role={actionState === "error" ? "alert" : "status"}
          aria-live={actionState === "error" ? "assertive" : "polite"}
          className={`sticky top-0 z-20 rounded-lg border px-4 py-3 text-sm shadow-sm ${
            actionState === "success"
              ? "border-emerald-500/25 bg-emerald-500/[0.06] text-emerald-700 dark:text-emerald-300"
              : actionState === "warning"
                ? "border-amber-500/25 bg-amber-500/[0.06] text-amber-700 dark:text-amber-300"
                : actionState === "error"
                  ? "border-rose-500/25 bg-rose-500/[0.06] text-rose-700 dark:text-rose-300"
                  : "border-border/70 bg-muted/30 text-muted-foreground"
          }`}
        >
          <div className="flex items-center gap-2">
            <RefreshCw className={`size-4 shrink-0 ${actionState === "running" ? "animate-spin" : ""}`} />
            <span>{actionNotice}</span>
          </div>
          {actionDetail ? (
            <details className="mt-2 text-xs">
              <summary className="cursor-pointer font-medium">{zh ? "技术详情" : "Technical details"}</summary>
              <div className="mt-1 break-words font-mono text-current/80">{actionDetail}</div>
            </details>
          ) : null}
        </div>
      )}

      <Tabs defaultValue="checks">
        <TabsList>
          <TabsTrigger value="checks">{t("dr.tab.checks")}</TabsTrigger>
          <TabsTrigger value="runtime">{t("dr.tab.runtime")}</TabsTrigger>
          <TabsTrigger value="maintenance">{t("dr.tab.maintenance")}</TabsTrigger>
        </TabsList>

        <TabsContent value="checks" className="mt-4 space-y-4">
          {LAYERS.map((layer) => (
            <Surface key={layer} className="p-5">
              <h3 className="mb-3">{t(`dr.layer.${layer}`)}</h3>
              <div className="divide-y divide-border/60">
                {visibleChecks.filter((c) => c.layer === layer).map((c, i) => (
                  <motion.div key={c.id} initial={{ opacity: 0, x: -8 }} animate={{ opacity: 1, x: 0 }} transition={{ delay: i * 0.04 }}
                    data-flat-row
                    className="flex flex-wrap items-center justify-between gap-2 py-3">
                    <div>
                      <div>{lang === "zh" ? c.name : c.nameEn}</div>
                      <div className="text-xs text-muted-foreground">{lang === "zh" ? c.detail : c.detailEn}</div>
                    </div>
                    <div className="flex items-center gap-2">
                      <StatusBadge status={c.status} label={c.status === "healthy" ? t("common.healthy") : c.status === "warning" ? t("common.warning") : t("common.error")} />
                      {c.status !== "healthy" && c.repair && (
                        <Button size="sm" variant="ghost" onClick={() => setPage(resolveDoctorDestination(c.repair))} className="text-primary">
                          {t("dr.repair")}<ChevronRight className="size-3.5" />
                        </Button>
                      )}
                    </div>
                  </motion.div>
                ))}
              </div>
            </Surface>
          ))}
        </TabsContent>

        <TabsContent value="runtime" className="mt-4 space-y-3">
          <CapabilitySummary payload={runtimePayload} status={runtimePayloadStatus} compact />
          <Surface className="flex flex-wrap items-center justify-between gap-3 p-4">
            <div>
              <div>{zh ? "Agent 连接状态" : "Agent connection"}</div>
              <div className="text-xs text-muted-foreground">{agentSummary}</div>
            </div>
            <div className="flex items-center gap-2">
              <StatusBadge
                status={agentOk === null ? "warning" : agentOk ? "healthy" : "error"}
                label={agentOk === null ? (zh ? "待验证" : "Pending") : agentOk ? t("common.healthy") : t("common.error")}
              />
              <Button size="sm" variant="outline" onClick={runAgentDoctor} disabled={running}>
                <RefreshCw className={`size-3.5 ${running ? "animate-spin" : ""}`} />
                {zh ? "检测" : "Detect"}
              </Button>
            </div>
          </Surface>
          <Surface className="flex flex-wrap items-center justify-between gap-3 p-4">
            <div>
              <div>{zh ? "d-token 运行状态" : "d-token runtime status"}</div>
              <div className="text-xs text-muted-foreground">{runtimeSummary}</div>
            </div>
            <StatusBadge
              status={runtimeOk === null ? "warning" : runtimeOk ? "healthy" : "error"}
              label={runtimeOk === null ? (zh ? "待验证" : "Pending") : runtimeOk ? t("common.healthy") : t("common.error")}
            />
          </Surface>
          <Surface className="flex flex-wrap items-center justify-between gap-3 p-4">
            <div>
              <div>{zh ? "d-token 活动路由配置" : "d-token active route configuration"}</div>
              <div className="text-xs text-muted-foreground">{activeConfigSummary}</div>
            </div>
            <StatusBadge
              status={activeConfigOk === null ? "warning" : activeConfigOk ? "healthy" : "error"}
              label={activeConfigOk === null ? (zh ? "待验证" : "Pending") : activeConfigOk ? t("common.healthy") : t("common.error")}
            />
          </Surface>
          {readinessResult && doctorSource === "native" && (
            <Surface className="p-4">
              <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
                <div>
                  <div className="font-medium">{zh ? DOCTOR_READINESS_TITLE : "Product readiness checks"}</div>
                  <div className="text-xs text-muted-foreground">{localizeReadinessOverall(readinessResult, lang)}</div>
                </div>
                <StatusBadge
                  status={readinessResult.ok ? "healthy" : "warning"}
                  label={`${readinessResult.passed}/${readinessResult.total}`}
                />
              </div>
              <div className="divide-y divide-border/60">
                {readinessResult.checks.map((check) => (
                  <div key={check.id} data-flat-row className="flex items-start justify-between gap-3 py-3">
                    <div className="min-w-0">
                      <div className="text-sm font-medium">{localizeReadinessCheck(check.id, check.ok, lang).label}</div>
                      <div className="mt-0.5 break-words text-xs text-muted-foreground">{localizeReadinessCheck(check.id, check.ok, lang).summary}</div>
                    </div>
                    <StatusBadge status={check.ok ? "healthy" : "warning"} label={check.ok ? t("common.healthy") : t("common.warning")} />
                  </div>
                ))}
              </div>
              <div className="mt-3 text-xs text-muted-foreground">
                {zh
                  ? "仅检查安装、连接、验证、诊断、状态、日志和脱敏导出证据；不会返回原始提示词、响应或凭据。"
                  : "Checks install, connection, verification, diagnosis, status, logs, and redacted export evidence only; raw prompts, responses, and credentials are never returned."}
              </div>
            </Surface>
          )}
          <Surface className="flex flex-wrap items-center justify-between gap-3 p-4">
            <div>
              <div>{zh ? "Agent 路由验证" : "Agent route verification"}</div>
              <div className="text-xs text-muted-foreground">{verificationSummary}</div>
            </div>
            <StatusBadge
              status={verificationOk === null ? "warning" : verificationOk ? "healthy" : "error"}
              label={verificationOk === null ? (zh ? "待验证" : "Pending") : verificationOk ? t("common.healthy") : t("common.error")}
            />
          </Surface>
          {verificationResult?.checks?.length ? (
            <Surface className="p-4">
              <div className="mb-3 flex items-center gap-2">
                <ShieldCheck className="size-4 text-primary" />
                <div>{zh ? "配置验证明细" : "Configuration verification details"}</div>
              </div>
              <div className="divide-y divide-border/60">
                {verificationResult.checks.map((check) => (
                  <div key={check.id} data-flat-row className="flex items-center justify-between gap-3 py-3">
                    <div>
                      <div className="text-sm">{localizeVerificationCheck(check.id, check.ok, lang).label}</div>
                      <div className="text-xs text-muted-foreground">{localizeVerificationCheck(check.id, check.ok, lang).summary}</div>
                    </div>
                    <StatusBadge status={check.ok ? "healthy" : "warning"} label={check.ok ? t("common.healthy") : t("common.warning")} />
                  </div>
                ))}
              </div>
            </Surface>
          ) : null}
        </TabsContent>

        <TabsContent value="maintenance" className="mt-4 space-y-4">
          <p className="text-sm text-muted-foreground">{t("dr.maint.desc")}</p>
          <div className="grid gap-4 sm:grid-cols-2">
            {[
              { icon: RotateCcw, label: t("dr.maint.restart"), page: "install" as PageId },
              { icon: PackageOpen, label: t("dr.maint.release"), page: "changes" as PageId },
              { icon: Wrench, label: t("dr.maint.rebuild"), page: resolveDoctorDestination("advanced") },
              { icon: ShieldCheck, label: t("dr.maint.clean"), page: resolveDoctorDestination("logs") },
            ].map((m) => (
              <Surface key={m.label} className="flex items-center justify-between p-4">
                <span className="flex items-center gap-3"><m.icon className="size-5 text-primary" />{m.label}</span>
                <Button size="sm" variant="outline" onClick={() => setPage(m.page)}>
                  {zh ? "打开" : "Open"}
                </Button>
              </Surface>
            ))}
          </div>
        </TabsContent>
      </Tabs>
    </div>
  );
}


function isNonNegativeFiniteNumber(value: unknown): boolean {
  if (typeof value !== "number" && typeof value !== "string") return false;
  if (typeof value === "string" && value.trim().length === 0) return false;
  const number = Number(value);
  return Number.isFinite(number) && number >= 0;
}

function finiteCountOrNull(value: unknown): number | null {
  if (!isNonNegativeFiniteNumber(value)) return null;
  return Math.floor(Number(value));
}

function formatEvidenceCount(value: unknown): string {
  return isNonNegativeFiniteNumber(value) ? String(Math.floor(Number(value))) : "—";
}

type LocalizedCheckCopy = { label: string; summary: string };

const READINESS_CHECK_LABELS: Record<string, { zh: string; en: string }> = {
  install_connect: { zh: "安装与连接计划", en: "Install and connection plan" },
  agent_detection: { zh: "Agent 与工具检测", en: "Agent and tool detection" },
  route_config: { zh: "Agent 路由配置", en: "Agent route configuration" },
  compression_data: { zh: "压缩与概览数据", en: "Compression and dashboard data" },
  hook_or_config_effect: { zh: "配置生效状态", en: "Configuration activation" },
  local_transaction_state: { zh: "本地事务状态", en: "Local transaction state" },
  headroom_derived_capabilities: { zh: "上下文优化能力", en: "Context optimization capabilities" },
  export_redaction: { zh: "日志与导出脱敏", en: "Log and export redaction" },
  doctor: { zh: "Doctor 诊断", en: "Doctor diagnostics" },
};

const VERIFICATION_CHECK_LABELS: Record<string, { zh: string; en: string }> = {
  active_config: { zh: "活动路由配置", en: "Active route configuration" },
  fallback: { zh: "回退边界", en: "Fallback boundary" },
  runtime_health: { zh: "本地服务健康状态", en: "Local service health" },
  runtime_active_config: { zh: "运行时路由加载", en: "Runtime route loading" },
  dashboard: { zh: "概览数据来源", en: "Dashboard data source" },
  export_redaction: { zh: "脱敏导出", en: "Redacted export" },
};

export function localizeReadinessCheck(id: string, ok: boolean, lang: "zh" | "en"): LocalizedCheckCopy {
  const label = READINESS_CHECK_LABELS[id]?.[lang] ?? (lang === "zh" ? "产品就绪检查项" : "Product readiness check");
  return {
    label,
    summary: ok
      ? (lang === "zh" ? `${label}已验证。` : `${label} verified.`)
      : (lang === "zh" ? `${label}尚未通过。` : `${label} has not passed.`),
  };
}

export function localizeVerificationCheck(id: string, ok: boolean, lang: "zh" | "en"): LocalizedCheckCopy {
  const label = VERIFICATION_CHECK_LABELS[id]?.[lang] ?? (lang === "zh" ? "路由验证项" : "Route verification check");
  return {
    label,
    summary: ok
      ? (lang === "zh" ? `${label}已验证。` : `${label} verified.`)
      : (lang === "zh" ? `${label}需要处理。` : `${label} needs attention.`),
  };
}

function localizeReadinessOverall(result: ProductReadinessResult, lang: "zh" | "en") {
  const passed = formatEvidenceCount(result.passed);
  const total = formatEvidenceCount(result.total);
  return result.ok
    ? (lang === "zh" ? `产品就绪检查已通过：${passed}/${total}。` : `Product readiness passed: ${passed}/${total}.`)
    : (lang === "zh" ? `产品就绪检查仍有未通过项：${passed}/${total}。` : `Product readiness still needs attention: ${passed}/${total}.`);
}

function localizeVerificationOverall(result: VerificationResult, lang: "zh" | "en") {
  const passed = formatEvidenceCount(result.passed);
  const total = formatEvidenceCount(result.total);
  return result.ok
    ? (lang === "zh" ? `Agent 路由验证已通过：${passed}/${total}。` : `Agent route verification passed: ${passed}/${total}.`)
    : (lang === "zh" ? `Agent 路由验证仍有未通过项：${passed}/${total}。` : `Agent route verification still needs attention: ${passed}/${total}.`);
}

function safeStatusText(value: unknown, fallback: string): string {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : fallback;
}

export function buildDoctorChecks(
  result: NativeActionResult | null,
  source: DoctorSource,
  state: {
    agentOk: boolean | null;
    agentSummary: string;
    runtimeOk: boolean | null;
    runtimeSummary: string;
    activeConfigOk: boolean | null;
    activeConfigSummary: string;
    verificationOk: boolean | null;
    verificationSummary: string;
    verificationResult: VerificationResult | null;
  },
): DoctorCheck[] {
  if (source !== "native") return nonNativeDoctorChecks(source);

  const statusFor = (ok: boolean | null): DoctorCheck["status"] => (ok === null ? "warning" : ok ? "healthy" : "error");
  const liveChecks: DoctorCheck[] = [
    {
      id: "live-agent",
      layer: "route",
      name: "Agent 连接状态",
      nameEn: "Agent connection",
      status: statusFor(state.agentOk),
      detail: state.agentSummary,
      detailEn: state.agentSummary,
      repair: state.agentOk ? undefined : "advanced",
    },
    {
      id: "live-runtime",
      layer: "runtime",
      name: "d-token runtime payload",
      nameEn: "d-token runtime payload",
      status: statusFor(state.runtimeOk),
      detail: state.runtimeSummary,
      detailEn: state.runtimeSummary,
      repair: state.runtimeOk ? undefined : "advanced",
    },
    {
      id: "live-active-config",
      layer: "route",
      name: "d-token active route config",
      nameEn: "d-token active route config",
      status: statusFor(state.activeConfigOk),
      detail: state.activeConfigSummary,
      detailEn: state.activeConfigSummary,
      repair: state.activeConfigOk ? undefined : "changes",
    },
    {
      id: "live-route-verification",
      layer: "security",
      name: "Agent route verification",
      nameEn: "Agent route verification",
      status: statusFor(state.verificationOk),
      detail: state.verificationSummary,
      detailEn: state.verificationSummary,
      repair: state.verificationOk ? undefined : "logs",
    },
  ];

  const verificationChecks: DoctorCheck[] = (state.verificationResult?.checks ?? []).map((check) => {
    const zhCopy = localizeVerificationCheck(check.id, check.ok, "zh");
    const enCopy = localizeVerificationCheck(check.id, check.ok, "en");
    return {
      id: `verify-${check.id}`,
      layer: check.id.includes("export") || check.id.includes("fallback") ? "security" : check.id.includes("runtime") ? "runtime" : "route",
      name: zhCopy.label,
      nameEn: enCopy.label,
      status: check.ok ? "healthy" : "warning",
      detail: zhCopy.summary,
      detailEn: enCopy.summary,
      repair: check.ok ? undefined : "logs",
    };
  });

  if (!result) return [...liveChecks, ...verificationChecks];

  const toolChecks: DoctorCheck[] = result.tools.map((tool) => ({
    id: `tool-${tool.id}`,
    layer: layerForTool(tool.id),
    name: `${tool.label} ${tool.installed ? "已检测" : "未检测到"}`,
    nameEn: `${tool.label} ${tool.installed ? "detected" : "missing"}`,
    // OpenCode is the current acceptance Agent. RTK and external Headroom are
    // optional no-touch references, so their absence is honest information,
    // not a failed product-readiness check.
    status: tool.installed ? "healthy" : tool.id === "opencode" ? "error" : "healthy",
    detail: tool.version ?? tool.path ?? (tool.installed ? "已检测到本机工具" : "未检测到本机工具"),
    detailEn: tool.version ?? tool.path ?? (tool.installed ? "Local tool detected" : "Local tool not detected"),
    repair: !tool.installed && tool.id === "opencode" ? "advanced" : undefined,
  }));

  return [...liveChecks, ...verificationChecks, ...toolChecks];
}

const previewDoctorChecks: DoctorCheck[] = [
  {
    id: "preview-runtime",
    layer: "runtime",
    name: "浏览器预览",
    nameEn: "Browser preview",
    status: "warning",
    detail: "真实 Doctor 需要在 Tauri 桌面窗口中读取 d-token 本地服务。",
    detailEn: "Real Doctor checks require the Tauri desktop window.",
  },
  {
    id: "preview-route",
    layer: "route",
    name: "Agent 路由验证",
    nameEn: "Agent route verification",
    status: "warning",
    detail: "预览模式不会读取本机 Agent 配置或密钥。",
    detailEn: "Preview mode does not read local agent configuration or secrets.",
  },
  {
    id: "preview-loopback",
    layer: "security",
    name: "本地回环边界",
    nameEn: "Loopback boundary",
    status: "warning",
    detail: "产品默认使用 127.0.0.1，本页预览不会连接外部服务。",
    detailEn: "The product defaults to 127.0.0.1 and this preview does not connect external services.",
  },
  {
    id: "preview-redaction",
    layer: "security",
    name: "凭据脱敏",
    nameEn: "Credential redaction",
    status: "warning",
    detail: "Doctor 不显示 API key、Authorization、Cookie 或会话令牌。",
    detailEn: "Doctor does not display API keys, Authorization headers, cookies, or session tokens.",
  },
];

function nonNativeDoctorChecks(source: Exclude<DoctorSource, "native">): DoctorCheck[] {
  if (source === "preview") return previewDoctorChecks;
  const detail = source === "stale"
    ? "诊断数据已过期，不能作为当前健康状态。"
    : source === "partial"
      ? "诊断数据不完整，不能确认当前健康状态。"
      : "无法读取真实诊断数据，当前健康状态未知。";
  const detailEn = source === "stale"
    ? "Diagnostic data is stale and cannot represent current health."
    : source === "partial"
      ? "Diagnostic data is incomplete; current health is unverified."
      : "Live diagnostic data is unavailable; current health is unknown.";
  return LAYERS.map((layer) => ({
    id: `${source}-${layer}`,
    layer,
    name: `${titledLayer(layer)}状态未知`,
    nameEn: `${layer} status unknown`,
    status: "warning",
    detail,
    detailEn,
  }));
}

function titledLayer(layer: DoctorCheck["layer"]): string {
  if (layer === "runtime") return "运行时";
  if (layer === "port") return "本地端口";
  if (layer === "route") return "路由";
  if (layer === "provider") return "Provider";
  return "安全边界";
}

function layerForTool(id: string): DoctorCheck["layer"] {
  if (id.includes("rtk") || id.includes("headroom") || id.includes("dtoken")) return "runtime";
  if (id.includes("cc") || id.includes("switch") || id.includes("opencode") || id.includes("claude") || id.includes("codex")) return "route";
  return "provider";
}
