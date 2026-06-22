/* Atelier Zero 内联 SVG 插画生成器（纯函数，确定性输出）。
 * 用于流程四步的矢量图示与品牌标记 —— 大图用真实生成图，小图示用矢量更清晰。
 * 返回 SVG 字符串，组件里用 dangerouslySetInnerHTML 注入。 */

const PAL = {
  paper: '#efe7d2', bone: '#f7f1de', paperDark: '#ddd2b6', stone: '#cdbf9f',
  ink: '#15140f', inkMute: '#5a5448', inkFaint: '#8b8676',
  coral: '#ed6f5c', mustard: '#e9b94a', olive: '#6e7448',
}

/* 品牌标记：迷你收敛波形 + 珊瑚红文字光标 */
export const BRAND_MARK =
  '<svg viewBox="0 0 24 24" fill="none" stroke="' + PAL.ink + '" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">' +
    '<path d="M3 13c1.5-4.5 3-4.5 4.2 0s2.7 3.6 3.8 .8 2.2-2 3 .2"/>' +
    '<line x1="15" y1="13" x2="19" y2="13"/>' +
    '<rect x="19.2" y="9.5" width="2" height="7" rx="1" fill="' + PAL.coral + '" stroke="none"/>' +
  '</svg>'

function folds(W, H) {
  const x1 = Math.round(W * 0.34), x2 = Math.round(W * 0.68)
  return '<line x1="' + x1 + '" y1="0" x2="' + x1 + '" y2="' + H + '" stroke="' + PAL.inkFaint + '" stroke-opacity=".12"/>' +
         '<line x1="' + x2 + '" y1="0" x2="' + x2 + '" y2="' + H + '" stroke="' + PAL.inkFaint + '" stroke-opacity=".10"/>'
}
function crosshair(cx, cy, r = 9) {
  return '<g stroke="' + PAL.inkFaint + '" stroke-width="1" stroke-opacity=".55">' +
    '<line x1="' + (cx - r) + '" y1="' + cy + '" x2="' + (cx + r) + '" y2="' + cy + '"/>' +
    '<line x1="' + cx + '" y1="' + (cy - r) + '" x2="' + cx + '" y2="' + (cy + r) + '"/></g>'
}
function matrix(x, y, cols, rows, gap = 14) {
  let d = ''
  for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) {
    d += '<circle cx="' + (x + c * gap) + '" cy="' + (y + r * gap) + '" r="1.4" fill="' + PAL.inkFaint + '" fill-opacity=".5"/>'
  }
  return d
}
function hairCircle(cx, cy, r, col = PAL.inkFaint, op = '.4') {
  return '<circle cx="' + cx + '" cy="' + cy + '" r="' + r + '" fill="none" stroke="' + col + '" stroke-width="1" stroke-opacity="' + op + '"/>'
}
/* 收敛波形 → 平直基线 → 珊瑚红光标 */
function waveConverge(x0, x1, baseY, amp, col = PAL.ink) {
  const N = 90; let d = ''; const mid = (x0 + x1) * 0.62
  for (let i = 0; i <= N; i++) {
    const x = x0 + (mid - x0) * (i / N)
    const t = i / N
    const a = amp * (1 - t)
    const y = baseY - a * Math.sin(t * Math.PI * 5)
    d += (i === 0 ? 'M' : 'L') + x.toFixed(1) + ' ' + y.toFixed(1) + ' '
  }
  d += 'L' + x1.toFixed(1) + ' ' + baseY.toFixed(1)
  const caretX = x1 + 18, caretH = amp * 0.62
  return '<path d="' + d + '" fill="none" stroke="' + col + '" stroke-width="7" stroke-linecap="round" stroke-linejoin="round"/>' +
    '<rect x="' + caretX + '" y="' + (baseY - caretH / 2) + '" width="9" height="' + caretH + '" rx="4.5" fill="' + PAL.coral + '"/>'
}
function textLines(x, y, w, n, gap = 16, col = PAL.inkMute) {
  let d = ''
  for (let i = 0; i < n; i++) {
    const ww = (i === n - 1) ? w * 0.55 : w * (0.8 + 0.2 * ((i % 3) / 3))
    d += '<rect x="' + x + '" y="' + (y + i * gap) + '" width="' + ww + '" height="5" rx="2.5" fill="' + col + '" fill-opacity=".55"/>'
  }
  return d
}

/* 流程小图示 + 备用拼贴；当前页面只用 m-ring / m-wave / m-lines / m-caret 四种 */
export function buildPlate(kind, ds = {}) {
  const fit = ds.fit === 'slice' ? 'xMidYMid slice' : 'xMidYMid meet'
  const W = 760, H = 760
  const bg = '<rect width="' + W + '" height="' + H + '" fill="' + PAL.bone + '"/>' + folds(W, H)
  let scene = ''

  switch (kind) {
    case 'm-ring':
      scene = hairCircle(W * 0.5, H * 0.5, 150, PAL.ink, '.5') + hairCircle(W * 0.5, H * 0.5, 96, PAL.coral, '.7') +
        '<text x="' + (W * 0.5) + '" y="' + (H * 0.57) + '" text-anchor="middle" font-family="JetBrains Mono, monospace" font-size="60" fill="' + PAL.inkMute + '">⌥</text>'
      break
    case 'm-wave':
      scene = waveConverge(W * 0.16, W * 0.78, H * 0.5, 150, PAL.ink) + hairCircle(W * 0.5, H * 0.5, 150, PAL.inkFaint, '.3')
      break
    case 'm-lines':
      scene = textLines(W * 0.22, H * 0.3, W * 0.56, 6, 30, PAL.inkMute) +
        '<rect x="' + (W * 0.22) + '" y="' + (H * 0.26) + '" width="6" height="' + (H * 0.5) + '" fill="' + PAL.coral + '"/>'
      break
    case 'm-caret':
      scene = textLines(W * 0.22, H * 0.34, W * 0.4, 3, 30, PAL.inkMute) +
        '<rect x="' + (W * 0.66) + '" y="' + (H * 0.34) + '" width="12" height="' + (H * 0.32) + '" rx="6" fill="' + PAL.coral + '"/>' +
        hairCircle(W * 0.7, H * 0.5, 80, PAL.inkFaint, '.3')
      break
    default:
      scene = waveConverge(W * 0.2, W * 0.7, H * 0.5, 90, PAL.ink)
  }

  return '<svg viewBox="0 0 ' + W + ' ' + H + '" preserveAspectRatio="' + fit + '" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Untype 图示">' +
    bg + scene + crosshair(W * 0.84, H * 0.16) + '</svg>'
}
