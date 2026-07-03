import { Sun, Moon, Monitor, Globe, Check } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { Switch } from "../ui/switch";
import { Button } from "../ui/button";
import { useI18n, useT } from "../../lib/i18n";
import { useTheme, useAppearance, ACCENTS, CHART_ACCENTS } from "../../lib/theme-context";
import { service } from "../../lib/fixtures";
import { cn } from "../ui/utils";

export function Settings() {
  const t = useT();
  const { lang, setLang } = useI18n();
  const { theme, setTheme } = useTheme();
  const { accent, setAccent, chartAccent, setChartAccent, glass, setGlass } = useAppearance();

  const themeOpts = [
    { k: "light", label: t("set.themeLight"), Icon: Sun },
    { k: "dark", label: t("set.themeDark"), Icon: Moon },
    { k: "system", label: t("set.themeSystem"), Icon: Monitor },
  ];

  return (
    <div className="space-y-8">
      <h1>{t("set.title")}</h1>

      {/* Appearance */}
      <Section title={t("set.appearance")}>
        <Row title={t("set.theme")}>
          <Segmented>
            {themeOpts.map((o) => (
              <SegBtn key={o.k} active={theme === o.k} onClick={() => setTheme(o.k)}>
                <o.Icon className="size-4" />{o.label}
              </SegBtn>
            ))}
          </Segmented>
        </Row>

        <Row title={t("set.language")}>
          <Segmented>
            <SegBtn active={lang === "zh"} onClick={() => setLang("zh")}><Globe className="size-4" />中文</SegBtn>
            <SegBtn active={lang === "en"} onClick={() => setLang("en")}><Globe className="size-4" />English</SegBtn>
          </Segmented>
        </Row>

        <Row title={t("set.accent")} desc={t("set.accentDesc")}>
          <div className="flex items-center gap-2.5">
            {ACCENTS.map((a) => (
              <Swatch key={a.key} color={a.color} active={accent === a.key} onClick={() => setAccent(a.key)} />
            ))}
          </div>
        </Row>

        <Row title={t("set.chartAccent")} desc={t("set.chartAccentDesc")}>
          <div className="flex items-center gap-2.5">
            {CHART_ACCENTS.map((c) => (
              <Swatch key={c.key} color={c.color} active={chartAccent === c.key} onClick={() => setChartAccent(c.key)} />
            ))}
          </div>
        </Row>

        <Row title={t("set.glass")} desc={t("set.glassDesc")}>
          <Switch checked={glass} onCheckedChange={setGlass} />
        </Row>
      </Section>

      {/* Runtime & Safety */}
      <Section title={t("set.data")}>
        <Row title={t("set.mode")} desc={t("set.modeDesc")}>
          <Segmented>
            {["Fixture", "Shadow", "Managed"].map((m, i) => (
              <SegBtn key={m} active={i === 0} onClick={() => {}}>{m}</SegBtn>
            ))}
          </Segmented>
        </Row>
        <Row title={t("set.portRange")} desc={t("set.portRangeDesc")}>
          <span className="font-mono text-sm text-muted-foreground">127.0.0.1 · 8000–8999</span>
        </Row>
        <Row title={t("set.redaction")} desc={t("set.redactionDesc")}>
          <Segmented>
            {[t("set.redactLow"), t("set.redactMed"), t("set.redactHigh")].map((r, i) => (
              <SegBtn key={r} active={i === 2} onClick={() => {}}>{r}</SegBtn>
            ))}
          </Segmented>
        </Row>
        <Row title={t("set.telemetry")} desc={t("set.telemetryDesc")}><Switch /></Row>
        <Row title={t("set.noproxy")} desc={t("set.noproxyDesc")}>
          <span className="font-mono text-sm text-muted-foreground">localhost,127.0.0.1</span>
        </Row>
      </Section>

      {/* About */}
      <Section title={t("set.about")}>
        <Row title={t("set.versionLabel")}><span className="text-muted-foreground">{service.version}</span></Row>
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
    <div className="flex items-center gap-1 rounded-full border border-[var(--glass-border)] bg-[var(--glass-bg)] p-1 [backdrop-filter:blur(12px)_saturate(140%)] [-webkit-backdrop-filter:blur(12px)_saturate(140%)]">
      {children}
    </div>
  );
}

function SegBtn({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button onClick={onClick} className={cn(
      "flex items-center gap-1.5 rounded-full px-3 py-1.5 text-sm transition-all duration-200",
      active ? "chip-active" : "chip",
    )}>
      {children}
    </button>
  );
}

function Swatch({ color, active, onClick }: { color: string; active: boolean; onClick: () => void }) {
  return (
    <button onClick={onClick} className={cn("flex size-7 items-center justify-center rounded-full ring-2 ring-offset-2 ring-offset-background transition-all", active ? "ring-foreground/40" : "ring-transparent hover:ring-border")} style={{ background: color }}>
      {active && <Check className="size-3.5 text-white" />}
    </button>
  );
}
