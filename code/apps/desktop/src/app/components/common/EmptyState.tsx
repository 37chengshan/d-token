// 统一页面状态组件：loading / empty / warning / error / success / confirming。
// 关联计划：release-plan-2026-07/phase-01 §6 页面状态矩阵、implementation-kickoff 总原则 8。
import type { ReactNode } from "react";
import { Loader2, Inbox, AlertTriangle, XCircle, CheckCircle2, ShieldAlert } from "lucide-react";
import { cn } from "../ui/utils";

export type PageStatus = "loading" | "empty" | "ready" | "warning" | "error" | "success" | "confirming";

export function EmptyState({
  status,
  title,
  description,
  actions,
  className,
}: {
  status: PageStatus;
  title: string;
  description?: string;
  actions?: ReactNode;
  className?: string;
}) {
  const icon = {
    loading: <Loader2 className="size-5 animate-spin" />,
    empty: <Inbox className="size-5" />,
    ready: <CheckCircle2 className="size-5" />,
    warning: <AlertTriangle className="size-5" />,
    error: <XCircle className="size-5" />,
    success: <CheckCircle2 className="size-5" />,
    confirming: <ShieldAlert className="size-5" />,
  }[status];

  const tone = {
    loading: "text-muted-foreground",
    empty: "text-muted-foreground",
    ready: "text-emerald-600 dark:text-emerald-400",
    warning: "text-amber-600 dark:text-amber-400",
    error: "text-red-600 dark:text-red-400",
    success: "text-emerald-600 dark:text-emerald-400",
    confirming: "text-amber-600 dark:text-amber-400",
  }[status];

  return (
    <div className={cn("flex flex-col items-center gap-2 border-y border-border/50 bg-muted/20 px-6 py-8 text-center", className)}>
      <div className={cn("flex items-center gap-2 text-sm font-medium", tone)}>
        {icon}
        <span>{title}</span>
      </div>
      {description && <p className="max-w-md text-xs text-muted-foreground">{description}</p>}
      {actions && <div className="mt-2 flex flex-wrap items-center justify-center gap-2">{actions}</div>}
    </div>
  );
}
