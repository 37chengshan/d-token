import { useEffect, useMemo, useState } from "react";
import { motion } from "motion/react";
import { Power, Network, ScanSearch, RefreshCw, Server, Route as RouteIcon, KeyRound, Wifi } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { CapabilitySummary } from "../common/CapabilitySummary";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Switch } from "../ui/switch";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { Input } from "../ui/input";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "../ui/alert-dialog";
import { useI18n, useT } from "../../lib/i18n";
import { invokeNative, isTauriRuntime, type NativeActionResult } from "../../lib/native";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";
import {
  isUsableRouteReview,
  routeApplyFailureMessage,
  routeApplyRequest,
  routeReviewFailureMessage,
  type RouteDraftApplyReview,
} from "../../lib/routes/review";

export {
  isUsableRouteReview,
  routeApplyFailureMessage,
  routeApplyRequest,
  routeReviewFailureMessage,
} from "../../lib/routes/review";

type Health = "healthy" | "warning" | "error";

const OUTPUT_LEVELS = [
  { key: "off", zh: "关闭", en: "Off" },
  { key: "safe", zh: "安全", en: "Safe" },
  { key: "balanced", zh: "均衡", en: "Balanced" },
  { key: "aggressive", zh: "激进", en: "Aggressive" },
  { key: "max", zh: "最大", en: "Max" },
] as const;

export function credentialReferenceForProvider(providerId: string): string {
  const slug = providerId
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "provider";
  return `os-credential://d-token/providers/${slug}`;
}

export function validateCredentialInput(value: string): boolean {
  const length = value.trim().length;
  return length > 0 && length <= 65_536;
}

export function Advanced() {
  const t = useT();
  const { lang } = useI18n();
  const { payload, status: payloadStatus, updatedAt, refresh: refreshPayload } = useRuntimeDashboard();
  const [activeConfig, setActiveConfig] = useState<Record<string, any> | null>(null);
  const [routeDraft, setRouteDraft] = useState({
    providerName: "",
    baseUrl: "",
    model: "",
    localPort: "",
  });
  const [routeSaving, setRouteSaving] = useState(false);
  const [routeReviewing, setRouteReviewing] = useState("");
  const [routeApplying, setRouteApplying] = useState("");
  const [routeReview, setRouteReview] = useState<RouteDraftApplyReview | null>(null);
  const [routeNotice, setRouteNotice] = useState("");
  const [providerCredential, setProviderCredential] = useState("");
  const [credentialSaving, setCredentialSaving] = useState(false);
  const [credentialStored, setCredentialStored] = useState(false);
  const [savedDrafts, setSavedDrafts] = useState<any[]>([]);
  const [projectPolicyState, setProjectPolicyState] = useState<"loading" | "loaded" | "missing" | "preview" | "error">("loading");
  const [savedProjectPolicy, setSavedProjectPolicy] = useState<Record<string, any> | null>(null);

  const refreshActiveConfig = async () => {
    try {
      const active = await invokeNative<Record<string, any>>("get_active_route_config_status");
      setActiveConfig(active);
    } catch (_error) {
      setActiveConfig(null);
    }
  };

  const refreshRouteDrafts = async () => {
    try {
      const result = await invokeNative<{ drafts: any[] }>("list_route_profile_drafts");
      setSavedDrafts(Array.isArray(result.drafts) ? result.drafts : []);
    } catch (_error) {
      setSavedDrafts([]);
    }
  };

  const refreshProjectPolicy = async () => {
    try {
      const saved = await invokeNative<{ state: string; payload?: Record<string, any> }>("get_app_settings", {
        section: "project-policy",
      });
      setSavedProjectPolicy(saved.payload ?? null);
      setProjectPolicyState(saved.state === "loaded" ? "loaded" : "missing");
    } catch (error) {
      setSavedProjectPolicy(null);
      setProjectPolicyState(error instanceof Error && error.message === "native_unavailable" ? "preview" : "error");
    }
  };

  useEffect(() => {
    void refreshActiveConfig();
    void refreshRouteDrafts();
    void refreshProjectPolicy();
    const timer = window.setInterval(() => {
      void refreshActiveConfig();
      void refreshRouteDrafts();
      void refreshProjectPolicy();
    }, 3000);
    return () => window.clearInterval(timer);
  }, []);

  const runtimeRouting = useMemo(() => buildRuntimeRouting(payload, payloadStatus), [payload, payloadStatus]);
  const serviceBinding = useMemo(() => serviceBindingFromPayload(payload), [payload]);
  const dataSourceLabel =
    payloadStatus === "runtime"
      ? `${payload?.desktop_data_source ?? payload?.source ?? "d-token-owned-runtime"} · ${runtimeRouting.providers.length} providers · ${runtimeRouting.routes.length} routes${updatedAt ? ` · ${updatedAt.toLocaleTimeString()}` : ""}`
      : payloadStatus === "browser-preview"
        ? lang === "zh" ? "浏览器预览 · 不读取本机配置" : "Browser preview · local configuration is not read"
        : payloadStatus === "loading"
          ? lang === "zh" ? "正在读取 d-token runtime" : "Loading d-token runtime"
          : lang === "zh" ? "runtime 暂不可用" : "Runtime unavailable";

  const providerId = "custom-upstream";
  const credentialConfigured = credentialStored || Boolean(activeConfig?.provider?.has_api_key_ref);

  const saveProviderCredential = async () => {
    if (!validateCredentialInput(providerCredential)) {
      setRouteNotice(lang === "zh" ? "请输入有效凭据；凭据不会写入配置或日志。" : "Enter a valid credential; it will not be written to config or logs.");
      return;
    }
    setCredentialSaving(true);
    setRouteNotice("");
    try {
      const result = await invokeNative<NativeActionResult>("store_provider_credential", {
        request: { provider_id: providerId, credential: providerCredential },
      });
      if (!result.ok) throw new Error(result.summary);
      setProviderCredential("");
      setCredentialStored(true);
      setRouteNotice(lang === "zh" ? "凭据已安全保存到系统凭据库；d-token 配置只保存引用。" : result.summary);
    } catch (error) {
      setProviderCredential("");
      setRouteNotice(error instanceof Error && error.message === "native_unavailable"
        ? "浏览器预览不会读取或保存凭据，请在 d-token 桌面窗口中操作。"
        : `凭据保存失败：${error instanceof Error ? error.message : "系统凭据库不可用"}`);
    } finally {
      setCredentialSaving(false);
    }
  };

  const saveRouteDraft = async () => {
    if (!routeDraft.providerName.trim() || !routeDraft.baseUrl.trim() || !routeDraft.model.trim()) {
      setRouteNotice(lang === "zh" ? "请先填写真实上游名称、地址和模型。" : "Enter the real upstream name, URL, and model first.");
      return;
    }
    if (isDtokenSelfLoop(routeDraft.baseUrl, routeDraft.localPort)) {
      setRouteNotice(lang === "zh" ? "上游地址不能指向 d-token 自己的本地端口，否则会形成请求循环。" : "The upstream URL cannot point back to d-token's own local port.");
      return;
    }
    if (!validPort(routeDraft.localPort)) {
      setRouteNotice(lang === "zh" ? "请输入 1–65535 之间的真实本地端口。" : "Enter the real local port between 1 and 65535.");
      return;
    }
    setRouteSaving(true);
    setRouteNotice("");
    try {
      const result = await invokeNative<NativeActionResult>("save_route_profile_draft", {
        request: {
          route_id: "custom-upstream-routed",
          route_name: "Agent -> d-token -> configured upstream",
          provider_id: providerId,
          provider_name: routeDraft.providerName,
          provider_kind: "custom",
          protocol: "openai",
          base_url: routeDraft.baseUrl,
          api_key_ref: credentialConfigured ? credentialReferenceForProvider(providerId) : null,
          models: [routeDraft.model],
          chain_mode: "direct",
          compression_policy: "balanced",
          local_port: Number(routeDraft.localPort),
          agent: "OpenCode",
        },
      });
      if (!result.ok) throw new Error(result.summary);
      setRouteNotice(result.summary);
      await refreshRouteDrafts();
      await refreshPayload();
      await refreshActiveConfig();
    } catch (error) {
      setRouteNotice(error instanceof Error && error.message === "native_unavailable"
        ? "当前是浏览器预览，保存路由草稿需要在 Tauri 桌面窗口中运行。"
        : `路由草稿保存失败：${String(error)}`);
    } finally {
      setRouteSaving(false);
    }
  };

  const reviewRouteDraft = async (routeId: string) => {
    if (!routeId) return;
    setRouteReviewing(routeId);
    setRouteNotice("");
    try {
      const result = await invokeNative<RouteDraftApplyReview>("preview_route_profile_draft_apply", {
        request: { route_id: routeId },
      });
      if (!isUsableRouteReview(result, routeId)) {
        setRouteReview(null);
        setRouteNotice(routeReviewFailureMessage(result, lang));
        return;
      }
      setRouteReview(result);
    } catch (error) {
      setRouteNotice(error instanceof Error && error.message === "native_unavailable"
        ? (lang === "zh" ? "浏览器预览无法审阅或应用真实路由草稿，请在桌面窗口中操作。" : "Browser preview cannot review or apply a real route draft. Use the desktop window.")
        : (lang === "zh" ? "桌面控制面未能生成路由审阅；未执行写入，请稍后重试。" : "The desktop control plane could not prepare the route review. Nothing was written; try again."));
    } finally {
      setRouteReviewing("");
    }
  };

  const applyReviewedRouteDraft = async () => {
    if (!routeReview || !isUsableRouteReview(routeReview, routeReview.route_id)) {
      setRouteReview(null);
      setRouteNotice(lang === "zh" ? "应用已阻止：请先审阅当前路由差异。" : "Apply was blocked: review the current route diff first.");
      return;
    }
    const review = routeReview;
    setRouteApplying(review.route_id);
    setRouteNotice("");
    try {
      const result = await invokeNative<NativeActionResult>("apply_route_profile_draft", {
        request: routeApplyRequest(review),
      });
      setRouteReview(null);
      if (!result.ok) {
        setRouteNotice(routeApplyFailureMessage(result.summary, lang));
        return;
      }
      setRouteNotice(lang === "zh"
        ? "路由草稿已应用并完成写后验证；事务已创建持久回滚点。"
        : "The route draft was applied and verified; the transaction created a durable rollback point.");
      await refreshRouteDrafts();
      await refreshPayload();
      await refreshActiveConfig();
    } catch (error) {
      setRouteReview(null);
      setRouteNotice(error instanceof Error && error.message === "native_unavailable"
        ? (lang === "zh" ? "浏览器预览无法提交路由变更，请在桌面窗口中操作。" : "Browser preview cannot submit route changes. Use the desktop window.")
        : (lang === "zh" ? "桌面控制面调用失败，未确认写入结果；请重新审阅当前差异后再试。" : "The desktop control-plane call failed and the write result is unconfirmed. Review the current diff before retrying."));
    } finally {
      setRouteApplying("");
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center gap-3">
        <h1>{t("adv.title")}</h1>
        <DataSourceBadge
          mode={payloadStatus === "runtime" ? "runtime" : payloadStatus === "browser-preview" ? "preview" : "unavailable"}
          label={payloadStatus === "runtime" ? (lang === "zh" ? "本地运行数据" : "Local runtime data") : dataSourceLabel}
        />
        <Badge variant={payloadStatus === "runtime" ? "default" : "secondary"}>{dataSourceLabel}</Badge>
      </div>

      <Tabs defaultValue="service">
        <TabsList className="flex-wrap">
          <TabsTrigger value="service">{t("adv.tab.service")}</TabsTrigger>
          <TabsTrigger value="ccswitch">{t("adv.tab.ccswitch")}</TabsTrigger>
          <TabsTrigger value="headroom">{t("adv.tab.headroom")}</TabsTrigger>
          <TabsTrigger value="routing">{t("adv.tab.routing")}</TabsTrigger>
        </TabsList>

        {/* Service & autostart */}
        <TabsContent value="service" className="mt-4 space-y-4">
          <ServiceRow />
          <GlassCard className="p-5">
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label={t("adv.bindHost")}><Input value={serviceBinding.host} readOnly /></Field>
              <Field label={t("adv.localPort")}><Input value={serviceBinding.port} readOnly /></Field>
            </div>
          </GlassCard>
        </TabsContent>

        {/* cc-switch */}
        <TabsContent value="ccswitch" className="mt-4 space-y-4">
          <CcSwitchPanel />
        </TabsContent>

        {/* Headroom scan */}
        <TabsContent value="headroom" className="mt-4 space-y-4">
          <CapabilitySummary payload={payload} status={payloadStatus} />
          <HeadroomScan payload={payload} payloadStatus={payloadStatus} refreshPayload={refreshPayload} refreshProjectPolicy={refreshProjectPolicy} />
          <GlassCard className="p-5">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
              <h3>{t("adv.projectPolicies")}</h3>
              <Badge variant={projectPolicyState === "loaded" ? "default" : "secondary"}>
                {projectPolicyState === "loaded"
                  ? lang === "zh" ? "已读取本地策略" : "Local policy loaded"
                  : projectPolicyState === "preview"
                    ? lang === "zh" ? "浏览器预览" : "Browser preview"
                    : projectPolicyState === "loading"
                      ? lang === "zh" ? "读取中" : "Loading"
                      : lang === "zh" ? "未写入策略" : "No saved policy"}
              </Badge>
            </div>
            <ProjectPolicyRows saved={savedProjectPolicy} state={projectPolicyState} lang={lang} />
          </GlassCard>
        </TabsContent>

        {/* Providers & routes */}
        <TabsContent value="routing" className="mt-4 space-y-4">
          <ActiveConfigCard
            activeConfig={activeConfig ?? payload?.desktop_active_config ?? null}
            runtimeActiveConfig={payload?.runtime_active_config ?? payload?.active_config ?? null}
          />

          <GlassCard className="p-5">
            <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
              <div>
                <h3 className="flex items-center gap-2"><RouteIcon className="size-4" />Agent d-token route draft</h3>
                <div className="text-xs text-muted-foreground">
                  {lang === "zh"
                    ? "保存到 d-token 本地 routing 草稿，不修改 OpenCode、Headroom、RTK、cc-switch 或系统代理。"
                    : "Saved as a d-token local routing draft. OpenCode, Headroom, RTK, cc-switch, and system proxy config are not changed."}
                </div>
              </div>
              <Button size="sm" onClick={saveRouteDraft} disabled={routeSaving || !routeDraft.providerName.trim() || !routeDraft.baseUrl.trim() || !routeDraft.model.trim() || !validPort(routeDraft.localPort)}>
                {routeSaving ? <RefreshCw className="size-3.5 animate-spin" /> : null}
                {lang === "zh" ? "保存草稿" : "Save draft"}
              </Button>
            </div>
            <div className="grid gap-3 md:grid-cols-4">
              <Field label={lang === "zh" ? "Provider 名称" : "Provider name"}>
                <Input
                  value={routeDraft.providerName}
                  onChange={(event) => setRouteDraft((draft) => ({ ...draft, providerName: event.target.value }))}
                />
              </Field>
              <Field label="Base URL">
                <Input
                  value={routeDraft.baseUrl}
                  onChange={(event) => setRouteDraft((draft) => ({ ...draft, baseUrl: event.target.value }))}
                />
              </Field>
              <Field label="Model">
                <Input
                  value={routeDraft.model}
                  onChange={(event) => setRouteDraft((draft) => ({ ...draft, model: event.target.value }))}
                />
              </Field>
              <Field label={lang === "zh" ? "本地端口" : "Local port"}>
                <Input
                  value={routeDraft.localPort}
                  onChange={(event) => setRouteDraft((draft) => ({ ...draft, localPort: event.target.value }))}
                />
              </Field>
            </div>
            <div className="glass-inset mt-3 grid gap-3 p-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-end">
              <Field label={lang === "zh" ? "模型服务凭据" : "Provider credential"}>
                <Input
                  type="password"
                  autoComplete="off"
                  value={providerCredential}
                  placeholder={credentialConfigured
                    ? (lang === "zh" ? "已保存在系统凭据库；输入新值可替换" : "Stored in OS credential store; enter a new value to replace")
                    : (lang === "zh" ? "仅发送到系统凭据库，不写入 d-token 配置" : "Stored only in the OS credential store")}
                  onChange={(event) => setProviderCredential(event.target.value)}
                />
              </Field>
              <Button
                type="button"
                variant="outline"
                onClick={() => void saveProviderCredential()}
                disabled={!isTauriRuntime() || credentialSaving || !validateCredentialInput(providerCredential)}
              >
                {credentialSaving ? <RefreshCw className="size-3.5 animate-spin" /> : <KeyRound className="size-3.5" />}
                {credentialConfigured
                  ? (lang === "zh" ? "替换系统凭据" : "Replace credential")
                  : (lang === "zh" ? "安全保存凭据" : "Store credential securely")}
              </Button>
            </div>
            <p className="mt-2 text-xs text-muted-foreground">
              {lang === "zh"
                ? "明文只在本次操作内短暂存在；路由草稿、日志、Dashboard 和导出只保留不透明引用。"
                : "Plaintext exists only for this operation; config, logs, Dashboard, and exports keep only an opaque reference."}
            </p>
            {routeNotice && <div className="glass-inset mt-3 p-3 text-xs text-muted-foreground">{routeNotice}</div>}
            {savedDrafts.length > 0 && (
              <div className="mt-4 space-y-2">
                {savedDrafts.slice(0, 3).map((draft, index) => {
                  const provider = draft?.config?.providers?.[0];
                  const route = draft?.config?.routes?.[0];
                  return (
                    <div key={`${route?.id ?? "route"}-${index}`} className="glass-inset flex flex-wrap items-center justify-between gap-3 p-3 text-xs">
                      <div>
                        <div className="font-medium">{route?.name ?? route?.id ?? (lang === "zh" ? "未命名路由草稿" : "Unnamed route draft")}</div>
                        <div className="text-muted-foreground">{provider?.name ?? provider?.id ?? "—"} · {provider?.base_url ?? "—"} · {route?.local_port ? `:${route.local_port}` : "—"}</div>
                      </div>
                      <div className="flex items-center gap-2">
                        <Badge variant="outline">{draft?.savedAt ?? "—"}</Badge>
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => void reviewRouteDraft(route?.id ?? "")}
                          disabled={!route?.id || Boolean(routeReviewing) || Boolean(routeApplying)}
                        >
                          {routeReviewing === route?.id ? <RefreshCw className="size-3.5 animate-spin" /> : null}
                          {routeReviewing === route?.id
                            ? (lang === "zh" ? "正在生成预览" : "Preparing review")
                            : (lang === "zh" ? "审阅并应用" : "Review & apply")}
                        </Button>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </GlassCard>

          <GlassCard className="p-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <div className="text-sm font-semibold">d-token Route Model</div>
                <div className="text-xs text-muted-foreground">
                  {payloadStatus === "runtime"
                    ? "ProviderProfile / RouteProfile from d-token-owned runtime payload. Fallback is disabled unless explicitly configured."
                    : payloadStatus === "browser-preview"
                      ? "浏览器预览不会读取或展示本机路由。"
                      : "Waiting for d-token runtime route model."}
                </div>
              </div>
              <Badge variant="outline">{runtimeRouting.source}</Badge>
            </div>
          </GlassCard>

          <GlassCard className="p-5">
            <h3 className="mb-3 flex items-center gap-2"><Server className="size-4" />{t("adv.providers")}</h3>
            <div className="space-y-2">
              {runtimeRouting.providers.map((p) => (
                <div key={p.id} className="glass-inset flex flex-wrap items-center justify-between gap-3 p-3">
                  <div className="flex items-center gap-3">
                    <div><div>{p.name}</div><div className="text-xs text-muted-foreground">{p.protocol} · {p.baseUrl}</div></div>
                    <Badge variant={p.kind === "official" ? "default" : "outline"}>{p.kind}</Badge>
                  </div>
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <KeyRound className="size-3.5" />{t("adv.credStore")}: {p.store} · {p.keyMasked}
                    <StatusBadge status={p.health} label={p.health === "healthy" ? t("common.healthy") : t("common.warning")} />
                  </div>
                </div>
              ))}
              {runtimeRouting.providers.length === 0 && <RoutingEmptyState kind="provider" />}
            </div>
          </GlassCard>

          <GlassCard className="p-5">
            <h3 className="mb-3 flex items-center gap-2"><RouteIcon className="size-4" />{t("adv.routes")}</h3>
            <div className="space-y-3">
              {runtimeRouting.routes.map((r) => (
                <div key={r.id} className="glass-inset p-3">
                  <div className="mb-2 flex items-center justify-between">
                    <span>{r.name}</span>
                    <Badge variant={r.risk === "low" ? "secondary" : "destructive"}>{r.risk}</Badge>
                  </div>
                  <div className="flex flex-wrap items-center gap-2 text-sm">
                    {[r.agent, "压缩", r.mode, r.target].map((n, i, arr) => (
                      <span key={i} className="flex items-center gap-2">
                        <span className="rounded-md bg-muted px-2 py-1 text-xs">{n}</span>
                        {i < arr.length - 1 && <span className="text-muted-foreground">→</span>}
                      </span>
                    ))}
                    <span className="ml-auto text-xs text-muted-foreground">{r.port ? `:${r.port}` : "—"} · {r.fallback}</span>
                  </div>
                </div>
              ))}
              {runtimeRouting.routes.length === 0 && <RoutingEmptyState kind="route" />}
            </div>
          </GlassCard>

          <GlassCard className="p-5">
            <h3 className="mb-3 flex items-center gap-2"><Wifi className="size-4" />{t("adv.ports")}</h3>
            <div className="space-y-2">
              {runtimeRouting.ports.map((p) => (
                <div key={p.id} className="glass-inset flex items-center justify-between p-3 text-sm">
                  <span>{p.host}:{p.port} <span className="text-muted-foreground">· {p.kind}</span></span>
                  <div className="flex items-center gap-2">
                    <Badge variant={p.confidence === "confirmed" ? "default" : "outline"}>{p.confidence}</Badge>
                    <StatusBadge status={p.health} label={p.owner} />
                  </div>
                </div>
              ))}
              {runtimeRouting.ports.length === 0 && <RoutingEmptyState kind="port" />}
            </div>
          </GlassCard>
        </TabsContent>
      </Tabs>

      <AlertDialog
        open={routeReview !== null}
        onOpenChange={(open) => {
          if (!open && !routeApplying) setRouteReview(null);
        }}
      >
        <AlertDialogContent className="max-w-2xl">
          <AlertDialogHeader>
            <AlertDialogTitle>{lang === "zh" ? "审阅并应用路由" : "Review and apply route"}</AlertDialogTitle>
            <AlertDialogDescription>
              {lang === "zh"
                ? "以下差异由桌面控制面只读生成。确认时会提交本次审阅的前后 SHA-256；如果草稿或活动配置发生变化，应用会被拒绝。"
                : "This diff was prepared read-only by the desktop control plane. Confirmation submits the reviewed before/after SHA-256 pair; apply is rejected if the draft or active config changes."}
            </AlertDialogDescription>
          </AlertDialogHeader>

          {routeReview && (
            <div className="space-y-3 text-sm">
              <div className="grid gap-3 sm:grid-cols-2">
                <ReviewField label={lang === "zh" ? "目标文件" : "Target file"} value={routeReview.target} />
                <ReviewField label={lang === "zh" ? "风险等级" : "Risk level"} value={routeReview.risk_level} />
              </div>
              <div className="glass-inset p-3 text-xs text-muted-foreground">
                {lang === "zh"
                  ? "确认后只更新 d-token 活动配置。事务会先创建持久回滚点，写入后验证；不会修改外部 Agent、Headroom、RTK、cc-switch、系统代理或凭据。"
                  : "Confirmation updates only the active d-token config. The transaction creates a durable rollback point before writing and verifies afterward; external agents, Headroom, RTK, cc-switch, system proxy, and credentials are unchanged."}
              </div>
              <div>
                <div className="mb-1.5 text-xs font-medium text-muted-foreground">{lang === "zh" ? "脱敏差异" : "Redacted diff"}</div>
                <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border/60 bg-muted/35 p-3 text-xs leading-5">{routeReview.diff}</pre>
              </div>
              <div className="grid gap-2 text-[11px] text-muted-foreground">
                <HashRow label={lang === "zh" ? "应用前 SHA-256" : "Before SHA-256"} value={routeReview.before_sha256} />
                <HashRow label={lang === "zh" ? "应用后 SHA-256" : "After SHA-256"} value={routeReview.after_sha256} />
              </div>
            </div>
          )}

          <AlertDialogFooter>
            <AlertDialogCancel disabled={Boolean(routeApplying)}>{lang === "zh" ? "取消" : "Cancel"}</AlertDialogCancel>
            <Button onClick={() => void applyReviewedRouteDraft()} disabled={Boolean(routeApplying)}>
              {routeApplying ? <RefreshCw className="size-4 animate-spin" /> : null}
              {routeApplying ? (lang === "zh" ? "正在应用" : "Applying") : (lang === "zh" ? "确认应用" : "Confirm apply")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function validPort(value: string) {
  if (!/^\d{1,5}$/.test(value.trim())) return false;
  const port = Number(value);
  return Number.isInteger(port) && port >= 1 && port <= 65_535;
}

function ReviewField({ label, value }: { label: string; value: string }) {
  return (
    <div className="glass-inset min-w-0 p-3">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 break-all text-sm font-medium">{value}</div>
    </div>
  );
}

function HashRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[132px_minmax(0,1fr)]">
      <span>{label}</span>
      <code className="break-all font-mono text-foreground/80">{value}</code>
    </div>
  );
}

type UiProvider = {
  id: string;
  name: string;
  kind: string;
  protocol: string;
  baseUrl: string;
  store: string;
  keyMasked: string;
  health: Health;
};

type UiRoute = {
  id: string;
  name: string;
  agent: string;
  mode: string;
  target: string;
  port: number | null;
  fallback: string;
  risk: "low" | "medium" | "high";
};

type UiPort = {
  id: string;
  host: string;
  port: number;
  kind: string;
  confidence: "confirmed" | "candidate" | "unknown";
  owner: string;
  health: Health;
};

function ActiveConfigCard({
  activeConfig,
  runtimeActiveConfig,
}: {
  activeConfig: Record<string, any> | null;
  runtimeActiveConfig: Record<string, any> | null;
}) {
  const state = safeText(activeConfig?.state, "unknown");
  const loaded = state === "loaded";
  const provider = activeConfig?.provider ?? {};
  const route = activeConfig?.route ?? {};
  const runtimeStatus = runtimeActiveConfig?.config ?? runtimeActiveConfig;
  const runtimeSource = safeText(runtimeStatus?.source, "—");
  const runtimeLoaded = runtimeStatus?.loaded === true;
  const providerName = safeText(provider.name, safeText(provider.id, "no provider"));
  const routeName = safeText(route.name, safeText(route.id, "no route"));
  const path = safeText(activeConfig?.path, "—");
  const fallbackDisabled = activeConfig?.fallback_disabled === true;
  return (
    <GlassCard className="p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className="text-sm font-semibold">Active d-token config</div>
          <div className="text-xs text-muted-foreground">
            {loaded
              ? `${routeName} -> ${providerName} -> ${safeText(provider.base_url, "base URL missing")}`
              : "No applied d-token active config yet. Save and apply a route draft to make runtime routing explicit."}
          </div>
          <div className="mt-1 text-[11px] text-muted-foreground">{path}</div>
          <div className="mt-1 text-[11px] text-muted-foreground">
            Runtime: {runtimeSource} / {runtimeLoaded ? "loaded" : "not loaded"} / {Number(runtimeStatus?.route_count ?? 0)} routes
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={loaded ? "default" : "secondary"}>{state}</Badge>
          <Badge variant={fallbackDisabled ? "outline" : "destructive"}>
            {fallbackDisabled ? "fallback disabled" : "fallback not proven disabled"}
          </Badge>
          <Badge variant="outline">
            {Number(activeConfig?.provider_count ?? 0)} providers / {Number(activeConfig?.route_count ?? 0)} routes
          </Badge>
        </div>
      </div>
    </GlassCard>
  );
}

function ProjectPolicyRows({
  saved,
  state,
  lang,
}: {
  saved: Record<string, any> | null;
  state: "loading" | "loaded" | "missing" | "preview" | "error";
  lang: "zh" | "en";
}) {
  const policy = saved?.policy ?? null;
  const recommendation = saved?.recommendation ?? null;
  const runtimeInput = saved?.runtime_input ?? null;
  if (state === "loaded" && policy) {
    const enabledFlags = [
      ["CCR", policy.enable_ccr],
      ["memory", policy.enable_memory],
      ["learn", policy.enable_learn],
      ["output shaper", policy.enable_output_shaper],
      ["RTK", policy.enable_integrated_rtk],
      ["cache aligner", policy.enable_cache_aligner],
      ["code compressor", policy.enable_code_compressor],
      ["JSON SmartCrusher", policy.enable_json_smartcrusher],
      ["text compressor", policy.enable_text_kompress],
    ].filter(([, enabled]) => Boolean(enabled));
    return (
      <div className="space-y-2">
        <div className="glass-inset flex flex-wrap items-center justify-between gap-3 p-3">
          <div>
            <div>{safeText(policy.project_id, "—")}</div>
            <div className="text-xs text-muted-foreground">
              {safeText(policy.root_path, "—")} · {(policy.language_stack ?? []).join(", ") || "—"}
            </div>
            <div className="mt-1 text-xs text-muted-foreground">
              {lang === "zh" ? "运行输入" : "Runtime input"}:
              {" "}avg {formatMetric(runtimeInput?.avg_request_tokens_7d)} tokens · noise {formatPercent(runtimeInput?.avg_cli_noise_ratio_7d)}
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-1.5">
            <Badge>{safeText(policy.level, safeText(policy.recommended_level, "—"))}</Badge>
            <Badge variant="outline">{safeText(recommendation?.confidence ?? policy.recommendation_confidence, "—")}</Badge>
            {finiteRatio(policy.holdout_ratio) !== null && <Badge variant="secondary">holdout {policy.holdout_ratio}</Badge>}
          </div>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {enabledFlags.map(([label]) => (
            <Badge key={label as string} variant="secondary" className="text-xs">{label as string}</Badge>
          ))}
        </div>
        <div className="text-xs text-muted-foreground">
          {saved?.privacy ?? (lang === "zh" ? "未返回隐私边界说明" : "No privacy-boundary statement returned")}
        </div>
      </div>
    );
  }

  if (state === "preview") {
    return (
      <div className="glass-inset p-3 text-sm text-muted-foreground">
        {lang === "zh"
          ? "浏览器预览不能读取本地项目策略。桌面窗口中扫描并应用后，这里会显示真实保存的策略。"
          : "Browser preview cannot read local project policy. After scanning and applying in the desktop app, the saved policy appears here."}
      </div>
    );
  }

  if (state === "loading") {
    return (
      <div className="glass-inset p-3 text-sm text-muted-foreground">
        {lang === "zh" ? "正在读取 d-token 本地项目策略。" : "Loading d-token local project policy."}
      </div>
    );
  }

  return (
    <div className="glass-inset p-3 text-sm text-muted-foreground">
      {lang === "zh"
        ? "还没有写入项目策略。点击上方扫描项目并应用推荐后，这里会显示真实配置。"
        : "No project policy has been saved yet. Scan a project and apply the recommendation to show real configuration here."}
    </div>
  );
}

function buildRuntimeRouting(payload: Record<string, any> | null, status: string) {
  const payloadProviders = Array.isArray(payload?.providers) ? payload.providers : [];
  const payloadRoutes = Array.isArray(payload?.routes) ? payload.routes : [];
  const payloadInstances = Array.isArray(payload?.instances) ? payload.instances : [];
  const hasRuntimeRouting = status === "runtime" && (payloadProviders.length > 0 || payloadRoutes.length > 0 || payloadInstances.length > 0);

  if (!hasRuntimeRouting) {
    return {
      source: status === "browser-preview" ? "browser-preview" : status === "runtime" ? "runtime-empty" : status,
      providers: [] as UiProvider[],
      routes: [] as UiRoute[],
      ports: [] as UiPort[],
    };
  }

  const providerRows: UiProvider[] = payloadProviders.map((provider: any) => ({
    id: safeText(provider.id, safeText(provider.name, "provider")),
    name: safeText(provider.name, safeText(provider.id, "Provider")),
    kind: safeText(provider.kind, "custom"),
    protocol: normalizeProtocol(provider.protocol),
    baseUrl: safeText(provider.base_url, safeText(provider.baseUrl, "not configured")),
    store: provider.api_key_ref ? "local-ref" : "none",
    keyMasked: provider.api_key_ref ? maskRef(provider.api_key_ref) : "no api key stored",
    health: providerStatusToHealth(provider),
  }));
  const providerById = new Map(providerRows.map((provider) => [provider.id, provider]));
  const instanceById = new Map(payloadInstances.map((instance: any) => [safeText(instance.id, ""), instance]));

  const routeRows: UiRoute[] = payloadRoutes.map((route: any) => {
    const provider = providerById.get(safeText(route.target_provider_id, ""));
    const instance = instanceById.get(safeText(route.instance_id, ""));
    const fallback = route.fallback_provider_id
      ? `explicit fallback: ${route.fallback_provider_id}`
      : route.fallback_disabled === true
        ? "fallback disabled"
        : "fallback unverified";
    return {
      id: safeText(route.id, safeText(route.name, "route")),
      name: safeText(route.name, safeText(route.id, "Route")),
      agent: safeText(instance?.agent, safeText(route.instance_id, "agent route")),
      mode: safeText(route.chain_mode, "compress_then_provider"),
      target: provider?.name ?? safeText(route.target_provider_id, "unknown provider"),
      port: positivePort(route.local_port ?? instance?.local_port),
      fallback,
      risk: route.fallback_provider_id || route.fallback_disabled !== true ? "medium" : "low",
    };
  });

  const portRows = uniquePorts([
    ...routeRows.map((route) => ({
      id: `route-${route.id}`,
      host: "127.0.0.1",
      port: route.port,
      kind: "dtoken_route",
      confidence: "confirmed" as const,
      owner: route.name,
      health: "warning" as Health,
    })),
    ...payloadInstances.map((instance: any) => ({
      id: `instance-${safeText(instance.id, "runtime")}`,
      host: "127.0.0.1",
      port: positivePort(instance.local_port),
      kind: "dtoken_instance",
      confidence: "confirmed" as const,
      owner: safeText(instance.agent, safeText(instance.name, "d-token")),
      health: processStatusToHealth(instance.process_status),
    })),
  ]);

  return {
    source: "d-token-owned-runtime",
    providers: providerRows,
    routes: routeRows,
    ports: portRows,
  };
}

function uniquePorts(rows: Array<Omit<UiPort, "port"> & { port: number | null }>) {
  const seen = new Set<string>();
  return rows.filter((row) => {
    const key = `${row.host}:${row.port}:${row.kind}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return row.port !== null;
  }) as UiPort[];
}

function safeText(value: unknown, fallback: string) {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}

function normalizeProtocol(value: unknown) {
  const text = safeText(value, "auto");
  return text === "open_ai" ? "openai" : text;
}

export function isDtokenSelfLoop(baseUrl: string, localPort: string) {
  try {
    const url = new URL(baseUrl.trim());
    const host = url.hostname.toLowerCase();
    const port = url.port || (url.protocol === "https:" ? "443" : "80");
    return (host === "127.0.0.1" || host === "localhost" || host === "::1") && port === localPort.trim();
  } catch {
    return false;
  }
}

function serviceBindingFromPayload(payload: Record<string, any> | null) {
  const endpoint = safeText(payload?.desktop_service_url, "");
  if (!endpoint) return { host: "—", port: "—" };
  try {
    const url = new URL(endpoint);
    return { host: url.hostname || "—", port: url.port || (url.protocol === "https:" ? "443" : url.protocol === "http:" ? "80" : "—") };
  } catch {
    return { host: "—", port: "—" };
  }
}

function maskRef(value: unknown) {
  const text = safeText(value, "api-key-ref");
  if (text.length <= 8) return `${text.slice(0, 2)}***`;
  return `${text.slice(0, 4)}***${text.slice(-4)}`;
}

function processStatusToHealth(value: unknown): Health {
  const text = safeText(value, "").toLowerCase();
  if (text.includes("running") || text.includes("healthy")) return "healthy";
  if (text.includes("stopped") || text.includes("missing")) return "warning";
  return "warning";
}

function providerStatusToHealth(provider: Record<string, any>): Health {
  const text = safeText(provider.health ?? provider.status, "").toLowerCase();
  if (text === "healthy" || text === "ok" || text === "ready") return "healthy";
  if (text === "error" || text === "failed" || text === "unreachable") return "error";
  return "warning";
}

function positivePort(value: unknown) {
  const port = typeof value === "number" ? value : Number(value);
  return Number.isInteger(port) && port > 0 && port <= 65_535 ? port : null;
}

function RoutingEmptyState({ kind }: { kind: "provider" | "route" | "port" }) {
  const { lang } = useI18n();
  const label = kind === "provider"
    ? (lang === "zh" ? "暂无已验证 Provider" : "No verified providers")
    : kind === "route"
      ? (lang === "zh" ? "暂无已验证路由" : "No verified routes")
      : (lang === "zh" ? "暂无已验证监听端口" : "No verified listening ports");
  return <div className="glass-inset p-3 text-sm text-muted-foreground">{label}</div>;
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return <div><label className="mb-1.5 block text-sm text-muted-foreground">{label}</label>{children}</div>;
}

type AdvancedSettingsPayload = {
  autostart_preference?: boolean;
  cc_switch_auto?: boolean;
  cc_switch_mode?: "auto" | "direct" | "proxy";
};

type PreferenceStatus = "loading" | "missing" | "saved" | "saving" | "preview" | "error";

function useAdvancedPreference() {
  const [settings, setSettings] = useState<AdvancedSettingsPayload>({});
  const [status, setStatus] = useState<PreferenceStatus>("loading");

  useEffect(() => {
    let cancelled = false;
    invokeNative<{ state: string; payload?: { advanced?: AdvancedSettingsPayload } & AdvancedSettingsPayload }>("get_app_settings", { section: "app" })
      .then((saved) => {
        if (cancelled) return;
        const payload = saved.payload?.advanced ?? saved.payload ?? {};
        setSettings((current) => ({ ...current, ...payload }));
        setStatus(saved.state === "loaded" ? "saved" : "missing");
      })
      .catch((error) => {
        if (cancelled) return;
        setStatus(error instanceof Error && error.message === "native_unavailable" ? "preview" : "error");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const savePreference = async (patch: AdvancedSettingsPayload) => {
    setSettings((current) => ({ ...current, ...patch }));
    if (status === "preview") return;
    setStatus("saving");
    try {
      const result = await invokeNative<NativeActionResult>("save_app_settings", {
        section: "app",
        payload: { advanced: patch },
      });
      if (!result.ok) throw new Error(result.summary);
      setStatus("saved");
    } catch (error) {
      setStatus(error instanceof Error && error.message === "native_unavailable" ? "preview" : "error");
    }
  };

  return { settings, status, savePreference };
}

function PreferenceBadge({ status }: { status: PreferenceStatus }) {
  const t = useT();
  const { lang } = useI18n();
  const label =
    status === "saving"
      ? t("adv.preferenceSaving")
      : status === "preview"
        ? t("adv.preferencePreview")
        : status === "missing"
          ? (lang === "zh" ? "尚未设置" : "Not configured")
          : status === "error"
            ? t("adv.preferenceError")
            : status === "loading"
              ? t("adv.preferenceLoading")
              : t("adv.preferenceSaved");
  return <Badge variant={status === "error" ? "destructive" : "outline"}>{label}</Badge>;
}

function ServiceRow() {
  const t = useT();
  const { settings, status, savePreference } = useAdvancedPreference();
  const disabled = status === "loading" || status === "saving";
  return (
    <GlassCard className="flex items-center justify-between p-5">
      <div className="flex items-center gap-3">
        <Power className="size-5 text-primary" />
        <div>
          <div className="flex items-center gap-2">
            {t("adv.autostart")}
            <PreferenceBadge status={status} />
          </div>
          <div className="max-w-md text-sm text-muted-foreground">{t("adv.autostartDesc")}</div>
        </div>
      </div>
      <Switch
        aria-label={t("adv.autostart")}
        checked={settings.autostart_preference ?? false}
        disabled={disabled}
        onCheckedChange={(checked) => void savePreference({ autostart_preference: checked })}
      />
    </GlassCard>
  );
}

function CcSwitchPanel() {
  const t = useT();
  const { lang } = useI18n();
  const { settings, status, savePreference } = useAdvancedPreference();
  const mode = settings.cc_switch_mode ?? null;
  const disabled = status === "loading" || status === "saving";
  const modes = [
    { k: "auto" as const, label: t("adv.ccMode.auto") },
    { k: "direct" as const, label: t("adv.ccMode.direct") },
    { k: "proxy" as const, label: t("adv.ccMode.proxy") },
  ];
  return (
    <>
      <GlassCard className="flex items-center justify-between p-5">
        <div className="flex items-center gap-3">
          <Network className="size-5 text-primary" />
          <div>
            <div className="flex items-center gap-2">
              {t("adv.ccAuto")}
              <PreferenceBadge status={status} />
            </div>
            <div className="max-w-md text-sm text-muted-foreground">{t("adv.ccAutoDesc")}</div>
          </div>
        </div>
        <Switch
          aria-label={t("adv.ccAuto")}
          checked={settings.cc_switch_auto ?? false}
          disabled={disabled}
          onCheckedChange={(checked) => void savePreference({ cc_switch_auto: checked })}
        />
      </GlassCard>

      <GlassCard className="p-5">
        <div className="mb-3 flex items-center justify-between">
          <span className="text-sm text-muted-foreground">{t("adv.ccMode")}</span>
          <div className="flex items-center gap-2 text-sm">
            {t("adv.ccCurrent")}: <Badge>{modes.find((item) => item.k === mode)?.label ?? "—"}</Badge>
          </div>
        </div>
        <div className="grid gap-2 sm:grid-cols-3">
          {modes.map((m) => (
            <button
              key={m.k}
              disabled={disabled}
              onClick={() => void savePreference({ cc_switch_mode: m.k })}
              className={mode === m.k ? "chip-active" : "chip"}>
              {m.label}
            </button>
          ))}
        </div>
      </GlassCard>
    </>
  );
}

function HeadroomScan({
  payload,
  payloadStatus,
  refreshPayload,
  refreshProjectPolicy,
}: {
  payload: Record<string, any> | null;
  payloadStatus: string;
  refreshPayload: () => Promise<void>;
  refreshProjectPolicy: () => Promise<void>;
}) {
  const t = useT();
  const { lang } = useI18n();
  const [scanning, setScanning] = useState(false);
  const [saving, setSaving] = useState(false);
  const [projectRoot, setProjectRoot] = useState("");
  const [result, setResult] = useState<any | null>(null);
  const [notice, setNotice] = useState("");
  const [savedSummary, setSavedSummary] = useState("");

  const recommendation = result?.recommendation;
  const recommendedLevel = recommendation?.recommended_level ?? null;
  const confidence = recommendation?.confidence ?? null;
  const runtimeInput = useMemo(() => projectPolicyInputFromPayload(payload, projectRoot), [payload, projectRoot]);

  const scan = async () => {
    if (!runtimeInput) {
      setNotice(lang === "zh"
        ? "本地服务尚未返回完整、可核验的策略输入，暂不能生成项目策略建议。"
        : "The local service has not returned complete, verifiable policy inputs yet.");
      return;
    }
    setScanning(true);
    setResult(null);
    setNotice("");
    setSavedSummary("");
    try {
      const next = await invokeNative<Record<string, any>>("recommend_project_policy", {
        request: runtimeInput,
      });
      setResult(next);
      setNotice(`${next.source ?? "dtoken-project-policy"} · ${next.privacy ?? ""} · ${runtimeInput.project_id}`);
    } catch (error) {
      setNotice(error instanceof Error && error.message === "native_unavailable"
        ? "当前是浏览器预览，真实策略推荐需要在 Tauri 桌面窗口中运行。"
        : `策略推荐失败：${String(error)}`);
    } finally {
      setScanning(false);
    }
  };

  const applyPolicy = async () => {
    if (!result || !runtimeInput) return;
    setSaving(true);
    try {
      const saved = await invokeNative<NativeActionResult>("save_app_settings", {
        section: "project-policy",
        payload: {
          policy: {
            project_id: runtimeInput.project_id,
            root_path: projectRoot,
            language_stack: runtimeInput.language_stack,
            level: recommendedLevel,
            recommended_level: recommendedLevel,
            recommendation_confidence: confidence,
            enable_ccr: Boolean(result.mapping?.enable_ccr),
            enable_memory: Boolean(result.mapping?.enable_memory),
            enable_learn: Boolean(result.mapping?.enable_learn),
            enable_output_shaper: Boolean(result.mapping?.enable_output_shaper),
            enable_mcp: Boolean(result.mapping?.enable_mcp),
            enable_cache_aligner: Boolean(result.mapping?.enable_cache_aligner),
            enable_code_compressor: Boolean(result.mapping?.enable_code_compressor),
            enable_json_smartcrusher: Boolean(result.mapping?.enable_json_smartcrusher),
            enable_text_kompress: Boolean(result.mapping?.enable_text_kompress),
            enable_integrated_rtk: Boolean(result.mapping?.enable_integrated_rtk),
            ...(typeof result.mapping?.output_verbosity === "string" ? { output_verbosity: result.mapping.output_verbosity } : {}),
            ...(finiteMetric(result.mapping?.holdout_ratio) !== null ? { holdout_ratio: finiteMetric(result.mapping.holdout_ratio) } : {}),
            notes: recommendation?.reasons ?? [],
          },
          recommendation,
          runtime_input: runtimeInput,
          source: result.source,
          privacy: "aggregate runtime metrics only; no raw prompts, responses, CCR content, keys, or cookies",
        },
      });
      if (!saved.ok) throw new Error(saved.summary);
      setNotice(saved.summary);
      setSavedSummary(
        lang === "zh"
          ? `${runtimeInput.project_id} 已写入 d-token 本地项目策略，推荐等级 ${recommendedLevel}。`
          : `${runtimeInput.project_id} saved to d-token local project policy with ${recommendedLevel} level.`,
      );
      await refreshProjectPolicy();
      await refreshPayload();
    } catch (error) {
      setNotice(error instanceof Error && error.message === "native_unavailable"
        ? "当前是浏览器预览，无法保存策略。"
        : `策略保存失败：${String(error)}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <GlassCard className="p-5">
      <div className="mb-4 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <ScanSearch className="size-5 text-primary" />
          <Input
            placeholder={lang === "zh" ? "输入真实项目目录" : "Enter the real project directory"}
            value={projectRoot}
            onChange={(event) => setProjectRoot(event.target.value)}
            className="w-64"
          />
        </div>
        <Button variant="outline" onClick={scan} disabled={scanning || !projectRoot.trim() || !runtimeInput}>
          {scanning ? <RefreshCw className="size-4 animate-spin" /> : <ScanSearch className="size-4" />}
          {scanning ? t("adv.scanning") : t("adv.scanProject")}
        </Button>
      </div>

      {!runtimeInput && (
        <div className="glass-inset mb-3 p-3 text-xs text-muted-foreground">
          {lang === "zh"
            ? "需要 runtime 返回完整 project_policy_input（含来源、语言栈、请求规模、噪声、检索率、失败率和风险标记）后才能启用。"
            : "Requires a complete runtime project_policy_input with provenance, stack, traffic, noise, retrieval, failure, and risk evidence."}
        </div>
      )}

      {notice && <div className="glass-inset mb-3 p-3 text-xs text-muted-foreground">{notice}</div>}
      {savedSummary && <div className="glass-inset mb-3 p-3 text-xs text-emerald-600 dark:text-emerald-300">{savedSummary}</div>}

      {recommendation && (
        <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} className="glass-inset p-4">
          <div className="mb-3 flex items-center justify-between">
            <span className="text-sm text-muted-foreground">{t("adv.recommendLevel")}</span>
            <span className="text-sm">{t("common.confidence")}: {confidence ?? "—"}</span>
          </div>
          <div className="mb-4 flex flex-wrap gap-2">
            {OUTPUT_LEVELS.map((l) => (
              <Badge key={l.key} variant={l.key === recommendedLevel ? "default" : "outline"} className="gap-1">
                {l[lang]}{l.key === recommendedLevel && ` · ${t("common.recommended")}`}
              </Badge>
            ))}
          </div>
          <div className="mb-3 space-y-1 text-xs text-muted-foreground">
            <div>Runtime input: {payloadStatus} · avg tokens {runtimeInput?.avg_request_tokens_7d ?? "—"} · noise {formatPercent(runtimeInput?.avg_cli_noise_ratio_7d)}</div>
            {(recommendation.reasons ?? []).map((reason: string) => <div key={reason}>Reason: {reason}</div>)}
            {(recommendation.risks ?? []).map((risk: string) => <div key={risk}>Risk: {risk}</div>)}
            <div>Expected savings: {formatRange(recommendation.expected_savings_min, recommendation.expected_savings_max)}</div>
            <div>{lang === "zh" ? "策略映射" : "Policy mapping"}: {mappingSummary(result.mapping, lang)}</div>
          </div>
          <Button size="sm" onClick={applyPolicy} disabled={saving}>
            {saving ? <RefreshCw className="size-3.5 animate-spin" /> : null}
            {t("adv.applyPolicy")}
          </Button>
        </motion.div>
      )}
    </GlassCard>
  );
}

function projectPolicyInputFromPayload(payload: Record<string, any> | null, projectRoot: string) {
  const input = payload?.project_policy_input;
  if (!input || typeof input !== "object" || Array.isArray(input)) return null;
  const source = safeText(input.source, "");
  const languageStack = Array.isArray(input.language_stack)
    ? input.language_stack.filter((item: unknown): item is string => typeof item === "string" && item.trim().length > 0)
    : [];
  const avgRequestTokens = finiteMetric(input.avg_request_tokens_7d);
  const noiseRatio = finiteRatio(input.avg_cli_noise_ratio_7d);
  const retrieveRate = finiteRatio(input.retrieve_rate_7d);
  const failureRate = finiteRatio(input.failure_rate_7d);
  if (
    !projectRoot.trim()
    || !source
    || languageStack.length === 0
    || avgRequestTokens === null
    || noiseRatio === null
    || retrieveRate === null
    || failureRate === null
    || typeof input.high_risk_task !== "boolean"
    || typeof input.manual_override !== "boolean"
  ) return null;
  return {
    project_id: projectIdFromRoot(projectRoot),
    language_stack: languageStack,
    avg_request_tokens_7d: Math.round(avgRequestTokens),
    avg_cli_noise_ratio_7d: noiseRatio,
    retrieve_rate_7d: retrieveRate,
    failure_rate_7d: failureRate,
    high_risk_task: input.high_risk_task,
    manual_override: input.manual_override,
    evidence_source: source,
  };
}

function projectIdFromRoot(root: string) {
  const cleaned = root.trim().replace(/[\\/]+$/g, "");
  const name = cleaned.split(/[\\/]/).filter(Boolean).pop() ?? "";
  return name
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function finiteMetric(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

function finiteRatio(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= 1 ? value : null;
}

function formatPercent(value: unknown) {
  const ratio = finiteRatio(value);
  return ratio === null ? "—" : `${Math.round(ratio * 100)}%`;
}

function formatMetric(value: unknown) {
  const metric = finiteMetric(value);
  return metric === null ? "—" : metric.toLocaleString("en-US");
}

function formatRange(min: unknown, max: unknown) {
  const minRatio = finiteRatio(min);
  const maxRatio = finiteRatio(max);
  return minRatio === null || maxRatio === null ? "—" : `${Math.round(minRatio * 100)}% – ${Math.round(maxRatio * 100)}%`;
}

function mappingSummary(mapping: Record<string, unknown> | null | undefined, lang: "zh" | "en") {
  if (!mapping) return "—";
  const labels = [
    ["CCR", mapping.enable_ccr],
    ["memory", mapping.enable_memory],
    ["learn", mapping.enable_learn],
    ["output shaper", mapping.enable_output_shaper],
    ["RTK", mapping.enable_integrated_rtk],
    ["cache aligner", mapping.enable_cache_aligner],
    ["code compressor", mapping.enable_code_compressor],
    ["JSON SmartCrusher", mapping.enable_json_smartcrusher],
    ["text compressor", mapping.enable_text_kompress],
  ].filter(([, enabled]) => enabled === true).map(([label]) => label);
  return labels.length > 0 ? labels.join(" · ") : (lang === "zh" ? "未返回启用项" : "No enabled capabilities returned");
}
