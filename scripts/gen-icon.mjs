/**
 * 应用图标生成器（零依赖，Node 内置 zlib 手写 PNG/ICO/ICNS）
 * 设计：应用主色渐变蓝圆角方底 (#3B82F6→#163E8C) + 白色双八分音符 ♫
 * 用法：node scripts/gen-icon.mjs   （输出覆盖 src-tauri/icons/ 下的图标）
 */
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "src-tauri", "icons");
mkdirSync(outDir, { recursive: true });

// ---------- 几何（归一化坐标 [0,1]²） ----------

const BG_TOP = [0x3b, 0x82, 0xf6]; // #3B82F6
const BG_BOTTOM = [0x16, 0x3e, 0x8c]; // #163E8C
const WHITE = [0xff, 0xff, 0xff];

/** 圆角方底片：中心 0.5,0.5，半边长 0.47，圆角 0.17 */
function inBackground(x, y) {
  const half = 0.47,
    r = 0.17;
  const dx = Math.abs(x - 0.5) - (half - r);
  const dy = Math.abs(y - 0.5) - (half - r);
  const ax = Math.max(dx, 0),
    ay = Math.max(dy, 0);
  return Math.hypot(ax, ay) + Math.min(Math.max(dx, dy), 0) - r <= 0;
}

/** 双八分音符 ♫：两根符干 + 上斜符杠 + 两个符头 */
function inGlyph(x, y) {
  // 符干
  const leftStem = x >= 0.335 && x <= 0.38 && y >= 0.3 && y <= 0.705;
  const rightStem = x >= 0.62 && x <= 0.665 && y >= 0.235 && y <= 0.67;
  // 符杠（左低右略高，厚 0.118）
  const beam =
    x >= 0.33 && x <= 0.67 && y >= beamTop(x) && y <= beamTop(x) + 0.118;
  // 符头（椭圆）
  const headL = ellipse(x, y, 0.3575, 0.702, 0.116, 0.086);
  const headR = ellipse(x, y, 0.6425, 0.667, 0.116, 0.086);
  return leftStem || rightStem || beam || headL || headR;
}

function beamTop(x) {
  return 0.31 - (0.19697 * (x - 0.335)) / 1;
}
function ellipse(x, y, cx, cy, rx, ry) {
  const nx = (x - cx) / rx,
    ny = (y - cy) / ry;
  return nx * nx + ny * ny <= 1;
}

// ---------- 渲染（4×4 超采样抗锯齿） ----------

function render(size) {
  const SS = 4;
  const rgba = Buffer.alloc(size * size * 4);
  for (let py = 0; py < size; py++) {
    for (let px = 0; px < size; px++) {
      let bgN = 0,
        glyphN = 0;
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const x = (px + (sx + 0.5) / SS) / size;
          const y = (py + (sy + 0.5) / SS) / size;
          if (inBackground(x, y)) {
            bgN++;
            if (inGlyph(x, y)) glyphN++;
          }
        }
      }
      const total = SS * SS;
      const i = (py * size + px) * 4;
      if (bgN === 0) {
        rgba[i + 3] = 0;
        continue;
      }
      // 背景垂直渐变
      const t = Math.min(Math.max((py + 0.5) / size - 0.06, 0) / 0.88, 1);
      let [r, g, b] = BG_TOP.map((c, k) => Math.round(c + (BG_BOTTOM[k] - c) * t));
      // 白色音符按覆盖率混合
      const a = glyphN / total;
      r = Math.round(r + (WHITE[0] - r) * a);
      g = Math.round(g + (WHITE[1] - g) * a);
      b = Math.round(b + (WHITE[2] - b) * a);
      rgba[i] = r;
      rgba[i + 1] = g;
      rgba[i + 2] = b;
      rgba[i + 3] = Math.round((bgN / total) * 255);
    }
  }
  return rgba;
}

// ---------- PNG 封装 ----------

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
function encodePNG(size, rgba) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  const raw = Buffer.alloc((size * 4 + 1) * size);
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0; // filter: none
    rgba.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// ---------- ICO 封装（Vista+ 支持 PNG 内嵌） ----------

function encodeICO(entries) {
  // entries: [{size, png}]
  const header = Buffer.alloc(6);
  header.writeUInt16LE(1, 2); // type: icon
  const count = entries.length;
  header.writeUInt16LE(count, 4);
  const dir = Buffer.alloc(16 * count);
  let offset = 6 + 16 * count;
  const blobs = [];
  entries.forEach((e, i) => {
    const base = i * 16;
    dir[base] = e.size >= 256 ? 0 : e.size; // width
    dir[base + 1] = e.size >= 256 ? 0 : e.size; // height
    dir.writeUInt16LE(1, base + 4); // planes
    dir.writeUInt16LE(32, base + 6); // bpp
    dir.writeUInt32LE(e.png.length, base + 8);
    dir.writeUInt32LE(offset, base + 12);
    offset += e.png.length;
    blobs.push(e.png);
  });
  return Buffer.concat([header, dir, ...blobs]);
}

// ---------- ICNS 封装（PNG 数据块） ----------

function encodeICNS(entries) {
  // entries: [{type, png}]  ic07=128 ic08=256 ic09=512
  const chunks = entries.map((e) => {
    const head = Buffer.alloc(8);
    head.write(e.type, "ascii");
    head.writeUInt32BE(8 + e.png.length, 4);
    return Buffer.concat([head, e.png]);
  });
  const total = 8 + chunks.reduce((n, c) => n + c.length, 0);
  const head = Buffer.alloc(8);
  head.write("icns", "ascii");
  head.writeUInt32BE(total, 4);
  return Buffer.concat([head, ...chunks]);
}

// ---------- 生成全部产物 ----------

const png32 = encodePNG(32, render(32));
const png48 = encodePNG(48, render(48));
const png64 = encodePNG(64, render(64));
const png128 = encodePNG(128, render(128));
const png256 = encodePNG(256, render(256));
const png512 = encodePNG(512, render(512));

writeFileSync(join(outDir, "32x32.png"), png32);
writeFileSync(join(outDir, "128x128.png"), png128);
writeFileSync(join(outDir, "128x128@2x.png"), png256);
writeFileSync(join(outDir, "icon.png"), png512);
writeFileSync(
  join(outDir, "icon.ico"),
  encodeICO([
    { size: 16, png: encodePNG(16, render(16)) },
    { size: 24, png: encodePNG(24, render(24)) },
    { size: 32, png: png32 },
    { size: 48, png: png48 },
    { size: 64, png: png64 },
    { size: 128, png: png128 },
    { size: 256, png: png256 },
  ])
);
writeFileSync(
  join(outDir, "icon.icns"),
  encodeICNS([
    { type: "ic07", png: png128 },
    { type: "ic08", png: png256 },
    { type: "ic09", png: png512 },
  ])
);

console.log("icons written to", outDir);
