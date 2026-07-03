import { useState } from "react";
import { motion, AnimatePresence } from "motion/react";
import { ChevronUp } from "lucide-react";
import { useT } from "../../lib/i18n";
import { service } from "../../lib/fixtures";
import { StatusDot } from "../common/StatusBadge";

export function StatusBar() {
  const t = useT();
  const [open, setOpen] = useState(false);

  const Item = ({ label, value, health }: { label: string; value: string; health?: "healthy" | "warning" | "error" }) => (
    <div className="flex items-center gap-2">
      {health && <StatusDot status={health} />}
      <span className="text-muted-foreground">{label}:</span>
      <span>{value}</span>
    </div>
  );

  return (
    <div className="glass-light border-t border-border/40 px-4 py-2 text-sm">
      <div className="glass-backdrop" />
      <div className="flex flex-wrap items-center gap-x-6 gap-y-1">
        <Item label={t("status.runtime")} value={service.running ? t("status.running") : t("status.stopped")} health={service.running ? "healthy" : "error"} />
        <Item label={t("status.route")} value={service.activeRoute} />
        <Item label={t("status.ports")} value={service.ports.join(", ")} />
        <Item label={t("status.mode")} value={service.mode} />
        <Item label={t("status.doctor")} value={service.doctor === "healthy" ? t("common.healthy") : service.doctor === "warning" ? t("common.warning") : t("common.error")} health={service.doctor} />
        <button onClick={() => setOpen((v) => !v)} className="ml-auto flex items-center gap-1 text-muted-foreground hover:text-foreground">
          {t("status.details")}
          <motion.span animate={{ rotate: open ? 0 : 180 }}><ChevronUp className="size-4" /></motion.span>
        </button>
      </div>
      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            className="overflow-hidden"
          >
            <div className="mt-2 grid grid-cols-2 gap-2 border-t border-[var(--glass-inset-border)] pt-2 text-muted-foreground md:grid-cols-4">
              <span>bind: {service.bindHost}</span>
              <span>version: {service.version}</span>
              <span>proxy: :{service.port}</span>
              <span>fallback: 无静默回退</span>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
