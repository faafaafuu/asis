// Ноа онлайн грузит все модули разом по <link rel="modulepreload"> в
// app.html. Модуль, забытый в этом списке, снова грузится цепочкой — по
// мобильной сети это секунды на каждое звено. Список сверяется с тем, что
// страница на самом деле импортирует.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));
const html = readFileSync(join(SRC, "app.html"), "utf8");

const attr = (tag, name) => new RegExp(`${name}="([^"]+)"`).exec(tag)?.[1];
const clean = (path) => relative(SRC, path).replaceAll("\\", "/");

/** Всё, что модуль тянет статическими импортами, — по цепочке. */
function graph(entry, seen = new Set()) {
  if (seen.has(entry)) return seen;
  seen.add(entry);
  const text = readFileSync(join(SRC, entry), "utf8");
  for (const match of text.matchAll(/^\s*(?:import|export)\b[^;]*?\bfrom\s*["']([^"']+)["']|^\s*import\s*["']([^"']+)["']/gm)) {
    const spec = match[1] ?? match[2];
    if (!spec.startsWith(".")) continue;
    graph(clean(join(SRC, dirname(entry), spec.split("?")[0])), seen);
  }
  return seen;
}

test("app.html предзагружает ровно те модули, что импортирует страница", () => {
  const scripts = [...html.matchAll(/<script[^>]*type="module"[^>]*>/g)].map(([tag]) => clean(join(SRC, attr(tag, "src"))));
  const preloaded = new Set([...html.matchAll(/<link[^>]*rel="modulepreload"[^>]*>/g)].map(([tag]) => clean(join(SRC, attr(tag, "href")))));
  const needed = new Set(scripts.flatMap((entry) => [...graph(entry)]));

  const missing = [...needed].filter((file) => !preloaded.has(file));
  const extra = [...preloaded].filter((file) => !needed.has(file));
  assert.deepEqual(missing, [], "нет в предзагрузке — добавьте <link rel=\"modulepreload\">");
  assert.deepEqual(extra, [], "предзагружается, но страница это не импортирует");
});
