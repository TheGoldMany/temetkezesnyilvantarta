import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Development: the TEMISZ server (temisz-server) answers /rpc on 1430.
export default defineConfig({
  // production: the app lives under /app/ next to the website (scripts/assemble.mjs)
  base: process.env.VITE_BASE ?? "/",
  plugins: [react(), tailwindcss()],
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
