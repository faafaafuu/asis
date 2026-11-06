// Цели Qrack: только учебные и только по курсам аккаунта; слияние правок.

import { test } from "node:test";
import assert from "node:assert/strict";

import { cleanGoal, mergeGoals, periodStart, goalsText } from "./goals.mjs";

const courses = new Map([["devops", { title: "DevOps", topics: new Map([["docker", "Docker"], ["k8s", "Kubernetes"]]) }]]);

test("срок цели начинается с понедельника, первого числа, первого января", () => {
  const thursday = new Date("2026-11-05T10:00:00Z");
  assert.equal(periodStart("day", thursday), "2026-11-05");
  assert.equal(periodStart("week", thursday), "2026-11-02");
  assert.equal(periodStart("month", thursday), "2026-11-01");
  assert.equal(periodStart("year", thursday), "2026-01-01");
});

test("цель про тему курса: заголовок сам, срок выровнен", () => {
  const goal = cleanGoal({ period: "week", kind: "lesson", course: "devops", topic: "docker", start: "2026-11-05" }, courses);
  assert.equal(goal.title, "Урок: Docker");
  assert.equal(goal.start, "2026-11-02");
  assert.equal(goal.done, false);
});

test("не учебная цель, чужой курс или тема, нет числа — не принимается", () => {
  assert.throws(() => cleanGoal({ period: "day", kind: "shopping", course: "devops" }, courses), /только учебный/);
  assert.throws(() => cleanGoal({ period: "day", kind: "lesson", course: "cooking", topic: "x" }, courses), /Курса «cooking» в аккаунте нет/);
  assert.throws(() => cleanGoal({ period: "day", kind: "lesson", course: "devops", topic: "nginx" }, courses), /Темы «nginx»/);
  assert.throws(() => cleanGoal({ period: "day", kind: "focus", course: "devops" }, courses), /target/);
  assert.throws(() => cleanGoal({ period: "day", kind: "custom", course: "devops" }, courses), /заголовок/);
});

test("слияние: свежая правка побеждает, удалённая не возвращается", () => {
  const now = Date.now();
  const a = { id: "goal-aaaa", title: "старое", updated: now - 2000 };
  const b = { id: "goal-aaaa", title: "новое", updated: now - 1000 };
  assert.equal(mergeGoals([a], [b])[0].title, "новое");
  assert.equal(mergeGoals([b], [a])[0].title, "новое", "старая правка с другого устройства не затирает");
  const gone = mergeGoals([b], [{ id: "goal-aaaa", deleted: true, updated: now }]);
  assert.equal(gone[0].deleted, true);
  assert.equal(mergeGoals(gone, [b])[0].deleted, true, "удалённая не воскресает со старого устройства");
});

test("цели словами — для нейросети", () => {
  const goal = cleanGoal({ period: "year", kind: "course", course: "devops", start: "2026-03-10" }, courses);
  assert.match(goalsText([goal], courses), /\[год с 2026-01-01\] Пройти курс «DevOps»/);
});
