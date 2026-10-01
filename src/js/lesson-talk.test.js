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
