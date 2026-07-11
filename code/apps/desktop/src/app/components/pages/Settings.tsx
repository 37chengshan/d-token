import { useCallback, useEffect, useState } from "react";
import {
  BrainCircuit,
  Check,
  Database,
  Globe,
  Info,
  LockKeyhole,
  Monitor,
  Moon,
  Palette,
  Power,
  RefreshCw,
  Server,
  ShieldCheck,
  Sun,
  Waves,
} from "lucide-react";
import desktopPackage from "../../../../package.json";
import { GlassCard } from "../common/GlassCard";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Switch } from "../ui/switch";
import { useI18n, useT } from "../../lib/i18n";
import {
  ACCENTS,
  CHART_ACCENTS,
  useAppearance,
  useTheme,
  type Accent,
  type ChartAccent,
  type FlowPalette,
  type FlowSpeed,
} from "../../lib/theme-context";
import { cn } from "../ui/utils";
import { invokeNative, type NativeActionResult } from "../../lib/native";

type SettingsState = "loading" | "loaded" | "preview" | "missing" | "error";
type ServiceState = "loading" | "running" | "unmanaged_running" | "stopped" | "preview" | "error";

export function Settings() {
  const t = useT();
  const { lang, setLang } = useI18n();
  const { theme, setTheme } = useTheme();
  const {
    accent,
    setAccent,
    chartAccent,
    setChartAccent,
    flowPalette,
    setFlowPalette,
    flowSpeed,
    setFlowSpeed,
    glass,
    setGlass,
  } = useAppearance();
  const [notice, setNotice] = useState("");
  const [saving, setSaving] = useState(false);
  const [settingsState, setSettingsState] = useState<SettingsState>("loading");
  const [serviceStatus, setServiceStatus] = useState<DtokenServiceStatus | null>(null);
  const [serviceState, setServiceState] = useState<ServiceState>("loading");
  const [serviceAction, setServiceAction] = useState<"start" | "stop" | null>(null);

  useEffect(() => {
    let cancelled = false;
    invokeNative<SavedSettings>("get_app_settings", { section: "app" })
      .then((saved) => {
        if (cancelled) return;
        const appearance = saved.payload?.appearance ?? {};
        if (isAccent(appearance.accent)) setAccent(appearance.accent);
        if (isChartAccent(appearance.chartAccent)) setChartAccent(appearance.chartAccent);
        if (isFlowPalette(appearance.flowPalette)) setFlowPalette(appearance.flowPalette);
        if (isFlowSpeed(appearance.flowSpeed)) setFlowSpeed(appearance.flowSpeed);
        if (typeof appearance.glass === "boolean") setGlass(appearance.glass);
        if (appearance.theme === "light" || appearance.theme === "dark" || appearance.theme === "system") {
          setTheme(appearance.theme);
        }
        if (appearance.lang === "zh" || appearance.lang === "en") setLang(appearance.lang);
        setSettingsState(saved.state === "loaded" ? "loaded" : "missing");
      })
      .catch((error) => {
        if (cancelled) return;
        setSettingsState(error instanceof Error && error.message === "native_unavailable" ? "preview" : "error");
      });
    return () => {
      cancelled = true;
    };
  }, [setAccent, setChartAccent, setFlowPalette, setFlowSpeed, setGlass, setLang, setTheme]);

  const refreshServiceStatus = useCallback(async () => {
    try {
      const status = await invokeNative<DtokenServiceStatus>("get_dtoken_service_status");
      setServiceStatus(status);
      setServiceState(status.state);
    } catch (error) {
      setServiceStatus(null);
      setServiceState(error instanceof Error && error.message === "native_unavailable" ? "preview" : "error");
    }
  }, []);

  useEffect(() => {
    void refreshServiceStatus();
    const timer = window.setInterval(() => {
      void refreshServiceStatus();
    }, 5000);
    return () => window.clearInterval(timer);
  }, [refreshServiceStatus]);

  const saveAppearance = async (appearance: AppearanceSettings) => {
    if (settingsState === "preview") {
      setNotice(lang === "zh"
        ? "浏览器预览：外观只在当前会话生效，没有伪装成本地已保存。"
        : "Browser preview: appearance only applies to this session and is not presented as saved.");
      return;
    }
    if (settingsState !== "loaded" && settingsState !== "missing") {
      setNotice(lang === "zh" ? "本地设置尚未就绪，请稍后再试。" : "Local settings are not ready yet.");
      return;
    }

    setSaving(true);
    try {
      const result = await invokeNative<NativeActionResult>("save_app_settings", {
        section: "app",
        payload: {
          appearance,
          scope: "d-token-local-settings",
          no_external_agent_write: true,
        },
      });
      if (!result.ok) throw new Error(result.summary || "save_failed");
      setNotice(result.summary);
      setSettingsState("loaded");
    } catch (error) {
      setNotice(error instanceof Error && error.message === "native_unavailable"
        ? (lang === "zh" ? "浏览器预览：外观只在当前会话生效。" : "Browser preview: appearance only applies to this session.")
        : `${lang === "zh" ? "设置保存失败" : "Failed to save settings"}：${String(error)}`);
    } finally {
      setSaving(false);
    }
  };

  const appearanceSnapshot = (patch: Partial<AppearanceSettings> = {}): AppearanceSettings => ({
    accent,
    chartAccent,
    flowPalette,
    flowSpeed,
    glass,
    theme: theme === "dark" || theme === "system" ? theme : "light",
    lang,
    ...patch,
  });

  const updateAppearance = <K extends keyof AppearanceSettings>(
    key: K,
    value: AppearanceSettings[K],
    apply: () => void,
  ) => {
    apply();
    void saveAppearance(appearanceSnapshot({ [key]: value } as Pick<AppearanceSettings, K>));
  };

  const runServiceAction = async (action: "start" | "stop") => {
    setServiceAction(action);
    setNotice("");
    try {
      const result = await invokeNative<NativeActionResult>(
        action === "start" ? "start_dtoken_service" : "stop_dtoken_service",
      );
      if (!result.ok) throw new Error(result.summary || "service_action_failed");
      setNotice(result.summary);
      await refreshServiceStatus();
    } catch (error) {
      setNotice(error instanceof Error && error.message === "native_unavailable"
        ? (lang === "zh" ? "浏览器预览无法管理本地服务，请在桌面窗口中操作。" : "The browser preview cannot manage the local service.")
        : `${lang === "zh" ? "服务操作失败" : "Service action failed"}：${String(error)}`);
    } finally {
      setServiceAction(null);
    }
  };

  const serviceView = describeService(serviceState, serviceStatus, lang);
  const canStart = serviceState === "stopped";
  const canStop = serviceState === "running" && serviceStatus?.managed === true;
  const serviceCommand = canStart ? "start" : canStop ? "stop" : null;
  const themeOpts = [
    { k: "light" as const, label: t("set.themeLight"), Icon: Sun },
    { k: "dark" as const, label: t("set.themeDark"), Icon: Moon },
    { k: "system" as const, label: t("set.themeSystem"), Icon: Monitor },
  ];
  const flowPalettes: { key: FlowPalette; label: string; colors: string[] }[] = [
    { key: "sunrise", label: lang === "zh" ? "日出" : "Sunrise", colors: ["#f97316", "#fb7185", "#38bdf8"] },
    { key: "aurora", label: lang === "zh" ? "极光" : "Aurora", colors: ["#10b981", "#38bdf8", "#a78bfa"] },
    { key: "ocean", label: lang === "zh" ? "海面" : "Ocean", colors: ["#0ea5e9", "#14b8a6", "#6366f1"] },
    { key: "mono", label: lang === "zh" ? "克制" : "Muted", colors: ["#94a3b8", "#cbd5e1", "#64748b"] },
  ];
  const flowSpeeds: { key: FlowSpeed; label: string }[] = [
    { key: "slow", label: lang === "zh" ? "慢" : "Slow" },
    { key: "normal", label: lang === "zh" ? "中" : "Normal" },
    { key: "fast", label: lang === "zh" ? "快" : "Fast" },
  ];

  return (
    <div className="space-y-8">
      <div className="flex flex-wrap items-center gap-3">
        <h1>{t("set.title")}</h1>
        <Badge variant={saving ? "secondary" : settingsState === "loaded" ? "default" : "outline"}>
          {settingsBadge(settingsState, saving, lang)}
        </Badge>
      </div>
      {notice && (
        <div role="status" className="glass-inset p-3 text-sm text-muted-foreground">
          {notice}
        </div>
      )}

      <Section
        id="runtime"
        title={lang === "zh" ? "运行与启动" : "Runtime & Startup"}
        description={lang === "zh" ? "查看并管理 d-token 自有本地服务。" : "Inspect and manage the d-token-owned local service."}
        icon={Server}
      >
        <Row title={lang === "zh" ? "本地服务" : "Local service"} desc={serviceView.detail}>
          <div className="flex flex-wrap items-center justify-end gap-2">
            <Badge variant={serviceView.badge}>{serviceView.label}</Badge>
            {serviceCommand ? (
              <Button
                size="sm"
                variant={serviceCommand === "stop" ? "outline" : "default"}
                aria-label={serviceCommand === "start" ? (lang === "zh" ? "启动服务" : "Start service") : (lang === "zh" ? "停止服务" : "Stop service")}
                disabled={serviceAction !== null}
                onClick={() => void runServiceAction(serviceCommand)}
              >
                {serviceAction ? <RefreshCw className="size-4 animate-spin" /> : <Power className="size-4" />}
                {serviceAction
                  ? (lang === "zh" ? "处理中" : "Working")
                  : serviceCommand === "start"
                    ? (lang === "zh" ? "启动" : "Start")
                    : (lang === "zh" ? "停止" : "Stop")}
              </Button>
            ) : null}
          </div>
        </Row>
        <Row
          title={lang === "zh" ? "本地服务地址" : "Local service address"}
          desc={lang === "zh" ? "只监听本机，不接受局域网或公网连接。" : "Loopback-only; no LAN or public connections."}
        >
          <span className="break-all font-mono text-sm text-muted-foreground">
            {serviceStatus?.service_url || "—"}
          </span>
        </Row>
        <Row
          title={lang === "zh" ? "自动启动 d-token 服务" : "Start d-token automatically"}
          desc={lang === "zh" ? "系统登录后自动启动；原生开机项尚未接通。" : "Start after login; native startup integration is not connected yet."}
        >
          <Unavailable>
            <Switch aria-label={lang === "zh" ? "自动启动 d-token 服务" : "Start d-token automatically"} checked={false} disabled />
          </Unavailable>
        </Row>
      </Section>

      <Section
        id="privacy"
        title={lang === "zh" ? "隐私与数据" : "Privacy & Data"}
        description={lang === "zh" ? "控制本机记录的保留与隐藏方式。" : "Control retention and redaction of local records."}
        icon={ShieldCheck}
      >
        <Row
          title={lang === "zh" ? "数据保留时间" : "Data retention"}
          desc={lang === "zh" ? "清理任务接通前不会假装已经应用保留策略。" : "Retention is not presented as active before cleanup is wired."}
        >
          <Unavailable>
            <Segmented>
              <SegBtn active={false} disabled label={lang === "zh" ? "保留 7 天" : "Keep 7 days"}>7 {lang === "zh" ? "天" : "days"}</SegBtn>
              <SegBtn active disabled label={lang === "zh" ? "保留 30 天" : "Keep 30 days"}>30 {lang === "zh" ? "天" : "days"}</SegBtn>
            </Segmented>
          </Unavailable>
        </Row>
        <Row
          title={lang === "zh" ? "日志隐藏级别" : "Log redaction"}
          desc={lang === "zh" ? "隐藏密钥、认证信息、路径和敏感地址。" : "Hide keys, authentication data, paths, and sensitive addresses."}
        >
          <Unavailable>
            <Segmented>
              <SegBtn active disabled label={lang === "zh" ? "高级隐藏" : "High redaction"}>{lang === "zh" ? "高" : "High"}</SegBtn>
            </Segmented>
          </Unavailable>
        </Row>
        <Row
          title={lang === "zh" ? "诊断数据上传" : "Diagnostic uploads"}
          desc={lang === "zh" ? "当前版本不会上传提示词、代码、完整输出或诊断记录。" : "This version does not upload prompts, code, full outputs, or diagnostics."}
        >
          <div className="flex items-center gap-2">
            <Badge variant="secondary">{lang === "zh" ? "始终关闭" : "Always off"}</Badge>
            <Switch aria-label={lang === "zh" ? "诊断数据上传" : "Diagnostic uploads"} checked={false} disabled />
          </div>
        </Row>
        <Row
          title={lang === "zh" ? "本地连接保护" : "Local connection protection"}
          desc={lang === "zh" ? "避免本机请求绕到系统代理或外部地址。" : "Keep local requests away from system proxies and external addresses."}
        >
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <LockKeyhole className="size-4 text-emerald-500" />
            <span className="font-mono">localhost · 127.0.0.1</span>
          </div>
        </Row>
      </Section>

      <Section
        id="learning"
        title={lang === "zh" ? "智能学习" : "Smart Learning"}
        description={lang === "zh" ? "所有扫描默认留在本机，并排除敏感文件。" : "Scanning stays local and excludes sensitive files by default."}
        icon={BrainCircuit}
      >
        <Row
          title={lang === "zh" ? "智能学习全面扫描" : "Smart Learning full scan"}
          desc={lang === "zh" ? "本地索引与扫描进度接通后才能启用。" : "Available after local indexing and scan progress are connected."}
        >
          <Unavailable>
            <Switch aria-label={lang === "zh" ? "智能学习全面扫描" : "Smart Learning full scan"} checked={false} disabled />
          </Unavailable>
        </Row>
        <Row
          title={lang === "zh" ? "扫描强度" : "Scan depth"}
          desc={lang === "zh" ? "决定识别范围与本机资源占用。" : "Controls discovery scope and local resource use."}
        >
          <Unavailable>
            <Segmented>
              <SegBtn active disabled label={lang === "zh" ? "标准扫描" : "Standard scan"}>{lang === "zh" ? "标准" : "Standard"}</SegBtn>
              <SegBtn active={false} disabled label={lang === "zh" ? "深度扫描" : "Deep scan"}>{lang === "zh" ? "深入" : "Deep"}</SegBtn>
            </Segmented>
          </Unavailable>
        </Row>
        <Row
          title={lang === "zh" ? "扫描范围" : "Scan scope"}
          desc={lang === "zh" ? "管理项目目录、排除目录和敏感文件规则。" : "Manage projects, excluded folders, and sensitive-file rules."}
        >
          <Unavailable>
            <Button size="sm" variant="outline" disabled>
              <Database className="size-4" />
              {lang === "zh" ? "管理范围" : "Manage scope"}
            </Button>
          </Unavailable>
        </Row>
      </Section>

      <Section
        id="updates"
        title={lang === "zh" ? "更新" : "Updates"}
        description={lang === "zh" ? "更新检查与安装尚未接通，不会显示虚假的最新状态。" : "Update checks are not connected and no fake current status is shown."}
        icon={RefreshCw}
      >
        <Row title={lang === "zh" ? "更新通道" : "Update channel"} desc={lang === "zh" ? "稳定版与预览版切换尚未接通。" : "Stable and preview channel switching is not connected."}>
          <Unavailable>
            <Segmented>
              <SegBtn active disabled label={lang === "zh" ? "稳定版" : "Stable"}>{lang === "zh" ? "稳定版" : "Stable"}</SegBtn>
              <SegBtn active={false} disabled label={lang === "zh" ? "预览版" : "Preview"}>{lang === "zh" ? "预览版" : "Preview"}</SegBtn>
            </Segmented>
          </Unavailable>
        </Row>
        <Row
          title={lang === "zh" ? "自动检查更新" : "Check for updates automatically"}
          desc={lang === "zh" ? "接通签名校验和更新服务后开放。" : "Available after signed update verification is connected."}
        >
          <Unavailable>
            <Switch aria-label={lang === "zh" ? "自动检查更新" : "Check for updates automatically"} checked={false} disabled />
          </Unavailable>
        </Row>
      </Section>

      <Section
        id="appearance"
        title={t("set.appearance")}
        description={lang === "zh" ? "这些选项会立即改变当前窗口。" : "These options immediately update the current window."}
        icon={Palette}
      >
        <Row title={t("set.theme")}>
          <Segmented>
            {themeOpts.map((option) => (
              <SegBtn
                key={option.k}
                active={theme === option.k}
                label={option.label}
                onClick={() => updateAppearance("theme", option.k, () => setTheme(option.k))}
              >
                <option.Icon className="size-4" />
                {option.label}
              </SegBtn>
            ))}
          </Segmented>
        </Row>

        <Row title={t("set.language")}>
          <Segmented>
            <SegBtn active={lang === "zh"} label="中文" onClick={() => updateAppearance("lang", "zh", () => setLang("zh"))}>
              <Globe className="size-4" />
              中文
            </SegBtn>
            <SegBtn active={lang === "en"} label="English" onClick={() => updateAppearance("lang", "en", () => setLang("en"))}>
              <Globe className="size-4" />
              English
            </SegBtn>
          </Segmented>
        </Row>

        <Row title={t("set.accent")} desc={t("set.accentDesc")}>
          <div className="flex flex-wrap items-center gap-2.5">
            {ACCENTS.map((item) => (
              <Swatch
                key={item.key}
                color={item.color}
                active={accent === item.key}
                label={`${t("set.accent")}: ${item.key}`}
                onClick={() => updateAppearance("accent", item.key, () => setAccent(item.key))}
              />
            ))}
          </div>
        </Row>

        <Row title={t("set.chartAccent")} desc={t("set.chartAccentDesc")}>
          <div className="flex flex-wrap items-center gap-2.5">
            {CHART_ACCENTS.map((item) => (
              <Swatch
                key={item.key}
                color={item.color}
                active={chartAccent === item.key}
                label={`${t("set.chartAccent")}: ${item.key}`}
                onClick={() => updateAppearance("chartAccent", item.key, () => setChartAccent(item.key))}
              />
            ))}
          </div>
        </Row>

        <Row
          title={lang === "zh" ? "背景颜色" : "Background colors"}
          desc={lang === "zh" ? "选择窗口底层缓慢流动的颜色组合。" : "Choose the slowly moving background palette."}
        >
          <Segmented>
            {flowPalettes.map((item) => (
              <SegBtn
                key={item.key}
                active={flowPalette === item.key}
                label={item.label}
                onClick={() => updateAppearance("flowPalette", item.key, () => setFlowPalette(item.key))}
              >
                <Palette className="size-4" />
                <span className="flex items-center gap-1" aria-hidden="true">
                  {item.colors.map((color) => (
                    <i key={color} className="inline-block size-2 rounded-full" style={{ background: color }} />
                  ))}
                </span>
                {item.label}
              </SegBtn>
            ))}
          </Segmented>
        </Row>

        <Row
          title={lang === "zh" ? "背景移动速度" : "Background speed"}
          desc={lang === "zh" ? "控制背景颜色的移动速度。" : "Control the background color movement."}
        >
          <Segmented>
            {flowSpeeds.map((item) => (
              <SegBtn
                key={item.key}
                active={flowSpeed === item.key}
                label={item.label}
                onClick={() => updateAppearance("flowSpeed", item.key, () => setFlowSpeed(item.key))}
              >
                <Waves className="size-4" />
                {item.label}
              </SegBtn>
            ))}
          </Segmented>
        </Row>

        <Row title={t("set.glass")} desc={lang === "zh" ? "在可读性不受影响时使用轻微透明效果。" : "Use subtle transparency when readability is preserved."}>
          <Switch
            aria-label={t("set.glass")}
            checked={glass}
            onCheckedChange={(value) => updateAppearance("glass", value, () => setGlass(value))}
          />
        </Row>
      </Section>

      <Section
        id="about"
        title={t("set.about")}
        description={lang === "zh" ? "版本与本地优先边界。" : "Version and local-first boundaries."}
        icon={Info}
      >
        <Row title={t("set.versionLabel")}>
          <span className="font-mono text-sm text-muted-foreground">{desktopPackage.version}</span>
        </Row>
        <Row
          title={lang === "zh" ? "数据边界" : "Data boundary"}
          desc={lang === "zh" ? "设置保存在 d-token 本地状态，不会写入外部 Agent 配置。" : "Settings stay in d-token local state and do not write external agent configuration."}
        >
          <div className="flex items-center gap-2 text-sm text-emerald-600 dark:text-emerald-400">
            <ShieldCheck className="size-4" />
            {lang === "zh" ? "本地优先" : "Local first"}
          </div>
        </Row>
      </Section>
    </div>
  );
}

function Section({
  id,
  title,
  description,
  icon: Icon,
  children,
}: {
  id: "runtime" | "privacy" | "learning" | "updates" | "appearance" | "about";
  title: string;
  description: string;
  icon: typeof Server;
  children: React.ReactNode;
}) {
  return (
    <section data-settings-section={id}>
      <div className="mb-3 flex items-start gap-3">
        <div className="mt-0.5 flex size-8 items-center justify-center rounded-md bg-primary/10 text-primary">
          <Icon className="size-4" />
        </div>
        <div>
          <h2 className="text-base font-semibold">{title}</h2>
          <p className="mt-0.5 text-sm text-muted-foreground">{description}</p>
        </div>
      </div>
      <GlassCard className="divide-y divide-border/50 overflow-hidden p-0" hover={false}>{children}</GlassCard>
    </section>
  );
}

function Row({ title, desc, children }: { title: string; desc?: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-4 p-5 sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0 sm:pr-6">
        <div className="font-medium">{title}</div>
        {desc && <div className="mt-1 max-w-2xl text-sm leading-5 text-muted-foreground">{desc}</div>}
      </div>
      <div className="min-w-0 sm:flex sm:max-w-[62%] sm:shrink-0 sm:justify-end">{children}</div>
    </div>
  );
}

function Unavailable({ children }: { children: React.ReactNode }) {
  const { lang } = useI18n();
  return (
    <div className="flex flex-wrap items-center justify-end gap-2">
      <Badge variant="outline">{lang === "zh" ? "尚未接通" : "Not connected"}</Badge>
      {children}
    </div>
  );
}

function Segmented({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-1 rounded-md border border-[var(--glass-border)] bg-[var(--glass-bg)] p-1 [backdrop-filter:blur(12px)_saturate(140%)] [-webkit-backdrop-filter:blur(12px)_saturate(140%)]">
      {children}
    </div>
  );
}

function SegBtn({
  active,
  label,
  onClick,
  disabled = false,
  children,
}: {
  active: boolean;
  label: string;
  onClick?: () => void;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      aria-pressed={active}
      title={label}
      className={cn(
        "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm transition-colors disabled:cursor-not-allowed disabled:opacity-55",
        active ? "chip-active" : "chip",
      )}
    >
      {children}
    </button>
  );
}

function Swatch({ color, active, label, onClick }: { color: string; active: boolean; label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      aria-pressed={active}
      title={label}
      className={cn(
        "flex size-7 items-center justify-center rounded-full ring-2 ring-offset-2 ring-offset-background transition-all",
        active ? "ring-foreground/40" : "ring-transparent hover:ring-border",
      )}
      style={{ background: color }}
    >
      {active && <Check className="size-3.5 text-white" />}
    </button>
  );
}

function describeService(
  state: ServiceState,
  status: DtokenServiceStatus | null,
  lang: "zh" | "en",
): { label: string; detail: string; badge: "default" | "secondary" | "outline" | "destructive" } {
  if (state === "running") {
    return {
      label: lang === "zh" ? "服务正在运行" : "Service running",
      detail: status?.summary || (lang === "zh" ? "由当前 d-token 桌面窗口管理。" : "Managed by this d-token desktop window."),
      badge: "default",
    };
  }
  if (state === "unmanaged_running") {
    return {
      label: lang === "zh" ? "服务已在运行" : "Service already running",
      detail: lang === "zh" ? "该进程不是由当前窗口启动，因此这里不会强行停止。" : "This process was not started by this window and will not be force-stopped here.",
      badge: "secondary",
    };
  }
  if (state === "stopped") {
    return {
      label: lang === "zh" ? "服务未启动" : "Service stopped",
      detail: lang === "zh" ? "启动后，Agent 才能通过 d-token 连接上游。" : "Start it before agents can connect upstream through d-token.",
      badge: "outline",
    };
  }
  if (state === "preview") {
    return {
      label: lang === "zh" ? "浏览器预览" : "Browser preview",
      detail: lang === "zh" ? "没有读取或控制真实本地服务。" : "No real local service is read or controlled.",
      badge: "outline",
    };
  }
  if (state === "error") {
    return {
      label: lang === "zh" ? "状态读取失败" : "Status unavailable",
      detail: lang === "zh" ? "无法确认服务是否运行，请打开 Doctor。" : "Service state could not be confirmed; open Doctor.",
      badge: "destructive",
    };
  }
  return {
    label: lang === "zh" ? "正在检查" : "Checking",
    detail: lang === "zh" ? "正在读取本地服务状态。" : "Reading local service state.",
    badge: "outline",
  };
}

function settingsBadge(state: SettingsState, saving: boolean, lang: "zh" | "en") {
  if (saving) return lang === "zh" ? "保存中" : "Saving";
  if (state === "loading") return lang === "zh" ? "读取中" : "Loading";
  if (state === "loaded") return lang === "zh" ? "本地设置已连接" : "Local settings connected";
  if (state === "missing") return lang === "zh" ? "首次使用" : "First use";
  if (state === "preview") return lang === "zh" ? "浏览器预览" : "Browser preview";
  return lang === "zh" ? "读取失败" : "Load failed";
}

type AppearanceSettings = {
  accent: Accent;
  chartAccent: ChartAccent;
  flowPalette: FlowPalette;
  flowSpeed: FlowSpeed;
  glass: boolean;
  theme: "light" | "dark" | "system";
  lang: "zh" | "en";
};

type SavedSettings = {
  state: "loaded" | "missing" | "unsupported_section" | string;
  payload?: {
    appearance?: Partial<Record<keyof AppearanceSettings, unknown>>;
  };
};

type DtokenServiceStatus = {
  state: "running" | "unmanaged_running" | "stopped";
  managed: boolean;
  service_url: string;
  summary: string;
};

function isAccent(value: unknown): value is Accent {
  return typeof value === "string" && ACCENTS.some((item) => item.key === value);
}

function isChartAccent(value: unknown): value is ChartAccent {
  return typeof value === "string" && CHART_ACCENTS.some((item) => item.key === value);
}

function isFlowPalette(value: unknown): value is FlowPalette {
  return value === "sunrise" || value === "aurora" || value === "ocean" || value === "mono";
}

function isFlowSpeed(value: unknown): value is FlowSpeed {
  return value === "slow" || value === "normal" || value === "fast";
}
