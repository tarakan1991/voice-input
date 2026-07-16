import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Порт 1420 зафиксирован в tauri.conf.json (devUrl)
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "safari14",
  },
});
