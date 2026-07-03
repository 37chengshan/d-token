import { motion } from "motion/react";
import type { ReactNode } from "react";
import { GlassCard } from "./GlassCard";

export function StatCard({
  label,
  value,
  sub,
  icon,
  accentBar,
  index = 0,
}: {
  label: string;
  value: ReactNode;
  sub?: ReactNode;
  icon?: ReactNode;
  accentBar?: string;
  index?: number;
}) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: index * 0.05, duration: 0.35 }}
    >
      <GlassCard className="overflow-hidden p-5">
        {accentBar && <div className="mb-4 h-1 w-10 rounded-full" style={{ background: accentBar }} />}
        <div className="flex items-start justify-between">
          <div className="text-sm text-muted-foreground">{label}</div>
          {icon && <div className="text-muted-foreground">{icon}</div>}
        </div>
        <div className="mt-2 text-3xl tracking-tight" style={{ fontWeight: 600 }}>{value}</div>
        {sub && <div className="mt-1 text-sm text-muted-foreground">{sub}</div>}
      </GlassCard>
    </motion.div>
  );
}
