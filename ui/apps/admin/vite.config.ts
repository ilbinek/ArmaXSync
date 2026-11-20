import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  base: "/admin/",
  plugins: [svelte()],
  // The statistics chunk (ECharts) is loaded on demand.
  build: { outDir: "build", emptyOutDir: true, target: "es2021", chunkSizeWarningLimit: 700 },
  server: {
    port: 5173,
    strictPort: true,
    proxy: { "/api": "http://127.0.0.1:8080", "/r": "http://127.0.0.1:8080" },
  },
  clearScreen: false,
});
