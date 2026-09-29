// tokens.css собирается из файлов дизайн-системы (src/styles/noah/). Правка
// источника без пересборки — и окна показывали бы старые цвета.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { buildTokens } from "./build-tokens.mjs";

test("tokens.css совпадает со сборкой из src/styles/noah/", () => {
  const built = readFileSync(fileURLToPath(new URL("../src/styles/tokens.css", import.meta.url)), "utf8").replace(/\r\n/g, "\n");
  assert.equal(built, buildTokens(), "пересоберите: node scripts/build-tokens.mjs");
});
