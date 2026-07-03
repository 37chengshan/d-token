import { Check, Globe, Monitor, Moon, Palette, Sun, Waves } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { Switch } from "../ui/switch";
import { useI18n, useT } from "../../lib/i18n";
import {
  ACCENTS,
  CHART_ACCENTS,
  useAppearance,
  useTheme,
  type FlowPalette,
  type FlowSpeed,
} from "../../lib/theme-context";
import { service } from "../../lib/fixtures";
import { cn } from "../ui/utils";

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

  const themeOpts = [
    { k: "light", label: t("set.themeLight"), Icon: Sun },
    { k: "dark", label: t("set.themeDark"), Icon: Moon },
    { k: "system", label: t("set.themeSystem"), Icon: Monitor },
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
      <h1>{t("set.title")}</h1>

      <Section title={t("set.appearance")}>
        <Row title={t("set.theme")}>
          <Segmented>
            {themeOpts.map((option) => (
              <SegBtn key={option.k} active={theme === option.k} onClick={() => setTheme(option.k)}>
                <option.Icon className="size-4" />
                {option.label}
              </SegBtn>
            ))}
          </Segmented>
        </Row>

        <Row title={t("set.language")}>
          <Segmented>
            <SegBtn active={lang === "zh"} onClick={() => setLang("zh")}>
              <Globe className="size-4" />
              中文
            </SegBtn>
            <SegBtn active={lang === "en"} onClick={() => setLang("en")}>
              <Globe className="size-4" />
              English
            </SegBtn>
          </Segmented>
        </Row>

        <Row title={t("set.accent")} desc={t("set.accentDesc")}>
          <div className="flex items-center gap-2.5">
            {ACCENTS.map((item) => (
              <Swatch key={item.key} color={item.color} active={accent === item.key} onClick={() => setAccent(item.key)} />
            ))}
          </div>
        </Row>

        <Row title={t("set.chartAccent")} desc={t("set.chartAccentDesc")}>
          <div className="flex items-center gap-2.5">
            {CHART_ACCENTS.map((item) => (
              <Swatch key={item.key} color={item.color} active={chartAccent === item.key} onClick={() => setChartAccent(item.key)} />
            ))}
          </div>
        </Row>

        <Row
          title={lang === "zh" ? "底层颜色组合" : "Background colors"}
          desc={lang === "zh" ? "控制窗口底层缓慢流动的颜色。" : "Controls the slowly flowing window background."}
        >
          <Segmented>
            {flowPalettes.map((item) => (
              <SegBtn key={item.key} active={flowPalette === item.key} onClick={() => setFlowPalette(item.key)}>
                <Palette className="size-4" />
                <span className="flex items-center gap-1">
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
          title={lang === "zh" ? "颜色流动速度" : "Flow speed"}
          desc={lang === "zh" ? "控制背景渐变移动速度。" : "Controls the animated background speed."}
        >
          <Segmented>
            {flowSpeeds.map((item) => (
              <SegBtn key={item.key} active={flowSpeed === item.key} onClick={() => setFlowSpeed(item.key)}>
                <Waves className="size-4" />
                {item.label}
              </SegBtn>
            ))}
          </Segmented>
        </Row>

        <Row title={t("set.glass")} desc={t("set.glassDesc")}>
          <Switch checked={glass} onCheckedChange={setGlass} />
        </Row>
      </Section>

      <Section title={t("set.data")}>
        <Row title={t("set.mode")} desc={t("set.modeDesc")}>
          <Segmented>
            {["Fixture", "Shadow", "Managed"].map((mode, index) => (
              <SegBtn key={mode} active={index === 0} onClick={() => {}}>
                {mode}
              </SegBtn>
            ))}
          </Segmented>
        </Row>
        <Row title={t("set.portRange")} desc={t("set.portRangeDesc")}>
          <span className="font-mono text-sm text-muted-foreground">127.0.0.1 · 8000-8999</span>
        </Row>
        <Row title={t("set.redaction")} desc={t("set.redactionDesc")}>
          <Segmented>
            {[t("set.redactLow"), t("set.redactMed"), t("set.redactHigh")].map((item, index) => (
              <SegBtn key={item} active={index === 2} onClick={() => {}}>
                {item}
              </SegBtn>
            ))}
          </Segmented>
        </Row>
        <Row title={t("set.telemetry")} desc={t("set.telemetryDesc")}>
          <Switch />
        </Row>
        <Row title={t("set.noproxy")} desc={t("set.noproxyDesc")}>
          <span className="font-mono text-sm text-muted-foreground">localhost,127.0.0.1</span>
        </Row>
      </Section>

      <Section title={t("set.about")}>
        <Row title={t("set.versionLabel")}>
          <span className="text-muted-foreground">{service.version}</span>
        </Row>
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="mb-3 text-sm text-muted-foreground">{title}</div>
      <GlassCard className="divide-y divide-border/50 p-0">{children}</GlassCard>
    </div>
  );
}

function Row({ title, desc, children }: { title: string; desc?: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-4 p-5">
      <div className="min-w-0">
        <div>{title}</div>
        {desc && <div className="mt-0.5 max-w-lg text-sm text-muted-foreground">{desc}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function Segmented({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-1 rounded-full border border-[var(--glass-border)] bg-[var(--glass-bg)] p-1 [backdrop-filter:blur(12px)_saturate(140%)] [-webkit-backdrop-filter:blur(12px)_saturate(140%)]">
      {children}
    </div>
  );
}

function SegBtn({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex items-center gap-1.5 rounded-full px-3 py-1.5 text-sm transition-all duration-200",
        active ? "chip-active" : "chip",
      )}
    >
      {children}
    </button>
  );
}

function Swatch({ color, active, onClick }: { color: string; active: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
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
