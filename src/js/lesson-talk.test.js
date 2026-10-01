import { test } from "node:test";
import assert from "node:assert/strict";
import { gradeOf, talkOrder } from "./lesson-talk.js";

test("оценка по первому слову ответа модели", () => {
  assert.equal(gradeOf("Верно. Всё по делу."), "good");
  assert.equal(gradeOf("Почти — забыли про трафик."), "hard");
  assert.equal(gradeOf("Неверно: это про перезапуск."), "again");
  assert.equal(gradeOf("  верно"), "good");
  assert.equal(gradeOf("Сложно сказать"), null);
  assert.equal(gradeOf(""), null);
});

test("сначала новые и слабые понятия, выученные — в конце", () => {
  const order = talkOrder([
    { id: "a", level: "mature" },
    { id: "b", level: "new" },
    { id: "c", level: "young" },
    { id: "d", level: "learning" },
  ]).map((c) => c.id);
  assert.deepEqual(order, ["b", "d", "c", "a"]);
});

import { phrases } from "./lesson-voice.js";

test("объяснение делится на фразы без разметки", () => {
  assert.deepEqual(phrases("**Порт** — номер программы. Как квартира в доме! А IP — сам дом…"), [
    "Порт — номер программы.",
    "Как квартира в доме!",
    "А IP — сам дом…",
  ]);
  assert.deepEqual(phrases("- пункт один\n- пункт два"), ["пункт один пункт два"]);
  assert.deepEqual(phrases(""), []);
});

import { chunks } from "./lesson-voice.js";

test("фразы склеиваются в куски для речи, не длиннее предела", () => {
  assert.deepEqual(chunks(["Раз.", "Два.", "Три."], 9), ["Раз. Два.", "Три."]);
  assert.deepEqual(chunks(["Очень длинная фраза."], 5), ["Очень длинная фраза."]);
  assert.deepEqual(chunks([]), []);
});
