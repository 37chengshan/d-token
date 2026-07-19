/*
 * Adapted for d-token from AiMaMi's AnimatedSegmentedControl at
 * commit 297c7af56f10fb371b77bc9b6b65aa320afcbe7e (Apache-2.0).
 * Modified: product-specific API, soft shell styling, equal-width default,
 * keyboard behavior, disabled items, and reduced-motion handling.
 */
import {
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { cn } from "../ui/utils";

export type SegmentedItem<T extends string> = {
  value: T;
  label: ReactNode;
  icon?: ReactNode;
  disabled?: boolean;
};

type IndicatorFrame = {
  left: number;
  top: number;
  width: number;
  height: number;
  opacity: number;
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
  const containerRef = useRef<HTMLDivElement | null>(null);
  const itemRefs = useRef(new Map<T, HTMLButtonElement | null>());
  const [focusedValue, setFocusedValue] = useState<T | null>(null);
  const [indicator, setIndicator] = useState<IndicatorFrame>({
    left: 0,
    top: 0,
    width: 0,
    height: 0,
    opacity: 0,
  });

  const columns = useMemo(() => `repeat(${Math.max(items.length, 1)}, minmax(0, 1fr))`, [items.length]);
  const itemValues = useMemo(() => items.map((item) => item.value).join("|"), [items]);
  const enabledItems = useMemo(() => items.filter((item) => !item.disabled), [items]);
  const activeIndex = items.findIndex((item) => item.value === value);
  const fallbackFocusableValue = enabledItems[0]?.value ?? null;

  const measure = () => {
    const container = containerRef.current;
    const activeButton = itemRefs.current.get(value);
    if (!container || !activeButton) {
      setIndicator((prev) => ({ ...prev, opacity: 0 }));
      return;
    }

    const containerRect = container.getBoundingClientRect();
    const activeRect = activeButton.getBoundingClientRect();

    setIndicator({
      left: activeRect.left - containerRect.left,
      top: activeRect.top - containerRect.top,
      width: activeRect.width,
      height: activeRect.height,
      opacity: 1,
    });
  };

  useLayoutEffect(() => {
    measure();
  }, [value, itemValues]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container || typeof ResizeObserver === "undefined") {
      return;
    }

    const observer = new ResizeObserver(() => {
      measure();
    });

    observer.observe(container);
    for (const item of items) {
      const node = itemRefs.current.get(item.value);
      if (node) {
        observer.observe(node);
      }
    }

    window.addEventListener("resize", measure);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, [items, itemValues, value]);

  function move(event: KeyboardEvent<HTMLButtonElement>, current: T) {
    if (enabledItems.length === 0) return;

    const currentIndex = enabledItems.findIndex((item) => item.value === current);
    const baseIndex = currentIndex === -1 ? 0 : currentIndex;
    let nextIndex: number | null = null;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") nextIndex = (baseIndex + 1) % enabledItems.length;
    if (event.key === "ArrowLeft" || event.key === "ArrowUp") nextIndex = (baseIndex - 1 + enabledItems.length) % enabledItems.length;
    if (event.key === "Home") nextIndex = 0;
    if (event.key === "End") nextIndex = enabledItems.length - 1;
    if (nextIndex === null) return;

    event.preventDefault();
    const next = enabledItems[nextIndex];
    if (!next) return;
    onValueChange(next.value);
    setFocusedValue(next.value);
    requestAnimationFrame(() => itemRefs.current.get(next.value)?.focus());
  }

  return (
    <div
      ref={containerRef}
      role="radiogroup"
      aria-label={ariaLabel}
      className={cn("shell-segmented", className)}
      style={{ gridTemplateColumns: columns }}
      data-reduced-motion={reducedMotion ? "true" : "false"}
    >
      <span
        aria-hidden="true"
        className="shell-segmented-indicator"
        style={{
          left: indicator.left,
          top: indicator.top,
          width: indicator.width,
          height: indicator.height,
          opacity: indicator.opacity,
          transitionDuration: reducedMotion ? "0ms" : undefined,
        }}
      />
      {items.map((item) => {
        const active = item.value === value;
        const tabbable =
          active ||
          focusedValue === item.value ||
          (activeIndex === -1 && item.value === fallbackFocusableValue);
        return (
          <button
            key={item.value}
            ref={(node) => {
              itemRefs.current.set(item.value, node);
            }}
            type="button"
            role="radio"
            disabled={item.disabled}
            aria-checked={active}
            data-segment-value={item.value}
            tabIndex={item.disabled ? -1 : tabbable ? 0 : -1}
            className="shell-segmented-item"
            onClick={() => {
              if (item.disabled) return;
              onValueChange(item.value);
            }}
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
