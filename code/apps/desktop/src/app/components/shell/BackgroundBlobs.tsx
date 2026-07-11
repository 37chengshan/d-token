import { useEffect, useRef } from "react";

const NORMAL_CELL_SIZE = 26;
const DEGRADED_CELL_SIZE = 30;
const NORMAL_FRAME_INTERVAL = 100;
const DEGRADED_FRAME_INTERVAL = 1000 / 6;
const BREATHING_PERIOD_MS = 60_000;
const STATIC_PHASE = 0.82;
const FIELD_THRESHOLD = 0.25;
const MAX_DPR = 1.25;
const SLOW_FRAME_MS = 4;
const FREEZE_FRAME_MS = 8;
const SLOW_FRAME_LIMIT = 3;
const GLYPHS = [".", ":", "+", "*"] as const;

type FieldCell = {
  accent: boolean;
  bias: number;
  glyphIndex: number;
  nx: number;
  ny: number;
  x: number;
  y: number;
};

type FieldTheme = {
  accent: string;
  accentAlphaMax: number;
  glyph: string;
  glyphAlphaMax: number;
  mono: string;
};

function clamp01(value: number) {
  return Math.max(0, Math.min(1, value));
}

function interpolate(from: number, to: number, amount: number) {
  return from + (to - from) * clamp01(amount);
}

function stableUnitValue(column: number, row: number, salt: number) {
  let value = Math.imul(column + 17, 0x45d9f3b) ^ Math.imul(row + 31, 0x27d4eb2d) ^ salt;
  value = Math.imul(value ^ (value >>> 15), 0x2c1b3c6d);
  value ^= value >>> 12;
  return (value >>> 0) / 0x1_0000_0000;
}

function workspaceMask(nx: number, ny: number) {
  let horizontal = 0;
  if (nx > 0.18 && nx <= 0.38) {
    horizontal = interpolate(0, 0.3, (nx - 0.18) / 0.2);
  } else if (nx > 0.38 && nx <= 0.68) {
    horizontal = interpolate(0.3, 1, (nx - 0.38) / 0.3);
  } else if (nx > 0.68) {
    horizontal = interpolate(1, 0.25, (nx - 0.68) / 0.32);
  }

  const topQuiet = ny <= 0.1 ? 0.08 : ny < 0.24 ? interpolate(0.08, 1, (ny - 0.1) / 0.14) : 1;
  return horizontal * topQuiet;
}

function isDarkSurface(styles: CSSStyleDeclaration) {
  const scheme = styles.colorScheme?.toLowerCase() ?? "";
  return (
    document.documentElement.classList.contains("dark") ||
    document.body.classList.contains("dark") ||
    scheme.split(/\s+/).includes("dark")
  );
}

function readTheme(canvas: HTMLCanvasElement): FieldTheme {
  const styles = getComputedStyle(canvas);
  const dark = isDarkSurface(styles);
  const glyph = styles.getPropertyValue("--ambient-glyph").trim() || (dark ? "#eee5da" : "#6f6258");
  const accent = styles.getPropertyValue("--ambient-accent").trim() || "#c56a49";

  return {
    accent,
    accentAlphaMax: 0.035,
    glyph,
    glyphAlphaMax: dark ? 0.045 : 0.035,
    mono: styles.getPropertyValue("--font-mono").trim() || "ui-monospace, SFMono-Regular, Consolas, monospace",
  };
}

export function BackgroundBlobs() {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const context = canvas.getContext("2d", { alpha: true });
    if (!context) return;

    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const glyphBuckets = Array.from({ length: 4 }, () => [] as FieldCell[]);
    const accentBuckets = Array.from({ length: 4 }, () => [] as FieldCell[]);

    let cells: FieldCell[] = [];
    let cssWidth = 0;
    let cssHeight = 0;
    let gridSize = NORMAL_CELL_SIZE;
    let frameInterval = NORMAL_FRAME_INTERVAL;
    let lastFrameAt = Number.NEGATIVE_INFINITY;
    let animationFrame = 0;
    let slowFrames = 0;
    let frozenForPerformance = false;
    let phaseAtFreeze = STATIC_PHASE;
    let theme = readTheme(canvas);
    const startedAt = performance.now();

    const documentIsActive = () => {
      const focused = typeof document.hasFocus !== "function" || document.hasFocus();
      return document.visibilityState === "visible" && focused;
    };

    const buildCells = () => {
      if (cssWidth < 1 || cssHeight < 1) {
        cells = [];
        return;
      }

      const columns = Math.ceil(cssWidth / gridSize);
      const rows = Math.ceil(cssHeight / gridSize);
      const nextCells: FieldCell[] = [];

      for (let row = 0; row < rows; row += 1) {
        for (let column = 0; column < columns; column += 1) {
          const x = (column + 0.5) * gridSize;
          const y = (row + 0.5) * gridSize;
          const bias = stableUnitValue(column, row, 0x51ed270b);
          nextCells.push({
            accent: stableUnitValue(column, row, 0x6c8e9cf5) < 0.02,
            bias,
            glyphIndex: Math.min(GLYPHS.length - 1, Math.floor(bias * GLYPHS.length)),
            nx: x / cssWidth,
            ny: y / cssHeight,
            x,
            y,
          });
        }
      }

      cells = nextCells;
    };

    const draw = (phase: number, measurePerformance: boolean) => {
      if (cssWidth < 1 || cssHeight < 1) return;

      const drawStartedAt = performance.now();
      context.clearRect(0, 0, cssWidth, cssHeight);
      glyphBuckets.forEach((bucket) => {
        bucket.length = 0;
      });
      accentBuckets.forEach((bucket) => {
        bucket.length = 0;
      });

      const primaryX = 0.73 + Math.cos(phase) * 0.055;
      const primaryY = 0.34 + Math.sin(phase * 0.74) * 0.045;
      const secondaryX = 0.65 + Math.sin(phase * 0.61 + 1.1) * 0.05;
      const secondaryY = 0.67 + Math.cos(phase * 0.69 + 0.45) * 0.055;
      const breathing = 0.94 + Math.cos(phase * 0.5) * 0.06;

      cells.forEach((cell) => {
        const mask = workspaceMask(cell.nx, cell.ny);
        if (mask <= 0.01) return;

        const primaryDx = (cell.nx - primaryX) / 0.26;
        const primaryDy = (cell.ny - primaryY) / 0.34;
        const secondaryDx = (cell.nx - secondaryX) / 0.34;
        const secondaryDy = (cell.ny - secondaryY) / 0.28;
        const primary = 1 / (1 + 2.7 * (primaryDx * primaryDx + primaryDy * primaryDy));
        const secondary = 1 / (1 + 3.1 * (secondaryDx * secondaryDx + secondaryDy * secondaryDy));
        const bias = 0.88 + cell.bias * 0.12;
        const field = (0.13 + primary * 0.72 + secondary * 0.36) * breathing * bias * mask;

        if (field < FIELD_THRESHOLD) return;

        const strength = clamp01((field - FIELD_THRESHOLD) / (1 - FIELD_THRESHOLD));
        const bucketIndex = Math.min(3, Math.floor(strength * 4));
        (cell.accent ? accentBuckets : glyphBuckets)[bucketIndex].push(cell);
      });

      context.font = `500 11px ${theme.mono}`;
      context.textAlign = "center";
      context.textBaseline = "middle";

      const paintBuckets = (buckets: FieldCell[][], color: string, minimumAlpha: number, maximumAlpha: number) => {
        context.fillStyle = color;
        buckets.forEach((bucket, index) => {
          if (bucket.length === 0) return;
          context.globalAlpha = interpolate(minimumAlpha, maximumAlpha, index / 3);
          bucket.forEach((cell) => {
            context.fillText(GLYPHS[cell.glyphIndex], cell.x, cell.y);
          });
        });
      };

      paintBuckets(glyphBuckets, theme.glyph, theme.glyphAlphaMax * 0.34, theme.glyphAlphaMax);
      paintBuckets(accentBuckets, theme.accent, 0.018, theme.accentAlphaMax);
      context.globalAlpha = 1;

      if (!measurePerformance) return;

      const duration = performance.now() - drawStartedAt;
      if (duration > FREEZE_FRAME_MS) {
        phaseAtFreeze = phase;
        frozenForPerformance = true;
        return;
      }

      slowFrames = duration > SLOW_FRAME_MS ? slowFrames + 1 : 0;
      if (slowFrames >= SLOW_FRAME_LIMIT && gridSize === NORMAL_CELL_SIZE) {
        gridSize = DEGRADED_CELL_SIZE;
        frameInterval = DEGRADED_FRAME_INTERVAL;
        slowFrames = 0;
        buildCells();
      }
    };

    const cancelAnimation = () => {
      if (animationFrame) window.cancelAnimationFrame(animationFrame);
      animationFrame = 0;
    };

    const shouldAnimate = () => !reducedMotion.matches && !frozenForPerformance && documentIsActive();

    const tick = (now: number) => {
      animationFrame = 0;
      if (!shouldAnimate()) return;

      if (now - lastFrameAt >= frameInterval) {
        lastFrameAt = now;
        const phase = ((now - startedAt) / BREATHING_PERIOD_MS) * Math.PI * 2;
        phaseAtFreeze = phase;
        draw(phase, true);
      }

      if (shouldAnimate()) animationFrame = window.requestAnimationFrame(tick);
    };

    const startAnimation = () => {
      if (animationFrame || !shouldAnimate()) return;
      lastFrameAt = Number.NEGATIVE_INFINITY;
      animationFrame = window.requestAnimationFrame(tick);
    };

    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      const nextWidth = Math.max(0, rect.width || canvas.clientWidth);
      const nextHeight = Math.max(0, rect.height || canvas.clientHeight);
      if (nextWidth < 1 || nextHeight < 1) return;

      cssWidth = nextWidth;
      cssHeight = nextHeight;
      const dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR);
      canvas.width = Math.max(1, Math.round(cssWidth * dpr));
      canvas.height = Math.max(1, Math.round(cssHeight * dpr));
      context.setTransform(dpr, 0, 0, dpr, 0, 0);
      buildCells();
      draw(reducedMotion.matches ? STATIC_PHASE : phaseAtFreeze, false);
    };

    const handleActivityChange = () => {
      if (shouldAnimate()) startAnimation();
      else cancelAnimation();
    };

    const handleReducedMotionChange = () => {
      cancelAnimation();
      if (reducedMotion.matches) {
        draw(STATIC_PHASE, false);
        return;
      }
      startAnimation();
    };

    const handleThemeChange = () => {
      theme = readTheme(canvas);
      if (document.visibilityState === "visible") draw(reducedMotion.matches ? STATIC_PHASE : phaseAtFreeze, false);
    };

    const resizeObserver =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(() => {
            resize();
          });
    resizeObserver?.observe(canvas);
    if (!resizeObserver) window.addEventListener("resize", resize, { passive: true });

    const themeObserver =
      typeof MutationObserver === "undefined" ? null : new MutationObserver(handleThemeChange);
    themeObserver?.observe(document.documentElement, {
      attributeFilter: ["class", "data-theme", "data-flow-palette", "data-accent", "data-chart-accent", "style"],
      attributes: true,
    });
    themeObserver?.observe(document.body, {
      attributeFilter: ["class", "data-theme", "style"],
      attributes: true,
    });

    document.addEventListener("visibilitychange", handleActivityChange);
    window.addEventListener("focus", handleActivityChange);
    window.addEventListener("blur", handleActivityChange);
    reducedMotion.addEventListener("change", handleReducedMotionChange);

    resize();
    if (!reducedMotion.matches) startAnimation();

    return () => {
      cancelAnimation();
      resizeObserver?.disconnect();
      themeObserver?.disconnect();
      if (!resizeObserver) window.removeEventListener("resize", resize);
      document.removeEventListener("visibilitychange", handleActivityChange);
      window.removeEventListener("focus", handleActivityChange);
      window.removeEventListener("blur", handleActivityChange);
      reducedMotion.removeEventListener("change", handleReducedMotionChange);
    };
  }, []);

  return <canvas ref={canvasRef} className="background-atmosphere background-ascii-field" aria-hidden="true" />;
}
