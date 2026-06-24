// ============================================================
// 运行平台判定：用于把 macOS 专属的 UI（⌘/⌥ 符号、⌘V/⌘Q、辅助功能权限引导等）
// 在 Windows 上换成对应说法（Ctrl/Alt/Win、Ctrl+V、Alt+F4 …）。
// WebView 里没有原生 OS API，靠 UA 判定即可：macOS WKWebView 含 "Macintosh"，
// Windows WebView2 含 "Windows"。
// ============================================================

const UA = typeof navigator !== "undefined" ? navigator.userAgent : "";

/** 当前是否运行在 macOS 上。 */
export const IS_MAC = /Mac|iPhone|iPad|iPod/.test(UA);

/** 当前是否运行在 Windows 上。 */
export const IS_WINDOWS = /Win/.test(UA);

/** 「粘贴」快捷键说法：macOS ⌘V / 其它 Ctrl+V。 */
export const PASTE_HINT = IS_MAC ? "⌘V" : "Ctrl+V";
