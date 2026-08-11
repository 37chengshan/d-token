export type PageId =
  | "overview"
  | "install"
  | "codebase-memory"
  | "advanced"
  | "analytics"
  | "visual"
  | "doctor"
  | "changes"
  | "logs"
  | "compatibility"
  | "settings";

export type NavItem = {
  id: PageId;
  labelKey: string;
  badge?: "beta" | "experimental";
};

export const NAV: NavItem[] = [
  { id: "overview", labelKey: "nav.overview" },
  { id: "install", labelKey: "nav.install" },
  { id: "codebase-memory", labelKey: "nav.codebaseMemory", badge: "experimental" },
  { id: "analytics", labelKey: "nav.analytics" },
  { id: "visual", labelKey: "nav.visual", badge: "beta" },
  { id: "doctor", labelKey: "nav.doctor" },
  { id: "changes", labelKey: "nav.changes" },
];

export const SETTINGS_NAV: NavItem = { id: "settings", labelKey: "nav.settings" };
