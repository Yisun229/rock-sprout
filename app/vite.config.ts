import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// 产品名只在 tauri.conf.json 里写一次：安装目录名、窗口标题、界面上的名字都从这儿取。
// 这样以后改名只动一处，不会漏掉某个页面里写死的字符串。
const conf = JSON.parse(
  readFileSync(fileURLToPath(new URL("./src-tauri/tauri.conf.json", import.meta.url)), "utf8"),
) as { productName?: string };
const PRODUCT_NAME = conf.productName;
if (!PRODUCT_NAME) {
  throw new Error("tauri.conf.json 里必须有 productName——它是安装目录名、窗口标题、界面名字的唯一来源");
}

// Tauri 会带自己的开发地址，端口固定住，免得前后端对不上。
export default defineConfig({
  plugins: [react()],
  define: {
    __PRODUCT_NAME__: JSON.stringify(PRODUCT_NAME),
  },
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
