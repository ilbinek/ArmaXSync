import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  base: "./",
  plugins: [svelte()],
  build: { outDir: "build", emptyOutDir: true, target: "es2021" },
  server: {
    port: 1420,
    strictPort: true,
  },
  clearScreen: false,
});
