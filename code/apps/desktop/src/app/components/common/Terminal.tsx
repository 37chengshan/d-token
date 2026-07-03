import { useEffect, useRef, useState } from "react";
import type { LogEntry } from "../../lib/fixtures";
import { useI18n } from "../../lib/i18n";

const LEVEL_COLOR: Record<LogEntry["type"], string> = {
  route: "text-sky-400",
  compression: "text-emerald-400",
  error: "text-rose-400",
  provider: "text-violet-400",
};
const LEVEL_TAG: Record<LogEntry["type"], string> = {
  route: "ROUTE", compression: "COMPRESS", error: "ERROR", provider: "PROVIDER",
};

// Terminal-style streaming log viewer. Lines print progressively with a
// blinking cursor, always on a dark surface regardless of theme.
export function Terminal({ lines }: { lines: LogEntry[] }) {
  const { lang } = useI18n();
  const [count, setCount] = useState(0);
  const boxRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setCount(0);
    const timer = setInterval(() => {
      setCount((c) => {
        if (c >= lines.length) { clearInterval(timer); return c; }
        return c + 1;
      });
    }, 220);
    return () => clearInterval(timer);
  }, [lines]);

  useEffect(() => {
    if (boxRef.current) boxRef.current.scrollTop = boxRef.current.scrollHeight;
  }, [count]);

  return (
    <div className="overflow-hidden rounded-lg border border-white/10 shadow-2xl">
      <div className="flex items-center gap-2 border-b border-white/10 bg-[#1b1b22] px-3 py-2">
        <span className="size-3 rounded-full bg-[#ff5f57]" />
        <span className="size-3 rounded-full bg-[#febc2e]" />
        <span className="size-3 rounded-full bg-[#28c840]" />
        <span className="ml-2 text-xs text-white/50">dtoken — event ledger — tail -f ~/.dtoken/events.log</span>
      </div>
      <div ref={boxRef} className="h-[420px] overflow-y-auto bg-[#0d0d12] p-3 font-mono text-[12px] leading-relaxed">
        <div className="text-white/40">$ dtoken logs --follow --redacted</div>
        {lines.slice(0, count).map((l) => (
          <div key={l.id} className="flex gap-2 whitespace-pre-wrap">
            <span className="text-white/35">{l.time}</span>
            <span className={`${LEVEL_COLOR[l.type]} w-[72px] shrink-0`}>[{LEVEL_TAG[l.type]}]</span>
            <span className="text-white/45">{l.agent}</span>
            <span className="text-white/85">{lang === "zh" ? l.msg : l.msgEn}</span>
          </div>
        ))}
        <div className="flex items-center gap-1 text-white/70">
          <span className="text-emerald-400">dtoken@local</span>
          <span className="text-white/40">:~$</span>
          <span className="inline-block h-3.5 w-2 animate-pulse bg-white/70" />
        </div>
      </div>
    </div>
  );
}
