import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @see https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // Tauri 需要固定端口，失败时报错而非自动换端口
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: false,
    hmr: {
      protocol: "ws",
      host: "localhost",
      port: 1421,
    },
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },

  // Tauri 使用 Chromium/Safari 现代版本
  build: {
    target: "safari15",
    minify: "esbuild",
    sourcemap: false,
  },
}));
