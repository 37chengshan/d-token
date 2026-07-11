import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { ThemeProvider as NextThemeProvider, useTheme } from "next-themes";
import { I18nContext, type Lang } from "./i18n";
import { invokeNative } from "./native";

export type Accent = "blue" | "green" | "orange" | "purple" | "slate";
export type ChartAccent = "green" | "blue" | "amber" | "rose" | "violet" | "orange";
export type FlowPalette = "sunrise" | "aurora" | "ocean" | "mono";
export type FlowSpeed = "slow" | "normal" | "fast";

export const ACCENTS: { key: Accent; color: string }[] = [
  { key: "blue", color: "#4f6f9b" },
  { key: "green", color: "#3b7a57" },
  { key: "orange", color: "#c7652a" },
  { key: "purple", color: "#80639b" },
  { key: "slate", color: "#657078" },
];

export const CHART_ACCENTS: { key: ChartAccent; color: string }[] = [
  { key: "green", color: "#10b981" },
  { key: "blue", color: "#38bdf8" },
  { key: "amber", color: "#f59e0b" },
  { key: "rose", color: "#fb7185" },
  { key: "violet", color: "#a78bfa" },
  { key: "orange", color: "#f97316" },
];

type AppearanceCtx = {
  accent: Accent;
  setAccent: (a: Accent) => void;
  chartAccent: ChartAccent;
  setChartAccent: (c: ChartAccent) => void;
  flowPalette: FlowPalette;
  setFlowPalette: (p: FlowPalette) => void;
  flowSpeed: FlowSpeed;
  setFlowSpeed: (s: FlowSpeed) => void;
  glass: boolean;
  setGlass: (g: boolean) => void;
};

const AppearanceContext = createContext<AppearanceCtx>({
  accent: "orange",
  setAccent: () => {},
  chartAccent: "orange",
  setChartAccent: () => {},
  flowPalette: "sunrise",
  setFlowPalette: () => {},
  flowSpeed: "slow",
  setFlowSpeed: () => {},
  glass: true,
  setGlass: () => {},
});

export function useAppearance() {
  return useContext(AppearanceContext);
}

// Re-export next-themes hook for convenience.
export { useTheme };

function AppearanceProvider({ children }: { children: ReactNode }) {
  const { setTheme } = useTheme();
  const [accent, setAccent] = useState<Accent>("orange");
  const [chartAccent, setChartAccent] = useState<ChartAccent>("orange");
  const [flowPalette, setFlowPalette] = useState<FlowPalette>("sunrise");
  const [flowSpeed, setFlowSpeed] = useState<FlowSpeed>("slow");
  const [glass, setGlass] = useState(true);
  const [lang, setLang] = useState<Lang>("zh");

  useEffect(() => {
    let cancelled = false;
    invokeNative<{ payload?: { appearance?: Record<string, unknown> } }>("get_app_settings", { section: "app" })
      .then((saved) => {
        if (cancelled) return;
        const appearance = saved.payload?.appearance ?? {};
        if (isAccent(appearance.accent)) setAccent(appearance.accent);
        if (isChartAccent(appearance.chartAccent)) setChartAccent(appearance.chartAccent);
        if (isFlowPalette(appearance.flowPalette)) setFlowPalette(appearance.flowPalette);
        if (isFlowSpeed(appearance.flowSpeed)) setFlowSpeed(appearance.flowSpeed);
        if (typeof appearance.glass === "boolean") setGlass(appearance.glass);
        if (isThemeMode(appearance.theme)) setTheme(appearance.theme);
        if (appearance.lang === "zh" || appearance.lang === "en") setLang(appearance.lang);
      })
      .catch(() => {
        // Browser preview keeps the default theme context.
      });
    return () => {
      cancelled = true;
    };
  }, [setTheme]);

  useEffect(() => {
    const root = document.documentElement;
    root.setAttribute("data-accent", accent);
    root.setAttribute("data-chart-accent", chartAccent);
    root.setAttribute("data-flow-palette", flowPalette);
    root.setAttribute("data-flow-speed", flowSpeed);
    root.setAttribute("data-glass", glass ? "on" : "off");
    root.setAttribute("lang", lang === "zh" ? "zh-CN" : "en");
  }, [accent, chartAccent, flowPalette, flowSpeed, glass, lang]);

  return (
    <I18nContext.Provider value={{ lang, setLang }}>
      <AppearanceContext.Provider
        value={{
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
        }}
      >
        {children}
      </AppearanceContext.Provider>
    </I18nContext.Provider>
  );
}

function isAccent(value: unknown): value is Accent {
  return value === "blue" || value === "green" || value === "orange" || value === "purple" || value === "slate";
}

function isChartAccent(value: unknown): value is ChartAccent {
  return value === "green" || value === "blue" || value === "amber" || value === "rose" || value === "violet" || value === "orange";
}

function isFlowPalette(value: unknown): value is FlowPalette {
  return value === "sunrise" || value === "aurora" || value === "ocean" || value === "mono";
}

function isFlowSpeed(value: unknown): value is FlowSpeed {
  return value === "slow" || value === "normal" || value === "fast";
}

function isThemeMode(value: unknown): value is "light" | "dark" | "system" {
  return value === "light" || value === "dark" || value === "system";
}

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <NextThemeProvider attribute="class" defaultTheme="light" enableSystem disableTransitionOnChange={false}>
      <AppearanceProvider>{children}</AppearanceProvider>
    </NextThemeProvider>
  );
}
