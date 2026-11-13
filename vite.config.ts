import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 前端构建配置:固定端口,禁用浏览器自动打开与清屏
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: "127.0.0.1",
    watch: {
      // Rust 构建产物不参与前端热更新,避免 EBUSY
      ignored: ["**/src-tauri/**", "**/dist/**", "**/.evolution/**", "**/.autonomous-upgrade/**"],
    },
  },
  build: {
    target: "chrome120",
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
  },
  envPrefix: ["VITE_", "TAURI_"],
});
