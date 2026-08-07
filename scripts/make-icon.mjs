// Генерирует исходную иконку app-icon.png (1024×1024) без графических зависимостей.
// Дальше из неё делаются все форматы: `npm run icon` (обёртка над `tauri icon`).
//
// Знак простой намеренно: тёмная плашка цвета попапа, акцентное кольцо и точка —
// та самая точка-маркер, которая появляется в шапке попапа после раскрытия.
import { writeFileSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { fileURLToPath } from "node:url";

const SIZE = 1024;
const BG = [27, 24, 21]; // #1B1815 — фолбэк-фон попапа
const ACCENT = [232, 178, 108]; // oklch(.86 .09 68) в sRGB

const pixels = Buffer.alloc(SIZE * SIZE * 4);

const center = SIZE / 2;
const ringOuter = SIZE * 0.31;
const ringInner = SIZE * 0.225;
const dotRadius = SIZE * 0.062;
const dotCenterY = center + SIZE * 0.245;
const cornerRadius = SIZE * 0.22;

/** Мягкое покрытие пикселя фигурой: 1 внутри, 0 снаружи, плавно на границе. */
function coverage(distance, edge) {
  return Math.min(1, Math.max(0, edge + 0.5 - distance));
}

for (let y = 0; y < SIZE; y++) {
  for (let x = 0; x < SIZE; x++) {
    const px = x + 0.5;
    const py = y + 0.5;

    // Скруглённый квадрат подложки.
    const dx = Math.max(Math.abs(px - center) - (center - cornerRadius), 0);
    const dy = Math.max(Math.abs(py - center) - (center - cornerRadius), 0);
    const plaque = coverage(Math.hypot(dx, dy), cornerRadius);

    // Кольцо и точка.
    const r = Math.hypot(px - center, py - center);
    const ring = coverage(r, ringOuter) * (1 - coverage(r, ringInner));
    const dot = coverage(Math.hypot(px - center, py - dotCenterY), dotRadius);
    const mark = Math.min(1, ring + dot);

    const i = (y * SIZE + x) * 4;
    for (let c = 0; c < 3; c++) {
      pixels[i + c] = Math.round(BG[c] * (1 - mark) + ACCENT[c] * mark);
    }
    pixels[i + 3] = Math.round(255 * plaque);
  }
}

/* ── Сборка PNG ────────────────────────────────────────────────────────── */

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body) >>> 0);
  return Buffer.concat([length, body, crc]);
}

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

function crc32(buf) {
  let c = 0xffffffff;
  for (const byte of buf) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return c ^ 0xffffffff;
}

// Каждая строка PNG начинается с байта фильтра; 0 — «без фильтра».
const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0;
  pixels.copy(raw, y * (SIZE * 4 + 1) + 1, y * SIZE * 4, (y + 1) * SIZE * 4);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // бит на канал
ihdr[9] = 6; // RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

const out = fileURLToPath(new URL("../app-icon.png", import.meta.url));
writeFileSync(out, png);
console.log(`app-icon.png — ${SIZE}×${SIZE}, ${(png.length / 1024).toFixed(1)} КБ`);
