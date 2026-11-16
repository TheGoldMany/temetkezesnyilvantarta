import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Each build has its own id; the app compares it with /version.json to offer
// a reload when a new version was deployed while it was open.
const BUILD_ID = process.env.VERCEL_GIT_COMMIT_SHA || process.env.GITHUB_SHA || String(Date.now());
const versionFile = {
  name: "temisz-version",
  generateBundle(this: { emitFile: (f: { type: "asset"; fileName: string; source: string }) => void }) {
    this.emitFile({ type: "asset", fileName: "version.json", source: JSON.stringify({ build: BUILD_ID }) });
  },
};

// Development: the TEMISZ server (temisz-server) answers /rpc on 1430.
export default defineConfig({
  // production: the app lives under /app/ next to the website (scripts/assemble.mjs)
  base: process.env.VITE_BASE ?? "/",
  plugins: [react(), tailwindcss(), versionFile],
  define: { __APP_BUILD__: JSON.stringify(BUILD_ID) },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    proxy: { "/rpc": "http://127.0.0.1:1430" },
    watch: { ignored: ["**/target/**"] },
  },
  build: { target: "es2022", sourcemap: false, chunkSizeWarningLimit: 1500 },
  test: { environment: "jsdom", include: ["src/**/*.test.{ts,tsx}"] },
} as any);
