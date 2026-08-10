import test from "node:test";
import assert from "node:assert/strict";

import { releaseNotes, sectionFor } from "./release-notes.mjs";

const CHANGELOG = `# Что менялось

## 0.1.10 — десятая

- строка десятой

## 0.1.1 — первая

- строка первой
- вторая строка

## 0.1.0 — начало

- самое начало
`;

test("берёт раздел нужной версии", () => {
  assert.equal(sectionFor(CHANGELOG, "0.1.1"), "- строка первой\n- вторая строка");
});

test("не путает 0.1.1 с 0.1.10", () => {
  // Подстрочное сравнение вернуло бы здесь раздел десятой версии — она идёт первой.
  assert.equal(sectionFor(CHANGELOG, "0.1.10"), "- строка десятой");
});

test("последний раздел не съедает конец файла", () => {
  assert.equal(sectionFor(CHANGELOG, "0.1.0"), "- самое начало");
});

test("неизвестная версия не ломает описание", () => {
  const notes = releaseNotes(CHANGELOG, "v9.9.9");
  assert.ok(!notes.includes("Что нового"), "раздела нет — и заголовка быть не должно");
  assert.ok(notes.includes("Скачать"), "часть про установку остаётся всегда");
});

test("тег с v и версия без него — одно и то же", () => {
  assert.ok(releaseNotes(CHANGELOG, "v0.1.1").includes("строка первой"));
});
