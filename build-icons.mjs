// 生成 NetVision Pro 应用图标(SVG -> 多尺寸 PNG + ICO)
// 用法: node build-icons.mjs  (需先 npm install 安装 sharp / png-to-ico)
import sharp from "sharp";
import pngToIco from "png-to-ico";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(root, "src-tauri", "icons");
fs.mkdirSync(outDir, { recursive: true });

// 512 仪表盘构图:深色底 + 青紫渐变弧 + 刻度 + 指针
function buildSvg() {
  const ticks = [];
  const cx = 256, cy = 256;
  for (let i = 0; i <= 30; i++) {
    const ang = (-225 + (270 * i) / 30) * (Math.PI / 180);
    const major = i % 5 === 0;
    const r1 = major ? 188 : 196;
    const r2 = 208;
    const x1 = cx + Math.cos(ang) * r1, y1 = cy + Math.sin(ang) * r1;
    const x2 = cx + Math.cos(ang) * r2, y2 = cy + Math.sin(ang) * r2;
    ticks.push(
      `<line x1="${x1.toFixed(1)}" y1="${y1.toFixed(1)}" x2="${x2.toFixed(1)}" y2="${y2.toFixed(1)}" ` +
      `stroke="${major ? "rgba(125,211,252,.85)" : "rgba(125,211,252,.30)"}" stroke-width="${major ? 8 : 5}" stroke-linecap="round"/>`
    );
  }
  const r = 150;
  const C = 2 * Math.PI * r;
  const needleAng = 52 * (Math.PI / 180); // 指向右上,约 78% 位置
  const nx = cx + Math.cos(needleAng) * 118, ny = cy + Math.sin(needleAng) * 118;
  return `<svg width="512" height="512" viewBox="0 0 512 512" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0b1122"/><stop offset="1" stop-color="#04060c"/>
    </linearGradient>
    <linearGradient id="arc" x1="0" y1="1" x2="1" y2="0">
      <stop offset="0" stop-color="#22d3ee"/><stop offset="0.55" stop-color="#38bdf8"/><stop offset="1" stop-color="#a78bfa"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.5" cy="0.5" r="0.5">
      <stop offset="0" stop-color="rgba(56,189,248,0.35)"/><stop offset="1" stop-color="rgba(56,189,248,0)"/>
    </radialGradient>
  </defs>
  <rect width="512" height="512" rx="118" fill="url(#bg)"/>
  <rect x="10" y="10" width="492" height="492" rx="110" fill="none" stroke="rgba(125,211,252,0.16)" stroke-width="5"/>
  <circle cx="256" cy="256" r="210" fill="url(#glow)"/>
  ${ticks.join("\n  ")}
  <circle cx="256" cy="256" r="${r}" fill="none" stroke="rgba(140,170,220,0.16)" stroke-width="40"
    stroke-dasharray="${(C * 0.75).toFixed(1)} ${(C * 0.25).toFixed(1)}" transform="rotate(135 256 256)" stroke-linecap="round"/>
  <circle cx="256" cy="256" r="${r}" fill="none" stroke="url(#arc)" stroke-width="40"
    stroke-dasharray="${(C * 0.75 * 0.78).toFixed(1)} ${C.toFixed(1)}" transform="rotate(135 256 256)" stroke-linecap="round"/>
  <line x1="256" y1="256" x2="${nx.toFixed(1)}" y2="${ny.toFixed(1)}" stroke="#e8f4ff" stroke-width="16" stroke-linecap="round"/>
  <circle cx="256" cy="256" r="26" fill="#0b1122" stroke="#7dd3fc" stroke-width="10"/>
  <circle cx="${nx.toFixed(1)}" cy="${ny.toFixed(1)}" r="12" fill="#a78bfa"/>
</svg>`;
}

const svg = Buffer.from(buildSvg());

async function main() {
  const pngs = [];
  for (const s of [32, 64, 128, 256, 512]) {
    const file = path.join(outDir, s === 512 ? "icon.png" : `${s}x${s}.png`);
    await sharp(svg, { density: 72 * (512 / 512) }).resize(s, s).png().toFile(file);
    if (s <= 256) pngs.push({ input: await sharp(svg).resize(s, s).png().toBuffer() });
    console.log("written", file);
  }
  const ico = await pngToIco(pngs.map((p) => p.input));
  fs.writeFileSync(path.join(outDir, "icon.ico"), ico);
  console.log("written icon.ico");
}
main().catch((e) => { console.error(e); process.exit(1); });
