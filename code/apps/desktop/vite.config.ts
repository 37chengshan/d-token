import { defineConfig } from "vitest/config";
import path from "path";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

function figmaAssetResolver() {
  return {
    name: "figma-asset-resolver",
    resolveId(id: string) {
      if (id.startsWith("figma:asset/")) {
        const filename = id.replace("figma:asset/", "");
        return path.resolve(__dirname, "src/assets", filename);
      }
    },
  };
}

export default defineConfig({
  plugins: [figmaAssetResolver(), react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  assetsInclude: ["**/*.svg", "**/*.csv"],
  build: {
    manifest: true,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          if (id.includes("react-dom") || /node_modules\/react\//.test(id)) return "react-vendor";
          if (id.includes("clsx") || id.includes("tailwind-merge")) return "ui-utils-vendor";
          if (id.includes("recharts") || id.includes("d3-") || id.includes("victory-vendor")) return "charts-vendor";
          if (id.includes("@tauri-apps")) return "tauri-vendor";
          return undefined;
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts?(x)"],
    restoreMocks: true,
  },
  server: {
    host: "127.0.0.1",
    port: 5180,
    strictPort: true,
  },
});
