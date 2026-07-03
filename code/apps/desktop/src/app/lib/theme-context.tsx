import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { ThemeProvider as NextThemeProvider, useTheme } from "next-themes";
import { I18nContext, type Lang } from "./i18n";

export type Accent = "blue" | "green" | "orange" | "purple" | "slate";
export type ChartAccent = "green" | "blue" | "amber" | "rose" | "violet" | "orange";
export type FlowPalette = "sunrise" | "aurora" | "ocean" | "mono";
export type FlowSpeed = "slow" | "normal" | "fast";

export const ACCENTS: { key: Accent; color: string }[] = [
  { key: "blue", color: "#2563eb" },
  { key: "green", color: "#16a34a" },
  { key: "orange", color: "#ea580c" },
  { key: "purple", color: "#7c3aed" },
  { key: "slate", color: "#475569" },
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
  flowPalette: "ocean",
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
  const [accent, setAccent] = useState<Accent>("orange");
  const [chartAccent, setChartAccent] = useState<ChartAccent>("orange");
  const [flowPalette, setFlowPalette] = useState<FlowPalette>("ocean");
  const [flowSpeed, setFlowSpeed] = useState<FlowSpeed>("slow");
  const [glass, setGlass] = useState(true);
  const [lang, setLang] = useState<Lang>("zh");

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

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <NextThemeProvider attribute="class" defaultTheme="light" enableSystem disableTransitionOnChange={false}>
      <AppearanceProvider>{children}</AppearanceProvider>
    </NextThemeProvider>
  );
}
