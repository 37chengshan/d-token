import type { Health } from "../runtime/types";

export type DoctorLayer = "runtime" | "port" | "route" | "provider" | "security";

export type DoctorCheck = {
  id: string;
  layer: DoctorLayer;
  name: string;
  nameEn: string;
  status: Health;
  detail: string;
  detailEn: string;
  repair?: string;
};
