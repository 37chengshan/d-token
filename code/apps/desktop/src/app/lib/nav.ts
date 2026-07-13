export type PageId = "overview" | "install" | "advanced" | "analytics" | "doctor" | "changes" | "logs" | "compatibility" | "settings";

export const NAV: { id: PageId; labelKey: string }[] = [
  { id: "overview", labelKey: "nav.overview" },
  { id: "install", labelKey: "nav.install" },
  { id: "analytics", labelKey: "nav.analytics" },
  { id: "doctor", labelKey: "nav.doctor" },
  { id: "changes", labelKey: "nav.changes" },
];

export const SETTINGS_NAV = { id: "settings" as PageId, labelKey: "nav.settings" };
