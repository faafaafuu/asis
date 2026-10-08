// Ноа в браузере: курсы и прогресс обучения в аккаунте.
//
// Сама Ноа работает в браузере (src/js/web): модель зовётся оттуда, ключ на
// сервер не приходит. Сервер только хранит то, что должно догнать человека на
// другом устройстве: курсы — в том числе собранные его нейросетью по MCP — и
// прогресс. Курс проверяется тем же кодом, что и в браузере (learn-core.js).

import { spawn } from "node:child_process";

import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";

import { validate, advice, normalizeCourse, mergeProgress } from "../../src/js/web/learn-core.js";
import { buildCourse, Stopped } from "../../src/js/web/course-builder.js";

const HERE = dirname(fileURLToPath(import.meta.url));
/** Методика курса — та же, что отдаёт нейросети MCP (course_format). */
const FORMAT_PATH = process.env.NOAH_COURSE_FORMAT ?? join(HERE, "..", "..", "src-tauri", "src", "course_format.md");
const courseFormat = () => (existsSync(FORMAT_PATH) ? readFileSync(FORMAT_PATH, "utf8").replace(/\r\n/g, "\n") : "");

/** Какими движками моста можно собирать курс. */
const BUILD_MODELS = new Set(["claude-code-bridge", "codex-bridge", "gemini-bridge", "qwen-bridge"]);

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

/**
 * Голос Ноа — тот же Silero, что в программе (assets/silero_server.py), на
 * этом сервере: служба noah-voice, только 127.0.0.1. Отдаётся MP3: WAV весит
 * сотню килобайт на секунду речи, для телефона и медленной сети это много.
 */
const VOICE_URL = (process.env.NOAH_VOICE_URL ?? "http://127.0.0.1:8644").replace(/\/$/, "");
const VOICES = new Set(["xenia", "baya", "kseniya", "aidar", "eugene"]);
/** Сколько фраз в час одному человеку: голос считает процессор сервера. */
const VOICE_PER_HOUR = 600;
const MAX_PHRASE = 1500;
const spoke = new Map();

/** Распознавание речи для браузеров, которые не распознают сами (Safari, Firefox). */
const STT_URL = (process.env.NOAH_STT_URL ?? "http://127.0.0.1:8645").replace(/\/$/, "");
const MAX_RECORDING = 4 * 1024 * 1024;
const heard = new Map();

/** Тело запроса как есть — запись голоса, не JSON. */
async function readRaw(req, limit, Fail) {
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > limit) throw new Fail(413, "Запись слишком длинная.");
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
}

/**
 * WAV → MP3 через ffmpeg. 24 кГц и 64 кбит/с: при 48 кбит/с кодер сам
 * срезал верхние частоты, и голос в браузере звучал глухо и «грязно».
 */
function toMp3(wav) {
  return new Promise((resolve, reject) => {
    const ff = spawn("ffmpeg", ["-hide_banner", "-loglevel", "error", "-i", "pipe:0", "-ac", "1", "-ar", "24000", "-c:a", "libmp3lame", "-b:a", "64k", "-f", "mp3", "pipe:1"]);
    const out = [];
    ff.stdout.on("data", (chunk) => out.push(chunk));
    ff.on("error", reject);
    ff.on("close", (code) => (code === 0 ? resolve(Buffer.concat(out)) : reject(new Error(`ffmpeg: ${code}`))));
    ff.stdin.on("error", () => {});
    ff.stdin.end(wav);
  });
}

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
    CREATE TABLE IF NOT EXISTS noa_builds (
      id TEXT PRIMARY KEY,
      user_id TEXT NOT NULL,
      goal TEXT NOT NULL,
      model TEXT NOT NULL DEFAULT '',
      course_id TEXT NOT NULL DEFAULT '',
      title TEXT NOT NULL DEFAULT '',
      status TEXT NOT NULL,
      stage TEXT NOT NULL DEFAULT '',
      done INTEGER NOT NULL DEFAULT 0,
      total INTEGER NOT NULL DEFAULT 0,
      message TEXT NOT NULL DEFAULT '',
      created TEXT NOT NULL DEFAULT (datetime('now')),
      updated TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS noa_builds_user ON noa_builds(user_id, created);
  `);
  // Какой моделью Claude собирать: sonnet точнее, haiku быстрее.
  if (!db.prepare("SELECT name FROM pragma_table_info('noa_builds') WHERE name = 'quality'").get()) {
    db.exec("ALTER TABLE noa_builds ADD COLUMN quality TEXT NOT NULL DEFAULT 'sonnet'");
  }

  /**
   * Номер человека — строкой. Число node:sqlite передаёт как дробное, и в
   * текстовой колонке оно ложится «2.0»: совпадало бы только пока везде
   * передают число, а не строку.
   */
  const need = (user) => {
    if (!user) throw new Fail(401, "Нужно войти.");
    return { ...user, id: String(user.id) };
  };

  /**
   * Версия курсов человека: меняется, когда курс добавили, дописали или
   * убрали. Страница спрашивает её часто, а весь список — только когда она
   * сменилась: курсы весят сотни килобайт.
   */
  const coursesVersion = (userId) => {
    const row = db.prepare("SELECT COUNT(*) AS n, COALESCE(MAX(updated), '') AS at FROM noa_courses WHERE user_id = ?").get(userId);
    const building = db.prepare("SELECT COUNT(*) AS n FROM noa_builds WHERE user_id = ? AND status = 'running'").get(userId).n;
    return `${row.n}:${row.at}:${building}`;
  };

  route("GET", /^\/api\/noa\/courses$/, ({ user: who }) => {
    const user = need(who);
    const rows = db.prepare("SELECT body FROM noa_courses WHERE user_id = ? ORDER BY updated DESC").all(user.id);
    return { courses: rows.map((row) => JSON.parse(row.body)), version: coursesVersion(user.id) };
  });

  route("GET", /^\/api\/noa\/courses\/version$/, ({ user: who }) => {
    const user = need(who);
    return { version: coursesVersion(user.id) };
  });

  // Методика курса — для сборки в браузере моделью человека.
  route("GET", /^\/api\/noa\/course-format$/, () => ({ text: courseFormat() }));

  route("POST", /^\/api\/noa\/courses$/, async ({ req, user: who }) => {
    const user = need(who);
    const { course } = await readJson(req, MAX_COURSE + 1000);
    return { report: saveCourse(db, user.id, course, { forApp: true }) };
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

  const bridge = async (path, init = {}, wait = BRIDGE_WAIT) => {
    let response;
    try {
      response = await fetch(`${BRIDGE_URL}${path}`, {
        ...init,
        headers: { ...(init.headers ?? {}), Authorization: `Bearer ${BRIDGE_TOKEN}` },
        signal: AbortSignal.timeout(wait),
      });
    } catch {
      throw new Fail(502, "Мост не отвечает — служба noa-bridge на сервере остановлена или занята.");
    }
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Fail(response.status, data.error ?? `Мост ответил ошибкой ${response.status}.`);
    return data;
  };

  // Мост — и по входу на сайт (Ноа онлайн), и по ключу площадки `noah_…`:
  // программа на телефоне туннеля к мосту не имеет и ходит сюда как к
  // обычной нейросети, с тем же ключом, что тянет курсы и прогресс.
  route("GET", /^\/api\/noa\/bridge\/models$/, ({ user, tokenUser }) => {
    bridgeUser(user ?? tokenUser);
    return bridge("/v1/models");
  });

  route("POST", /^\/api\/noa\/bridge\/chat\/completions$/, async ({ req, user, tokenUser }) => {
    bridgeUser(user ?? tokenUser);
    const { model, messages, noa_model: level } = await readJson(req);
    if (!Array.isArray(messages) || !messages.length) throw new Fail(400, "Нет сообщений.");
    // Модель Claude выбирается только у Claude; у Codex, Gemini, Qwen — своя.
    const quality = String(model ?? "").startsWith("claude") && ["haiku", "sonnet", "opus"].includes(level) ? level : "";
    // noa_web — мост ответит разово, не трогая сессию Ноа на компьютере.
    return bridge("/v1/chat/completions", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: String(model ?? ""), messages, noa_web: true, ...(quality ? { noa_model: quality } : {}) }),
    });
  });

  /* ── Сборка курса на сервере — через мост ─────────────────────────────
     Идёт, даже если страницу закрыли: человек написал тему, ушёл, вернулся —
     курс уже дорастает. Переживает и перезапуск сервера: недособранный курс
     лежит в аккаунте вместе с планом, и сборка продолжается с него. */

  const running = new Map();

  /** Урок на тысячи слов модель пишет минуты: большим ответам — до 10 минут. */
  const LONG_ANSWER = 600;

  const bridgeChat = async (model, messages, long = false, quality = "") => {
    const data = await bridge(
      "/v1/chat/completions",
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model,
          messages,
          noa_web: true,
          ...(long ? { noa_timeout: LONG_ANSWER } : {}),
          ...(quality ? { noa_model: quality } : {}),
        }),
      },
      long ? (LONG_ANSWER + 30) * 1000 : BRIDGE_WAIT,
    );
    const text = data?.choices?.[0]?.message?.content;
    if (typeof text !== "string" || !text.trim()) throw new Error("Мост прислал пустой ответ.");
    return text;
  };

  const buildRow = (row) => ({
    id: row.id,
    status: row.status,
    stage: row.stage,
    done: row.done,
    total: row.total,
    title: row.title,
    courseId: row.course_id,
    message: row.message,
    goal: row.goal,
    updated: row.updated,
  });

  const updateBuild = (id, fields) => {
    const sets = Object.keys(fields).map((key) => `${key} = ?`);
    db.prepare(`UPDATE noa_builds SET ${sets.join(", ")}, updated = datetime('now') WHERE id = ?`).run(...Object.values(fields), id);
  };

  const runBuild = (job) => {
    if (running.has(job.id)) return;
    const control = { stop: false };
    running.set(job.id, control);
    const saved = job.course_id
      ? db.prepare("SELECT body FROM noa_courses WHERE user_id = ? AND course_id = ?").get(job.user_id, job.course_id)
      : null;
    const resume = saved ? JSON.parse(saved.body) : null;
    const taken = new Set(db.prepare("SELECT course_id FROM noa_courses WHERE user_id = ?").all(job.user_id).map((row) => row.course_id));
    buildCourse({
      goal: job.goal,
      format: courseFormat(),
      chat: (messages, opts) => {
        const engine = job.model || "claude-code-bridge";
        // Модель Claude выбирается только у Claude; у Codex, Gemini, Qwen — своя.
        return bridgeChat(engine, messages, Boolean(opts?.long), engine.startsWith("claude") ? job.quality : "");
      },
      save: (course) => {
        saveCourse(db, job.user_id, course, { forApp: true });
      },
      progress: (p) =>
        updateBuild(job.id, {
          stage: p.stage,
          done: p.done,
          total: p.total,
          message: p.message,
          ...(p.title ? { title: p.title } : {}),
          ...(p.courseId ? { course_id: p.courseId } : {}),
        }),
      stopped: () => control.stop,
      resume: resume?.building ? resume : null,
      taken,
    })
      .then(() => updateBuild(job.id, { status: "done", stage: "done" }))
      .catch((err) => {
        const halted = err instanceof Stopped;
        if (!halted) console.error("сборка курса:", err?.message ?? err);
        updateBuild(job.id, {
          status: halted ? "stopped" : "failed",
          message: halted ? "Сборка остановлена — готовые темы остались." : String(err?.message ?? err),
        });
      })
      .finally(() => running.delete(job.id));
  };

  // Сервер перезапустили посреди сборки — продолжаем с того, что сохранено.
  if (BRIDGE_TOKEN) {
    for (const job of db.prepare("SELECT * FROM noa_builds WHERE status = 'running'").all()) runBuild(job);
  } else {
    db.prepare("UPDATE noa_builds SET status = 'failed', message = 'Мост не настроен.' WHERE status = 'running'").run();
  }

  // Сборка курса — и по входу на сайт, и по ключу площадки: её зовёт и
  // программа (на компьютере и телефоне), курс приходит в неё сам.
  route("POST", /^\/api\/noa\/builds$/, async ({ req, user: who, tokenUser }) => {
    const user = bridgeUser(who ?? tokenUser);
    const { goal, model, quality } = await readJson(req);
    const text = String(goal ?? "").trim();
    if (text.length < 10) throw new Fail(400, "Опишите курс подробнее: о чём он и для чего — хотя бы одним предложением.");
    if (text.length > 3000) throw new Fail(400, "Слишком длинно — хватит пары абзацев.");
    const active = db.prepare("SELECT title FROM noa_builds WHERE user_id = ? AND status = 'running'").get(user.id);
    if (active) throw new Fail(409, `Уже собирается курс${active.title ? ` «${active.title}»` : ""} — дождитесь его или остановите.`);
    const id = randomUUID();
    const engine = BUILD_MODELS.has(model) ? model : "claude-code-bridge";
    const level = ["haiku", "sonnet", "opus"].includes(quality) ? quality : "sonnet";
    db.prepare(
      "INSERT INTO noa_builds (id, user_id, goal, model, quality, status, stage, message) VALUES (?, ?, ?, ?, ?, 'running', 'plan', 'Составляю план курса')",
    ).run(id, user.id, text, engine, level);
    runBuild(db.prepare("SELECT * FROM noa_builds WHERE id = ?").get(id));
    return { build: buildRow(db.prepare("SELECT * FROM noa_builds WHERE id = ?").get(id)) };
  });

  route("GET", /^\/api\/noa\/builds$/, ({ user: who, tokenUser }) => {
    const user = need(who ?? tokenUser);
    const rows = db.prepare("SELECT * FROM noa_builds WHERE user_id = ? ORDER BY created DESC LIMIT 5").all(user.id);
    return { builds: rows.map(buildRow), version: coursesVersion(user.id) };
  });

  route("POST", /^\/api\/noa\/builds\/([0-9a-f-]{36})\/stop$/, ({ user: who, tokenUser, match }) => {
    const user = need(who ?? tokenUser);
    const row = db.prepare("SELECT * FROM noa_builds WHERE id = ? AND user_id = ?").get(match[1], user.id);
    if (!row) throw new Fail(404, "Такой сборки нет.");
    const control = running.get(row.id);
    if (control) control.stop = true;
    else if (row.status === "running") updateBuild(row.id, { status: "stopped", message: "Сборка остановлена." });
    return { ok: true };
  });

  route("POST", /^\/api\/noa\/tts$/, async ({ req, res, user: who }) => {
    const user = need(who);
    const { text, voice, rate } = await readJson(req);
    const phrase = String(text ?? "").trim().slice(0, MAX_PHRASE);
    if (!phrase) throw new Fail(400, "Нечего читать.");
    const hour = Date.now() - 3_600_000;
    const recent = (spoke.get(user.id) ?? []).filter((at) => at > hour);
    if (recent.length >= VOICE_PER_HOUR) throw new Fail(429, "Голоса на этот час хватит — дальше читаю голосом браузера.");
    spoke.set(user.id, [...recent, Date.now()]);
    let wav;
    try {
      const response = await fetch(`${VOICE_URL}/tts`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ text: phrase, speaker: VOICES.has(voice) ? voice : "xenia", rate: Math.min(Math.max(Number(rate) || 1.1, 0.6), 2) }),
        signal: AbortSignal.timeout(60_000),
      });
      if (!response.ok) throw new Error(String(response.status));
      wav = Buffer.from(await response.arrayBuffer());
    } catch {
      throw new Fail(503, "Голос сейчас недоступен.");
    }
    const mp3 = await toMp3(wav).catch(() => null);
    res.writeHead(200, {
      "Content-Type": mp3 ? "audio/mpeg" : "audio/wav",
      "Content-Length": (mp3 ?? wav).length,
      "Cache-Control": "no-store",
      "X-Content-Type-Options": "nosniff",
    });
    res.end(mp3 ?? wav);
  });

  route("POST", /^\/api\/noa\/stt$/, async ({ req, user: who }) => {
    const user = need(who);
    const hour = Date.now() - 3_600_000;
    const recent = (heard.get(user.id) ?? []).filter((at) => at > hour);
    if (recent.length >= VOICE_PER_HOUR) throw new Fail(429, "Распознавания на этот час хватит — напишите текстом.");
    heard.set(user.id, [...recent, Date.now()]);
    const audio = await readRaw(req, MAX_RECORDING, Fail);
    if (audio.length < 1000) return { text: "" };
    let response;
    try {
      response = await fetch(`${STT_URL}/stt`, { method: "POST", body: audio, signal: AbortSignal.timeout(60_000) });
    } catch {
      throw new Fail(503, "Распознавание сейчас недоступно — напишите текстом.");
    }
    const data = await response.json().catch(() => ({}));
    if (!response.ok) throw new Fail(422, data.error ?? "Запись не разобралась.");
    return { text: String(data.text ?? "") };
  });

  route("GET", /^\/api\/noa\/progress$/, ({ user: who }) => readProgress(db, need(who).id));

  route("PUT", /^\/api\/noa\/progress$/, async ({ req, user: who }) => {
    const user = need(who);
    const { data } = await readJson(req, MAX_PROGRESS + 1000);
    return { ok: true, ...writeProgress(db, user.id, data, Fail) };
  });
}

/** Прогресс аккаунта: `{ data, savedAt }`, `data` — как learning.json. */
export function readProgress(db, userId) {
  const row = db.prepare("SELECT body, saved_at FROM noa_progress WHERE user_id = ?").get(userId);
  return row ? { data: JSON.parse(row.body), savedAt: row.saved_at } : { data: null, savedAt: 0 };
}

/**
 * Прогресс с устройства — в аккаунт, слитым с тем, что там уже есть: у
 * каждого курса остаётся более свежий (mergeProgress). Отдаёт общий итог —
 * устройство берёт его себе, и так программа, телефон и браузер сходятся.
 */
export function writeProgress(db, userId, incoming, Fail) {
  if (!incoming || typeof incoming !== "object" || Array.isArray(incoming)) throw new Fail(400, "Прогресс — объект.");
  const merged = mergeProgress(readProgress(db, userId).data, incoming);
  const body = JSON.stringify(merged);
  if (body.length > MAX_PROGRESS) throw new Fail(413, "Прогресс слишком большой.");
  const at = Date.now();
  db.prepare(
    "INSERT INTO noa_progress (user_id, body, saved_at) VALUES (?, ?, ?) ON CONFLICT(user_id) DO UPDATE SET body = excluded.body, saved_at = excluded.saved_at",
  ).run(userId, body, at);
  return { data: merged, savedAt: at };
}

/**
 * Досылает программе курсы аккаунта, которых у неё нет и которые ей ещё ни
 * разу не отправлялись: собранные на сайте до того, как сайт начал ставить
 * их в очередь. Удалённый на компьютере курс не возвращается — он уже
 * отправлялся.
 */
export function queueMissingForApp(db, userId, have) {
  const known = new Set(have.map(String));
  const sent = db.prepare("SELECT 1 FROM course_jobs WHERE user_id = ? AND course_id = ? LIMIT 1");
  for (const row of db.prepare("SELECT course_id, body FROM noa_courses WHERE user_id = ?").all(String(userId))) {
    if (known.has(row.course_id) || sent.get(userId, row.course_id)) continue;
    queueForApp(db, userId, row.course_id, row.body);
  }
}

/**
 * Курс — и в очередь программе на компьютере: собранный на сайте курс иначе
 * жил только в браузере, и в окне «Обучение» его не было. Прежний
 * незабранный вариант того же курса заменяется — программа возьмёт свежий.
 */
function queueForApp(db, userId, courseId, body) {
  try {
    db.prepare("UPDATE course_jobs SET status = 'replaced' WHERE user_id = ? AND course_id = ? AND kind = 'course' AND status = 'pending'").run(userId, courseId);
    db.prepare("INSERT INTO course_jobs (id, user_id, course_id, kind, payload) VALUES (?, ?, ?, ?, ?)").run(randomUUID(), userId, courseId, "course", body);
  } catch (err) {
    console.error("курс не встал в очередь программе:", err.message);
  }
}

/**
 * Проверяет и сохраняет курс в аккаунт. Отдаёт итог словами — его видит и
 * человек на сайте, и нейросеть, собравшая курс по MCP. Ошибка — исключение
 * со списком того, что исправить.
 */
export function saveCourse(db, owner, raw, { forApp = false } = {}) {
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
  if (forApp) queueForApp(db, userId, course.id, body);
  const concepts = course.topics.reduce((sum, t) => sum + t.concepts.length, 0);
  const questions = course.topics.reduce((sum, t) => sum + t.tasks.length + t.exam.length, 0) + course.final.length;
  const notes = advice(course);
  return (
    `Курс «${course.title}» сохранён: тем ${course.topics.length}, понятий ${concepts}, вопросов ${questions}. Он уже в Ноа онлайн — noahlab.ru/app.` +
    (notes.length ? `\nЧто улучшить:\n- ${notes.join("\n- ")}` : "")
  );
}

/** Тема в курс аккаунта: добавить или заменить тему с тем же id. */
export function saveTopic(db, owner, courseId, topic, options = {}) {
  const userId = String(owner);
  const row = db.prepare("SELECT body FROM noa_courses WHERE user_id = ? AND course_id = ?").get(userId, courseId);
  if (!row) return null;
  const course = JSON.parse(row.body);
  course.topics = Array.isArray(course.topics) ? course.topics : [];
  const at = course.topics.findIndex((known) => known?.id === topic?.id);
  if (at >= 0) course.topics[at] = topic;
  else course.topics.push(topic);
  return saveCourse(db, userId, course, options);
}
