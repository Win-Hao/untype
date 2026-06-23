import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vitejs.dev/config/
// 开发态用根路径 '/'；生产构建用 '/untype/'，适配 GitHub Pages 项目站点
// （https://win-hao.github.io/untype/）。所有图片路径已统一走
// import.meta.env.BASE_URL（见 src/lib/asset.js），改这里即可整体迁移到
// 别的子路径；迁到根域名 / 自定义域名时把生产 base 也改回 '/' 即可。
export default defineConfig(({ command }) => ({
  plugins: [react()],
  base: command === 'build' ? '/untype/' : '/',
  // 显式给空的内联 postcss 配置：本站不用 PostCSS/Tailwind。
  // 否则 Vite 会向上目录搜索，误用到仓库根的 postcss.config.js（那是主 App 的 Tailwind 配置，
  // 依赖 tailwindcss，本子项目没装 → 构建报「Cannot find module 'tailwindcss'」）。
  css: { postcss: {} },
}))
