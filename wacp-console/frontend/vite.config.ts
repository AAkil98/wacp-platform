import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { resolve } from "path";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": resolve(__dirname, "src"),
    },
  },
  server: {
    proxy: {
      "/api": {
        target: "http://[::1]:8080",
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: "dist",
    // Sourcemaps are normally off for shipped builds (shrinks dist/, keeps
    // source off the network). Re-enabled inline for E2E coverage collection
    // so Playwright's V8 coverage can be resolved back to TS source via c8's
    // sourcemap consumer. Gate behind an env flag so release builds never
    // emit them.
    sourcemap: process.env.E2E_COVERAGE === "1" ? "inline" : false,
  },
});
