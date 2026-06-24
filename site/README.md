# Untype — 落地页（React + Vite）

「Untype」语音转文字 App 的单页落地页，**Atelier Zero** 编辑拼贴视觉语言，组件化重构版。
原先的单文件原型已重构为本目录的组件化工程版本（仓库根目录的 `index.html` 现为 App 主窗入口，与官网无关）。

## 快速开始

```bash
cd site
npm install      # 安装 react / react-dom / gsap / vite
npm run dev      # 本地开发，默认 http://localhost:5173
npm run build    # 产出静态站到 dist/，可直接部署到任意静态托管
npm run preview  # 本地预览构建产物
```

## 技术栈

- **Vite 5** + **React 18**（JSX，无 TypeScript）
- **GSAP + ScrollTrigger**：滚动揭示动画（批处理、`once`、`power3.out`），
  尊重 `prefers-reduced-motion`，并在出错时降级为直接显示，绝不白屏。
- 纯 CSS 设计令牌（`src/index.css`，Atelier Zero 画板原样保留）。

## 目录结构

```
site/
├── index.html                # Vite 入口
├── vite.config.js
├── public/assets/            # 10 张 Seedream 生成的拼贴图（hero/about/...）
└── src/
    ├── main.jsx              # 挂载点
    ├── App.jsx               # 组合所有章节
    ├── index.css             # Atelier Zero 样式表（设计令牌 + 组件样式）
    ├── lib/plates.js         # 内联 SVG 插画生成器 + 品牌标记
    ├── hooks/
    │   ├── useReveals.js     # GSAP ScrollTrigger 滚动揭示（含降级）
    │   └── useHeadroom.js    # 下滑隐藏 / 上滑出现的粘性导航
    └── components/           # 按章节拆分：Hero / About / Capabilities / Labs ...
```

## 设计说明

- **强调色**：珊瑚红 `#ed6f5c`，单一强调色、每屏至多用两次（Atelier Zero 规则）。
- **字体**：Playfair Display（衬线大标题斜体强调）+ Inter Tight / Inter（正文）。
- **图片**：大图为 Seedream（doubao-seedream-5.0）生成的拼贴；流程四步用矢量 SVG 图示。
- **对外链接**：GitHub / 下载 / 更新日志 / 反馈等站外链接统一收敛在 `src/lib/links.js`，
  换仓库或下载入口只改这一处。图片路径统一走 `src/lib/asset.js`（`import.meta.env.BASE_URL`），
  因此整站可在任意 `base`（根域名 / 子路径 / 自定义域名）下正确加载。

## 部署

官网通过 **GitHub Pages + Actions** 发布：推送到 `main` 且改动落在 `site/**` 时，
`.github/workflows/deploy-site.yml` 会自动 `npm ci && npm run build` 并部署到
<https://win-hao.github.io/untype/>。生产构建的 `base` 为 `/untype/`（见 `vite.config.js`，
开发态仍是 `/`）。

也可 `npm run build` 后把 `dist/` 丢到 Vercel / Netlify / Cloudflare Pages 等任意静态托管；
迁到根域名或自定义域名时把生产 `base` 改回 `/` 即可（资源路径已自适应）。
