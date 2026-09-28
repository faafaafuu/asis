// Ноа в браузере: курсы и прогресс обучения в аккаунте.
//
// Сама Ноа работает в браузере (src/js/web): модель зовётся оттуда, ключ на
// сервер не приходит. Сервер только хранит то, что должно догнать человека на
// другом устройстве: курсы — в том числе собранные его нейросетью по MCP — и
// прогресс. Курс проверяется тем же кодом, что и в браузере (learn-core.js).

import { validate, advice, normalizeCourse } from "../../src/js/web/learn-core.js";

/** Предел курса и прогресса: больше не бывает у настоящих, а место не резиновое. */
const MAX_COURSE = 2_000_000;
const MAX_PROGRESS = 1_000_000;
const MAX_COURSES = 50;

/**
 * Мост к подпискам (Claude Code, Codex, Gemini, Qwen) живёт на этом же сервере
 * и доступен только владельцу: он платит за подписки. Кому можно — номера
 * аккаунтов в NOAH_BRIDGE_USERS; токен моста — из его же файла настроек,
 * который служба сайта подключает к себе (EnvironmentFile).
 */
const BRIDGE_URL = (process.env.NOAH_BRIDGE_URL ?? "http://127.0.0.1:8791").replace(/\/$/, "");
const BRIDGE_TOKEN = process.env.NOA_BRIDGE_TOKEN ?? "";
const BRIDGE_USERS = new Set(
  String(process.env.NOAH_BRIDGE_USERS ?? "")
    .split(",")
    .map((id) => id.trim())
    .filter(Boolean),
);
/** Мост отвечает до трёх минут (своё время ожидания у него 180 с). */
const BRIDGE_WAIT = 200_000;

export function mountNoa({ route, db, Fail, readJson }) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS noa_courses (
      user_id TEXT NOT NULL,
      course_id TEXT NOT NULL,
      body TEXT NOT NULL,
      updated TEXT NOT NULL DEFAULT (datetime('now')),
      PRIMARY KEY (user_id, course_id)
    );
    CREATE TABLE IF NOT EXISTS noa_progress (
      user_id TEXT PRIMARY KEY,
      body TEXT NOT NULL,
      saved_at INTEGER NOT NULL
    );
  `);

  /**
   * Номер человека — строкой. Число node:sqlite передаёт как дробное, и в
   * текстовой колонке оно ложится «2.0»: совпадало бы только пока везде
   * передают число, а не строку.
   */
  const need = (user) => {
    if (!user) throw new Fail(401, "Нужно войти.");
    return { ...user, id: String(user.id) };
  };

  route("GET", /^\/api\/noa\/courses$/, ({ user: who }) => {
    const user = need(who);
    const rows = db.prepare("SELECT body FROM noa_courses WHERE user_id = ? ORDER BY updated DESC").all(user.id);
    return { courses: rows.map((row) => JSON.parse(row.body)) };
  });

  route("POST", /^\/api\/noa\/courses$/, async ({ req, user: who }) => {
    const user = need(who);
    const { course } = await readJson(req, MAX_COURSE + 1000);
    return { report: saveCourse(db, user.id, course) };
  });

  route("DELETE", /^\/api\/noa\/courses\/([A-Za-z0-9_-]{1,64})$/, ({ user: who, match }) => {
    const user = need(who);
    db.prepare("DELETE FROM noa_courses WHERE user_id = ? AND course_id = ?").run(user.id, match[1]);
    return { ok: true };
  });

  const bridgeUser = (who) => {
    const user = need(who);
    if (!BRIDGE_TOKEN || !BRIDGE_USERS.has(user.id)) throw new Fail(403, "Мост доступен только владельцу.");
    return user;
  };

  const bridge = async (path, init = {}) => {
    let response;
    try {
      response = await fetch(`${BRIDGE_URL}${path}`, {
        ...init,
        headers: { ...(init.headers ?? {}), Authorization: `Bearer ${BRIDGE_TOKEN}` },
        signal: AbortSignal.timeout(BRIDGE_WAIT),
      });
    } catch {
      throw new Fail(502, "Мост не отвечает — служба noa-bridge на сервере остановлена или занята.");
    }
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Fail(response.status, data.error ?? `Мост ответил ошибкой ${response.status}.`);
    return data;
  };

  route("GET", /^\/api\/noa\/bridge\/models$/, ({ user }) => {
    bridgeUser(user);
    return bridge("/v1/models");
  });

  route("POST", /^\/api\/noa\/bridge\/chat\/completions$/, async ({ req, user }) => {
    bridgeUser(user);
    const { model, messages } = await readJson(req);
    if (!Array.isArray(messages) || !messages.length) throw new Fail(400, "Нет сообщений.");
    // noa_web — мост ответит разово, не трогая сессию Ноа на компьютере.
    return bridge("/v1/chat/completions", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: String(model ?? ""), messages, noa_web: true }),
    });
  });

  route("GET", /^\/api\/noa\/progress$/, ({ user: who }) => {
    const user = need(who);
    const row = db.prepare("SELECT body, saved_at FROM noa_progress WHERE user_id = ?").get(user.id);
    return row ? { data: JSON.parse(row.body), savedAt: row.saved_at } : { data: null, savedAt: 0 };
  });

  route("PUT", /^\/api\/noa\/progress$/, async ({ req, user: who }) => {
    const user = need(who);
    const { data, savedAt } = await readJson(req, MAX_PROGRESS + 1000);
    if (!data || typeof data !== "object" || Array.isArray(data)) throw new Fail(400, "Прогресс — объект.");
    const body = JSON.stringify(data);
    if (body.length > MAX_PROGRESS) throw new Fail(413, "Прогресс слишком большой.");
    const at = Number.isFinite(savedAt) ? Math.min(savedAt, Date.now() + 60_000) : Date.now();
    db.prepare(
      "INSERT INTO noa_progress (user_id, body, saved_at) VALUES (?, ?, ?) ON CONFLICT(user_id) DO UPDATE SET body = excluded.body, saved_at = excluded.saved_at WHERE excluded.saved_at >= noa_progress.saved_at",
    ).run(user.id, body, at);
    return { ok: true };
  });
}

/**
 * Проверяет и сохраняет курс в аккаунт. Отдаёт итог словами — его видит и
 * человек на сайте, и нейросеть, собравшая курс по MCP. Ошибка — исключение
 * со списком того, что исправить.
 */
export function saveCourse(db, owner, raw) {
  const userId = String(owner);
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) throw new Error("course — объект курса по course_format.");
  const body = JSON.stringify(raw);
  if (body.length > MAX_COURSE) throw new Error("Курс слишком большой — не больше 2 МБ.");
  const problems = validate(raw);
  if (problems.length) throw new Error(`Курс не принят:\n- ${problems.join("\n- ")}`);
  const course = normalizeCourse(raw);
  const count = db.prepare("SELECT COUNT(*) AS n FROM noa_courses WHERE user_id = ? AND course_id != ?").get(userId, course.id).n;
  if (count >= MAX_COURSES) throw new Error(`Курсов уже ${MAX_COURSES} — уберите ненужные на noahlab.ru/app.`);
  db.prepare(
    "INSERT INTO noa_courses (user_id, course_id, body) VALUES (?, ?, ?) ON CONFLICT(user_id, course_id) DO UPDATE SET body = excluded.body, updated = datetime('now')",
  ).run(userId, course.id, body);
  const concepts = course.topics.reduce((sum, t) => sum + t.concepts.length, 0);
  const questions = course.topics.reduce((sum, t) => sum + t.tasks.length + t.exam.length, 0) + course.final.length;
  const notes = advice(course);
  return (
    `Курс «${course.title}» сохранён: тем ${course.topics.length}, понятий ${concepts}, вопросов ${questions}. Он уже в Ноа онлайн — noahlab.ru/app.` +
    (notes.length ? `\nЧто улучшить:\n- ${notes.join("\n- ")}` : "")
  );
}

/** Тема в курс аккаунта: добавить или заменить тему с тем же id. */
export function saveTopic(db, owner, courseId, topic) {
  const userId = String(owner);
  const row = db.prepare("SELECT body FROM noa_courses WHERE user_id = ? AND course_id = ?").get(userId, courseId);
  if (!row) return null;
  const course = JSON.parse(row.body);
  course.topics = Array.isArray(course.topics) ? course.topics : [];
  const at = course.topics.findIndex((known) => known?.id === topic?.id);
  if (at >= 0) course.topics[at] = topic;
  else course.topics.push(topic);
  return saveCourse(db, userId, course);
}
