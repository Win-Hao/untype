// ============================================================
// 全局热键：键名美化 + 浏览器 KeyboardEvent → 后端 token。
// 单键、单修饰键区分左右、组合键；逻辑与原 Svelte 版一致。
// ============================================================

import { IS_MAC } from "./platform";

// macOS 用符号（⌘⌃⌥⇧），Windows 用单词（Win/Ctrl/Alt/Shift）。区分左右的前缀保留 左/右。
const SIDE_SYMBOL_MAC: Record<string, string> = {
  MetaLeft: "左⌘",
  MetaRight: "右⌘",
  ControlLeft: "左⌃",
  ControlRight: "右⌃",
  AltLeft: "左⌥",
  AltRight: "右⌥",
  ShiftLeft: "左⇧",
  ShiftRight: "右⇧",
};
const SIDE_SYMBOL_WIN: Record<string, string> = {
  MetaLeft: "左Win",
  MetaRight: "右Win",
  ControlLeft: "左Ctrl",
  ControlRight: "右Ctrl",
  AltLeft: "左Alt",
  AltRight: "右Alt",
  ShiftLeft: "左Shift",
  ShiftRight: "右Shift",
};
const SIDE_SYMBOL: Record<string, string> = IS_MAC ? SIDE_SYMBOL_MAC : SIDE_SYMBOL_WIN;

const COMBO_MOD_MAC: Record<string, string> = {
  meta: "⌘",
  command: "⌘",
  cmd: "⌘",
  super: "⌘",
  control: "⌃",
  ctrl: "⌃",
  alt: "⌥",
  option: "⌥",
  shift: "⇧",
};
const COMBO_MOD_WIN: Record<string, string> = {
  meta: "Win",
  command: "Win",
  cmd: "Win",
  super: "Win",
  control: "Ctrl",
  ctrl: "Ctrl",
  alt: "Alt",
  option: "Alt",
  shift: "Shift",
};
const COMBO_MOD: Record<string, string> = IS_MAC ? COMBO_MOD_MAC : COMBO_MOD_WIN;

function tokenPretty(t: string): string {
  if (SIDE_SYMBOL[t]) return SIDE_SYMBOL[t];
  const lc = t.toLowerCase();
  if (COMBO_MOD[lc]) return COMBO_MOD[lc];
  if (/^Key[A-Z]$/.test(t)) return t.slice(3);
  if (/^Digit[0-9]$/.test(t)) return t.slice(5);
  return t; // F1–F20 / Space 等原样
}

/** 加速器字符串 → 好看的符号（mac: "AltRight"→"右⌥"、"Alt+Space"→"⌥Space"；
 *  Windows: "AltRight"→"右Alt"、"Control+Shift+KeyK"→"Ctrl+Shift+K"）。
 *  Windows 用单词，组合键以 "+" 连接才不挤成一团；mac 用符号，空串连接更紧凑。 */
export function prettyShortcut(accel: string): string {
  if (!accel) return "未设置";
  return accel.split("+").map(tokenPretty).join(IS_MAC ? "" : "+");
}

/** 浏览器 KeyboardEvent.code → 后端认识的主键 token（仅放行确定能用的） */
export function mainKeyToken(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code;
  if (/^Digit[0-9]$/.test(code)) return code;
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  if (code === "Space") return code;
  return null;
}

export const MOD_KEYS = ["Meta", "Control", "Alt", "Shift"];
