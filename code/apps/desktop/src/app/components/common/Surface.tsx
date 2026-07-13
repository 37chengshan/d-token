import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "../ui/utils";

type SurfaceVariant = "primary" | "section" | "row";

type SurfaceProps = Omit<HTMLAttributes<HTMLDivElement>, "children"> & {
  children: ReactNode;
  variant?: SurfaceVariant;
  /** Compatibility while page call sites migrate from GlassCard. */
  strong?: boolean;
  hover?: boolean;
};

/**
 * Quiet, opaque-enough product surface inspired by AiMaMi's grouped panels.
 * It intentionally has no backdrop-filter, pseudo-glass edge, or nested blur layer.
 */
export function Surface({
  children,
  className,
  variant = "section",
  strong = false,
  hover = false,
  ...props
}: SurfaceProps) {
  const resolvedVariant = strong ? "primary" : variant;

  return (
    <div
      data-surface={resolvedVariant}
      className={cn(
        "surface",
        `surface-${resolvedVariant}`,
        hover && "surface-interactive",
        props.onClick && "cursor-pointer",
        className,
      )}
      {...props}
    >
      {children}
    </div>
  );
}

export function MetricStrip({ children, className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div data-surface="metric-strip" className={cn("metric-strip", className)} {...props}>
      {children}
    </div>
  );
}

export function FlatRows({ children, className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div data-flat-rows className={cn("flat-rows", className)} {...props}>
      {children}
    </div>
  );
}

export function FlatRow({ children, className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div data-flat-row className={cn("flat-row", className)} {...props}>
      {children}
    </div>
  );
}
