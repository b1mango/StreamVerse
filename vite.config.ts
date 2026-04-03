import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  server: {
    host: "127.0.0.1",
    port: 1420
  },
  build: {
    target: "chrome105",
    cssMinify: true,
    rollupOptions: {
      output: {
        manualChunks: {
          tauri: ["@tauri-apps/api"],
        },
      },
    },
  },
  clearScreen: false
});
