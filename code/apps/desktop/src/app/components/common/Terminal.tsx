import type { RuntimeLogEntry } from "../../lib/runtime/types";
import { useI18n, translateCopy } from "../../lib/i18n";

const LEVEL_COLOR: Record<RuntimeLogEntry["type"], string> = {
  route: "text-sky-400",
  compression: "text-emerald-400",
  error: "text-rose-400",
  provider: "text-violet-400",
};
const LEVEL_TAG: Record<RuntimeLogEntry["type"], string> = {
  route: "ROUTE", compression: "COMPRESS", error: "ERROR", provider: "PROVIDER",
};

export function Terminal({ lines }: { lines: RuntimeLogEntry[] }) {
  const { lang } = useI18n();

  return (
    <div className="overflow-hidden rounded-lg border border-white/10 shadow-2xl">
      <div className="flex items-center gap-2 border-b border-white/10 bg-[#1b1b22] px-3 py-2">
        <span className="size-2 rounded-full bg-white/25" aria-hidden="true" />
        <span className="text-xs text-white/60">
          {translateCopy("term.redactedRuntimeLogs", lang, { v1: lines.length })}
        </span>
      </div>
      <div className="h-[420px] overflow-y-auto bg-[#0d0d12] p-3 font-mono text-[12px] leading-relaxed">
        {lines.length === 0 ? (
          <div className="flex h-full items-center justify-center text-white/45">
            {translateCopy("term.noRuntimeLogRecordsYet", lang)}
          </div>
        ) : lines.map((line) => (
          <div key={line.id} className="flex gap-2 whitespace-pre-wrap">
            <span className="text-white/35">{line.time}</span>
            <span className={`${LEVEL_COLOR[line.type]} w-[72px] shrink-0`}>[{LEVEL_TAG[line.type]}]</span>
            <span className="text-white/45">{line.agent}</span>
            <span className="text-white/85">{lang === "zh" ? line.msg : line.msgEn}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
