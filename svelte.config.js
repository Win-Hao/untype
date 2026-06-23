// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({
      fallback: "index.html",
    }),
    // 把页面 CSS 内联进 HTML（阈值 > 实际 ~30KB）。否则 SvelteKit 在 <head> 注入
    // <link rel="stylesheet">，那是「渲染阻塞」资源——WebKit 会等它加载完才画第一帧，
    // 拖慢主窗冷启动首帧。内联后无外链阻塞，主窗 UI 首帧更快画出。
    inlineStyleThreshold: 100000,
  },
};

export default config;
