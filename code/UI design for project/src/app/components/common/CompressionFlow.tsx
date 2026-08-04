import { motion } from "motion/react";
import { GlassCard } from "./GlassCard";
import { ArrowDown, Layers, Zap, Shield, Database } from "lucide-react";

interface FlowStage {
  icon: typeof Zap;
  label: string;
  detail: string;
  tokens: number;
  color: string;
}

const stages: FlowStage[] = [
  { icon: Database, label: "Input tokens", detail: "Raw prompt from agent", tokens: 100, color: "var(--chart-1)" },
  { icon: Layers, label: "RTK compression", detail: "Context dedup & merge", tokens: 31, color: "var(--info)" },
  { icon: Zap, label: "Runtime optimize", detail: "Prompt shaping & trim", tokens: 29, color: "var(--success)" },
  { icon: Shield, label: "Route adapter", detail: "Provider-specific pack", tokens: 8, color: "var(--chart-4)" },
];

export function CompressionFlow({ savedToday, compressionRate }: { savedToday: number; compressionRate: number }) {
  const fmt = (n: number) => {
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + "M";
    if (n >= 1_000) return (n / 1_000).toFixed(1) + "k";
    return String(n);
  };

  return (
    <GlassCard className="p-5">
      <div className="mb-4 flex items-center justify-between">
        <div>
          <h3>Compression pipeline</h3>
          <p className="text-sm text-muted-foreground">How tokens flow through d-token layers</p>
        </div>
        <div className="text-right">
          <div className="text-2xl font-semibold text-primary">{Math.round(compressionRate * 100)}%</div>
          <div className="text-xs text-muted-foreground">total reduction</div>
        </div>
      </div>

      <div className="relative flex flex-col gap-3">
        {stages.map((stage, i) => {
          const Icon = stage.icon;
          const isLast = i === stages.length - 1;
          const width = (stage.tokens / 100) * 100;

          return (
            <motion.div
              key={stage.label}
              initial={{ opacity: 0, x: -20 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: i * 0.1, duration: 0.4 }}
              className="relative"
            >
              <div className="glass-inset flex items-center gap-4 p-3.5">
                <div
                  className="flex size-9 shrink-0 items-center justify-center rounded-lg"
                  style={{ background: stage.color, opacity: 0.15 }}
                >
                  <Icon className="size-4" style={{ color: stage.color }} />
                </div>

                <div className="min-w-0 flex-1">
                  <div className="flex items-center justify-between text-sm">
                    <span className="font-medium">{stage.label}</span>
                    <span className="font-mono text-xs" style={{ color: stage.color }}>
                      {stage.tokens}%
                    </span>
                  </div>
                  <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-muted">
                    <motion.div
                      initial={{ width: 0 }}
                      animate={{ width: `${width}%` }}
                      transition={{ duration: 0.8, delay: i * 0.15 }}
                      className="h-full rounded-full"
                      style={{ background: stage.color }}
                    />
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">{stage.detail}</div>
                </div>
              </div>

              {!isLast && (
                <div className="flex justify-center py-1">
                  <motion.div
                    animate={{ y: [0, 3, 0] }}
                    transition={{ duration: 1.5, repeat: Infinity, delay: i * 0.2 }}
                  >
                    <ArrowDown className="size-3 text-muted-foreground" />
                  </motion.div>
                </div>
              )}
            </motion.div>
          );
        })}
      </div>

      <motion.div
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.5 }}
        className="mt-4 flex items-center justify-between rounded-lg border border-primary/20 bg-primary/5 p-3"
      >
        <div className="flex items-center gap-2 text-sm">
          <Zap className="size-4 text-primary" />
          <span>Output to provider</span>
        </div>
        <div className="text-right">
          <div className="font-mono text-sm font-semibold text-primary">{fmt(savedToday)} tokens saved</div>
          <div className="text-xs text-muted-foreground">today</div>
        </div>
      </motion.div>
    </GlassCard>
  );
}
