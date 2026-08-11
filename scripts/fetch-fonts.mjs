// Скачивает шрифты дизайн-референса в src/assets/fonts и генерирует
// src/styles/fonts.css с локальными @font-face.
//
// Наборов два, по темам:
//   • Light/Dark — IBM Plex Sans + IBM Plex Mono + Instrument Serif;
//   • Neon/Synthwave — Rajdhani + JetBrains Mono.
//
// Exo 2 нужен из-за Rajdhani: кириллицы у того нет вовсе, только латиница.
// Без пары русские заголовки в неоновых темах падали бы на случайный系ый шрифт
// и ломали вид. Exo 2 того же техно-склада, в стопке идёт следом за Rajdhani —
// браузер сам берёт из него те буквы, которых в первом не нашлось.
//
// Зачем: попап — системное окно, которое обязано появляться мгновенно и работать без
// сети (CSP приложения запрещает внешние запросы). Тянуть Google Fonts во время показа
// нельзя, иначе первый кадр будет в фолбэк-шрифте.
//
// Запускать вручную при обновлении набора шрифтов: `node scripts/fetch-fonts.mjs`.
// Результат коммитится в репозиторий.
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const CSS_URL =
  "https://fonts.googleapis.com/css2?family=Instrument+Serif:ital@0;1" +
  "&family=IBM+Plex+Sans:wght@400;500;600" +
  "&family=IBM+Plex+Mono:wght@400;500" +
  "&family=Rajdhani:wght@600;700" +
  "&family=Exo+2:wght@600;700" +
  "&family=JetBrains+Mono:wght@400;500" +
  "&display=swap";

// UA современного браузера — иначе Google отдаёт ttf вместо woff2.
const UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";

// Берём только те подмножества, которые реально нужны интерфейсу на русском:
// базовую латиницу, кириллицу и кириллицу-ext (в ней, например, «ѣ» и украинские знаки).
const KEEP = {
  "U+0000-00FF": "latin",
  "U+0301,U+0400-045F": "cyrillic",
  "U+0460-052F": "cyrillic-ext",
};

const FONT_DIR = fileURLToPath(new URL("../src/assets/fonts/", import.meta.url));
const CSS_OUT = fileURLToPath(new URL("../src/styles/fonts.css", import.meta.url));

const css = await (await fetch(CSS_URL, { headers: { "user-agent": UA } })).text();

const faces = [];
for (const block of css.split("@font-face").slice(1)) {
  const family = /font-family:\s*'([^']+)'/.exec(block)?.[1];
  const style = /font-style:\s*(\w+)/.exec(block)?.[1] ?? "normal";
  const weight = /font-weight:\s*(\d+)/.exec(block)?.[1] ?? "400";
  const url = /url\((https:[^)]+\.woff2)\)/.exec(block)?.[1];
  const range = /unicode-range:\s*([^;]+);/.exec(block)?.[1]?.trim();
  if (!family || !url || !range) continue;
  const compact = range.replace(/\s+/g, "");
  const subset = Object.entries(KEEP).find(([prefix]) => compact.startsWith(prefix))?.[1];
  if (!subset) continue;
  const slug = `${family.toLowerCase().replace(/\s+/g, "-")}-${weight}-${style}-${subset}`;
  faces.push({ family, style, weight, url, range, file: `${slug}.woff2` });
}

await mkdir(FONT_DIR, { recursive: true });
await mkdir(fileURLToPath(new URL("../src/styles/", import.meta.url)), { recursive: true });
for (const f of faces) {
  const bytes = new Uint8Array(await (await fetch(f.url, { headers: { "user-agent": UA } })).arrayBuffer());
  await writeFile(FONT_DIR + f.file, bytes);
  console.log(`${f.file} — ${(bytes.length / 1024).toFixed(1)} КБ`);
}

const out = [
  "/* Сгенерировано scripts/fetch-fonts.mjs — не редактировать вручную. */",
  "/* Шрифты дизайн-референса, подключённые локально: попап работает офлайн. */",
  "",
  ...faces.map((f) =>
    [
      "@font-face {",
      `  font-family: '${f.family}';`,
      `  font-style: ${f.style};`,
      `  font-weight: ${f.weight};`,
      "  font-display: swap;",
      `  src: url('../assets/fonts/${f.file}') format('woff2');`,
      `  unicode-range: ${f.range};`,
      "}",
    ].join("\n"),
  ),
  "",
].join("\n");

await writeFile(CSS_OUT, out);
console.log(`\n${faces.length} начертаний → src/styles/fonts.css`);
