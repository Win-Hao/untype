/* 复刻 mockups/untype-app-icon.html 的图标设计（squircle + 波形→基线→光标 连续笔画 + 单一蓝光标），
   把 OKLCH 转成 sRGB hex/rgba，输出可移植的独立 SVG（浅/深/小尺寸简化标记）。 */
const fs = require("fs");
const path = require("path");
const OUT = __dirname;

/* ---- OKLCH → sRGB ---- */
function oklchToRgb(L, C, h) {
  const hr = (h * Math.PI) / 180;
  const a = C * Math.cos(hr), b = C * Math.sin(hr);
  const l_ = L + 0.3963377774 * a + 0.2158037573 * b;
  const m_ = L - 0.1055613458 * a - 0.0638541728 * b;
  const s_ = L - 0.0894841775 * a - 1.291485548 * b;
  const l = l_ ** 3, m = m_ ** 3, s = s_ ** 3;
  const lr = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
  const lg = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
  const lb = -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s;
  const gam = (c) => {
    c = Math.max(0, Math.min(1, c));
    return c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055;
  };
  return [lr, lg, lb].map((x) => Math.round(gam(x) * 255));
}
const hex = (L, C, h) => "#" + oklchToRgb(L, C, h).map((v) => v.toString(16).padStart(2, "0")).join("");
const rgba = (L, C, h, A) => { const [r, g, b] = oklchToRgb(L, C, h); return `rgba(${r},${g},${b},${A})`; };

/* ---- macOS 超椭圆 squircle ---- */
function squircle(cx, cy, r, n) {
  const steps = 220; let d = "";
  for (let i = 0; i <= steps; i++) {
    const t = (i / steps) * 2 * Math.PI;
    const ct = Math.cos(t), st = Math.sin(t);
    const x = cx + r * Math.sign(ct) * Math.pow(Math.abs(ct), 2 / n);
    const y = cy + r * Math.sign(st) * Math.pow(Math.abs(st), 2 / n);
    d += (i === 0 ? "M" : "L") + x.toFixed(2) + " " + y.toFixed(2) + " ";
  }
  return d + "Z";
}

/* macOS 标准圆角矩形：内容 824×824 居中于 1024 画布，圆角半径 ~184（≈22.4%），
   比超椭圆 n=5 圆润得多，符合 Big Sur 图标网格（Dock 里才像正经 macOS 图标）。 */
function roundedRect(x, y, w, h, r) {
  return (
    `M${x + r} ${y} H${x + w - r} A${r} ${r} 0 0 1 ${x + w} ${y + r} ` +
    `V${y + h - r} A${r} ${r} 0 0 1 ${x + w - r} ${y + h} ` +
    `H${x + r} A${r} ${r} 0 0 1 ${x} ${y + h - r} ` +
    `V${y + r} A${r} ${r} 0 0 1 ${x + r} ${y} Z`
  );
}

/* 波形 → 基线 连续笔画（振幅自左向右递减，收敛为一行文字基线） */
const WAVE =
  "M 226 512 C 256 384, 300 384, 330 512 C 360 640, 404 640, 432 512 " +
  "C 456 424, 494 446, 520 512 C 540 558, 562 530, 582 512 L 742 512";

function iconSVG(variant, compact) {
  const dark = variant === "dark";
  const tileTop = dark ? hex(0.33, 0.012, 255) : hex(0.992, 0.003, 250);
  const tileBot = dark ? hex(0.23, 0.012, 255) : hex(0.955, 0.007, 250);
  const ink = dark ? hex(0.93, 0.006, 255) : hex(0.38, 0.02, 250);
  const accent = dark ? hex(0.70, 0.085, 250) : hex(0.64, 0.072, 250);
  const hair = dark ? rgba(0.50, 0.02, 255, 0.55) : rgba(0.72, 0.012, 250, 0.6);
  const sheen = dark ? "rgba(255,255,255,0.10)" : "rgba(255,255,255,0.80)";
  const sq = roundedRect(100, 100, 824, 824, 184);

  let art;
  if (compact) {
    const bar = (x, h, f) => `<rect x="${x}" y="${512 - h / 2}" width="46" height="${h}" rx="23" fill="${f}"/>`;
    art = bar(360, 150, ink) + bar(440, 250, ink) + bar(520, 150, ink) +
      `<rect x="616" y="372" width="46" height="280" rx="23" fill="${accent}"/>`;
  } else {
    art = `<path d="${WAVE}" fill="none" stroke="${ink}" stroke-width="40" stroke-linecap="round" stroke-linejoin="round"/>` +
      `<rect x="752" y="404" width="42" height="216" rx="21" fill="${accent}"/>`;
  }

  return `<svg viewBox="0 0 1024 1024" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Untype app icon">
  <defs>
    <linearGradient id="tg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="${tileTop}"/><stop offset="1" stop-color="${tileBot}"/>
    </linearGradient>
    <linearGradient id="sh" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="${sheen}"/><stop offset="0.42" stop-color="rgba(255,255,255,0)"/>
    </linearGradient>
    <clipPath id="cp"><path d="${sq}"/></clipPath>
  </defs>
  <path d="${sq}" fill="url(#tg)"/>
  <g clip-path="url(#cp)">
    <rect x="0" y="0" width="1024" height="470" fill="url(#sh)"/>
    ${art}
  </g>
  <path d="${sq}" fill="none" stroke="${hair}" stroke-width="2.5"/>
</svg>`;
}

const files = {
  "untype-light.svg": iconSVG("light", false),
  "untype-dark.svg": iconSVG("dark", false),
  "untype-light-compact.svg": iconSVG("light", true),
  "untype-dark-compact.svg": iconSVG("dark", true),
};
for (const [name, svg] of Object.entries(files)) {
  fs.writeFileSync(path.join(OUT, name), svg);
  console.log("wrote", name);
}
console.log("accent(light)=", hex(0.64, 0.072, 250), " ink(light)=", hex(0.38, 0.02, 250),
  " tileTop=", hex(0.992, 0.003, 250), " accent(dark)=", hex(0.70, 0.085, 250));
