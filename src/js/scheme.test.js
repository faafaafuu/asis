import { test } from "node:test";
import assert from "node:assert/strict";
import { splitScheme, parseScheme, layers } from "./scheme.js";

const block = '```scheme\n{"title":"Docker","nodes":[{"id":"f","label":"Dockerfile"},{"id":"i","label":"Образ"},{"id":"c","label":"Контейнер"}],"edges":[{"from":"f","to":"i","label":"build"},{"from":"i","to":"c","label":"run"}]}\n```';

test("схема отделяется от текста рассказа", () => {
  const { text, scheme } = splitScheme(`Образ собирается из файла. Как понял?\n\n${block}`);
  assert.equal(text, "Образ собирается из файла. Как понял?");
  assert.equal(scheme.title, "Docker");
  assert.equal(scheme.nodes.length, 3);
  assert.deepEqual(scheme.edges.map((e) => e.label), ["build", "run"]);
});

test("без схемы — текст как есть", () => {
  assert.deepEqual(splitScheme("Просто текст."), { text: "Просто текст.", scheme: null });
});

test("кривая схема отбрасывается, лишнее чистится", () => {
  assert.equal(parseScheme("{не json"), null);
  assert.equal(parseScheme({ nodes: [{ id: "a", label: "один" }] }), null, "одного блока мало");
  const s = parseScheme({ nodes: [{ id: "a", label: "A" }, { id: "b", label: "B" }], edges: [{ from: "a", to: "z" }, { from: "a", to: "a" }, { from: "a", to: "b" }] });
  assert.equal(s.edges.length, 1, "стрелки к неизвестным и в себя убраны");
});

test("блоки встают слоями по стрелкам", () => {
  const s = parseScheme(JSON.parse(block.replace(/```scheme|```/g, "")));
  assert.deepEqual(layers(s).map((c) => c.map((n) => n.id)), [["f"], ["i"], ["c"]]);
  const cycle = parseScheme({ nodes: [{ id: "a", label: "A" }, { id: "b", label: "B" }], edges: [{ from: "a", to: "b" }, { from: "b", to: "a" }] });
  assert.ok(layers(cycle).flat().length === 2, "цикл не теряет блоки");
});
