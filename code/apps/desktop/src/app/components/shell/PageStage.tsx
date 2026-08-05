/*
 * Adapted for d-token from AiMaMi's PageStage at
 * commit 297c7af56f10fb371b77bc9b6b65aa320afcbe7e (Apache-2.0).
 * Modified: AnimatePresence lifecycle, d-token spacing, reduced-motion path,
 * and non-interactive heading readiness measurement for lazy content.
 */
import { type ReactNode, useEffect, useRef } from "react";
import { performanceNow, recordPerformanceDuration } from "../../lib/performance";

export function PageStage({ children, reducedMotion }: { children: ReactNode; reducedMotion: boolean }) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const enteredAtRef = useRef(performanceNow());

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;

    let cancelled = false;
    let observer: MutationObserver | null = null;
    let timer = 0;

    const observePageHeading = (): boolean => {
      if (cancelled) return true;
      const heading = root.querySelector<HTMLElement>("h1");
      if (!heading) return false;
      recordPerformanceDuration("page_stage", "heading_ready", enteredAtRef.current, "ok");
      return true;
    };

    const frame = requestAnimationFrame(() => {
      if (observePageHeading()) return;
      // Lazy routes: h1 appears after Suspense resolves.
      if (typeof MutationObserver !== "undefined") {
        observer = new MutationObserver(() => {
          if (observePageHeading()) {
            observer?.disconnect();
            observer = null;
          }
        });
        observer.observe(root, { childList: true, subtree: true });
      }
      timer = window.setTimeout(() => {
        observer?.disconnect();
        observer = null;
        observePageHeading();
      }, 1500);
    });

    return () => {
      cancelled = true;
      cancelAnimationFrame(frame);
      observer?.disconnect();
      if (timer) window.clearTimeout(timer);
    };
  }, []);

  return (
    <div
      ref={rootRef}
      className="shell-page-stage"
      data-page-motion="enter"
      data-reduced-motion={reducedMotion ? "true" : "false"}
    >
      {children}
    </div>
  );
}
