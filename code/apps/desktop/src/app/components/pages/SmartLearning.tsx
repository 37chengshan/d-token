// 智能学习 UI 容器：扫描按钮、候选目录列表、排序说明、深度扫描、写入结果。
// 关联计划：release-plan-2026-07/phase-03 §10 UI 模块拆解、implementation-kickoff Sprint 3。
import { useMemo, useState } from "react";
import { motion } from "motion/react";
import { Search, Sparkles, FileText, ShieldCheck, RotateCcw, Filter } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { EmptyState } from "../common/EmptyState";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { Input } from "../ui/input";
import { useT } from "../../lib/i18n";
import type { LearningIndexRecord } from "../../lib/learning/types";
import { isTauriRuntime } from "../../lib/native";

export function SmartLearning() {
  const t = useT();
  const nativeRuntime = isTauriRuntime();
  const [scanning, setScanning] = useState(false);
  const [candidates, setCandidates] = useState<LearningIndexRecord[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const filtered = useMemo(
    () => candidates.filter((c) => {
      if (!query) return true;
      const q = query.toLowerCase();
      return c.folderPath.toLowerCase().includes(q) || c.lightIndex.languageFingerprints.some((l) => l.toLowerCase().includes(q));
    }),
    [candidates, query],
  );

  const startScan = () => {
    if (nativeRuntime) return;
    setScanning(true);
    setCandidates([]);
    window.setTimeout(() => {
      setCandidates(buildPreviewCandidates());
      setScanning(false);
    }, 900);
  };

  const selectedRecord = candidates.find((c) => c.folderPath === selected);

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="text-base font-semibold">{t("sl.title")}</h2>
          <p className="mt-1 text-sm text-muted-foreground">{t("sl.subtitle")}</p>
        </div>
        <div className="flex items-center gap-2">
          {scanning ? (
            <Button variant="secondary" size="sm" onClick={() => setScanning(false)}>
              <RotateCcw className="size-3.5" /> {t("sl.stop")}
            </Button>
          ) : (
            <Button size="sm" onClick={startScan} disabled={nativeRuntime}>
              <Search className="size-3.5" />
              {nativeRuntime ? t("sl.nativeUnavailableShort") : t("sl.previewScan")}
            </Button>
          )}
        </div>
      </div>

      <div className={`flex items-center gap-2 rounded-lg border px-3 py-2 text-xs ${nativeRuntime ? "border-amber-500/25 bg-amber-500/8 text-amber-800 dark:text-amber-200" : "border-sky-500/25 bg-sky-500/8 text-sky-800 dark:text-sky-200"}`}>
        <ShieldCheck className="size-3.5 shrink-0" />
        <span>{nativeRuntime ? t("sl.nativeUnavailable") : t("sl.previewNotice")}</span>
      </div>

      {scanning && (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Sparkles className="size-4 animate-pulse" /> {t("sl.scanning")}
        </div>
      )}

      {!scanning && candidates.length === 0 && (
        <EmptyState
          status="empty"
          title={nativeRuntime ? t("sl.nativeUnavailableShort") : t("sl.previewEmpty")}
        />
      )}

      {candidates.length > 0 && (
        <div className="flex items-center gap-2">
          <Filter className="size-4 text-muted-foreground" />
          <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder={t("sl.search")} className="h-8 max-w-xs" />
          <span className="text-xs text-muted-foreground">{t("sl.scanProgress")}: {candidates.length}</span>
        </div>
      )}

      <div className="grid gap-4 lg:grid-cols-2">
        <GlassCard className="p-4" hover={false}>
          <h3 className="mb-2 text-sm font-medium">{t("sl.candidates")}</h3>
          <div className="space-y-2">
            {filtered.map((c) => (
              <motion.button key={c.folderPath} type="button" onClick={() => setSelected(c.folderPath)}
                className={`w-full rounded-lg border p-3 text-left transition-colors ${selected === c.folderPath ? "border-[var(--accent-500)] bg-[var(--accent-500)]/5" : "border-border/50 hover:border-border"}`}
                whileHover={{ y: -1 }}>
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate text-sm font-medium">{c.lightIndex.folderName}</span>
                  <Badge variant="secondary" className="shrink-0">{t("sl.score")}: {(c.score.total * 100).toFixed(0)}</Badge>
                </div>
                <div className="mt-1 truncate text-xs text-muted-foreground">{c.folderPath}</div>
                {c.explanations.length > 0 && (
                  <ul className="mt-1.5 space-y-0.5 text-xs text-muted-foreground">
                    {c.explanations.slice(0, 3).map((e, i) => (<li key={i}>• {e}</li>))}
                  </ul>
                )}
                <div className="mt-1.5 flex flex-wrap gap-1">
                  {c.lightIndex.languageFingerprints.map((l) => (<Badge key={l} variant="outline" className="text-[10px]">{l}</Badge>))}
                  {c.riskFlags.filter((f) => f !== "none").map((f) => (<Badge key={f} variant="outline" className="text-[10px] text-amber-600">{t("sl.risk")}: {f}</Badge>))}
                </div>
              </motion.button>
            ))}
          </div>
        </GlassCard>
        <GlassCard className="p-4" hover={false}>
          <h3 className="mb-2 text-sm font-medium">{t("sl.output")}</h3>
          {selectedRecord ? (
            <div className="space-y-3 text-sm">
              <div><div className="text-xs text-muted-foreground">{t("sl.deepScan")}</div><div className="mt-1 font-medium">{selectedRecord.lightIndex.folderName}</div></div>
              <div><div className="text-xs text-muted-foreground">{t("sl.execute")}</div>
                <div className="mt-1 rounded-md border border-sky-500/20 bg-sky-500/5 p-2 text-xs text-muted-foreground">
                  {t("sl.previewNotice")}
                </div>
              </div>
              <Button size="sm" disabled><FileText className="size-3.5" /> {t("sl.previewReadOnly")}</Button>
            </div>
          ) : (
            <EmptyState
              status="empty"
              title={nativeRuntime ? t("sl.nativeUnavailableShort") : t("sl.previewEmpty")}
            />
          )}
        </GlassCard>
      </div>
    </div>
  );
}

function buildPreviewCandidates(): LearningIndexRecord[] {
  const now = Date.now();
  const mk = (name: string, path: string, daysAgo: number, hits: number, markers: string[], langs: string[], hasDoc: boolean, risk: LearningIndexRecord["riskFlags"]): LearningIndexRecord => {
    const idx = { folderName: name, lastModifiedMs: now - daysAgo * 86400000, fileCountBucket: "medium" as const, hasGit: markers.includes(".git"), projectMarkers: markers, hasAgentDoc: hasDoc, hasAgentSession: hits > 0, languageFingerprints: langs };
    return { folderPath: path, folderHash: `h${name}`, lastIndexedAt: new Date(now).toISOString(), lightIndex: idx, score: { total: 0.7 - daysAgo * 0.01 + hits * 0.02, recency: 0, agentActivity: 0, projectConfidence: 0, docGap: 0, complexity: 0, userPreference: 0, dtokenHistory: 0, riskPenalty: risk[0] !== "none" ? 0.3 : 0 }, explanations: [daysAgo <= 1 ? "最近 1 天内有修改" : `最近 ${daysAgo} 天内有修改`, hits > 0 ? `被 Agent 访问 ${hits} 次` : "", langs.length > 0 ? `包含 ${langs.join(" + ")} 项目结构` : "", !hasDoc ? "缺少 Agent 项目说明文档" : ""].filter(Boolean), riskFlags: risk, deepScanStatus: "not-started" };
  };
  return [
    mk("d-token", "D:/D-TOKEN/d-token", 0, 5, ["package.json","Cargo.toml",".git"], ["TypeScript/JS","Rust"], true, ["none"]),
    mk("headroom", "D:/D-TOKEN/headroom", 2, 3, [".git","pyproject.toml"], ["Python"], false, ["none"]),
    mk("demo-app", "~/projects/demo-app", 12, 1, ["package.json",".git"], ["TypeScript/JS"], false, ["none"]),
  ];
}
