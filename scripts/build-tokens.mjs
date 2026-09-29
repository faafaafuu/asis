// Собирает src/styles/tokens.css из файлов дизайн-системы в src/styles/noah/.
//
// Источник правды — файлы хендоффа дизайна (core.css и темы) как есть, плюс
// наши общие правила (base.css). Окна и страницы подключают один tokens.css:
// по мобильной связи каждый лишний файл — ещё одно соединение. Порядок важен:
// theme-dark.css задаёт :root — тему по умолчанию, остальные темы идут после
// и перекрывают её по атрибуту data-theme.
//
// Запуск: `node scripts/build-tokens.mjs`. Тест scripts/tokens.test.mjs
// проверяет, что собранный файл не отстал от источников.

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const DIR = fileURLToPath(new URL("../src/styles/", import.meta.url));

export const PARTS = [
  "core.css",
  "theme-dark.css",
  "theme-noah.css",
  "theme-light.css",
  "theme-system.css",
  "theme-neon.css",
  "theme-synthwave.css",
  "base.css",
];

const HEADER = `/* Дизайн-токены NOAH — все темы одним файлом.
   СОБРАНО из src/styles/noah/ скриптом scripts/build-tokens.mjs — руками не править:
   меняйте файлы в noah/ и пересоберите. Тема — атрибут data-theme на <html>:
   noah | system | light | dark | neon | synthwave; шкала — data-scale. */
`;

export function buildTokens() {
  const body = PARTS.map((name) => readFileSync(`${DIR}noah/${name}`, "utf8").replace(/\r\n/g, "\n").trimEnd()).join("\n\n");
  return `${HEADER}\n${body}\n`;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  writeFileSync(`${DIR}tokens.css`, buildTokens());
  console.log("tokens.css собран");
}
