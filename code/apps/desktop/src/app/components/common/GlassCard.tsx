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
    <div
      onClick={onClick}
      className={cn(
        variant,
        "rounded-xl text-card-foreground",
        hover && "glass-card-hover",
        onClick && "cursor-pointer",
        className,
      )}
    >
      {/* Extended backdrop: height=200% so nearby elements contribute to blur */}
      <div className="glass-backdrop" />
      {/* 3D glass edge: simulates light refracting through glass thickness */}
      {edge && <div className="glass-edge" />}
      {children}
    </div>
  );
}
