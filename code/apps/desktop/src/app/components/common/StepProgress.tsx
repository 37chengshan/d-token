import { motion } from "motion/react";
import { Check, Loader2 } from "lucide-react";
import { cn } from "../ui/utils";

export type Step = { key: string; label: string };

/**
 * current: index of the in-progress step; steps before it are done. Pass a
 * negative current (e.g. -1) to render every step pending (nothing running).
 *
 * indeterminate: the exact in-progress sub-step is unknown (e.g. a
 * synchronous native transaction without progress callbacks); the active node
 * shows a breathing pulse instead of a spinner so the UI never claims a
 * specific stage it cannot observe.
 *
 * reducedMotion: disables transform / spin / pulse animation entirely — the
 * node and connector render statically and every phase label stays readable.
 */
export function StepProgress({
  steps,
  current,
  done,
  indeterminate = false,
  reducedMotion = false,
}: {
  steps: Step[];
  current: number;
  done: boolean;
  indeterminate?: boolean;
  reducedMotion?: boolean;
}) {
  const running = !done && current >= 0;
  return (
    <div
      className="flex items-center"
      data-reduced-motion={reducedMotion ? "true" : "false"}
      data-progress-done={done ? "true" : "false"}
      data-progress-indeterminate={indeterminate && running ? "true" : "false"}
    >
      {steps.map((s, i) => {
        const isDone = done || (current >= 0 && i < current);
        const isActive = running && i === current;
        const circle = (
          <div
            className={cn(
              "flex size-8 items-center justify-center rounded-full border text-sm",
              isDone && "border-transparent bg-primary text-primary-foreground",
              isActive && "border-primary text-primary",
              !isDone && !isActive && "border-border text-muted-foreground",
            )}
          >
            {isDone
              ? <Check className="size-4" />
              : isActive && indeterminate
                ? <span data-progress-indeterminate-dot="true" className={cn("size-2 rounded-full bg-primary", !reducedMotion && "animate-pulse")} />
                : isActive
                  ? <Loader2 className={cn("size-4", !reducedMotion && "animate-spin")} />
                  : i + 1}
          </div>
        );
        return (
          <div key={s.key} className="flex flex-1 items-center last:flex-none">
            <div className="flex flex-col items-center gap-1.5">
              {reducedMotion
                ? circle
                : <motion.div
                    animate={{ scale: isActive ? 1.1 : 1 }}
                    transition={{ duration: 0.28, ease: "easeOut" }}
                    className={cn(isActive && "step-active-pulse")}
                  >{circle}</motion.div>}
              <span className={cn("text-xs", isActive ? "text-foreground" : "text-muted-foreground")}>{s.label}</span>
            </div>
            {i < steps.length - 1 && (
              <div className="mx-2 h-0.5 flex-1 overflow-hidden rounded-full bg-border">
                {reducedMotion
                  ? <div className={cn("h-full bg-primary", isDone ? "w-full" : "w-0")} />
                  : (
                    <motion.div
                      className="h-full bg-primary"
                      initial={{ width: "0%" }}
                      animate={{ width: isDone ? "100%" : "0%" }}
                      transition={{ duration: 0.4 }}
                    />
                  )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
