/*
 * Adapted for d-token from AiMaMi's AnimatedSegmentedControl at
 * commit 297c7af56f10fb371b77bc9b6b65aa320afcbe7e (Apache-2.0).
 * Modified: product-specific API, styling, keyboard behavior and reduced-motion handling.
 */
import { type KeyboardEvent, type ReactNode, useMemo, useState } from "react";
import { cn } from "../ui/utils";

export type SegmentedItem<T extends string> = {
  value: T;
  label: ReactNode;
  icon?: ReactNode;
};

export function AnimatedSegmentedControl<T extends string>({
  items,
  value,
  onValueChange,
  ariaLabel,
  reducedMotion,
  className,
}: {
  items: readonly SegmentedItem<T>[];
  value: T;
  onValueChange: (value: T) => void;
  ariaLabel: string;
  reducedMotion: boolean;
  className?: string;
}) {
  const [focusedValue, setFocusedValue] = useState<T | null>(null);
  const index = Math.max(0, items.findIndex((item) => item.value === value));
  const columns = useMemo(() => `repeat(${Math.max(items.length, 1)}, minmax(0, 1fr))`, [items.length]);

  function move(event: KeyboardEvent<HTMLButtonElement>, current: T) {
    const currentIndex = items.findIndex((item) => item.value === current);
    let nextIndex: number | null = null;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") nextIndex = (currentIndex + 1) % items.length;
    if (event.key === "ArrowLeft" || event.key === "ArrowUp") nextIndex = (currentIndex - 1 + items.length) % items.length;
    if (event.key === "Home") nextIndex = 0;
    if (event.key === "End") nextIndex = items.length - 1;
    if (nextIndex === null) return;
    event.preventDefault();
    const next = items[nextIndex];
    if (!next) return;
    onValueChange(next.value);
    setFocusedValue(next.value);
    requestAnimationFrame(() => document.querySelector<HTMLButtonElement>(`[data-segment-value="${next.value}"]`)?.focus());
  }

  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className={cn("shell-segmented", className)}
      style={{ gridTemplateColumns: columns }}
      data-reduced-motion={reducedMotion ? "true" : "false"}
    >
      <span
        aria-hidden="true"
        className="shell-segmented-indicator"
        style={{ width: `${100 / Math.max(items.length, 1)}%`, transform: `translateX(${index * 100}%)` }}
      />
      {items.map((item) => {
        const active = item.value === value;
        return (
          <button
            key={item.value}
            type="button"
            role="radio"
            aria-checked={active}
            data-segment-value={item.value}
            tabIndex={active || focusedValue === item.value ? 0 : -1}
            className="shell-segmented-item"
            onClick={() => onValueChange(item.value)}
            onKeyDown={(event) => move(event, item.value)}
          >
            {item.icon}
            <span>{item.label}</span>
          </button>
        );
      })}
    </div>
  );
}
