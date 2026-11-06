// Цели обучения: год → месяц → неделя → день, каждая — про курс аккаунта.
// Здесь — данные: своя копия на устройстве (цели видны и без сети),
// сверка с аккаунтом, прогресс цели по курсу и подсказки на день. Цели
// ставят человек, Ноа и нейросеть по MCP (platform/server/goals.mjs).
//
// Задача дня отмечается сама, когда сделано то, о чём она: урок прочитан,
// проверка сдана, практика пройдена, минуты фокуса набраны. Большая цель
// показывает, сколько из её меньших сделано, — так одно тянет другое.

import { call, prefs } from "./core.js";
import { store, topicCard, isDone, currentTopic } from "./store.js";

export const PERIODS = [
  { id: "day", name: "День", next: "завтра" },
  { id: "week", name: "Неделя", next: "на следующую неделю" },
  { id: "month", name: "Месяц", next: "на следующий месяц" },
  { id: "year", name: "Год", next: "на следующий год" },
];
const BIGGER = { day: "week", week: "month", month: "year", year: null };

/** Виды целей — те же, что принимает сервер. `go` — куда ведёт цель. */
export const KINDS = {
  lesson: { name: "Урок темы", topic: true, go: "lesson" },
  check: { name: "Проверка темы", topic: true, go: "check" },
  drill: { name: "Наизусть", topic: true, go: "drill" },
  practice: { name: "Практика", topic: true, go: "practice" },
  interview: { name: "Собеседование", go: "interview" },
  cards: { name: "Карточки", target: "карточек", go: "review" },
  focus: { name: "Фокус", target: "минут", go: "focus" },
  topics: { name: "Темы курса", target: "тем", go: "course" },
  course: { name: "Курс целиком", go: "course" },
  custom: { name: "Своё", title: true, go: "course" },
};

const KEY = "goals";
const PENDING = "goals.pending";

/* ── Сроки ────────────────────────────────────────────────────────────── */

const iso = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/** Начало срока по местному времени: день, понедельник, первое число, 1 января. */
export function periodStart(period, date = new Date()) {
  const d = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  if (period === "week") d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  if (period === "month") d.setDate(1);
  if (period === "year") d.setMonth(0, 1);
  return iso(d);
}

/** Соседний срок: −1 — прошлый, +1 — следующий. */
export function shiftStart(period, start, by) {
  const d = new Date(`${start}T12:00:00`);
  if (period === "day") d.setDate(d.getDate() + by);
  if (period === "week") d.setDate(d.getDate() + 7 * by);
  if (period === "month") d.setMonth(d.getMonth() + by, 1);
  if (period === "year") d.setFullYear(d.getFullYear() + by, 0, 1);
  return periodStart(period, d);
}

const MONTHS = ["январь", "февраль", "март", "апрель", "май", "июнь", "июль", "август", "сентябрь", "октябрь", "ноябрь", "декабрь"];

/** Срок словами: «Сегодня», «Неделя с 2 ноября», «Ноябрь 2026», «2026 год». */
export function periodLabel(period, start) {
  const d = new Date(`${start}T12:00:00`);
  const now = periodStart(period);
  if (period === "day") {
    if (start === now) return "Сегодня";
    if (start === shiftStart("day", now, 1)) return "Завтра";
    if (start === shiftStart("day", now, -1)) return "Вчера";
    return d.toLocaleDateString("ru-RU", { day: "numeric", month: "long", weekday: "short" });
  }
  if (period === "week") return `${start === now ? "Эта неделя" : "Неделя"} с ${d.toLocaleDateString("ru-RU", { day: "numeric", month: "long" })}`;
  if (period === "month") return `${MONTHS[d.getMonth()].replace(/^./, (c) => c.toUpperCase())} ${d.getFullYear()}`;
  return `${d.getFullYear()} год`;
}

/* ── Хранение и сверка ────────────────────────────────────────────────── */

const local = () => prefs.get(KEY, []);
const pending = () => prefs.get(PENDING, []);

function merge(...lists) {
  const byId = new Map();
  for (const goal of lists.flat()) {
    if (!goal?.id) continue;
    const known = byId.get(goal.id);
    if (!known || Number(goal.updated) >= Number(known.updated)) byId.set(goal.id, goal);
  }
  return [...byId.values()];
}

let pushTimer = 0;
const listeners = new Set();
/**
 * Оповестить подписчиков — по снимку списка: подписчик, который в ответ
 * пересоздаёт себя и подписывается заново, иначе вызывался бы в том же
 * проходе снова и снова (так зависала главная).
 */
const changed = () => [...listeners].forEach((fn) => fn());

export const goals = {
  /** Цели на устройстве — без удалённых. */
  all() {
    return local().filter((g) => !g.deleted);
  },

  /** Цели срока: вид и начало. */
  of(period, start) {
    return this.all().filter((g) => g.period === period && g.start === start);
  },

  byId(id) {
    return this.all().find((g) => g.id === id) ?? null;
  },

  /** Меньшие цели — те, что тянет эта. */
  children(goal) {
    return this.all().filter((g) => g.parent === goal.id);
  },

  /** Большие цели того же курса на соседнем сроке — к ним цепляется новая. */
  parents(period, course) {
    const bigger = BIGGER[period];
    return bigger ? this.all().filter((g) => g.period === bigger && g.course === course && !g.done) : [];
  },

  onChange(fn) {
    listeners.add(fn);
    return () => listeners.delete(fn);
  },

  /** Сверка с аккаунтом: свои правки — туда, чужие (MCP, другое устройство) — сюда. */
  async pull() {
    const out = pending();
    try {
      const account = out.length ? await call("learn_goals_save", { goals: out }) : await call("learn_goals");
      prefs.set(PENDING, []);
      // Слиянием, а не заменой: в ответе и удалённые — отметками, и у каждой цели остаётся свежая правка.
      prefs.set(KEY, merge(local(), account ?? []));
      changed();
      return true;
    } catch {
      return false;
    }
  },

  /** Сохранить цель (новую или правку): на устройстве — сразу, в аккаунт — пачкой. */
  save(goal) {
    const next = { ...goal, id: goal.id || newId(), updated: Date.now(), created: goal.created || Date.now() };
    prefs.set(KEY, merge(local(), [next]));
    prefs.set(PENDING, merge(pending(), [next]));
    changed();
    schedulePush();
    return next;
  },

  remove(id) {
    const tomb = { id, deleted: true, updated: Date.now() };
    prefs.set(KEY, merge(local(), [tomb]));
    prefs.set(PENDING, merge(pending(), [tomb]));
    changed();
    schedulePush();
  },
};

function schedulePush() {
  clearTimeout(pushTimer);
  pushTimer = setTimeout(() => goals.pull(), 1500);
}

function newId() {
  const bytes = crypto.getRandomValues(new Uint8Array(12));
  return `g-${[...bytes].map((b) => b.toString(16).padStart(2, "0")).join("")}`;
}

/* ── Прогресс цели ────────────────────────────────────────────────────── */

const after = (at, goal) => Number(at) >= Number(goal.created || 0) - 60_000;

/**
 * Насколько цель сделана: `{ done, value, total, text, auto }`. `auto` —
 * отмечается сама по курсу; иначе — галочкой. Большая цель без своего
 * признака — по меньшим.
 */
export function progressOf(goal) {
  const course = store.course(goal.course);
  const topic = goal.topic ? topicCard(course, goal.topic) : null;
  const yes = (done, text = "") => ({ done, value: done ? 1 : 0, total: 1, text, auto: true });
  const kids = goals.children(goal);
  const rollup = kids.length ? { value: kids.filter((g) => g.done).length, total: kids.length } : null;
  if (!course) return { done: goal.done, value: 0, total: 1, text: "курса на устройстве нет", auto: false };
  switch (goal.kind) {
    case "lesson":
      return yes(Boolean(topic?.read), topic?.read ? "урок прочитан" : "урок не прочитан");
    case "check": {
      const best = topic?.examBest ?? null;
      return yes(best != null && best >= (course.topicPass ?? 70), best != null ? `лучший ${best}%` : "проверки не было");
    }
    case "drill": {
      const done = prefs.get("drilled", []).includes(`${goal.course}/${goal.topic}`);
      return yes(done, done ? "выучено" : "понятия ещё не выучены");
    }
    case "practice": {
      const own = store.practiceOf(goal.course, goal.topic);
      return yes(Boolean(own?.done), own ? (own.done ? "пройдена" : `шаг ${own.step + 1} из ${own.total}`) : "не начата");
    }
    case "interview": {
      const past = prefs.get("interviews", []).filter((i) => i.course === goal.course && (!goal.topic || i.topic === goal.topic) && after(i.at, goal));
      return yes(past.length > 0, past.length ? `пройдено, ${past.at(-1).score}%` : "ещё не было");
    }
    case "focus": {
      const minutes = goal.period === "day" ? course.focus?.today ?? 0 : course.focus?.week ?? 0;
      const total = goal.target || 25;
      return { done: minutes >= total, value: Math.min(minutes, total), total, text: `${minutes} из ${total} мин`, auto: goal.period === "day" || goal.period === "week" };
    }
    case "cards": {
      const due = course.mastery?.due ?? 0;
      if (!due) return { done: true, value: goal.target, total: goal.target, text: "все карточки повторены", auto: true };
      return { done: goal.done, value: goal.done ? goal.target : 0, total: goal.target, text: `к повторению ${due}`, auto: false };
    }
    case "topics": {
      const now = course.topics.filter(isDone).length;
      const base = Number.isFinite(goal.base) ? goal.base : now;
      const made = Math.max(0, now - base);
      return { done: made >= goal.target, value: Math.min(made, goal.target), total: goal.target, text: `${made} из ${goal.target} тем`, auto: true, base };
    }
    case "course": {
      const done = course.topics.length > 0 && course.topics.every(isDone);
      if (rollup && !done) return { done: false, ...rollup, text: `${course.percent}% курса · ${rollup.value} из ${rollup.total} целей`, auto: true };
      return { done, value: course.percent, total: 100, text: `${course.percent}% курса`, auto: true };
    }
    default:
      if (rollup) return { done: goal.done || (rollup.total > 0 && rollup.value === rollup.total), ...rollup, text: `${rollup.value} из ${rollup.total} целей`, auto: false };
      return { done: goal.done, value: goal.done ? 1 : 0, total: 1, text: "", auto: false };
  }
}

/** Сделанное по курсу отмечается само; у «тем» запоминается, с чего начали. */
export function settle() {
  for (const goal of goals.all()) {
    const p = progressOf(goal);
    if (goal.kind === "topics" && !Number.isFinite(goal.base) && Number.isFinite(p.base)) goals.save({ ...goal, base: p.base });
    if (p.auto && p.done && !goal.done) goals.save({ ...goal, done: true, doneAt: Date.now() });
  }
}

/* ── Подсказки на день ────────────────────────────────────────────────── */

/**
 * Что поставить на сегодня по текущему курсу: следующий шаг темы, карточки,
 * фокус. Уже стоящие на сегодня — не повторяются.
 */
export function suggestDay(course = store.currentCourse()) {
  if (!course) return [];
  const today = periodStart("day");
  const have = goals.of("day", today);
  const same = (kind, topic = "") => have.some((g) => g.kind === kind && g.course === course.id && (g.topic || "") === topic);
  const week = goals.of("week", periodStart("week")).find((g) => g.course === course.id && !g.done);
  const parent = week?.id ?? "";
  const out = [];
  const topic = currentTopic(course);
  if (topic && !topic.read && !same("lesson", topic.id)) out.push({ kind: "lesson", topic: topic.id, title: `Урок: ${topic.title}` });
  else if (topic && topic.read && !isDone(topic) && !same("check", topic.id)) out.push({ kind: "check", topic: topic.id, title: `Проверка: ${topic.title}` });
  const due = course.mastery?.due ?? 0;
  if (due && !same("cards")) out.push({ kind: "cards", target: Math.min(due, 30), title: `Повторить ${Math.min(due, 30)} карточек` });
  if (!same("focus")) out.push({ kind: "focus", target: 25, title: "Фокус 25 мин" });
  return out.map((g) => ({ ...g, period: "day", start: today, course: course.id, parent, by: "noa", topic: g.topic ?? "" }));
}
