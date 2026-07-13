import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { ThemeProvider as NextThemeProvider, useTheme } from "next-themes";
import { I18nContext, type Lang } from "./i18n";
import { invokeNative, type NativeActionResult } from "./native";

export type Accent = "blue" | "green" | "orange" | "purple" | "slate";
export type ChartAccent = "green" | "blue" | "amber" | "rose" | "violet" | "orange";
export type ThemeMode = "light" | "dark" | "system";
export type AppearanceLoadState = "loading" | "loaded" | "missing" | "preview" | "error";

export type AppearanceState = {
  theme: ThemeMode;
  resolvedTheme: "light" | "dark";
  accent: Accent;
  chartAccent: ChartAccent;
  glass: false;
  lang: Lang;
};

export type AppearancePatch = Partial<Omit<AppearanceState, "resolvedTheme" | "glass">>;

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

const APPEARANCE_CACHE_KEY = "d-token.appearance.v2";
const NEXT_THEME_STORAGE_KEY = "d-token.theme";

const DEFAULT_APPEARANCE: Omit<AppearanceState, "resolvedTheme"> = {
  theme: "light",
  accent: "orange",
  chartAccent: "orange",
  glass: false,
  lang: "zh",
};

type AppearanceContextValue = AppearanceState & {
  loadState: AppearanceLoadState;
  saving: boolean;
  persistenceNotice: string;
  updateAppearance: (patch: AppearancePatch) => void;
  setAccent: (accent: Accent) => void;
  setChartAccent: (accent: ChartAccent) => void;
  setTheme: (theme: ThemeMode) => void;
};

const fallbackContext: AppearanceContextValue = {
  ...DEFAULT_APPEARANCE,
  resolvedTheme: "light",
  loadState: "loading",
  saving: false,
  persistenceNotice: "",
  updateAppearance: () => {},
  setAccent: () => {},
  setChartAccent: () => {},
  setTheme: () => {},
};

const AppearanceContext = createContext<AppearanceContextValue>(fallbackContext);

export function useAppearance() {
  return useContext(AppearanceContext);
}

// Kept for components owned by next-themes (for example the toast renderer).
export { useTheme };

function AppearanceProvider({ children, initial }: { children: ReactNode; initial: Omit<AppearanceState, "resolvedTheme"> }) {
  const { setTheme: setNextTheme } = useTheme();
  const [appearance, setAppearance] = useState(initial);
  const [resolvedTheme, setResolvedTheme] = useState<"light" | "dark">(() => resolveTheme(initial.theme));
  const [loadState, setLoadState] = useState<AppearanceLoadState>("loading");
  const [saving, setSaving] = useState(false);
  const [persistenceNotice, setPersistenceNotice] = useState("");
  const appearanceRef = useRef(appearance);
  const touchedKeysRef = useRef(new Set<keyof AppearancePatch>());
  const loadPromiseRef = useRef<Promise<void> | null>(null);
  const saveQueueRef = useRef(Promise.resolve());
  const saveCountRef = useRef(0);
  const updateRevisionRef = useRef(0);

  const applyPatch = useCallback((patch: AppearancePatch, markTouched: boolean) => {
    if (markTouched) {
      Object.keys(patch).forEach((key) => touchedKeysRef.current.add(key as keyof AppearancePatch));
    }

    const next = normalizeAppearance({ ...appearanceRef.current, ...patch });
    appearanceRef.current = next;
    setAppearance(next);
    cacheAppearance(next);
    applyAppearanceToRoot(next);
    setResolvedTheme(resolveTheme(next.theme));
    setNextTheme(next.theme);
  }, [setNextTheme]);

  useEffect(() => {
    applyAppearanceToRoot(appearanceRef.current);
    setNextTheme(appearanceRef.current.theme);

    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const updateSystemTheme = () => {
      if (appearanceRef.current.theme !== "system") return;
      const resolved = media.matches ? "dark" : "light";
      setResolvedTheme(resolved);
      document.documentElement.classList.toggle("dark", resolved === "dark");
      document.documentElement.style.colorScheme = resolved;
    };
    media.addEventListener("change", updateSystemTheme);
    return () => media.removeEventListener("change", updateSystemTheme);
  }, [setNextTheme]);

  useEffect(() => {
    if (loadPromiseRef.current) return;
    loadPromiseRef.current = invokeNative<SavedSettings>("get_app_settings", { section: "app" })
      .then((saved) => {
        const nativePatch = parseAppearance(saved.payload?.appearance);
        const untouchedPatch = Object.fromEntries(
          Object.entries(nativePatch).filter(([key]) => !touchedKeysRef.current.has(key as keyof AppearancePatch)),
        ) as AppearancePatch;
        if (Object.keys(untouchedPatch).length > 0) applyPatch(untouchedPatch, false);
        setLoadState(saved.state === "loaded" ? "loaded" : "missing");
      })
      .catch((error) => {
        setLoadState(error instanceof Error && error.message === "native_unavailable" ? "preview" : "error");
      });
  }, [applyPatch]);

  const persistCurrentAppearance = useCallback((revision: number) => {
    saveCountRef.current += 1;
    setSaving(true);
    saveQueueRef.current = saveQueueRef.current
      .catch(() => undefined)
      .then(async () => {
        await loadPromiseRef.current;
        const current = appearanceRef.current;
        const result = await invokeNative<NativeActionResult>("save_app_settings", {
          section: "app",
          payload: {
            appearance: {
              ...current,
            },
            scope: "d-token-local-settings",
            no_external_agent_write: true,
          },
        });
        if (!result.ok) throw new Error(result.summary || "save_failed");
        if (revision === updateRevisionRef.current) {
          setLoadState("loaded");
          setPersistenceNotice(result.summary);
        }
      })
      .catch((error) => {
        const preview = error instanceof Error && error.message === "native_unavailable";
        if (revision === updateRevisionRef.current) {
          if (preview) setLoadState("preview");
          setPersistenceNotice(preview ? "session_only" : `save_failed:${String(error)}`);
        }
      })
      .finally(() => {
        saveCountRef.current -= 1;
        if (saveCountRef.current === 0) setSaving(false);
      });
  }, []);

  const updateAppearance = useCallback((patch: AppearancePatch) => {
    updateRevisionRef.current += 1;
    applyPatch(patch, true);
    persistCurrentAppearance(updateRevisionRef.current);
  }, [applyPatch, persistCurrentAppearance]);

  const value: AppearanceContextValue = {
    ...appearance,
    resolvedTheme,
    loadState,
    saving,
    persistenceNotice,
    updateAppearance,
    setAccent: (accent) => updateAppearance({ accent }),
    setChartAccent: (chartAccent) => updateAppearance({ chartAccent }),
    setTheme: (theme) => updateAppearance({ theme }),
  };

  return (
    <I18nContext.Provider value={{ lang: appearance.lang, setLang: (lang) => updateAppearance({ lang }) }}>
      <AppearanceContext.Provider value={value}>{children}</AppearanceContext.Provider>
    </I18nContext.Provider>
  );
}

function resolveTheme(theme: ThemeMode): "light" | "dark" {
  if (theme !== "system") return theme;
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function applyAppearanceToRoot(appearance: Omit<AppearanceState, "resolvedTheme">) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const resolved = resolveTheme(appearance.theme);
  root.classList.toggle("dark", resolved === "dark");
  root.style.colorScheme = resolved;
  root.setAttribute("data-theme", appearance.theme);
  root.setAttribute("data-accent", appearance.accent);
  root.setAttribute("data-chart-accent", appearance.chartAccent);
  root.setAttribute("data-glass", "off");
  root.setAttribute("lang", appearance.lang === "zh" ? "zh-CN" : "en");
  root.removeAttribute("data-flow-palette");
  root.removeAttribute("data-flow-speed");
  root.style.removeProperty("--flow-color-a");
  root.style.removeProperty("--flow-color-b");
  root.style.removeProperty("--flow-color-c");
  root.style.removeProperty("--flow-gradient-duration");
}

function cacheAppearance(appearance: Omit<AppearanceState, "resolvedTheme">) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(APPEARANCE_CACHE_KEY, JSON.stringify(appearance));
    window.localStorage.setItem(NEXT_THEME_STORAGE_KEY, appearance.theme);
  } catch {
    // A denied localStorage write must not prevent current-session appearance updates.
  }
}

function readCachedAppearance(): Omit<AppearanceState, "resolvedTheme"> {
  if (typeof window === "undefined") return DEFAULT_APPEARANCE;
  try {
    const raw = window.localStorage.getItem(APPEARANCE_CACHE_KEY);
    if (!raw) return DEFAULT_APPEARANCE;
    return normalizeAppearance({ ...DEFAULT_APPEARANCE, ...parseAppearance(JSON.parse(raw)) });
  } catch {
    return DEFAULT_APPEARANCE;
  }
}

function normalizeAppearance(value: Omit<AppearanceState, "resolvedTheme">): Omit<AppearanceState, "resolvedTheme"> {
  return {
    theme: isThemeMode(value.theme) ? value.theme : "light",
    accent: isAccent(value.accent) ? value.accent : "orange",
    chartAccent: isChartAccent(value.chartAccent) ? value.chartAccent : "orange",
    glass: false,
    lang: value.lang === "en" ? "en" : "zh",
  };
}

function parseAppearance(value: unknown): AppearancePatch {
  if (!value || typeof value !== "object") return {};
  const source = value as Record<string, unknown>;
  const patch: AppearancePatch = {};
  if (isThemeMode(source.theme)) patch.theme = source.theme;
  if (isAccent(source.accent)) patch.accent = source.accent;
  if (isChartAccent(source.chartAccent)) patch.chartAccent = source.chartAccent;
  if (source.lang === "zh" || source.lang === "en") patch.lang = source.lang;
  return patch;
}

function isAccent(value: unknown): value is Accent {
  return value === "blue" || value === "green" || value === "orange" || value === "purple" || value === "slate";
}

function isChartAccent(value: unknown): value is ChartAccent {
  return value === "green" || value === "blue" || value === "amber" || value === "rose" || value === "violet" || value === "orange";
}

function isThemeMode(value: unknown): value is ThemeMode {
  return value === "light" || value === "dark" || value === "system";
}

type SavedSettings = {
  state: "loaded" | "missing" | "unsupported_section" | string;
  payload?: { appearance?: Record<string, unknown> };
};

export function AppProviders({ children }: { children: ReactNode }) {
  const initial = readCachedAppearance();
  return (
    <NextThemeProvider
      attribute="class"
      defaultTheme={initial.theme}
      storageKey={NEXT_THEME_STORAGE_KEY}
      enableSystem
      disableTransitionOnChange={false}
    >
      <AppearanceProvider initial={initial}>{children}</AppearanceProvider>
    </NextThemeProvider>
  );
}
