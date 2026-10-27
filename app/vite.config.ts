import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 会带自己的开发地址，端口固定住，免得前后端对不上。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    // Tauri 用的 WebView2 内核版本较新，不用兼容老浏览器
    target: "chrome105",
  },
});
