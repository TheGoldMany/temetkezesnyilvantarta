import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri expects a fixed port; the dev bridge (lunda-devserver) answers /rpc on 1421.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    proxy: { "/rpc": "http://127.0.0.1:1421" },
    watch: { ignored: ["**/src-tauri/**", "**/target/**", "**/.devdata/**"] },
  },
  build: { target: "es2022", sourcemap: false, chunkSizeWarningLimit: 1500 },
  test: { environment: "jsdom", include: ["src/**/*.test.{ts,tsx}"] },
} as any);
