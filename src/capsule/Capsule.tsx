import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { ClipboardCopy, TriangleAlert } from "lucide-react";
import { cn } from "@/lib/utils";
import { PASTE_HINT } from "@/lib/platform";
import { useTauriEvent } from "@/lib/useTauriEvent";

// 胶囊状态：录音中 / 处理中(thinking) / 失败提示态（已输出原文 / 已复制）。
// 成功不再有「完成」态——文字注入后 Rust 直接收起；失败态由 capsule-result 触发、停留再收起。
type Phase = "recording" | "polishing" | "raw" | "clipboard";

// 录音波形＝真·频谱均衡器：后端(viz.rs)每块音频做小 FFT、按对数人声频段发来 N_BANDS 个 0..1 能量值；
// 这里镜像成中心对称的条——中心=最低频段(人声基频)，越往两边频率越高(齿音时两端窜)。
const N_BANDS = 11; // 必须与 src-tauri/src/viz.rs 的 N_BANDS 一致
const WAVE_BARS = N_BANDS * 2 - 1; // 21：镜像后条数

// 宽度平滑伸缩（灵动岛式）：外框宽 = 内容裸宽 + 左右内距。
const PAD_X = 16; // = 原 px-4

const zeros = () => Array(N_BANDS).fill(0) as number[];

/**
 * 内容块：挂载时用 useLayoutEffect 同步测裸宽（读 offsetWidth 触发 reflow 拿即时值），
 * 取代异步 ResizeObserver 回调——后者晚一两帧，会让态切换时「内容已变、外框却迟一拍才伸缩」产生顿挫。
 * 每个 phase 是独立的内容块（key=phase），只在挂载时测一次；ResizeObserver 仅兜底。
 */
function ContentBlock({
  equiwidth,
  onMeasure,
  children,
}: {
  equiwidth: boolean;
  onMeasure: (w: number) => void;
  children: React.ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    onMeasure(el.offsetWidth); // 挂载即同步测量 → 外框与内容同帧变化
    const ro = new ResizeObserver(() => {
      if (ref.current) onMeasure(ref.current.offsetWidth);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [onMeasure]);

  return (
    <motion.div
      ref={ref}
      className={cn("content", equiwidth && "equiwidth")}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
    >
      {children}
    </motion.div>
  );
}

export default function Capsule() {
  const [phase, setPhase] = useState<Phase>("recording");
  // 出现/消失：窗口显隐由 Rust 控制，浮现动画在前端做（CSS .stage/.entered，非 framer-motion——见 capsule.css 注释）。
  const [entered, setEntered] = useState(false);
  const [bands, setBands] = useState<number[]>(zeros);
  // 每次 capsule-show/reset 自增 → 作为 AnimatePresence 的 key 整块重挂：
  // 瞬切成全新内容、且不留「上一轮 thinking 的离场块被隐藏窗口冻结、下次显示 resume 露出」的残块
  //（即「重录闪 thinking」真因）。普通 phase 切换（同一 nonce 内）才走 crossfade。
  const [showNonce, setShowNonce] = useState(0);

  const [contentW, setContentW] = useState(0);
  const [boxW, setBoxW] = useState(0);
  // contentW>0 守卫：重挂内容那一帧 offsetWidth 可能短暂为 0，挡住、别让外框塌成 0。
  useEffect(() => {
    if (contentW > 0) setBoxW(contentW + PAD_X * 2);
  }, [contentW]);
  const onMeasure = useCallback((w: number) => setContentW(w), []);

  // 失败提示态给发丝边一点琥珀提示；其余（录音/思考）保持中性白边。作为 --ring 注入驱动 box-shadow。
  const ringColor =
    phase === "raw" || phase === "clipboard"
      ? "rgba(251,191,36,0.45)"
      : "rgba(255,255,255,0.12)";

  const specBars = Array.from(
    { length: WAVE_BARS },
    (_, i) => bands[Math.abs(i - (WAVE_BARS - 1) / 2)] ?? 0,
  );

  // 以「全新波形」瞬切呈现录音态：清波形 + 自增 nonce 强制整块重挂。
  const showRecording = useCallback(() => {
    setBands(zeros());
    setPhase("recording");
    setShowNonce((n) => n + 1);
  }, []);

  useTauriEvent("capsule-show", () => {
    showRecording(); // 无条件以全新波形出现（清掉任何残留）
    setEntered(true);
  });
  useTauriEvent("capsule-hide", () => setEntered(false));
  useTauriEvent<string>("recording-state", (payload) => {
    if (payload === "recording") {
      setPhase("recording");
      setBands(zeros()); // 新一轮录音清零
    }
  });
  useTauriEvent<number[]>("capsule-level", (incoming) => {
    // 后端发来 N_BANDS 个 0..1 频段能量；每段各做「快起慢落」EMA——更跟手又不抖。
    setBands((prev) =>
      prev.map((cur, i) => {
        const t = incoming[i] ?? 0;
        return cur + (t - cur) * (t > cur ? 0.6 : 0.28); // 快起(0.6) / 慢落(0.28)
      }),
    );
  });
  useTauriEvent("polishing", () => setPhase("polishing"));
  useTauriEvent<string>("capsule-result", (payload) => {
    // 仅失败态会发来：注入失败(clipboard) / 润色失败(raw)
    setPhase(payload === "clipboard" ? "clipboard" : "raw");
  });
  useTauriEvent("capsule-reset", () => {
    showRecording();
    setEntered(false);
  });

  const equiwidth = phase === "recording" || phase === "polishing";

  return (
    <div className="flex h-screen w-screen items-center justify-center">
      {/* stage：出现/消失浮现层（opacity+缩放+上浮），与内层宽度 morph 解耦 */}
      <div className={cn("stage", entered && "entered")}>
        {/* capsule：宽度显式控制；overflow 裁切实现三态切换时两侧对称伸缩 */}
        <div
          className="capsule"
          style={{ width: `${boxW}px`, ["--ring" as string]: ringColor } as React.CSSProperties}
        >
          {/* thinking 流光：扫过整枚胶囊；放在内容外，不随内容重挂被撕掉 */}
          <div className={cn("sheen", phase === "polishing" && "on")} />
          {/* key={showNonce} 整块重挂边界：show/reset 硬切（无 exit 残块）；同一 nonce 内 phase 切换走 crossfade */}
          <AnimatePresence key={showNonce} initial={false}>
            <ContentBlock key={phase} equiwidth={equiwidth} onMeasure={onMeasure}>
              {phase === "polishing" ? (
                <>
                  <span className="dots" aria-hidden="true">
                    <i />
                    <i />
                    <i />
                  </span>
                  <span>thinking…</span>
                </>
              ) : phase === "raw" ? (
                <>
                  <TriangleAlert size={14} className="text-amber-400" />
                  <span>已输出原文</span>
                </>
              ) : phase === "clipboard" ? (
                <>
                  <ClipboardCopy size={14} className="text-amber-400" />
                  <span>已复制 · {PASTE_HINT} 粘贴</span>
                </>
              ) : (
                <span className="wave">
                  {specBars.map((v, i) => (
                    <span
                      key={i}
                      className="bar"
                      style={{
                        height: `${2 + v * 22}px`,
                        opacity: 0.5 + 0.5 * v,
                        boxShadow: `0 0 ${1 + v * 7}px rgba(255,255,255,${0.15 + v * 0.5})`,
                      }}
                    />
                  ))}
                </span>
              )}
            </ContentBlock>
          </AnimatePresence>
        </div>
      </div>
    </div>
  );
}
