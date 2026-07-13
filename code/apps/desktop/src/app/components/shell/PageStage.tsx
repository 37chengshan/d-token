/*
 * Adapted for d-token from AiMaMi's PageStage at
 * commit 297c7af56f10fb371b77bc9b6b65aa320afcbe7e (Apache-2.0).
 * Modified: AnimatePresence lifecycle, d-token spacing and explicit reduced-motion path.
 */
import type { ReactNode } from "react";
import { motion } from "motion/react";

export function PageStage({ children, reducedMotion }: { children: ReactNode; reducedMotion: boolean }) {
  return (
    <motion.div
      className="shell-page-stage"
      data-page-motion="enter"
      data-reduced-motion={reducedMotion ? "true" : "false"}
      initial={reducedMotion ? false : { opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      exit={reducedMotion ? undefined : { opacity: 0, y: -2 }}
      transition={
        reducedMotion
          ? { duration: 0 }
          : { opacity: { duration: 0.18 }, y: { duration: 0.18 }, ease: [0.22, 0.84, 0.32, 1] }
      }
    >
      {children}
    </motion.div>
  );
}
