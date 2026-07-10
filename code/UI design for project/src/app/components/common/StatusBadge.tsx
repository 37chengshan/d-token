import { CheckCircle2, AlertTriangle, XCircle } from "lucide-react";
import type { Health } from "../../lib/fixtures";
import { cn } from "../ui/utils";

const MAP: Record<Health, { cls: string; Icon: typeof CheckCircle2 }> = {
  healthy: { cls: "text-success bg-success/10", Icon: CheckCircle2 },
  warning: { cls: "text-warning bg-warning/10", Icon: AlertTriangle },
  error: { cls: "text-destructive bg-destructive/10", Icon: XCircle },
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
  const color = status === "healthy" ? "bg-success" : status === "warning" ? "bg-warning" : "bg-destructive";
  return (
    <span className="relative inline-flex size-2.5">
      <span className={cn("absolute inline-flex h-full w-full rounded-full opacity-60 animate-ping", color)} />
      <span className={cn("relative inline-flex size-2.5 rounded-full", color)} />
    </span>
  );
}
