import { test } from "node:test";
import assert from "node:assert/strict";
import { sectionTitle } from "./lesson-agent.js";

test("заголовок раздела из «## …»", () => {
  assert.equal(sectionTitle("Вступление\n\n## Слои образа\n\nТекст"), "Слои образа");
  assert.equal(sectionTitle("Без заголовка"), "");
});
