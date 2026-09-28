// Сборка курса моделью: план → темы → финал, сохранение после каждой темы,
// починка кривых ответов, продолжение и остановка.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { buildCourse, shapeTopic, shapePlan, slug, cut, jsonIn, Stopped } from "./course-builder.js";
import { validate } from "./learn-core.js";

const FORMAT = readFileSync(new URL("../../../src-tauri/src/course_format.md", import.meta.url), "utf8");

const lesson = (name) =>
  [
    `Вступление к теме ${name}: зачем она нужна и что будет дальше.`,
    ...["Кабель", "Разъём", "Проверка", "Ошибки"].map(
      (part) => `## ${part}\n\n${`${part} в теме ${name}: что это, зачем нужно, как устроено внутри и где ошибаются. `.repeat(6)}`,
    ),
  ].join("\n\n");

const concepts = (prefix) =>
  ["витая пара", "коннектор", "обжим", "тестер", "категория", "экран"].map((term, at) => ({
    id: at === 0 ? "Витая пара" : `${prefix}-${at}`,
    term,
    definition: `${term} — понятие номер ${at}.`,
    analogy: "как провод в доме",
    pitfall: at % 2 ? "путают" : "",
    related: at === 1 ? ["Витая пара", "nowhere/xx"] : [],
  }));

function fakeModel({ planTopics = 2 } = {}) {
  const calls = [];
  const chat = async (messages) => {
    const system = messages[0].content;
    calls.push(system.slice(0, 40));
    if (system.includes("Сейчас нужен только план")) {
      return (
        "Вот план:\n```json\n" +
        JSON.stringify({
          id: "Слаботочка",
          title: "Слаботочные системы",
          description: "Для монтажника.",
          topics: Array.from({ length: planTopics }, (_, at) => ({ id: at ? "cctv" : "Кабели", title: at ? "Видеонаблюдение" : "Кабели", summary: "…" })),
        }) +
        "\n```"
      );
    }
    if (system.includes("пишешь урок")) return "```markdown\n" + lesson("x") + "\n```";
    if (system.includes("по готовому уроку")) {
      const prefix = messages[1].content.includes("«Кабели»") ? "cab" : "cam";
      return JSON.stringify({
        concepts: concepts(prefix),
        cards: [{ front: "Чем 5e отличается от 6?", back: "Полосой пропускания.", concept: "Витая пара" }],
        tasks: [
          { kind: "choice", q: "Чем обжимают?", options: ["кримпер", "молоток"], answer: "кримпер", concept: "Витая пара" },
          { kind: "choice", q: "Сломанный вопрос", options: ["а"], answer: 3 },
        ],
        exam: [{ kind: "open", q: "Как проверить линию?", points: ["тестер"], reference: "Тестером.", concept: `${prefix}-3` }],
      });
    }
    if (system.includes("финальный экзамен")) {
      return JSON.stringify({ final: [{ kind: "open", q: "Как связаны кабель и камера?", points: ["питание"], reference: "PoE.", concept: "kabeli/vitaya-para" }] });
    }
    throw new Error(`неожиданный запрос: ${system.slice(0, 60)}`);
  };
  return { chat, calls };
}

test("id из кириллицы — транслитом; длинное режется по предложению", () => {
  assert.equal(slug("Витая пара"), "vitaya-para");
  assert.equal(slug("  "), "x");
  assert.equal(cut("Первое предложение. Второе длинное предложение.", 25), "Первое предложение.");
  assert.deepEqual(jsonIn('ответ: {"a": [1, 2,],}'), { a: [1, 2] });
});

test("план: id по формату, без повторов и без занятых", () => {
  const plan = shapePlan({ title: "Курс", topics: [{ title: "Тема" }, { title: "Тема" }] }, new Set(["kurs"]));
  assert.equal(plan.id, "kurs-2");
  assert.deepEqual(
    plan.topics.map((t) => t.id),
    ["tema", "tema-2"],
  );
  assert.equal(shapePlan({ title: "Одна тема", topics: [{ title: "x" }] }), null, "меньше двух тем — не план");
});

test("тема: связи только в существующее, сломанные вопросы выброшены", () => {
  const topic = shapeTopic({ id: "kabeli", title: "Кабели", summary: "…" }, lesson("x"), { concepts: concepts("cab"), tasks: [{ q: "?", options: ["а"], answer: 5 }], exam: [] }, new Set());
  const twisted = topic.concepts.find((c) => c.term === "коннектор");
  assert.deepEqual(twisted.related, ["vitaya-para"], "своя связь осталась, в никуда — выброшена");
  assert.equal(topic.tasks.length, 0);
});

test("сборка: курс проходит проверку, сохраняется после каждой темы, финал на стык тем", async () => {
  const { chat } = fakeModel();
  const saves = [];
  const stages = [];
  const course = await buildCourse({
    goal: "слаботочка для монтажника",
    format: FORMAT,
    chat,
    save: (c) => saves.push(structuredClone(c)),
    progress: (p) => stages.push(p.stage),
  });
  assert.equal(course.id, "slabotochka");
  assert.equal(course.topics.length, 2);
  assert.deepEqual(validate(course), []);
  assert.equal(saves.length, 3, "тема, тема, итог");
  assert.equal(saves[0].building.done, 1);
  assert.equal(saves[0].building.total, 2);
  assert.equal(saves[0].topics.length, 1, "курс виден уже после первой темы");
  assert.equal(saves[2].building, undefined, "готовый курс — без пометки о сборке");
  assert.equal(course.final.length, 1);
  assert.equal(course.final[0].concept, "kabeli/vitaya-para");
  const task = course.topics[0].tasks[0];
  assert.equal(task.answer, 0, "ответ текстом варианта — стал номером");
  assert.equal(task.id, "kabeli-t1", "id вопросов — по теме, без повторов на весь курс");
  assert.ok(stages.includes("plan") && stages.at(-1) === "done");
});

test("продолжение: берёт сохранённое и собирает только недостающие темы", async () => {
  const { chat } = fakeModel();
  const saves = [];
  await buildCourse({ goal: "x", format: FORMAT, chat, save: (c) => saves.push(structuredClone(c)) });
  const partial = saves[0];
  const second = fakeModel();
  const resumed = await buildCourse({ goal: "", format: FORMAT, chat: second.chat, save: () => {}, resume: partial });
  assert.equal(resumed.topics.length, 2);
  assert.ok(!second.calls.some((c) => c.includes("план")), "план второй раз не составляется");
  assert.equal(second.calls.filter((c) => c.includes("пишешь урок")).length, 1, "только одна недостающая тема");
});

test("остановка: после первой темы сборка прекращается, сделанное сохранено", async () => {
  const { chat } = fakeModel({ planTopics: 3 });
  const saves = [];
  await assert.rejects(
    buildCourse({ goal: "x", format: FORMAT, chat, save: (c) => saves.push(c), stopped: () => saves.length > 0 }),
    Stopped,
  );
  assert.equal(saves.length, 1);
  assert.equal(saves[0].topics.length, 1);
});

test("плохие вопросы переспрашиваются отдельно — понятия второй раз не пишутся", async () => {
  let conceptCalls = 0;
  let questionCalls = 0;
  const chat = async (messages) => {
    const system = messages[0].content;
    if (system.includes("Сейчас нужен только план")) {
      return JSON.stringify({ id: "net", title: "Сеть", description: "…", topics: [{ id: "a", title: "Кабели", summary: "…" }, { id: "b", title: "Разъёмы", summary: "…" }] });
    }
    if (system.includes("пишешь урок")) return lesson("x");
    if (system.includes("понятия и карточки")) {
      conceptCalls++;
      return JSON.stringify({ concepts: concepts("c"), cards: [] });
    }
    if (system.includes("задачи и мини-экзамен")) {
      questionCalls++;
      const exam = questionCalls % 2 ? [] : [{ kind: "open", q: "Как?", points: ["так"], reference: "Так.", concept: "c-2" }];
      return JSON.stringify({ tasks: [{ kind: "open", q: "Зачем?", points: ["затем"], reference: "Затем." }], exam });
    }
    return JSON.stringify({ final: [] });
  };
  const course = await buildCourse({ goal: "x", format: FORMAT, chat, save: () => {} });
  assert.equal(course.topics.length, 2);
  assert.equal(conceptCalls, 2, "по разу на тему");
  assert.equal(questionCalls, 4, "по два раза на тему: первый ответ без экзамена");
});

test("урок следующей темы пишется, пока составляются вопросы текущей", async () => {
  const log = [];
  const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const chat = async (messages) => {
    const system = messages[0].content;
    const about = messages[1].content.includes("Разъёмы") ? 2 : 1;
    if (system.includes("Сейчас нужен только план")) {
      return JSON.stringify({ id: "net", title: "Сеть", description: "…", topics: [{ id: "a", title: "Кабели", summary: "…" }, { id: "b", title: "Разъёмы", summary: "…" }] });
    }
    if (system.includes("пишешь урок")) {
      log.push(`урок ${about} начат`);
      await pause(20);
      return lesson("x");
    }
    if (system.includes("понятия и карточки")) return JSON.stringify({ concepts: concepts("c"), cards: [] });
    if (system.includes("задачи и мини-экзамен")) {
      log.push(`вопросы ${about} начаты`);
      await pause(60);
      log.push(`вопросы ${about} готовы`);
      return JSON.stringify({ tasks: [{ kind: "open", q: "Зачем?", points: ["затем"], reference: "Затем." }], exam: [{ kind: "open", q: "Как?", points: ["так"], reference: "Так." }] });
    }
    return JSON.stringify({ final: [] });
  };
  await buildCourse({ goal: "x", format: FORMAT, chat, save: () => {} });
  assert.ok(log.indexOf("урок 2 начат") < log.indexOf("вопросы 1 готовы"), log.join(" → "));
});
