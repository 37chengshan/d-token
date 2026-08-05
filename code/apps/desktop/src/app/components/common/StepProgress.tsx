import { motion } from "motion/react";
import { Check, Loader2 } from "lucide-react";
import { cn } from "../ui/utils";

export type Step = { key: string; label: string };

// current: index of the in-progress step; steps before it are done.
export function StepProgress({ steps, current, done }: { steps: Step[]; current: number; done: boolean }) {
  return (
    <div className="flex items-center">
      {steps.map((s, i) => {
        const isDone = done || i < current;
        const isActive = !done && i === current;
        return (
          <div key={s.key} className="flex flex-1 items-center last:flex-none">
            <div className="flex flex-col items-center gap-1.5">
              <motion.div
                animate={{ scale: isActive ? 1.1 : 1 }}
                className={cn(
                  "flex size-8 items-center justify-center rounded-full border text-sm",
                  isDone && "border-transparent bg-primary text-primary-foreground",
                  isActive && "border-primary text-primary",
                  !isDone && !isActive && "border-border text-muted-foreground",
                )}
              >
                {isDone ? <Check className="size-4" /> : isActive ? <Loader2 className="size-4 animate-spin" /> : i + 1}
              </motion.div>
              <span className={cn("text-xs", isActive ? "text-foreground" : "text-muted-foreground")}>{s.label}</span>
            </div>
            {i < steps.length - 1 && (
              <div className="mx-2 h-0.5 flex-1 overflow-hidden rounded-full bg-border">
                <motion.div
                  className="h-full bg-primary"
                  initial={{ width: "0%" }}
                  animate={{ width: isDone ? "100%" : "0%" }}
                  transition={{ duration: 0.4 }}
                />
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
