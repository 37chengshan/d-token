import { LayoutGrid, Download, SlidersHorizontal, BarChart3, Stethoscope, GitCompareArrows, ScrollText, Settings } from "lucide-react";

export type PageId = "overview" | "install" | "advanced" | "analytics" | "doctor" | "changes" | "logs" | "settings";

export const NAV: { id: PageId; icon: typeof LayoutGrid; labelKey: string }[] = [
  { id: "overview", icon: LayoutGrid, labelKey: "nav.overview" },
  { id: "install", icon: Download, labelKey: "nav.install" },
  { id: "advanced", icon: SlidersHorizontal, labelKey: "nav.advanced" },
  { id: "analytics", icon: BarChart3, labelKey: "nav.analytics" },
  { id: "doctor", icon: Stethoscope, labelKey: "nav.doctor" },
  { id: "changes", icon: GitCompareArrows, labelKey: "nav.changes" },
  { id: "logs", icon: ScrollText, labelKey: "nav.logs" },
];

export const SETTINGS_NAV = { id: "settings" as PageId, icon: Settings, labelKey: "nav.settings" };
