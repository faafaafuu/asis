// Тесты нормализации термина. Запуск: `npm test` (встроенный раннер Node, без зависимостей).
import { test } from "node:test";
import assert from "node:assert/strict";

import { normalizeTerm } from "./term.js";

test("обычное слово проходит без изменений", () => {
  assert.equal(normalizeTerm("альбедо"), "альбедо");
});

test("схлопывает переносы и повторяющиеся пробелы", () => {
  assert.equal(normalizeTerm("  ледниковый \n\t  щит  "), "ледниковый щит");
});

test("снимает кавычки и пунктуацию по краям", () => {
  assert.equal(normalizeTerm("«альбедо»,"), "альбедо");
  assert.equal(normalizeTerm("(изостазия)"), "изостазия");
  assert.equal(normalizeTerm("...криоконит?"), "криоконит");
});

test("дефис внутри слова остаётся частью термина", () => {
  assert.equal(normalizeTerm("бизнес-логика"), "бизнес-логика");
});

test("длинное выделение режется по границе слова", () => {
  const long =
    "Моделирование таких процессов требует учёта изостазии и обратной связи альбедо";
  const result = normalizeTerm(long);
  assert.ok(result.length <= 60, `длина ${result.length} больше лимита`);
  assert.ok(long.startsWith(result), "обрезка не должна менять текст, только укорачивать");
  assert.ok(!result.endsWith(" "), "хвостовой пробел не нужен");
  assert.ok(!/\S$/.test(result) || !long[result.length]?.match(/\S/), "слово не разорвано");
});

test("многоточие не дописывается: этот текст уходит в модель как термин", () => {
  const result = normalizeTerm("а".repeat(200));
  assert.ok(!result.includes("…"));
  assert.equal(result.length, 60);
});

test("пустое и невалидное выделение не роняют попап", () => {
  assert.equal(normalizeTerm(""), "");
  assert.equal(normalizeTerm("   \n  "), "");
  assert.equal(normalizeTerm(null), "");
  assert.equal(normalizeTerm(undefined), "");
});

test("детерминированность: одно и то же выделение даёт один и тот же ключ", () => {
  // На этом равенстве держится проверка «ответ пришёл для того же термина» в PopupView.
  const raw = "  «Криосфера»  ";
  assert.equal(normalizeTerm(raw), normalizeTerm(raw));
});
