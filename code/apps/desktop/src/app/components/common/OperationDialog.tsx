import type { ReactNode } from "react";
import { AlertTriangle, LoaderCircle } from "lucide-react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "../ui/alert-dialog";
import { cn } from "../ui/utils";

export type OperationRisk = "normal" | "warning" | "danger";

export function OperationDialog({
  open,
  onOpenChange,
  title,
  summary,
  impact,
  details,
  confirmLabel,
  cancelLabel,
  risk = "normal",
  pending = false,
  confirmDisabled = false,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  summary: string;
  impact?: string;
  details?: ReactNode;
  confirmLabel: string;
  cancelLabel: string;
  risk?: OperationRisk;
  pending?: boolean;
  confirmDisabled?: boolean;
  onConfirm: () => void | Promise<void>;
}) {
  return (
    <AlertDialog open={open} onOpenChange={(next) => { if (!pending) onOpenChange(next); }}>
      <AlertDialogContent className="max-w-xl" data-operation-risk={risk}>
        <AlertDialogHeader>
          <AlertDialogTitle className="flex items-center gap-2">
            {risk !== "normal" && <AlertTriangle className={cn("size-4", risk === "danger" ? "text-destructive" : "text-amber-600")} />}
            {title}
          </AlertDialogTitle>
          <AlertDialogDescription>{summary}</AlertDialogDescription>
        </AlertDialogHeader>
        {impact && <div className="rounded-lg border border-border bg-muted/40 p-3 text-sm text-muted-foreground">{impact}</div>}
        {details}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={pending}>{cancelLabel}</AlertDialogCancel>
          <AlertDialogAction
            onClick={(event) => {
              event.preventDefault();
              void onConfirm();
            }}
            disabled={pending || confirmDisabled}
            className={risk === "danger" ? "bg-destructive text-white hover:bg-destructive/90" : undefined}
          >
            {pending && <LoaderCircle className="size-4 animate-spin" />}
            {confirmLabel}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
