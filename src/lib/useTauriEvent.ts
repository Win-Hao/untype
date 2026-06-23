import { useEffect, useRef } from "react";
import { listen, type Event } from "@tauri-apps/api/event";

// 后端 emit 的事件名集中此处，避免拼写漂移。
export type TauriEventName =
  | "recording-state"
  | "asr-error"
  | "inject-error"
  | "llm-error"
  | "mic-level"
  | "capsule-show"
  | "capsule-hide"
  | "capsule-level"
  | "polishing"
  | "capsule-result"
  | "capsule-reset";

/**
 * 订阅单个 Tauri 事件。handler 用 ref 存最新值，避免因 handler 变化而反复重订阅；
 * 仅在挂载时 listen、卸载时 unlisten（与原 Svelte onMount 的注册/清理一致）。
 */
export function useTauriEvent<T>(
  event: TauriEventName,
  handler: (payload: T, raw: Event<T>) => void,
) {
  const ref = useRef(handler);
  ref.current = handler;

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<T>(event, (e) => ref.current(e.payload, e)).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [event]);
}
