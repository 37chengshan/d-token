import { motion } from "motion/react";

// Single faint accent blob drifting slowly — barely perceptible,
// provides depth for glass surfaces without drawing attention.
// Main color motion comes from the flowing `.app-bg` gradient.
export function BackgroundBlobs() {
  return (
    <div className="pointer-events-none fixed inset-0 z-0 overflow-hidden">
      <motion.div
        className="absolute h-[480px] w-[480px] rounded-full blur-[120px]"
        style={{ background: "var(--accent-500, #f97316)", opacity: 0.08 }}
        initial={{ x: "10%", y: "-5%" }}
        animate={{ x: ["10%", "30%", "15%", "10%"], y: ["-5%", "25%", "50%", "-5%"] }}
        transition={{ duration: 50, repeat: Infinity, ease: "easeInOut" }}
      />
    </div>
  );
}
