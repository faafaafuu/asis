// Проверка фронтенда без браузера: синтаксис модулей, наличие дизайн-токенов из
// SPEC §11 и целостность ссылок в HTML. Запуск: `npm run check`.
//
// Это не замена визуальной сверке с `AI Popup.dc.html`, а страховка от опечаток:
// попап — единственная видимая часть продукта, и «файл не найден» здесь стоит дорого.
import { readFile, readdir, access } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const run = promisify(execFile);
const SRC = fileURLToPath(new URL("../src/", import.meta.url));

let failures = 0;
const fail = (msg) => {
  failures++;
  console.error(`  ✗ ${msg}`);
};
const ok = (msg) => console.log(`  ✓ ${msg}`);

async function walk(dir) {
  const out = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...(await walk(path)));
    else out.push(path);
  }
  return out;
}

const files = await walk(SRC);

// 1. Синтаксис всех JS-модулей.
console.log("Синтаксис JS:");
for (const file of files.filter((f) => f.endsWith(".js"))) {
  try {
    await run(process.execPath, ["--check", file]);
  } catch (err) {
    fail(`${file.replace(SRC, "")}: ${String(err.stderr).split("\n").slice(0, 3).join(" ")}`);
  }
}
if (!failures) ok("все модули разбираются");

// 2. Дизайн-токены из SPEC §11 — значения должны присутствовать буквально.
console.log("Дизайн-токены (SPEC §11):");
const tokens = await readFile(join(SRC, "styles/tokens.css"), "utf8");
const REQUIRED = [
  ["радиус окна 10px", "--pop-radius: 10px"],
  ["радиус меню 9px", "--menu-radius: 9px"],
  ["padding 14px", "--pop-pad: 14px"],
  ["ширина 400px", "--pop-width: 400px"],
  ["ширина раскрытая 480px", "--pop-width-expanded: 480px"],
  ["max-height тела 340px", "--pop-body-max-height: 340px"],
  ["зазор 12px", "--pop-gap: 12px"],
  ["screen inset 12px", "--screen-inset: 12px"],
  ["анимация 0.13s", "--pop-in: 0.13s"],
  ["accent dark", "oklch(0.86 0.09 68)"],
  ["accent light", "oklch(0.48 0.11 48)"],
  ["error dark", "#e2705f"],
  ["error light", "#c2503c"],
  ["фон dark", "rgba(27, 24, 21, 0.87)"],
  ["фон light", "rgba(253, 251, 246, 0.9)"],
  ["фолбэк dark", "#1b1815"],
  ["фолбэк light", "#fdfbf6"],
  ["blur 22px", "blur(22px) saturate(150%)"],
];
for (const [name, needle] of REQUIRED) {
  if (tokens.toLowerCase().includes(needle.toLowerCase())) ok(name);
  else fail(`нет токена: ${name} (${needle})`);
}

// 3. Ссылки в HTML ведут на существующие файлы.
console.log("Ссылки в HTML:");
for (const file of files.filter((f) => f.endsWith(".html"))) {
  const html = await readFile(file, "utf8");
  const refs = [...html.matchAll(/(?:href|src)="(\.[^"]+)"/g)].map((m) => m[1]);
  for (const ref of refs) {
    const target = resolve(dirname(file), ref);
    try {
      await access(target);
    } catch {
      fail(`${file.replace(SRC, "")} → ${ref} не существует`);
    }
  }
}
if (!failures) ok("все ссылки на месте");

// 4. Шрифты, на которые ссылается fonts.css, действительно скачаны.
console.log("Шрифты:");
const fontsCss = await readFile(join(SRC, "styles/fonts.css"), "utf8");
const fontRefs = [...fontsCss.matchAll(/url\('([^']+)'\)/g)].map((m) => m[1]);
for (const ref of fontRefs) {
  try {
    await access(resolve(join(SRC, "styles"), ref));
  } catch {
    fail(`нет файла шрифта ${ref}`);
  }
}
ok(`${fontRefs.length} начертаний на месте`);

// 5. Поле под тень задано в двух местах: константой в JS (её отправляют в Rust,
//    который сдвигает окно) и padding-ом в CSS. Разъедутся — попап встанет со
//    смещением, и заметить это можно будет только глазами на живом рабочем столе.
console.log("Отступ под тень:");
const windowJs = await readFile(join(SRC, "js/popup-window.js"), "utf8");
const windowCss = await readFile(join(SRC, "styles/window.css"), "utf8");
const jsInset = /SHADOW_INSET\s*=\s*(\d+)/.exec(windowJs)?.[1];
const cssInset = /\.window-mount\s*\{[^}]*padding:\s*(\d+)px/.exec(windowCss)?.[1];
if (jsInset && cssInset && jsInset === cssInset) ok(`${jsInset}px в JS и CSS совпадают`);
else fail(`SHADOW_INSET=${jsInset} в JS, padding=${cssInset} в CSS`);

console.log(failures ? `\nПроблем: ${failures}` : "\nВсё в порядке.");
process.exit(failures ? 1 : 0);
