import { test } from "node:test";
import assert from "node:assert/strict";
import { NEXT, sectionTitle, splitNext } from "./lesson-agent.js";

test("метка «дальше» снимается и узнаётся", () => {
  assert.deepEqual(splitNext(`Верно, главное понял. Идём дальше. ${NEXT}`), { next: true, text: "Верно, главное понял. Идём дальше." });
  assert.deepEqual(splitNext("Почти, но упустил про память."), { next: false, text: "Почти, но упустил про память." });
});

test("заголовок раздела из «## …»", () => {
  assert.equal(sectionTitle("Вступление\n\n## Слои образа\n\nТекст"), "Слои образа");
  assert.equal(sectionTitle("Без заголовка"), "");
});
