import { motion } from "motion/react";
import { ArrowRight } from "lucide-react";
import type { ReactNode } from "react";

export function FlowDiagram({ nodes }: { nodes: { label: string; sub?: string; icon?: ReactNode }[] }) {
  return (
    <div className="flex flex-wrap items-center gap-2">
      {nodes.map((n, i) => (
        <div key={i} className="flex items-center gap-2">
          <motion.div
            initial={{ opacity: 0, scale: 0.9 }}
            animate={{ opacity: 1, scale: 1 }}
            transition={{ duration: 0.28, ease: "easeOut", delay: Math.min(i * 0.06, 0.36) }}
            className="rounded-md bg-muted/45 px-3 py-2 text-center"
          >
            <div className="flex items-center justify-center gap-1.5 text-sm">{n.icon}{n.label}</div>
            {n.sub && <div className="text-xs text-muted-foreground">{n.sub}</div>}
          </motion.div>
          {i < nodes.length - 1 && (
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ duration: 0.28, ease: "easeOut", delay: Math.min(i * 0.06, 0.36) + 0.06 }}
            >
              <ArrowRight className="size-4 text-muted-foreground" />
            </motion.div>
          )}
        </div>
      ))}
    </div>
  );
}
