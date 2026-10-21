import { test } from "node:test";
import assert from "node:assert/strict";

import { applyPlan, applySection, cardOf, imageUrl, resized, validId, WIDE } from "./guides.mjs";
import { jsonIn } from "../../src/js/web/course-builder.js";

const draft = { id: "g1", title: "деревья москвы", goal: "деревья москвы", emoji: "📖", sections: [], status: "building", created: 1 };

test("план ложится в путеводитель", () => {
  const raw = jsonIn('Вот план:\n```json\n{"title": "Деревья Москвы", "subtitle": "Кто растёт во дворах", "emoji": "🌳", "intro": "Липы и клёны.", "cover": "Липа", "sections": [{"title": "Липы", "emoji": "🍃", "about": "Самое городское дерево"}, {"title": "Клёны", "about": "Осенью красные"}, {"title": ""}]}\n```');
  const guide = applyPlan(draft, raw);
  assert.equal(guide.title, "Деревья Москвы");
  assert.equal(guide.coverHint, "Липа");
  assert.deepEqual(guide.sections.map((s) => [s.id, s.title, s.emoji]), [["s1", "Липы", "🍃"], ["s2", "Клёны", "•"]]);
  assert.equal(applyPlan(draft, { sections: [{ title: "Один" }] }), null);
});

test("раздел — текст, карточки, цифры", () => {
  const section = { id: "s1", title: "Липы", emoji: "🍃", about: "…" };
  const raw = { lead: "Липа", text: ["Абзац", " "], items: [{ name: "Липа мелколистная", text: "Пахнет", tags: ["аллеи", "", "мёд", "x", "y"], image: "Липа сердцевидная" }, { name: "" }], facts: [{ label: "Высота", value: "до 30 м" }, { label: "", value: "x" }], tips: ["Смотрите в июле"], wow: "" };
  const done = applySection(section, raw, false);
  assert.deepEqual(done.text, ["Абзац"]);
  assert.equal(done.items.length, 1);
  assert.deepEqual(done.items[0].tags, ["аллеи", "мёд", "x"]);
  assert.equal(done.items[0].imageHint, "Липа сердцевидная");
  assert.deepEqual(done.facts, [{ label: "Высота", value: "до 30 м" }]);
  assert.equal(done.title, "Липы");
  assert.equal(applySection(section, { text: [], items: [] }, false), null);
  assert.equal(applySection({ ...section, title: "своё" }, { title: "Медоносы", emoji: "🐝", text: ["a"] }, true).title, "Медоносы");
});

test("картинка — широкая копия, если оригинал больше", () => {
  assert.equal(resized("https://u/a/Tilia.jpg/330px-Tilia.jpg?x=1", 960), "https://u/a/Tilia.jpg/960px-Tilia.jpg?x=1");
  assert.equal(resized("https://u/a/Tilia.jpg", 960), null);
  const big = { thumbnail: { source: "https://u/x/Oak.jpg/320px-Oak.jpg" }, originalimage: { source: "https://u/x/Oak.jpg", width: 4000 } };
  assert.equal(imageUrl(big, WIDE), "https://u/x/Oak.jpg/960px-Oak.jpg");
  const small = { thumbnail: { source: "https://u/x/Oak.jpg/320px-Oak.jpg" }, originalimage: { source: "https://u/x/Oak.jpg", width: 600 } };
  assert.equal(imageUrl(small, WIDE), "https://u/x/Oak.jpg");
});

test("карточка полки и id без путей", () => {
  const card = cardOf({ ...draft, sections: [{ text: ["a"] }, { items: [] }] });
  assert.equal(card.sections, 2);
  assert.equal(card.written, 1);
  assert.ok(validId("g1734567890"));
  assert.ok(!validId("../x"));
  assert.ok(!validId(""));
});

test("картинка по поиску — только о том же, и не портрет чужого человека", async () => {
  const { relevant, aboutPerson } = await import("./guides.mjs");
  assert.equal(relevant("Маск, Илон", ["Электропроводка"]), false);
  assert.equal(relevant("Лианозово (парк культуры и отдыха)", ["Лианозовский парк"]), true);
  assert.equal(relevant("Electrical wiring", ["Electrical wiring", "Электропроводка"]), true);
  assert.equal(aboutPerson({ title: "Маск, Илон", description: "американский предприниматель" }, ["Электропроводка"]), true);
  assert.equal(aboutPerson({ title: "Цицин, Николай Васильевич", description: "советский ботаник" }, ["Цицин"]), false);
});

test("найденное поиском — о теме путеводителя", async () => {
  const { onTopic } = await import("./guides.mjs");
  assert.equal(onTopic({ title: "Гейша", extract: "Гейша — японская артистка, развлекающая гостей" }, "Кофе: от зерна до чашки"), false);
  assert.equal(onTopic({ title: "Обработка кофе", extract: "Обработка кофейных ягод" }, "Кофе: от зерна до чашки"), true);
});
