import { test } from "node:test";
import assert from "node:assert/strict";
import { sectionTitle, lastQuestion } from "./lesson-agent.js";

test("заголовок раздела из «## …»", () => {
  assert.equal(sectionTitle("Вступление\n\n## Слои образа\n\nТекст"), "Слои образа");
  assert.equal(sectionTitle("Без заголовка"), "");
});

test("вопрос Ноа — последний вопрос в реплике", () => {
  assert.equal(lastQuestion("Образ — шаблон. Зачем он нужен? Расскажи, как понял: чем образ отличается от контейнера?"), "Расскажи, как понял: чем образ отличается от контейнера?");
  assert.equal(lastQuestion("Без вопросов."), "");
});
