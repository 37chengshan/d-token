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
import { useI18n, useT } from "../../lib/i18n";
import { service, ccSwitch, projects, providers, routes, ports, outputLevels, type Health } from "../../lib/fixtures";
import { invokeNative, isTauriRuntime, type NativeActionResult } from "../../lib/native";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";

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
    providerName: "OpenCode Zen",
    baseUrl: "https://opencode.ai/zen/v1",
    model: "deepseek-v4-flash-free",
    localPort: "18001",
  });
  const [routeSaving, setRouteSaving] = useState(false);
  const [routeApplying, setRouteApplying] = useState("");
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
  const dataSourceLabel =
    payloadStatus === "runtime"
      ? `${payload?.desktop_data_source ?? payload?.source ?? "d-token-owned-runtime"} · ${runtimeRouting.providers.length} providers · ${runtimeRouting.routes.length} routes${updatedAt ? ` · ${updatedAt.toLocaleTimeString()}` : ""}`
      : payloadStatus === "browser-preview"
        ? lang === "zh" ? "浏览器预览 · 示例数据" : "Browser preview · sample data"
        : payloadStatus === "loading"
          ? lang === "zh" ? "正在读取 d-token runtime" : "Loading d-token runtime"
          : lang === "zh" ? "runtime 暂不可用" : "Runtime unavailable";

  const providerId = "opencode-zen";
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
    setRouteSaving(true);
    setRouteNotice("");
    try {
      const result = await invokeNative<NativeActionResult>("save_route_profile_draft", {
        request: {
          route_id: "opencode-real-routed",
          route_name: "Agent d-token routed",
          provider_id: "opencode-zen",
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

  const applyRouteDraft = async (routeId: string) => {
    setRouteApplying(routeId);
    setRouteNotice("");
    try {
      const result = await invokeNative<NativeActionResult>("apply_route_profile_draft", {
        request: { route_id: routeId },
      });
      setRouteNotice(result.summary);
      await refreshRouteDrafts();
      await refreshPayload();
      await refreshActiveConfig();
    } catch (error) {
      setRouteNotice(error instanceof Error && error.message === "native_unavailable"
        ? "当前是浏览器预览，应用路由草稿需要在 Tauri 桌面窗口中运行。"
        : `路由草稿应用失败：${String(error)}`);
    } finally {
      setRouteApplying("");
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center gap-3">
        <h1>{t("adv.title")}</h1>
        <DataSourceBadge />
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
              <Field label={t("adv.bindHost")}><Input defaultValue={service.bindHost} /></Field>
              <Field label={t("adv.localPort")}><Input defaultValue={String(service.port)} /></Field>
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
              <Button size="sm" onClick={saveRouteDraft} disabled={routeSaving}>
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
                        <div className="font-medium">{route?.name ?? route?.id ?? "d-token route draft"}</div>
                        <div className="text-muted-foreground">{provider?.name ?? provider?.id ?? "provider"} · {provider?.base_url ?? "base URL"} · :{route?.local_port ?? "18001"}</div>
                      </div>
                      <div className="flex items-center gap-2">
                        <Badge variant="outline">{draft?.savedAt ?? "saved"}</Badge>
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => void applyRouteDraft(route?.id ?? "")}
                          disabled={!route?.id || routeApplying === route?.id}
                        >
                          {routeApplying === route?.id ? <RefreshCw className="size-3.5 animate-spin" /> : null}
                          {lang === "zh" ? "应用" : "Apply"}
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
                      ? "Browser preview: showing sample route model."
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
                    <span className="ml-auto text-xs text-muted-foreground">:{r.port} · {r.fallback}</span>
                  </div>
                </div>
              ))}
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
            </div>
          </GlassCard>
        </TabsContent>
      </Tabs>
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
  port: number;
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
  const runtimeSource = safeText(runtimeStatus?.source, "runtime unknown");
  const runtimeLoaded = runtimeStatus?.loaded === true;
  const providerName = safeText(provider.name, safeText(provider.id, "no provider"));
  const routeName = safeText(route.name, safeText(route.id, "no route"));
  const path = safeText(activeConfig?.path, "%LOCALAPPDATA%\\d-token\\active-config\\dtoken-config.json");
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
            <div>{safeText(policy.project_id, "desktop-project")}</div>
            <div className="text-xs text-muted-foreground">
              {safeText(policy.root_path, "project root")} · {(policy.language_stack ?? []).join(", ") || "stack pending"}
            </div>
            <div className="mt-1 text-xs text-muted-foreground">
              {lang === "zh" ? "运行输入" : "Runtime input"}:
              {" "}avg {Number(runtimeInput?.avg_request_tokens_7d ?? 0).toLocaleString()} tokens · noise {Math.round(Number(runtimeInput?.avg_cli_noise_ratio_7d ?? 0) * 100)}%
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-1.5">
            <Badge>{safeText(policy.level, safeText(policy.recommended_level, "balanced"))}</Badge>
            <Badge variant="outline">{safeText(recommendation?.confidence ?? policy.recommendation_confidence, "confidence")}</Badge>
            <Badge variant="secondary">holdout {Number(policy.holdout_ratio ?? 0.1)}</Badge>
          </div>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {enabledFlags.map(([label]) => (
            <Badge key={label as string} variant="secondary" className="text-xs">{label as string}</Badge>
          ))}
        </div>
        <div className="text-xs text-muted-foreground">
          {saved?.privacy ?? "aggregate runtime metrics only; no raw prompts or responses are stored"}
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
    const isPreview = status === "browser-preview";
    return {
      source: isPreview ? "browser-preview" : status === "runtime" ? "runtime-empty" : status,
      providers: isPreview ? providers : [],
      routes: isPreview ? routes : [],
      ports: isPreview ? ports : [],
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
    health: provider.base_url || provider.baseUrl ? "healthy" : "warning",
  }));
  const providerById = new Map(providerRows.map((provider) => [provider.id, provider]));
  const instanceById = new Map(payloadInstances.map((instance: any) => [safeText(instance.id, ""), instance]));

  const routeRows: UiRoute[] = payloadRoutes.map((route: any) => {
    const provider = providerById.get(safeText(route.target_provider_id, ""));
    const instance = instanceById.get(safeText(route.instance_id, ""));
    const fallback = route.fallback_provider_id ? `explicit fallback: ${route.fallback_provider_id}` : "fallback disabled";
    return {
      id: safeText(route.id, safeText(route.name, "route")),
      name: safeText(route.name, safeText(route.id, "Route")),
      agent: safeText(instance?.agent, safeText(route.instance_id, "agent route")),
      mode: safeText(route.chain_mode, "compress_then_provider"),
      target: provider?.name ?? safeText(route.target_provider_id, "unknown provider"),
      port: Number(route.local_port ?? instance?.local_port ?? 18001),
      fallback,
      risk: route.fallback_provider_id ? "medium" : "low",
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
      health: "healthy" as Health,
    })),
    ...payloadInstances.map((instance: any) => ({
      id: `instance-${safeText(instance.id, "runtime")}`,
      host: "127.0.0.1",
      port: Number(instance.local_port ?? 18001),
      kind: "dtoken_instance",
      confidence: "confirmed" as const,
      owner: safeText(instance.agent, safeText(instance.name, "d-token")),
      health: processStatusToHealth(instance.process_status),
    })),
  ]);

  return {
    source: "d-token-owned-runtime",
    providers: providerRows.length > 0 ? providerRows : providers,
    routes: routeRows.length > 0 ? routeRows : routes,
    ports: portRows.length > 0 ? portRows : ports,
  };
}

function uniquePorts(rows: UiPort[]) {
  const seen = new Set<string>();
  return rows.filter((row) => {
    const key = `${row.host}:${row.port}:${row.kind}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return Number.isFinite(row.port) && row.port > 0;
  });
}

function safeText(value: unknown, fallback: string) {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}

function normalizeProtocol(value: unknown) {
  const text = safeText(value, "auto");
  return text === "open_ai" ? "openai" : text;
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
  return "healthy";
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return <div><label className="mb-1.5 block text-sm text-muted-foreground">{label}</label>{children}</div>;
}

type AdvancedSettingsPayload = {
  autostart_preference?: boolean;
  cc_switch_auto?: boolean;
  cc_switch_mode?: "auto" | "direct" | "proxy";
};

type PreferenceStatus = "loading" | "saved" | "saving" | "preview" | "error";

function useAdvancedPreference() {
  const [settings, setSettings] = useState<AdvancedSettingsPayload>({
    autostart_preference: true,
    cc_switch_auto: true,
    cc_switch_mode: "auto",
  });
  const [status, setStatus] = useState<PreferenceStatus>("loading");

  useEffect(() => {
    let cancelled = false;
    invokeNative<{ state: string; payload?: AdvancedSettingsPayload }>("get_app_settings", { section: "advanced" })
      .then((saved) => {
        if (cancelled) return;
        setSettings((current) => ({ ...current, ...(saved.payload ?? {}) }));
        setStatus("saved");
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
      await invokeNative<NativeActionResult>("save_app_settings", {
        section: "advanced",
        payload: patch,
      });
      setStatus("saved");
    } catch (error) {
      setStatus(error instanceof Error && error.message === "native_unavailable" ? "preview" : "error");
    }
  };

  return { settings, status, savePreference };
}

function PreferenceBadge({ status }: { status: PreferenceStatus }) {
  const t = useT();
  const label =
    status === "saving"
      ? t("adv.preferenceSaving")
      : status === "preview"
        ? t("adv.preferencePreview")
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
        checked={settings.autostart_preference ?? true}
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
  const mode = settings.cc_switch_mode ?? "auto";
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
          checked={settings.cc_switch_auto ?? true}
          disabled={disabled}
          onCheckedChange={(checked) => void savePreference({ cc_switch_auto: checked })}
        />
      </GlassCard>

      <GlassCard className="p-5">
        <div className="mb-3 flex items-center justify-between">
          <span className="text-sm text-muted-foreground">{t("adv.ccMode")}</span>
          <div className="flex items-center gap-2 text-sm">
            {t("adv.ccCurrent")}: <Badge>{lang === "zh" ? ccSwitch.decision : ccSwitch.decisionEn}</Badge>
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
  const [projectRoot, setProjectRoot] = useState("~/code/web-dashboard");
  const [result, setResult] = useState<any | null>(null);
  const [notice, setNotice] = useState("");
  const [savedSummary, setSavedSummary] = useState("");

  const recommendation = result?.recommendation;
  const recommendedLevel = recommendation?.recommended_level ?? null;
  const confidence = recommendation?.confidence ?? null;
  const confidencePct = confidence === "high" ? 92 : confidence === "medium" ? 76 : confidence === "low" ? 48 : 0;
  const runtimeInput = useMemo(() => projectPolicyInputFromPayload(payload, projectRoot), [payload, projectRoot]);

  const scan = async () => {
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
    if (!result) return;
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
            enable_mcp: true,
            enable_cache_aligner: true,
            enable_code_compressor: true,
            enable_json_smartcrusher: true,
            enable_text_kompress: true,
            enable_integrated_rtk: true,
            output_verbosity: "auto",
            holdout_ratio: Number(result.mapping?.holdout_ratio ?? 0.1),
            notes: recommendation?.reasons ?? [],
          },
          recommendation,
          runtime_input: runtimeInput,
          source: result.source,
          privacy: "aggregate runtime metrics only; no raw prompts, responses, CCR content, keys, or cookies",
        },
      });
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
            placeholder="~/code/web-dashboard"
            value={projectRoot}
            onChange={(event) => setProjectRoot(event.target.value)}
            className="w-64"
          />
        </div>
        <Button variant="outline" onClick={scan} disabled={scanning}>
          {scanning ? <RefreshCw className="size-4 animate-spin" /> : <ScanSearch className="size-4" />}
          {scanning ? t("adv.scanning") : t("adv.scanProject")}
        </Button>
      </div>

      {notice && <div className="glass-inset mb-3 p-3 text-xs text-muted-foreground">{notice}</div>}
      {savedSummary && <div className="glass-inset mb-3 p-3 text-xs text-emerald-600 dark:text-emerald-300">{savedSummary}</div>}

      {recommendation && (
        <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} className="glass-inset p-4">
          <div className="mb-3 flex items-center justify-between">
            <span className="text-sm text-muted-foreground">{t("adv.recommendLevel")}</span>
            <span className="text-sm">{t("common.confidence")}: {confidencePct}% · {confidence}</span>
          </div>
          <div className="mb-4 flex flex-wrap gap-2">
            {outputLevels.map((l) => (
              <Badge key={l.key} variant={l.key === recommendedLevel ? "default" : "outline"} className="gap-1">
                {l[lang]}{l.key === recommendedLevel && ` · ${t("common.recommended")}`}
              </Badge>
            ))}
          </div>
          <div className="mb-3 space-y-1 text-xs text-muted-foreground">
            <div>Runtime input: {payloadStatus} · avg tokens {runtimeInput.avg_request_tokens_7d} · noise {Math.round((runtimeInput.avg_cli_noise_ratio_7d ?? 0) * 100)}%</div>
            {(recommendation.reasons ?? []).map((reason: string) => <div key={reason}>Reason: {reason}</div>)}
            {(recommendation.risks ?? []).map((risk: string) => <div key={risk}>Risk: {risk}</div>)}
            <div>Expected savings: {Math.round((recommendation.expected_savings_min ?? 0) * 100)}% - {Math.round((recommendation.expected_savings_max ?? 0) * 100)}%</div>
            <div>映射: CCR=1 · memory=1 · learn=1 · output_shaper=1 · holdout=0.1</div>
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
  const stats = payload?.stats ?? {};
  const requestLogs = Array.isArray(payload?.request_logs) ? payload.request_logs : [];
  const tokens = stats?.tokens ?? {};
  const before = Number(tokens.input ?? 0);
  const totalRequests = Number(stats?.requests?.total ?? requestLogs.length);
  const failed = Number(stats?.requests?.failed ?? requestLogs.filter((log: any) => log?.error).length);
  const transforms = requestLogs.flatMap((log: any) => Array.isArray(log?.transforms_applied) ? log.transforms_applied : []);
  const retrieveCount = transforms.filter((item: unknown) => String(item).toLowerCase().includes("retrieve")).length;
  const noiseCandidates = transforms.filter((item: unknown) => {
    const text = String(item).toLowerCase();
    return text.includes("rtk") || text.includes("output") || text.includes("compress");
  }).length;

  return {
    project_id: projectIdFromRoot(projectRoot),
    language_stack: inferLanguageStack(projectRoot),
    avg_request_tokens_7d: totalRequests > 0 ? Math.max(1, Math.round(before / totalRequests)) : 47_307,
    avg_cli_noise_ratio_7d: totalRequests > 0 ? clamp(noiseCandidates / Math.max(1, transforms.length), 0.05, 0.75) : 0.42,
    retrieve_rate_7d: totalRequests > 0 ? clamp(retrieveCount / totalRequests, 0, 1) : 0.02,
    failure_rate_7d: totalRequests > 0 ? clamp(failed / totalRequests, 0, 1) : 0,
    high_risk_task: false,
    manual_override: false,
  };
}

function projectIdFromRoot(root: string) {
  const cleaned = root.trim().replace(/[\\/]+$/g, "");
  const name = cleaned.split(/[\\/]/).filter(Boolean).pop() ?? "desktop-project";
  return name
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "") || "desktop-project";
}

function inferLanguageStack(root: string) {
  const text = root.toLowerCase();
  const stack = ["typescript"];
  if (text.includes("tauri") || text.includes("rust")) stack.push("rust");
  if (text.includes("python") || text.includes("py")) stack.push("python");
  if (text.includes("web") || text.includes("react")) stack.push("react");
  return Array.from(new Set(stack));
}

function clamp(value: number, min: number, max: number) {
  if (!Number.isFinite(value)) return min;
  return Math.max(min, Math.min(max, value));
}
