import { motion } from "motion/react";
import { heatmap, months } from "../../lib/fixtures";
import { useT } from "../../lib/i18n";

// Renders a GitHub-style activity heatmap. Cell color intensity comes from
// the analytics chart-accent variable so it follows the user's chosen color.
export function Heatmap() {
  const t = useT();
  const opacities = [0.08, 0.3, 0.55, 0.8, 1];

  // group cells into 12 weeks-ish columns (months) x 7 rows
  const cols = months.map((_, m) => heatmap.filter((c) => c.m === m));

  return (
    <div>
      <div className="mb-2 flex gap-[6px] pl-1 text-xs text-muted-foreground">
        {months.map((mo) => (
          <div key={mo} className="w-[18px] text-center">{mo}</div>
        ))}
      </div>
      <div className="flex gap-[6px]">
        {cols.map((col, ci) => (
          <div key={ci} className="flex flex-col gap-[4px]">
            {col.map((cell, ri) => (
              <motion.div
                key={ri}
                initial={{ scale: 0, opacity: 0 }}
                animate={{ scale: 1, opacity: 1 }}
                transition={{ delay: (ci * 7 + ri) * 0.004 }}
                className="size-[14px] rounded-[3px]"
                style={{
                  background: cell.level === 0 ? "var(--muted)" : "var(--chart-accent)",
                  opacity: cell.level === 0 ? 1 : opacities[cell.level],
                }}
                title={`${months[cell.m]} · level ${cell.level}`}
              />
            ))}
          </div>
        ))}
      </div>
      <div className="mt-3 flex items-center justify-end gap-2 text-xs text-muted-foreground">
        <span>{t("an.less")}</span>
        {opacities.map((o, i) => (
          <span key={i} className="size-[12px] rounded-[3px]" style={{ background: "var(--chart-accent)", opacity: o }} />
        ))}
        <span>{t("an.more")}</span>
      </div>
    </div>
  );
}
