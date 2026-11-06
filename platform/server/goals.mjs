// Цели обучения Qrack — год, месяц, неделя, день. Каждая — про курс
// аккаунта: урок, проверка или практика темы, карточки, минуты фокуса,
// темы за срок. «Своё» — тоже про курс. Цели годом тянут месячные, те —
// недельные, недельные — задачи дня (parent).
//
// Цели живут в аккаунте: их правят приложение (на телефоне и в браузере) и
// нейросеть по MCP. Список сливается по id — у каждой цели остаётся более
// свежая правка, удалённая хранит отметку, чтобы не вернуться с другого
// устройства.

import { randomUUID } from "node:crypto";

export const PERIODS = ["day", "week", "month", "year"];

/** Виды целей — только учебные. Для каких нужна тема, для каких — число. */
export const KINDS = {
  lesson: { name: "урок темы", topic: true },
  check: { name: "проверка темы", topic: true },
  drill: { name: "наизусть", topic: true },
  practice: { name: "практика темы", topic: true },
  interview: { name: "собеседование", topic: false },
  cards: { name: "повторить карточки", target: true },
  focus: { name: "фокус, минут", target: true },
  topics: { name: "пройти тем", target: true },
  course: { name: "пройти курс целиком" },
  custom: { name: "своё по курсу", title: true },
};

const MAX_GOALS = 500;
const TOMBSTONE_DAYS = 60;

/** Начало срока цели: день — сам день, неделя — понедельник, месяц — 1-е, год — 1 января. */
export function periodStart(period, date = new Date()) {
  const d = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()));
  if (period === "week") d.setUTCDate(d.getUTCDate() - ((d.getUTCDay() + 6) % 7));
  if (period === "month") d.setUTCDate(1);
  if (period === "year") d.setUTCMonth(0, 1);
  return d.toISOString().slice(0, 10);
}

/** Курсы человека: id → id тем. По ним проверяется, что цель про существующее. */
export function courseIndex(db, userId) {
  const index = new Map();
  for (const row of db.prepare("SELECT course_id, body FROM noa_courses WHERE user_id = ?").all(String(userId))) {
    try {
      const course = JSON.parse(row.body);
      index.set(row.course_id, { title: String(course.title ?? row.course_id), topics: new Map((course.topics ?? []).map((t) => [String(t.id), String(t.title ?? t.id)])) });
    } catch {
      /* битый курс — целей к нему не ставим */
    }
  }
  return index;
}

const text = (value, max) => String(value ?? "").trim().slice(0, max);

/** Заголовок по виду цели — если его не дали. */
function autoTitle(goal, courses) {
  const course = courses.get(goal.course);
  const topic = goal.topic ? course?.topics.get(goal.topic) : "";
  switch (goal.kind) {
    case "lesson":
      return `Урок: ${topic}`;
    case "check":
      return `Проверка: ${topic}`;
    case "drill":
      return `Наизусть: ${topic}`;
    case "practice":
      return `Практика: ${topic}`;
    case "interview":
      return topic ? `Собеседование: ${topic}` : `Собеседование по курсу «${course?.title}»`;
    case "cards":
      return `Повторить ${goal.target} карточек`;
    case "focus":
      return `Фокус ${goal.target} мин`;
    case "topics":
      return `Пройти ${goal.target} тем курса «${course?.title}»`;
    case "course":
      return `Пройти курс «${course?.title}»`;
    default:
      return goal.title;
  }
}

/**
 * Цель в чистом виде или исключение со словами, что не так. `courses` — из
 * courseIndex: цель про курс, которого нет, не принимается.
 */
export function cleanGoal(raw, courses, { by = "user", now = Date.now() } = {}) {
  if (!raw || typeof raw !== "object") throw new Error("Цель — объект.");
  const period = PERIODS.includes(raw.period) ? raw.period : null;
  if (!period) throw new Error(`Срок цели (period) — одно из: ${PERIODS.join(", ")}.`);
  const kind = Object.hasOwn(KINDS, raw.kind) ? raw.kind : null;
  if (!kind) throw new Error(`Вид цели (kind) — только учебный: ${Object.entries(KINDS).map(([id, k]) => `${id} — ${k.name}`).join("; ")}.`);
  const course = text(raw.course, 64);
  if (!courses.has(course)) {
    const known = [...courses.entries()].map(([id, c]) => `${id} («${c.title}»)`).join(", ");
    throw new Error(`Курса «${course}» в аккаунте нет. Цели ставятся только по курсам аккаунта${known ? `: ${known}` : " — сначала нужен курс"}.`);
  }
  const topic = text(raw.topic, 64);
  if (KINDS[kind].topic && !topic) throw new Error(`Для цели «${KINDS[kind].name}» нужна тема (topic).`);
  if (topic && !courses.get(course).topics.has(topic)) {
    throw new Error(`Темы «${topic}» в курсе «${course}» нет. Темы: ${[...courses.get(course).topics.keys()].join(", ")}.`);
  }
  let target = null;
  if (KINDS[kind].target) {
    target = Math.round(Number(raw.target));
    if (!Number.isFinite(target) || target < 1 || target > 10_000) throw new Error(`Для цели «${KINDS[kind].name}» нужно число target от 1 до 10000.`);
  }
  const start = /^\d{4}-\d{2}-\d{2}$/.test(String(raw.start ?? "")) ? periodStart(period, new Date(`${raw.start}T00:00:00Z`)) : periodStart(period);
  const goal = {
    id: /^[A-Za-z0-9_-]{6,64}$/.test(String(raw.id ?? "")) ? String(raw.id) : randomUUID(),
    period,
    start,
    kind,
    course,
    topic: topic || "",
    target,
    title: text(raw.title, 140),
    note: text(raw.note, 400),
    parent: /^[A-Za-z0-9_-]{6,64}$/.test(String(raw.parent ?? "")) ? String(raw.parent) : "",
    // «Пройти N тем»: сколько тем было пройдено, когда цель поставили.
    base: raw.base === undefined || raw.base === null || !Number.isFinite(Number(raw.base)) ? undefined : Math.max(0, Math.round(Number(raw.base))),
    done: Boolean(raw.done),
    doneAt: raw.done ? Number(raw.doneAt) || now : 0,
    by: ["user", "noa", "ai"].includes(raw.by) ? raw.by : by,
    created: Number(raw.created) || now,
    updated: Number(raw.updated) || now,
  };
  if (KINDS[kind].title && !goal.title) throw new Error("Для своей цели нужен заголовок (title).");
  if (!goal.title) goal.title = autoTitle(goal, courses);
  return goal;
}

/** Слияние: по id остаётся более свежая правка; удалённые — отметкой. */
export function mergeGoals(stored, incoming) {
  const byId = new Map();
  for (const goal of [...(stored ?? []), ...(incoming ?? [])]) {
    if (!goal?.id) continue;
    const known = byId.get(goal.id);
    if (!known || Number(goal.updated) >= Number(known.updated)) byId.set(goal.id, goal);
  }
  const old = Date.now() - TOMBSTONE_DAYS * 86_400_000;
  return [...byId.values()].filter((goal) => !(goal.deleted && Number(goal.updated) < old));
}

export function readGoals(db, userId) {
  const row = db.prepare("SELECT body FROM qrack_goals WHERE user_id = ?").get(String(userId));
  return row ? JSON.parse(row.body) : [];
}

export function writeGoals(db, userId, goals) {
  const live = goals.filter((goal) => !goal.deleted);
  if (live.length > MAX_GOALS) throw new Error(`Целей уже ${MAX_GOALS} — уберите выполненные.`);
  db.prepare(
    "INSERT INTO qrack_goals (user_id, body, updated) VALUES (?, ?, datetime('now')) ON CONFLICT(user_id) DO UPDATE SET body = excluded.body, updated = excluded.updated",
  ).run(String(userId), JSON.stringify(goals));
}

/** Цели словами — для нейросети по MCP. */
export function goalsText(goals, courses) {
  const live = goals.filter((g) => !g.deleted);
  if (!live.length) return "Целей пока нет.";
  const order = { year: 0, month: 1, week: 2, day: 3 };
  const label = { year: "год", month: "месяц", week: "неделя", day: "день" };
  return live
    .sort((a, b) => order[a.period] - order[b.period] || a.start.localeCompare(b.start))
    .map((g) => `${g.done ? "✓" : "○"} [${label[g.period]} с ${g.start}] ${g.title} — ${courses.get(g.course)?.title ?? g.course}${g.topic ? ` / ${g.topic}` : ""} (id: ${g.id}${g.parent ? `, в цели ${g.parent}` : ""})`)
    .join("\n");
}

export function mountGoals({ route, db, Fail, readJson, need }) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS qrack_goals (
      user_id TEXT PRIMARY KEY,
      body TEXT NOT NULL,
      updated TEXT NOT NULL DEFAULT (datetime('now'))
    );
  `);

  route("GET", /^\/api\/noa\/goals$/, ({ user, tokenUser }) => {
    const who = need(user ?? tokenUser);
    // С отметками удалённых: устройство сливает их со своим списком, а не заменяет его.
    return { goals: readGoals(db, who.id), kinds: KINDS };
  });

  // Правки с устройства: новые, изменённые и удалённые (deleted) — слиянием.
  route("POST", /^\/api\/noa\/goals$/, async ({ req, user, tokenUser }) => {
    const who = need(user ?? tokenUser);
    const { goals } = await readJson(req, 512 * 1024);
    if (!Array.isArray(goals)) throw new Fail(400, "goals — список.");
    const courses = courseIndex(db, who.id);
    const incoming = [];
    for (const raw of goals.slice(0, MAX_GOALS * 2)) {
      if (raw?.deleted && raw.id) {
        incoming.push({ id: String(raw.id), deleted: true, updated: Number(raw.updated) || Date.now() });
        continue;
      }
      try {
        incoming.push(cleanGoal(raw, courses));
      } catch {
        // Цель про удалённый курс или тему — молча не принимаем: она пропадёт и с устройства.
      }
    }
    const merged = mergeGoals(readGoals(db, who.id), incoming);
    try {
      writeGoals(db, who.id, merged);
    } catch (err) {
      throw new Fail(413, err.message);
    }
    return { goals: merged };
  });
}
