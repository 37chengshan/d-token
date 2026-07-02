import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { dict, type Lang, type DictKey } from "./i18n";

/* ------------------------------------------------------------------ */
/* Theme                                                               */
/* ------------------------------------------------------------------ */

type Theme = "light" | "dark";

interface ThemeCtx {
  theme: Theme;
  toggleTheme: () => void;
  setTheme: (t: Theme) => void;
}

const ThemeContext = createContext<ThemeCtx | null>(null);

function detectInitialTheme(): Theme {
  try {
    const saved = localStorage.getItem("dtoken.theme");
    if (saved === "light" || saved === "dark") return saved;
  } catch {
    /* ignore */
  }
  if (
    typeof window !== "undefined" &&
    window.matchMedia &&
    window.matchMedia("(prefers-color-scheme: dark)").matches
  ) {
    return "dark";
  }
  return "light";
}

/* ------------------------------------------------------------------ */
/* Language                                                            */
/* ------------------------------------------------------------------ */

interface I18nCtx {
  lang: Lang;
  setLang: (l: Lang) => void;
  t: (key: DictKey) => string;
}

const I18nContext = createContext<I18nCtx | null>(null);

function detectInitialLang(): Lang {
  try {
    const saved = localStorage.getItem("dtoken.lang");
    if (saved === "zh" || saved === "en") return saved;
  } catch {
    /* ignore */
  }
  if (typeof navigator !== "undefined" && navigator.language) {
    return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
  }
  return "zh";
}

/* ------------------------------------------------------------------ */
/* Combined provider                                                   */
/* ------------------------------------------------------------------ */

export function AppProviders({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(detectInitialTheme);
  const [lang, setLangState] = useState<Lang>(detectInitialLang);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    try {
      localStorage.setItem("dtoken.theme", theme);
    } catch {
      /* ignore */
    }
  }, [theme]);

  useEffect(() => {
    document.documentElement.setAttribute("lang", lang);
    try {
      localStorage.setItem("dtoken.lang", lang);
    } catch {
      /* ignore */
    }
  }, [lang]);

  const themeValue = useMemo<ThemeCtx>(
    () => ({
      theme,
      setTheme: setThemeState,
      toggleTheme: () =>
        setThemeState((prev) => (prev === "dark" ? "light" : "dark")),
    }),
    [theme]
  );

  const i18nValue = useMemo<I18nCtx>(
    () => ({
      lang,
      setLang: setLangState,
      t: (key: DictKey) => {
        const entry = dict[key];
        if (!entry) return key;
        return entry[lang] ?? entry.en ?? key;
      },
    }),
    [lang]
  );

  return (
    <ThemeContext.Provider value={themeValue}>
      <I18nContext.Provider value={i18nValue}>{children}</I18nContext.Provider>
    </ThemeContext.Provider>
  );
}

export function useTheme(): ThemeCtx {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within AppProviders");
  return ctx;
}

export function useI18n(): I18nCtx {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useI18n must be used within AppProviders");
  return ctx;
}
