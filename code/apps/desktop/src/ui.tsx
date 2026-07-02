import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type CSSProperties,
} from "react";
import { ArrowUpRight, ArrowDownRight } from "lucide-react";

type Tone = "success" | "warning" | "danger" | "info" | "neutral";

const stateTone: Record<string, Tone> = {
  ok: "success",
  warn: "warning",
  err: "danger",
};

export function Badge({
  children,
  tone = "neutral",
  dot = false,
  pulse = false,
}: {
  children: ReactNode;
  tone?: Tone;
  dot?: boolean;
  pulse?: boolean;
}) {
  const cls = tone === "neutral" ? "badge" : `badge ${tone}`;
  return (
    <span className={cls}>
      {dot && <span className={pulse ? "dot pulse" : "dot"} />}
      {children}
    </span>
  );
}

export function StateBadge({
  state,
  label,
}: {
  state: "ok" | "warn" | "err";
  label: string;
}) {
  return (
    <Badge tone={stateTone[state]} dot pulse={state !== "ok"}>
      {label}
    </Badge>
  );
}

export function Card({
  children,
  pad = true,
  style,
  className = "",
}: {
  children: ReactNode;
  pad?: boolean;
  style?: CSSProperties;
  className?: string;
}) {
  return (
    <div className={`card ${pad ? "pad" : ""} ${className}`} style={style}>
      {children}
    </div>
  );
}

export function CardHead({
  icon,
  title,
  sub,
  action,
}: {
  icon?: ReactNode;
  title: string;
  sub?: string;
  action?: ReactNode;
}) {
  return (
    <div className="card-head">
      {icon && <div className="head-icon">{icon}</div>}
      <div>
        <h3>{title}</h3>
        {sub && <div className="sub">{sub}</div>}
      </div>
      {action && (
        <>
          <div className="spacer" />
          {action}
        </>
      )}
    </div>
  );
}

export function SectionTitle({ children }: { children: ReactNode }) {
  return <div className="section-title">{children}</div>;
}

/* Count-up animated number */
export function AnimatedNumber({
  value,
  format,
  duration = 900,
}: {
  value: number;
  format?: (n: number) => string;
  duration?: number;
}) {
  const [display, setDisplay] = useState(0);
  const raf = useRef<number>(0);

  useEffect(() => {
    const start = performance.now();
    const from = 0;
    const step = (now: number) => {
      const p = Math.min(1, (now - start) / duration);
      const eased = 1 - Math.pow(1 - p, 3);
      setDisplay(from + (value - from) * eased);
      if (p < 1) raf.current = requestAnimationFrame(step);
    };
    raf.current = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf.current);
  }, [value, duration]);

  return <>{format ? format(display) : Math.round(display).toLocaleString()}</>;
}

export function StatCard({
  icon,
  label,
  value,
  format,
  delta,
  deltaLabel,
  accent = "#6366f1",
}: {
  icon: ReactNode;
  label: string;
  value: number;
  format?: (n: number) => string;
  delta?: number;
  deltaLabel?: string;
  accent?: string;
}) {
  return (
    <div className="stat-card" style={{ ["--accent" as string]: accent }}>
      <div className="stat-icon">{icon}</div>
      <div className="stat-label">{label}</div>
      <div className="stat-value">
        <AnimatedNumber value={value} format={format} />
      </div>
      {delta !== undefined && (
        <div className={`stat-delta ${delta >= 0 ? "up" : "down"}`}>
          {delta >= 0 ? <ArrowUpRight size={14} /> : <ArrowDownRight size={14} />}
          {Math.abs(delta)}% {deltaLabel}
        </div>
      )}
    </div>
  );
}

export function Progress({
  value,
  tone,
}: {
  value: number;
  tone?: "success" | "warning";
}) {
  const [w, setW] = useState(0);
  useEffect(() => {
    const id = setTimeout(() => setW(value), 60);
    return () => clearTimeout(id);
  }, [value]);
  return (
    <div className={`progress ${tone ?? ""}`}>
      <span style={{ width: `${w}%` }} />
    </div>
  );
}

/* Simple animated SVG sparkline */
export function Sparkline({
  data,
  color = "#6366f1",
  height = 48,
  width = 220,
}: {
  data: number[];
  color?: string;
  height?: number;
  width?: number;
}) {
  const max = Math.max(...data, 1);
  const min = Math.min(...data, 0);
  const range = max - min || 1;
  const stepX = width / (data.length - 1);
  const pts = data.map((d, i) => {
    const x = i * stepX;
    const y = height - ((d - min) / range) * (height - 8) - 4;
    return [x, y] as const;
  });
  const line = pts.map((p) => `${p[0]},${p[1]}`).join(" ");
  const area = `0,${height} ${line} ${width},${height}`;
  const gid = `spark-${color.replace(/[^a-z0-9]/gi, "")}`;
  return (
    <svg width={width} height={height} className="sparkline" style={{ overflow: "visible" }}>
      <defs>
        <linearGradient id={gid} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={color} stopOpacity="0.28" />
          <stop offset="100%" stopColor={color} stopOpacity="0" />
        </linearGradient>
      </defs>
      <polygon points={area} fill={`url(#${gid})`} />
      <polyline
        points={line}
        fill="none"
        stroke={color}
        strokeWidth="2.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        style={{
          strokeDasharray: 600,
          strokeDashoffset: 600,
          animation: "dash 1.2s var(--ease) forwards",
        }}
      />
      <circle cx={pts[pts.length - 1][0]} cy={pts[pts.length - 1][1]} r="3.5" fill={color} />
      <style>{`@keyframes dash { to { stroke-dashoffset: 0; } }`}</style>
    </svg>
  );
}

export function Tabs({
  tabs,
  active,
  onChange,
}: {
  tabs: { id: string; label: string }[];
  active: string;
  onChange: (id: string) => void;
}) {
  return (
    <div className="tabs">
      {tabs.map((t) => (
        <button
          key={t.id}
          className={`tab ${active === t.id ? "active" : ""}`}
          onClick={() => onChange(t.id)}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}

export function KV({ k, v }: { k: string; v: ReactNode }) {
  return (
    <div className="kv">
      <span className="k">{k}</span>
      <span className="v">{v}</span>
    </div>
  );
}
