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
