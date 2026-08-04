import { useCallback, useState } from "react";

/**
 * Session-scoped tab memory for secondary page Tabs.
 * Shared Windows/macOS path — sessionStorage only, no platform fork.
 */
export function readSessionTab<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  if (typeof window === "undefined") return fallback;
  try {
    const saved = window.sessionStorage.getItem(key);
    if (saved && (allowed as readonly string[]).includes(saved)) return saved as T;
  } catch {
    /* ignore quota / private mode */
  }
  return fallback;
}

export function writeSessionTab(key: string, value: string): void {
  if (typeof window === "undefined") return;
  try {
    window.sessionStorage.setItem(key, value);
  } catch {
    /* ignore */
  }
}

export function useSessionTab<T extends string>(key: string, allowed: readonly T[], fallback: T) {
  const [value, setValueState] = useState<T>(() => readSessionTab(key, allowed, fallback));
  const setValue = useCallback(
    (next: string) => {
      if (!(allowed as readonly string[]).includes(next)) return;
      const tab = next as T;
      setValueState(tab);
      writeSessionTab(key, tab);
    },
    [allowed, key],
  );
  return [value, setValue] as const;
}
