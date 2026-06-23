// ============================================================
// 全局热键：键名美化 + 浏览器 KeyboardEvent → 后端 token。
// 单键、单修饰键区分左右、组合键；逻辑与原 Svelte 版一致。
// ============================================================

const SIDE_SYMBOL: Record<string, string> = {
  MetaLeft: "左⌘",
  MetaRight: "右⌘",
  ControlLeft: "左⌃",
  ControlRight: "右⌃",
  AltLeft: "左⌥",
  AltRight: "右⌥",
  ShiftLeft: "左⇧",
  ShiftRight: "右⇧",
};

const COMBO_MOD: Record<string, string> = {
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

function tokenPretty(t: string): string {
  if (SIDE_SYMBOL[t]) return SIDE_SYMBOL[t];
  const lc = t.toLowerCase();
  if (COMBO_MOD[lc]) return COMBO_MOD[lc];
  if (/^Key[A-Z]$/.test(t)) return t.slice(3);
  if (/^Digit[0-9]$/.test(t)) return t.slice(5);
  return t; // F1–F20 / Space 等原样
}

/** 加速器字符串 → 好看的符号（如 "AltRight"→"右⌥"，"Alt+Space"→"⌥Space"） */
export function prettyShortcut(accel: string): string {
  if (!accel) return "未设置";
  return accel.split("+").map(tokenPretty).join("");
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
