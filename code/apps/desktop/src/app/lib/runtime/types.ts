import { z } from "zod";

export const DesktopDataSchema = z.object({
  desktop_revision: z.string().trim().min(1),
  desktop_data_source: z.string().trim().min(1).optional(),
  desktop_collected_at: z.string().trim().min(1).optional(),
  source: z.string().trim().min(1).optional(),
}).catchall(z.unknown());

export const DesktopSummarySchema = DesktopDataSchema;

export type DesktopData = z.infer<typeof DesktopDataSchema>;
export type DesktopSummary = z.infer<typeof DesktopSummarySchema>;

export type RuntimeLogKind = "route" | "compression" | "error" | "provider";

export type Health = "healthy" | "warning" | "error";

export type RuntimeLogEntry = {
  id: string;
  time: string;
  type: RuntimeLogKind;
  agent: string;
  msg: string;
  msgEn: string;
};
