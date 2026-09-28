// Обучение в браузере ведёт себя как в программе: те же правила проверки,
// повторения и прогресса. Сверка с learning.rs, recall.rs и srs.rs.

import { test } from "node:test";
import assert from "node:assert/strict";

import { createLearning, validate, answerCard, newCardState, levelOf, sections, choiceOf } from "./learn-core.js";

const lesson = [
  "# Сети",
  "",
  "Вступление про то, как компьютеры находят друг друга.",
  "",
  "## Адреса",
  "",
  "IP-адрес — номер устройства в сети. Пакет доставляется по адресу получателя, как письмо по почтовому адресу. ".repeat(3),
  "",
  "## Порты",
  "",
  "Порт — номер программы внутри устройства: адрес ведёт к дому, порт — к квартире. ".repeat(3),
  "",
  "## Имена",
  "",
  "DNS превращает имя сайта в IP-адрес: это телефонная книга интернета. ".repeat(3),
].join("\n");

const concept = (id, term, definition) => ({ id, term, definition, mnemonic: "", analogy: "дом и квартира", pitfall: "путают" });

function sampleCourse() {
  return {
    id: "net",
    title: "Сети",
    description: "Основы сетей",
    topics: [
      {
        id: "basics",
        title: "Основы",
        summary: "Адреса, порты, имена",
        lesson,
        concepts: [
          concept("ip", "IP-адрес", "номер устройства в сети"),
          concept("port", "порт", "номер программы внутри устройства"),
          concept("dns", "DNS", "превращает имя сайта в IP-адрес"),
          concept("packet", "пакет", "порция данных с адресом получателя"),
          concept("router", "маршрутизатор", "пересылает пакеты между сетями"),
          { ...concept("mask", "маска", "отделяет номер сети от номера устройства"), related: ["ip"] },
        ],
        tasks: [
          { id: "t1", kind: "choice", q: "Что ведёт к программе?", options: ["IP", "порт"], answer: 1, explain: "Порт — квартира.", concept: "port" },
          { id: "t2", kind: "open", q: "Зачем DNS?", points: ["имя в адрес"], reference: "Чтобы по имени найти IP.", concept: "dns" },
        ],
        exam: [
          { id: "e1", kind: "choice", q: "DNS — это…", options: ["книга имён", "кабель"], answer: 0, concept: "dns" },
          { id: "e2", kind: "choice", q: "IP — это…", options: ["номер устройства", "номер программы"], answer: 0, concept: "ip" },
        ],
      },
    ],
    final: [],
  };
}

function setup({ ai = null, clock } = {}) {
  const store = { data: {}, saves: 0, save() { this.saves++; } };
  const learning = createLearning({ courses: () => [sampleCourse()], store, ai, clock });
  return { learning, store };
}

test("образцовый курс проходит проверку, сломанный — нет", () => {
  assert.deepEqual(validate(sampleCourse()), []);
  const broken = sampleCourse();
  broken.topics[0].tasks[0].answer = 5;
  broken.topics[0].concepts[5].related = ["nowhere"];
  const problems = validate(broken);
  assert.ok(problems.some((p) => p.includes("t1: answer")));
  assert.ok(problems.some((p) => p.includes("связь «nowhere»")));
});

test("урок делится на разделы, как в окне", () => {
  const parts = sections(lesson);
  assert.equal(parts.length, 3);
  assert.ok(parts[0].includes("Вступление") && parts[0].includes("## Адреса"));
});

test("вариант узнаётся по номеру и по словам", () => {
  assert.equal(choiceOf("второй", ["a", "b"]), 1);
  assert.equal(choiceOf("это книга имён", ["книга имён", "кабель"]), 0);
});

test("расписание: удачные ответы отодвигают карточку, провал возвращает через 10 минут", () => {
  const NOW = 1_800_000_000;
  const card = newCardState();
  let now = NOW;
  const intervals = [];
  for (let i = 0; i < 5; i++) {
    answerCard(card, "good", now);
    intervals.push(card.interval);
    now = card.due;
  }
  assert.ok(intervals.every((v, i) => i === 0 || v > intervals[i - 1]));
  assert.equal(levelOf(card), "mature");
  answerCard(card, "again", now);
  assert.equal(card.due, now + 600, "через десять минут");
  assert.equal(card.streak, 0);
  assert.equal(card.lapses, 1);
});

test("задача с вариантами засчитывается и попадает в прогресс", async () => {
  const { learning, store } = setup();
  const verdict = await learning.check("net", "t1", 1);
  assert.equal(verdict.score, 100);
  assert.equal(store.data.courses.net.topics.basics.tasks.t1, 100);
  const wrong = await learning.check("net", "e1", 1);
  assert.equal(wrong.right, false);
  assert.deepEqual(store.data.courses.net.topics.basics.mistakes, ["e1"]);
});

test("открытый ответ без модели — оценка самому", async () => {
  const { learning } = setup();
  const verdict = await learning.check("net", "t2", "по имени находит адрес");
  assert.equal(verdict.score, null);
  learning.selfGrade("net", "t2", true);
  assert.equal(learning.overview()[0].topics[0].tasksDone, 1);
});

test("открытый ответ проверяет модель по пунктам", async () => {
  const ai = { chat: async () => 'Вот: {"covered":[1],"partial":[],"wrong":"","feedback":"Верно."}' };
  const { learning } = setup({ ai });
  const verdict = await learning.check("net", "t2", "по имени находит адрес");
  assert.equal(verdict.score, 100);
  assert.deepEqual(verdict.covered, [1]);
});

test("мини-экзамен без ответов в вопросах; сдача считает балл", async () => {
  const { learning } = setup();
  const exam = learning.exam("net", "basics");
  assert.equal(exam.questions.length, 2);
  assert.ok(exam.questions.every((q) => q.answer === null && !q.points.length));
  const result = await learning.submit("net", "basics", { e1: 0, e2: 0 });
  assert.equal(result.score, 100);
  assert.equal(learning.overview()[0].topics[0].status, "done");
});

test("новые карточки — только из прочитанных тем; оценка ставит расписание", () => {
  const { learning } = setup();
  assert.equal(learning.review("net").length, 0, "урок не прочитан");
  learning.read("net", "basics");
  const queue = learning.review("net");
  assert.equal(queue.length, 6);
  const graded = learning.gradeCard("net", queue[0].key, "good");
  // Разброс в пару процентов: сутки превращаются в 23–25 часов.
  assert.match(graded.next, /^(завтра|через 2\d ч)$/);
  assert.equal(learning.overview()[0].mastery.learning, 1);
});

test("связи понятий видны с обеих сторон и на карте", () => {
  const { learning } = setup();
  const ip = learning.concepts("net", "basics").find((c) => c.id === "ip");
  assert.deepEqual(ip.links.map((l) => l.concept), ["mask"]);
  assert.deepEqual(learning.map("net").edges, [{ from: "basics/ip", to: "basics/mask", cross: false }]);
});

test("серия дней: сегодня без занятий серию не рвёт", () => {
  let now = new Date(2026, 8, 20, 12);
  const { learning } = setup({ clock: () => new Date(now) });
  learning.focusDone("net", { minutes: 25, goal: "адреса" });
  now = new Date(2026, 8, 21, 9);
  const stats = learning.overview()[0].focus;
  assert.equal(stats.streak, 1);
  assert.equal(stats.today, 0);
  assert.equal(stats.week, 25);
});

test("разбор раздела пишется один раз и берётся из хранилища", async () => {
  let calls = 0;
  const ai = { chat: async () => (calls++, "Подробно. ".repeat(40)) };
  const { learning } = setup({ ai });
  const first = await learning.deep("net", "basics", 1);
  const second = await learning.deep("net", "basics", 1);
  assert.equal(first, second);
  assert.equal(calls, 1);
  assert.equal(await learning.deep("net", "basics", 2, true), null);
});

test("обсуждение помнит историю одного раздела", async () => {
  const seen = [];
  const ai = { chat: async (messages) => (seen.push(messages.length), "Ответ.") };
  const { learning } = setup({ ai });
  const target = { course: "net", topic: "basics", section: 1 };
  await learning.ask(target, "не понял");
  await learning.ask(target, "а порт?");
  assert.deepEqual(seen, [2, 4]);
  await learning.ask({ ...target, section: 2 }, "а тут?");
  assert.equal(seen[2], 2, "другой раздел — новый разговор");
});
