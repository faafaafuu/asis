// Собирает анимацию работы попапа (docs/demo.png — анимированный APNG) из кадров,
// снятых в headless Chromium на демо-документе.
//
// APNG, а не GIF: он даёт полный цвет без квантования в 256 оттенков (в интерфейсе
// много близких полутонов бежевого и коричневого, на палитре они грязнятся) и умеет
// разную длительность у разных кадров. GitHub отображает его в README как обычную
// картинку.
//
// Запуск: node scripts/make-demo.mjs <каталог-с-кадрами>
// Кадры ожидаются как f1.png…fN.png, длительности заданы в FRAMES.

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { deflateSync, inflateSync } from "node:zlib";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

/** Кадр и сколько миллисекунд он висит. Пауза подобрана под чтение, а не под скорость. */
const FRAMES = [
  { file: "f1.png", delayMs: 1300 }, // Loading: спиннер и скелетон
  { file: "f2.png", delayMs: 1900 }, // Success: определение
  { file: "f3.png", delayMs: 2400 }, // Раскрытие: простыми словами и примеры
  { file: "f4.png", delayMs: 3600 }, // Follow-up: вопрос и ответ в треде
];

/** Обрезка: убираем поля страницы, оставляя попап. */
const CROP = { x: 4, y: 4, width: 520, height: 424 };

const srcDir = process.argv[2];
if (!srcDir) {
  console.error("Укажите каталог с кадрами: node scripts/make-demo.mjs <dir>");
  process.exit(1);
}

/* ── Чтение PNG ─────────────────────────────────────────────────────────── */

function readPng(path) {
  const buf = readFileSync(path);
  let offset = 8;
  let width = 0;
  let height = 0;
  let colorType = 0;
  const idat = [];

  while (offset < buf.length) {
    const length = buf.readUInt32BE(offset);
    const type = buf.toString("ascii", offset + 4, offset + 8);
    if (type === "IHDR") {
      width = buf.readUInt32BE(offset + 8);
      height = buf.readUInt32BE(offset + 12);
      colorType = buf[offset + 17];
    }
    if (type === "IDAT") idat.push(buf.subarray(offset + 8, offset + 8 + length));
    offset += 12 + length;
  }

  const channels = colorType === 6 ? 4 : 3;
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * channels + 1;
  const pixels = Buffer.alloc(width * height * channels);

  // Снятие построчных фильтров PNG.
  for (let y = 0; y < height; y++) {
    const filter = raw[y * stride];
    for (let x = 0; x < width * channels; x++) {
      const value = raw[y * stride + 1 + x];
      const a = x >= channels ? pixels[y * width * channels + x - channels] : 0;
      const b = y > 0 ? pixels[(y - 1) * width * channels + x] : 0;
      const c = x >= channels && y > 0 ? pixels[(y - 1) * width * channels + x - channels] : 0;
      let out;
      switch (filter) {
        case 0:
          out = value;
          break;
        case 1:
          out = value + a;
          break;
        case 2:
          out = value + b;
          break;
        case 3:
          out = value + ((a + b) >> 1);
          break;
        case 4: {
          const p = a + b - c;
          const pa = Math.abs(p - a);
          const pb = Math.abs(p - b);
          const pc = Math.abs(p - c);
          out = value + (pa <= pb && pa <= pc ? a : pb <= pc ? b : c);
          break;
        }
        default:
          throw new Error(`неизвестный фильтр PNG: ${filter}`);
      }
      pixels[y * width * channels + x] = out & 255;
    }
  }

  return { width, height, channels, pixels };
}

function crop(image, box) {
  const { channels } = image;
  const out = Buffer.alloc(box.width * box.height * channels);
  for (let y = 0; y < box.height; y++) {
    const from = ((box.y + y) * image.width + box.x) * channels;
    image.pixels.copy(out, y * box.width * channels, from, from + box.width * channels);
  }
  return { width: box.width, height: box.height, channels, pixels: out };
}

/* ── Запись APNG ────────────────────────────────────────────────────────── */

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

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body) >>> 0);
  return Buffer.concat([length, body, crc]);
}

/** Пиксели → сжатый поток PNG (фильтр 0 на каждой строке). */
function compress(image) {
  const rowBytes = image.width * image.channels;
  const raw = Buffer.alloc(image.height * (rowBytes + 1));
  for (let y = 0; y < image.height; y++) {
    raw[y * (rowBytes + 1)] = 0;
    image.pixels.copy(raw, y * (rowBytes + 1) + 1, y * rowBytes, (y + 1) * rowBytes);
  }
  return deflateSync(raw, { level: 9 });
}

const images = FRAMES.map((frame) => crop(readPng(join(srcDir, frame.file)), CROP));
const { width, height, channels } = images[0];

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(width, 0);
ihdr.writeUInt32BE(height, 4);
ihdr[8] = 8;
ihdr[9] = channels === 4 ? 6 : 2;

// acTL: сколько кадров и сколько раз проигрывать (0 — бесконечно).
const actl = Buffer.alloc(8);
actl.writeUInt32BE(images.length, 0);
actl.writeUInt32BE(0, 4);

function fctl(sequence, frame) {
  const data = Buffer.alloc(26);
  data.writeUInt32BE(sequence, 0);
  data.writeUInt32BE(width, 4);
  data.writeUInt32BE(height, 8);
  data.writeUInt32BE(0, 12); // x_offset
  data.writeUInt32BE(0, 16); // y_offset
  // Длительность — дробь: миллисекунды к 1000.
  data.writeUInt16BE(frame.delayMs, 20);
  data.writeUInt16BE(1000, 22);
  data[24] = 0; // dispose: не очищать
  data[25] = 0; // blend: заменять целиком
  return chunk("fcTL", data);
}

const parts = [
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("acTL", actl),
];

let sequence = 0;
images.forEach((image, index) => {
  parts.push(fctl(sequence++, FRAMES[index]));
  const data = compress(image);
  if (index === 0) {
    // Первый кадр — обычный IDAT: так картинка остаётся валидным PNG
    // для всего, что не понимает анимацию.
    parts.push(chunk("IDAT", data));
  } else {
    const seq = Buffer.alloc(4);
    seq.writeUInt32BE(sequence++, 0);
    parts.push(chunk("fdAT", Buffer.concat([seq, data])));
  }
});

parts.push(chunk("IEND", Buffer.alloc(0)));

const outDir = fileURLToPath(new URL("../docs/", import.meta.url));
mkdirSync(outDir, { recursive: true });
const outPath = join(outDir, "demo.png");
const png = Buffer.concat(parts);
writeFileSync(outPath, png);

console.log(
  `docs/demo.png — ${images.length} кадров, ${width}×${height}, ${(png.length / 1024).toFixed(0)} КБ`,
);
