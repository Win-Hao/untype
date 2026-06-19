<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { ClipboardCopy, TriangleAlert } from "lucide-svelte";
  import { fade } from "svelte/transition";

  // 胶囊状态：录音中 / 处理中(thinking) / 失败提示态（已输出原文 / 已复制）。
  // 成功不再有「完成」态——文字注入后 Rust 直接收起（见 pipeline.rs）；失败态由 capsule-result 触发、停留再收起。
  type Phase = "recording" | "polishing" | "raw" | "clipboard";
  let phase = $state<Phase>("recording");

  // 出现/消失：窗口显隐仍由 Rust 控制，浮现动画在前端做。
  // Rust show 后 emit `capsule-show`(entered=true：原地放大+淡入)；hide 前 emit `capsule-hide`
  // (entered=false：缩小+淡出)，Rust 等这段出场动画结束才真正 hide 窗口（见 lib.rs hide_capsule）。
  let entered = $state(false);
  // 入场时若要把残留态（如上一轮的 thinking）瞬切回录音态：置 true 让这一次内容切换跳过 crossfade，
  // 否则快速重录时入场动画会闪出正在淡出的 thinking。下一拍即恢复正常过渡。
  let instantSwap = $state(false);
  // 每次 capsule-show 自增，并入 {#key} → 强制重建内容块。用于清掉「成功出场时 thinking 的 out:fade
  // 被 cap.hide() 冻结、残留在 DOM」那一块：下次显示无条件瞬切成全新波形，不让冻结的 thinking 露出。
  let showNonce = $state(0);

  // 宽度平滑伸缩（灵动岛式）：内容绝对居中、不撑外框；测内容裸宽 → 外框宽 = 裸宽 + 左右内距。
  // 三态切换时外框两侧对称伸缩、中心不动；overflow:hidden 让新文字从中间向两侧展开露出。
  const PAD_X = 16; // = 原 px-4
  let contentW = $state(0);
  let boxW = $state(0);
  // contentW>0 守卫：{#key} 重建内容的那一帧 clientWidth 可能短暂为 0，挡住、别让外框塌成 0。
  $effect(() => {
    if (contentW > 0) boxW = contentW + PAD_X * 2;
  });
  // 用 action 在挂载时「同步」测裸宽（读 offsetWidth 触发 reflow 拿即时值），取代 bind:clientWidth 的
  // ResizeObserver 异步回调——后者晚一两帧，会让态切换时「内容已变、外框却迟一拍才伸缩」，产生顿挫。
  function measureContent(node: HTMLElement) {
    const measure = () => (contentW = node.offsetWidth);
    measure(); // 挂载即同步测量 → 外框与内容同帧变化
    const ro = new ResizeObserver(measure); // 兜底：内容自身尺寸变化
    ro.observe(node);
    return { destroy: () => ro.disconnect() };
  }

  // 录音波形＝真·频谱均衡器：后端(viz.rs)每块音频做小 FFT、按对数人声频段发来 N_BANDS 个 0..1 能量值；
  // 这里镜像成中心对称的条——中心=最低频段(人声基频/能量最足→最高)，越往两边频率越高(齿音时两端窜)。
  const N_BANDS = 11; // 必须与 src-tauri/src/viz.rs 的 N_BANDS 一致
  const WAVE_BARS = N_BANDS * 2 - 1; // 21：镜像后条数
  let bands = $state<number[]>(Array(N_BANDS).fill(0)); // 平滑后各频段能量 0..1
  const specBars = $derived(
    Array.from({ length: WAVE_BARS }, (_, i) => bands[Math.abs(i - (WAVE_BARS - 1) / 2)] ?? 0),
  );

  // 失败提示态给发丝边一点琥珀提示；其余（录音/思考）保持中性白边。
  // 作为 --ring 注入，驱动 .capsule 的 border-color（随状态过渡）。
  const ringColor = $derived(
    phase === "raw" || phase === "clipboard" ? "rgba(251,191,36,0.45)" : "rgba(255,255,255,0.12)",
  );

  // 以「全新波形」瞬切呈现录音态：清波形 + 自增 nonce 强制 {#key} 重建内容块 + instantSwap 跳过过渡。
  // capsule-show 和 capsule-reset 共用——关键是 reset 也走瞬切：成功出场后 reset 把 thinking→波形 的
  // crossfade 留成「正在离场」的残块，会被隐藏窗口冻结、下次显示时 resume 露出（即「重录闪 thinking」真因）。
  // 瞬切(过渡时长 0)让 thinking 块立即从 DOM 移除，根本不产生可被冻结的离场块。
  function showRecording() {
    bands = Array(N_BANDS).fill(0);
    instantSwap = true;
    phase = "recording";
    showNonce += 1;
    tick().then(() => (instantSwap = false));
  }

  onMount(() => {
    const subs = [
      listen("capsule-show", () => {
        showRecording(); // 无条件以全新波形出现（清掉任何残留，含 reset 跳过时仍是 thinking 的情况）
        entered = true;
      }),
      listen("capsule-hide", () => (entered = false)),
      listen<string>("recording-state", (e) => {
        if (e.payload === "recording") {
          phase = "recording";
          bands = Array(N_BANDS).fill(0); // 新一轮录音清零
        }
      }),
      listen<number[]>("capsule-level", (e) => {
        // 后端发来 N_BANDS 个 0..1 频段能量；每段各做「快起慢落」EMA——更跟手又不抖。
        const incoming = e.payload;
        bands = bands.map((cur, i) => {
          const t = incoming[i] ?? 0;
          return cur + (t - cur) * (t > cur ? 0.6 : 0.28); // 快起(0.6) / 慢落(0.28)
        });
      }),
      listen("polishing", () => (phase = "polishing")),
      listen<string>("capsule-result", (e) => {
        // 仅失败态会发来：注入失败(clipboard) / 润色失败(raw)，直接切到提示态
        phase = e.payload === "clipboard" ? "clipboard" : "raw";
      }),
      // 隐藏后归位：瞬切复位录音态（不留会被隐藏窗口冻结的 thinking→波形 离场残块），entered 复位。
      listen("capsule-reset", () => {
        showRecording();
        entered = false;
      }),
    ];
    return () => subs.forEach((s) => s.then((f) => f()));
  });
</script>

<svelte:head>
  <style>
    html,
    body {
      background: transparent !important;
      margin: 0;
      overflow: hidden;
    }
  </style>
</svelte:head>

<div class="flex h-screen w-screen items-center justify-center">
  <!-- stage：出现/消失浮现层（opacity+缩放+上浮），与内层宽度 morph 解耦、互不干扰 -->
  <div class="stage" class:entered>
    <!-- capsule：宽高显式控制；overflow 裁切实现三态切换时两侧对称伸缩 -->
    <div
      class="capsule rounded-full bg-neutral-900/90 text-sm font-medium text-neutral-100 select-none"
      style="width: {boxW}px; --ring: {ringColor}"
    >
      <!-- thinking 流光：扫过整枚胶囊；放在 {#key} 外，不随内容重建被撕掉 -->
      <div class="sheen" class:on={phase === "polishing"}></div>
      {#key `${phase}·${showNonce}`}
        <!-- content：居中、grid 叠放、nowrap；use:measureContent 同步测裸宽驱动外框。
             录音/思考两态加 equiwidth 等宽，消除「录音→thinking」的外框抖动。
             in/out fade 同时进行 → 旧内容渐隐、新内容渐显的 crossfade（不再硬切）。 -->
        <div
          class="content"
          class:equiwidth={phase === "recording" || phase === "polishing"}
          use:measureContent
          in:fade={{ duration: instantSwap ? 0 : 150 }}
          out:fade={{ duration: instantSwap ? 0 : 150 }}
        >
          {#if phase === "polishing"}
            <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
            <span>thinking…</span>
          {:else if phase === "raw"}
            <TriangleAlert size={14} class="text-amber-400" />
            <span>已输出原文</span>
          {:else if phase === "clipboard"}
            <ClipboardCopy size={14} class="text-amber-400" />
            <span>已复制 · ⌘V 粘贴</span>
          {:else}
            <span class="wave">
              {#each specBars as v, i (i)}
                <span
                  class="bar"
                  style="height: {2 + v * 22}px; opacity: {0.5 + 0.5 * v}; box-shadow: 0 0 {1 +
                    v * 7}px rgba(255,255,255,{0.15 + v * 0.5})"
                ></span>
              {/each}
            </span>
          {/if}
        </div>
      {/key}
    </div>
  </div>
</div>

<style>
  /* 出现/消失：原地中心放大 + 淡入（无位移）；出场反向缩小淡出。
     静止态 transform:none 是关键——常驻 transform（哪怕 scale(1)）会把文字推进 GPU 合成层、
     丢失亚像素抗锯齿而发虚；动画结束回到 none 才清晰。故也不用 will-change。 */
  .stage {
    opacity: 0;
    transform: scale(0.9);
    transition:
      opacity 190ms cubic-bezier(0.22, 1, 0.36, 1),
      transform 190ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  .stage.entered {
    opacity: 1;
    transform: none;
  }
  /* 宽度平滑伸缩 + ring 色过渡。grid 单格 + place-items:center：各态内容叠放在同一格、居中，
     切换时新旧内容重叠做 crossfade（波纹渐隐 / 文字渐显）。不用 absolute+translate 居中——半像素定位会让文字/波纹发糊。 */
  .capsule {
    position: relative; /* 锚定 .sheen 绝对定位 */
    display: grid;
    place-items: center;
    height: 36px;
    overflow: hidden;
    /* 玻璃质感：发丝边（吊环色随状态）+ 分层柔和投影 + 顶部内高光 + 底部暗线。
       关键：描边用 box-shadow 而非 border，且不加 backdrop-filter——
       透明窗里 backdrop-filter 裁到圆角不抗锯齿（弧边发毛刺），real border 又是硬边（看着像“边框”）；
       box-shadow 的环会沿圆角平滑抗锯齿，而 90% 不透明的暗胶囊本就几乎透不出背景，去掉模糊无损观感。 */
    box-shadow:
      0 0 0 1px var(--ring, rgba(255, 255, 255, 0.12)),
      0 12px 30px -10px rgba(0, 0, 0, 0.55),
      0 4px 12px -4px rgba(0, 0, 0, 0.4),
      inset 0 1px 0 rgba(255, 255, 255, 0.12),
      inset 0 -1px 0 rgba(0, 0, 0, 0.3);
    /* 宽度形变带轻微弹性回弹（width 非 transform，不触发文字发糊那个坑） */
    transition:
      width 360ms cubic-bezier(0.34, 1.4, 0.5, 1),
      box-shadow 300ms ease;
  }
  .content {
    grid-area: 1 / 1; /* 各态叠在同一格 → 切换时新旧内容重叠，配合 in/out fade 做 crossfade */
    display: flex;
    align-items: center;
    justify-content: center; /* equiwidth 留白时内容居中 */
    gap: 8px; /* = 原 gap-2 */
    white-space: nowrap;
  }
  /* 录音/思考两态等宽（min-width 取较宽的 thinking ≈87px），消除「录音→thinking」那 4px 的外框抖动；
     内容居中、两侧留白极小几乎无感。已输出原文/已复制等提示态按自身宽度。 */
  .content.equiwidth {
    min-width: 88px;
  }
  .wave {
    display: flex;
    height: 24px;
    align-items: center;
    gap: 1.5px;
  }
  .bar {
    width: 2.5px;
    border-radius: 9999px;
    background: white;
    /* 高度由后端频谱（每条=一个频段）驱动 + 前端每段「快起慢落」EMA；过渡收紧让起伏跟手。
       不再叠「自呼吸」动画——频谱本身就是动态的，静默时各条归底、不再无谓地动。 */
    transition:
      height 110ms ease-out,
      opacity 110ms ease-out,
      box-shadow 110ms ease-out;
  }

  /* thinking 流光：一道柔光扫过整枚胶囊（不确定态指示，非进度条）。
     用「背景渐变 + 扫 background-position」实现，不用绝对定位 + filter 的 ::before：
     背景天然被 border-radius 裁切，绝不会逃出圆角——而带 animation+filter 的子元素在 WebKit/WKWebView
     会被提升为合成层、逃出圆角 overflow 裁剪（这正是之前「扫光跑到胶囊外」的根因）。 */
  .sheen {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    opacity: 0;
    transition: opacity 0.4s ease;
    pointer-events: none;
    background-image: linear-gradient(
      105deg,
      transparent 42%,
      rgba(255, 255, 255, 0.18) 50%,
      rgba(255, 255, 255, 0.06) 54%,
      transparent 62%
    );
    background-size: 220% 100%;
    background-repeat: no-repeat;
    background-position: 150% 0; /* 静止：高光停在右外侧、不可见 */
  }
  /* 动画只挂在 .on 上 → 每次进入 thinking 都从头扫起，出现时机一致（修「时机不固定」） */
  .sheen.on {
    opacity: 1;
    animation: sweep 1.8s cubic-bezier(0.22, 0.61, 0.18, 1) infinite;
  }
  @keyframes sweep {
    from {
      background-position: 150% 0;
    }
    to {
      background-position: -70% 0;
    }
  }

  /* thinking 呼吸点：替代转圈，更柔（仍是不确定态指示，非进度条） */
  .dots {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .dots i {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.78);
    animation: dotbob 1.1s cubic-bezier(0.22, 0.61, 0.18, 1) infinite;
  }
  .dots i:nth-child(2) {
    animation-delay: 0.14s;
  }
  .dots i:nth-child(3) {
    animation-delay: 0.28s;
  }
  @keyframes dotbob {
    0%,
    100% {
      transform: translateY(0);
      opacity: 0.4;
    }
    40% {
      transform: translateY(-3px);
      opacity: 1;
    }
  }
</style>
