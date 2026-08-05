import { cn } from "../ui/utils";

interface LoadingSpinnerProps {
  size?: "sm" | "md" | "lg";
  label?: string;
  className?: string;
}

export function LoadingSpinner({ size = "md", label, className }: LoadingSpinnerProps) {
  const sizeMap = {
    sm: "size-4",
    md: "size-6",
    lg: "size-8",
  };

  return (
    <div className={cn("flex flex-col items-center justify-center gap-2", className)}>
      <div
        className={cn(
          "animate-spin rounded-full border-2 border-muted border-t-primary",
          sizeMap[size]
        )}
      />
      {label && (
        <span className="text-sm text-muted-foreground">{label}</span>
      )}
    </div>
  );
}

interface EmptyStateProps {
  icon?: React.ReactNode;
  title: string;
  description?: string;
  action?: React.ReactNode;
  className?: string;
}

export function EmptyState({ icon, title, description, action, className }: EmptyStateProps) {
  return (
    <div className={cn("flex flex-col items-center justify-center p-8 text-center", className)}>
      {icon && (
        <div className="mb-4 text-muted-foreground">{icon}</div>
      )}
      <h3 className="mb-1 text-sm font-medium">{title}</h3>
      {description && (
        <p className="mb-4 text-xs text-muted-foreground">{description}</p>
      )}
      {action && action}
    </div>
  );
}
