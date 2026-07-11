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
