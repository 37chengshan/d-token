import { motion } from "motion/react";
import type { ReactNode } from "react";
import { cn } from "../ui/utils";

export function GlassCard({
  children,
  className,
  strong = false,
  hover = true,
  edge = true,
  onClick,
}: {
  children: ReactNode;
  className?: string;
  strong?: boolean;
  hover?: boolean;
  edge?: boolean;
  onClick?: () => void;
}) {
  const variant = strong ? "glass-strong" : "glass";

  return (
    <motion.div
      onClick={onClick}
      whileHover={hover ? { y: -2 } : undefined}
      transition={{ type: "spring", stiffness: 300, damping: 24 }}
      className={cn(
        variant,
        "rounded-xl text-card-foreground",
        onClick && "cursor-pointer",
        className,
      )}
    >
      {/* Extended backdrop: height=200% so nearby elements contribute to blur */}
      <div className="glass-backdrop" />
      {/* 3D glass edge: simulates light refracting through glass thickness */}
      {edge && <div className="glass-edge" />}
      {children}
    </motion.div>
  );
}
