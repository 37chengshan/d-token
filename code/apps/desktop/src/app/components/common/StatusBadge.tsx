import { CheckCircle2, AlertTriangle, XCircle } from "lucide-react";
import type { Health } from "../../lib/runtime/types";
import { cn } from "../ui/utils";

const MAP: Record<Health, { cls: string; Icon: typeof CheckCircle2 }> = {
  healthy: { cls: "text-emerald-600 bg-emerald-500/10", Icon: CheckCircle2 },
  warning: { cls: "text-amber-600 bg-amber-500/10", Icon: AlertTriangle },
  error: { cls: "text-rose-600 bg-rose-500/10", Icon: XCircle },
};

export function StatusBadge({ status, label, className }: { status: Health; label: string; className?: string }) {
  const { cls, Icon } = MAP[status];
  return (
    <span className={cn("inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-sm", cls, className)}>
      <Icon className="size-3.5" />
      {label}
    </span>
  );
}

export function StatusDot({ status }: { status: Health }) {
  const color = status === "healthy" ? "bg-emerald-500" : status === "warning" ? "bg-amber-500" : "bg-rose-500";
  return (
    <span className="relative inline-flex size-2.5" aria-hidden="true">
      <span className={cn("relative inline-flex size-2.5 rounded-full", color)} />
    </span>
  );
}
