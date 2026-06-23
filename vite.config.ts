import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  resolve: {
    alias: {
      // @ → src（与 shadcn / components.json 约定一致）
      "@": resolve(__dirname, "src"),
    },
  },

  // 静态资源沿用原 SvelteKit 的 static/（favicon 等）
  publicDir: "static",

  // 两个独立窗口 = 两个 HTML 入口（主窗 index.html / 胶囊 capsule.html）。
  // 胶囊性能敏感，独立打包不夹带设置页代码。产物落到 ../build 对齐 tauri.conf frontendDist。
  build: {
    outDir: "build",
    emptyOutDir: true,
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        capsule: resolve(__dirname, "capsule.html"),
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
