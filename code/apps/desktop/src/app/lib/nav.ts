/**
 * The five product-level destinations exposed by the desktop shell.
 *
 * Historical destinations are intentionally not part of PageId. They are
 * accepted only at the persistence/deep-link boundary and are normalized by
 * migratePageId before entering React state. This keeps page transitions
 * type-safe while allowing existing session state and bookmarks to recover.
 */
export type PageId =
  | "configuration"
  | "advanced-configuration"
  | "analysis-logs"
  | "diagnostics"
  | "settings";

export type LegacyPageId =
  | "overview"
  | "install"
  | "codebase-memory"
  | "advanced"
  | "analytics"
  | "visual"
  | "doctor"
  | "changes"
  | "logs"
  | "compatibility";

export type NavItem = {
  id: PageId;
  labelKey: string;
  badge?: "beta";
};

export const NAV: NavItem[] = [
  { id: "configuration", labelKey: "nav.configuration" },
  { id: "advanced-configuration", labelKey: "nav.advancedConfiguration" },
  { id: "analysis-logs", labelKey: "nav.analysisLogs" },
  { id: "diagnostics", labelKey: "nav.diagnostics" },
];

export const SETTINGS_NAV: NavItem = { id: "settings", labelKey: "nav.settings" };

export const PAGE_IDS = new Set<PageId>([
  ...NAV.map((item) => item.id),
  SETTINGS_NAV.id,
]);

const LEGACY_PAGE_MIGRATIONS: Record<LegacyPageId, PageId> = {
  overview: "configuration",
  install: "configuration",
  "codebase-memory": "configuration",
  visual: "configuration",
  advanced: "advanced-configuration",
  analytics: "analysis-logs",
  changes: "analysis-logs",
  logs: "analysis-logs",
  doctor: "diagnostics",
  compatibility: "diagnostics",
};

/**
 * Normalize persisted/session/deep-link values before they enter PageId state.
 * Unknown values fail closed to the primary configuration page.
 */
export function migratePageId(value: unknown): PageId {
  if (typeof value !== "string") return "configuration";
  if (PAGE_IDS.has(value as PageId)) return value as PageId;
  return Object.prototype.hasOwnProperty.call(LEGACY_PAGE_MIGRATIONS, value)
    ? LEGACY_PAGE_MIGRATIONS[value as LegacyPageId]
    : "configuration";
}

export function isPageId(value: unknown): value is PageId {
  return typeof value === "string" && PAGE_IDS.has(value as PageId);
}
