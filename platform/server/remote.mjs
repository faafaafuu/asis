// MCP по ссылке и связь с Ноа на компьютере.
//
// Человек вставляет в свою нейросеть (Claude.ai, Claude Desktop, Cursor…)
// ссылку `https://…/mcp?key=noah_…` со страницы «Подключить ИИ». Нейросеть
// пишет модуль и отправляет его сюда черновиком. Ноа на компьютере человека
// раз в несколько секунд забирает черновики по тому же ключу, проверяет их по
// регламенту, ставит прошедшие и присылает отчёт — нейросеть получает его
// ответом на тот же вызов. Код модулей здесь не запускается никогда.
//
// Курсы обучения идут тем же путём: нейросеть собирает курс, он ждёт здесь
// черновиком, Ноа забирает его, проверяет своим валидатором и присылает отчёт.
// Курс забирается кусками по несколько килобайт: с части каналов длинный
// TCP-ответ до зарубежного сервера обрывается на первых десятках килобайт.

import { randomUUID } from "node:crypto";
import { authChallenge } from "./mcpauth.mjs";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const STANDARD_PATH = process.env.NOAH_STANDARD ?? join(HERE, "..", "..", "src-tauri", "src", "module_format.md");
const COURSE_FORMAT_PATH = process.env.NOAH_COURSE_FORMAT ?? join(HERE, "..", "..", "src-tauri", "src", "course_format.md");
/** Кусок курса за один запрос Ноа — в символах JSON. */
const PART_CHARS = 6000;
/** Самый большой курс, который принимаем. */
const MAX_COURSE_CHARS = 4 * 1024 * 1024;
const PROTOCOL = "2025-06-18";
/** Сколько ждать отчёта Ноа в одном вызове create_module. */
const WAIT_MS = 75_000;
/** Ноа на связи, если заходила не позже этого. */
const ONLINE_MS = 90_000;
/** Взятый в проверку черновик возвращается в очередь, если отчёта нет. */
const STUCK_MS = 5 * 60_000;

const REMOTE_NOTES = `
## Если ты подключён к NOAH по ссылке

Ты работаешь не на компьютере пользователя, а через площадку NOAH. Всё так же, но:

- \`environment\` показывает, что установлено у пользователя, и на связи ли Ноа.
  Если Ноа не на связи — попроси пользователя запустить Ноа и вставить ключ площадки
  в Ноа → Настройки → Площадка.
- \`create_module\` отправляет модуль на компьютер пользователя: Ноа проверяет его
  по регламенту и ставит. Ответ — отчёт проверки. Ошибки — исправь и отправь снова.
- \`module_status\` — последний отчёт, если ответ не успел прийти.
- \`publish_module\` — выложить прошедший проверку модуль в библиотеку NOAH от имени
  пользователя. Спроси его перед публикацией.
`;

const COURSE_NOTES = `
## Если ты подключён к NOAH по ссылке

- \`create_course\` отправляет курс на компьютер пользователя: NOAH проверяет его и
  показывает в окне «Обучение». Ответ — отчёт проверки: ошибки исправь и отправь курс
  снова, «Что улучшить» — доработай, если пользователь не против.
- Большой курс удобнее собирать по частям: \`create_course\` с первыми темами, затем
  \`add_topic\` на каждую следующую. Тема с тем же id заменяется.
- \`list_courses\` — какие курсы уже есть у пользователя, \`course_status\` — последний
  отчёт, если ответ не успел прийти.
- NOAH не на связи — курс сохранится и будет проверен, когда NOAH появится.
`;

function courseFormatText() {
  const base = existsSync(COURSE_FORMAT_PATH)
    ? readFileSync(COURSE_FORMAT_PATH, "utf8").replace(/\r\n/g, "\n")
    : "Формат курса недоступен.";
  return `${base}\n${COURSE_NOTES}`;
}

function standardText() {
  const base = existsSync(STANDARD_PATH) ? readFileSync(STANDARD_PATH, "utf8").replace(/\r\n/g, "\n") : "Регламент недоступен.";
  return `${base}\n${REMOTE_NOTES}`;
}

const INSTRUCTIONS =
  "NOAH — мультимодальная оболочка для ИИ на компьютере пользователя. Через эти инструменты ты " +
  "собираешь пользователю модуль под его задачу: пишешь небольшой MCP-сервер, NOAH на его " +
  "компьютере проверяет модуль по регламенту и запускает, после чего пользователь пользуется им " +
  "голосом. Порядок строгий: 1) module_format — прочитай регламент целиком; 2) environment — узнай, " +
  "на чём писать и на связи ли NOAH; 3) уточни у пользователя задачу; 4) create_module; 5) если " +
  "в отчёте ошибки — исправь и отправь снова. Не говори, что модуль готов, до успешного отчёта. " +
  "Прошедший модуль можно опубликовать в библиотеке (publish_module) — с согласия пользователя. " +
  "Ещё здесь собирают курсы обучения по любой теме: course_format — прочитай формат и методику " +
  "целиком, затем create_course (и add_topic для следующих тем); NOAH проверит курс и покажет его " +
  "в окне «Обучение». Не говори, что курс готов, до успешного отчёта.";

const TOOLS = [
  { name: "module_format", description: "Регламент модуля NOAH с примерами. Вызови перед create_module.", inputSchema: { type: "object", properties: {} } },
  { name: "environment", description: "Что установлено у пользователя (Node.js, Python) и на связи ли NOAH на его компьютере.", inputSchema: { type: "object", properties: {} } },
  {
    name: "create_module",
    description: "Отправить модуль на компьютер пользователя: NOAH проверит его по регламенту и поставит. Ответ — отчёт проверки.",
    inputSchema: {
      type: "object",
      properties: {
        module: { type: "object", description: "module.json по регламенту" },
        files: { type: "object", description: "Файлы сервера: имя → содержимое", additionalProperties: { type: "string" } },
      },
      required: ["module"],
    },
  },
  {
    name: "module_status",
    description: "Последний отчёт проверки модуля по id.",
    inputSchema: { type: "object", properties: { id: { type: "string" } }, required: ["id"] },
  },
  {
    name: "search_modules",
    description: "Найти готовые модули в библиотеке NOAH. Перед тем как писать свой, проверь, нет ли готового.",
    inputSchema: { type: "object", properties: { query: { type: "string" } } },
  },
  { name: "course_format", description: "Формат и методика курса обучения NOAH с примерами. Вызови перед create_course.", inputSchema: { type: "object", properties: {} } },
  {
    name: "create_course",
    description: "Создать или заменить курс обучения на компьютере пользователя. NOAH проверит его и покажет в окне «Обучение». Ответ — отчёт проверки.",
    inputSchema: { type: "object", properties: { course: { type: "object", description: "Курс в формате из course_format" } }, required: ["course"] },
  },
  {
    name: "add_topic",
    description: "Добавить тему в курс пользователя или заменить тему с тем же id. Ответ — отчёт проверки курса.",
    inputSchema: {
      type: "object",
      properties: { course: { type: "string", description: "id курса" }, topic: { type: "object", description: "Тема в формате из course_format" } },
      required: ["course", "topic"],
    },
  },
  { name: "list_courses", description: "Курсы обучения, которые уже есть у пользователя.", inputSchema: { type: "object", properties: {} } },
  {
    name: "course_status",
    description: "Последний отчёт проверки курса по id.",
    inputSchema: { type: "object", properties: { id: { type: "string" } }, required: ["id"] },
  },
  { name: "list_my_modules", description: "Модули пользователя: черновики с результатом проверки и опубликованные.", inputSchema: { type: "object", properties: {} } },
  {
    name: "publish_module",
    description: "Опубликовать прошедший проверку модуль в библиотеке NOAH от имени пользователя. Спроси его перед публикацией.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string" },
        description: { type: "string", description: "Описание для страницы модуля, 1–3 абзаца" },
        category: { type: "string", enum: ["work", "home", "finance", "dev", "health", "media", "other"] },
      },
      required: ["id"],
    },
  },
];

/** Страница по адресу подключения — для тех, кто открыл его как сайт. */
const MCP_PAGE = (site) => `<!doctype html><html lang="ru"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>Адрес подключения NOAH</title>
<link rel="stylesheet" href="/styles.css">
<style>body{font-family:system-ui,sans-serif}.mcp a{color:#2b5bc4;text-decoration:underline}.mcp{max-width:640px;margin:8vh auto;padding:28px}.mcp h1{margin:0 0 14px}.mcp code{font-size:15px}.mcp ol{padding-left:20px;line-height:1.6}</style>
</head><body><main class="mcp plate">
<h1>Это адрес подключения, а не страница</h1>
<p><code>${site}/mcp</code> — MCP-сервер NOAH. Его не открывают и не присылают в чат: его вставляют в настройки нейросети, и она получает инструменты NOAH.</p>
<ol>
<li><b>Claude</b> (сайт и приложение): Настройки → Коннекторы → Добавить свой коннектор → вставьте адрес.</li>
<li><b>ChatGPT</b>: Настройки → Коннекторы → Дополнительно → режим разработчика → Создать → вставьте адрес.</li>
<li><b>Cursor</b>: Settings → MCP → Add new MCP server → тип «streamable http» → вставьте адрес.</li>
<li>Любая другая нейросеть с поддержкой MCP-серверов по адресу (Streamable HTTP).</li>
</ol>
<p>При подключении нейросеть один раз откроет вход в NOAH — войдите или создайте аккаунт. Если в вашей нейросети нет раздела про коннекторы или MCP, подключить NOAH к ней нельзя: чат умеет только читать страницы.</p>
<p><a href="/#/connect">Подробнее — на странице «Подключить ИИ»</a></p>
</main></body></html>`;

export function mountRemote({ route, db, Fail, readJson, userForKey, publishModule, lint, validId, send, maxBody, publicUrl }) {
  // Регламент для раздела документации на сайте — тот же текст, что читает нейросеть.
  route("GET", /^\/api\/docs\/standard$/, () => ({ text: standardText() }));

  db.exec(`
    CREATE TABLE IF NOT EXISTS drafts (
      id TEXT PRIMARY KEY,
      user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      module_id TEXT NOT NULL,
      manifest TEXT NOT NULL,
      files TEXT NOT NULL,
      status TEXT NOT NULL DEFAULT 'pending',
      report TEXT NOT NULL DEFAULT '',
      tools TEXT NOT NULL DEFAULT '[]',
      taken TEXT,
      created TEXT NOT NULL DEFAULT (datetime('now')),
      updated TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS drafts_user ON drafts(user_id, status);
    CREATE TABLE IF NOT EXISTS devices (
      user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
      env TEXT NOT NULL DEFAULT '',
      seen INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE IF NOT EXISTS course_jobs (
      id TEXT PRIMARY KEY,
      user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      course_id TEXT NOT NULL,
      kind TEXT NOT NULL,
      payload TEXT NOT NULL,
      status TEXT NOT NULL DEFAULT 'pending',
      report TEXT NOT NULL DEFAULT '',
      taken TEXT,
      created TEXT NOT NULL DEFAULT (datetime('now')),
      updated TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS course_jobs_user ON course_jobs(user_id, status);
  `);
  // Список курсов с компьютера пользователя — для list_courses.
  if (!db.prepare("SELECT name FROM pragma_table_info('devices') WHERE name = 'courses'").get()) {
    db.exec("ALTER TABLE devices ADD COLUMN courses TEXT NOT NULL DEFAULT ''");
  }

  const appUser = (req) => {
    const match = /^Bearer\s+(\S+)$/.exec(String(req.headers.authorization ?? ""));
    const user = match ? userForKey(match[1]) : null;
    if (!user) throw new Fail(401, "Ключ площадки не подошёл.");
    return user;
  };
  const touch = (userId, env) => {
    db.prepare(
      `INSERT INTO devices (user_id, env, seen) VALUES (?, ?, ?)
       ON CONFLICT(user_id) DO UPDATE SET seen = excluded.seen, env = CASE WHEN excluded.env = '' THEN devices.env ELSE excluded.env END`,
    ).run(userId, env ?? "", Date.now());
  };

  /* ── Ноа на компьютере ─────────────────────────────────────────────────── */

  route("POST", /^\/api\/app\/hello$/, async ({ req }) => {
    const user = appUser(req);
    const { env, courses } = await readJson(req);
    touch(user.id, String(env ?? "").slice(0, 2000));
    if (typeof courses === "string") {
      db.prepare("UPDATE devices SET courses = ? WHERE user_id = ?").run(courses.slice(0, 20000), user.id);
    }
    return { name: user.name };
  });

  // Курсы: список ждущих, затем каждый кусками, затем отчёт.
  route("GET", /^\/api\/app\/course-jobs$/, ({ req }) => {
    const user = appUser(req);
    touch(user.id, "");
    const stuckBefore = new Date(Date.now() - STUCK_MS).toISOString();
    db.prepare("UPDATE course_jobs SET status = 'pending' WHERE user_id = ? AND status = 'checking' AND taken < ?").run(user.id, stuckBefore);
    const rows = db
      .prepare("SELECT id, course_id, kind, length(payload) AS size FROM course_jobs WHERE user_id = ? AND status = 'pending' ORDER BY created LIMIT 2")
      .all(user.id);
    const take = db.prepare("UPDATE course_jobs SET status = 'checking', taken = ?, updated = datetime('now') WHERE id = ?");
    for (const row of rows) take.run(new Date().toISOString(), row.id);
    return { jobs: rows.map((row) => ({ id: row.id, course: row.course_id, kind: row.kind, size: row.size, part: PART_CHARS })) };
  });

  route("GET", /^\/api\/app\/course-jobs\/([0-9a-f-]{36})\/part$/, ({ req, match, url }) => {
    const user = appUser(req);
    const row = db.prepare("SELECT payload FROM course_jobs WHERE id = ? AND user_id = ?").get(match[1], user.id);
    if (!row) throw new Fail(404, "Нет такого курса.");
    const at = Math.max(0, Number(url.searchParams.get("at") ?? 0) | 0);
    return { text: row.payload.slice(at, at + PART_CHARS), size: row.payload.length };
  });

  route("POST", /^\/api\/app\/course-jobs\/([0-9a-f-]{36})\/report$/, async ({ req, match }) => {
    const user = appUser(req);
    const { ok, report } = await readJson(req);
    const info = db
      .prepare("UPDATE course_jobs SET status = ?, report = ?, updated = datetime('now') WHERE id = ? AND user_id = ?")
      .run(ok ? "passed" : "failed", String(report ?? "").slice(0, 20000), match[1], user.id);
    if (!info.changes) throw new Fail(404, "Нет такого курса.");
    return { ok: true };
  });

  route("GET", /^\/api\/app\/drafts$/, ({ req }) => {
    const user = appUser(req);
    touch(user.id, "");
    const stuckBefore = new Date(Date.now() - STUCK_MS).toISOString();
    db.prepare("UPDATE drafts SET status = 'pending' WHERE user_id = ? AND status = 'checking' AND taken < ?").run(user.id, stuckBefore);
    const rows = db.prepare("SELECT id, manifest, files FROM drafts WHERE user_id = ? AND status = 'pending' ORDER BY created LIMIT 3").all(user.id);
    const take = db.prepare("UPDATE drafts SET status = 'checking', taken = ?, updated = datetime('now') WHERE id = ?");
    for (const row of rows) take.run(new Date().toISOString(), row.id);
    return { drafts: rows.map((row) => ({ id: row.id, manifest: JSON.parse(row.manifest), files: JSON.parse(row.files) })) };
  });

  route("POST", /^\/api\/app\/drafts\/([0-9a-f-]{36})\/report$/, async ({ req, match }) => {
    const user = appUser(req);
    const { ok, report, tools } = await readJson(req);
    const info = db
      .prepare("UPDATE drafts SET status = ?, report = ?, tools = ?, updated = datetime('now') WHERE id = ? AND user_id = ?")
      .run(ok ? "passed" : "failed", String(report ?? "").slice(0, 20000), JSON.stringify(Array.isArray(tools) ? tools.slice(0, 25) : []), match[1], user.id);
    if (!info.changes) throw new Fail(404, "Нет такого черновика.");
    return { ok: true };
  });

  /** Для страницы «Подключить ИИ»: на связи ли NOAH этого человека. */
  route("GET", /^\/api\/my\/device$/, ({ user }) => {
    if (!user) throw new Fail(401, "Войдите.");
    const row = db.prepare("SELECT seen FROM devices WHERE user_id = ?").get(user.id);
    const drafts = db.prepare("SELECT COUNT(*) AS n FROM drafts WHERE user_id = ? AND status = 'passed'").get(user.id).n;
    return { seen: row?.seen ?? 0, online: Boolean(row && Date.now() - row.seen < ONLINE_MS), built: drafts };
  });

  /* ── Инструменты MCP ───────────────────────────────────────────────────── */

  const online = (user) => {
    const row = db.prepare("SELECT env, seen, courses FROM devices WHERE user_id = ?").get(user.id);
    return { row, on: Boolean(row && Date.now() - row.seen < ONLINE_MS) };
  };
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const offlineHint =
    "NOAH на компьютере пользователя сейчас не на связи. Попроси его запустить NOAH и вставить ключ площадки " +
    "в NOAH → Настройки → Площадка (ключ — в кабинете на сайте). Черновик сохранён: NOAH проверит его, как только " +
    "появится на связи, — тогда вызови module_status.";

  /** Ставит курс или тему в очередь и ждёт отчёта Ноа. */
  const courseJob = async (user, courseId, kind, payload) => {
    const text = JSON.stringify(payload);
    if (text.length > MAX_COURSE_CHARS) throw new Error("Курс слишком большой — отправь первые темы create_course, остальные add_topic.");
    const id = randomUUID();
    // Новый курс целиком заменяет ещё не взятый прежний — темы к нему остаются.
    if (kind === "course") {
      db.prepare("UPDATE course_jobs SET status = 'replaced' WHERE user_id = ? AND course_id = ? AND kind = 'course' AND status = 'pending'").run(
        user.id,
        courseId,
      );
    }
    db.prepare("INSERT INTO course_jobs (id, user_id, course_id, kind, payload) VALUES (?, ?, ?, ?, ?)").run(id, user.id, courseId, kind, text);
    if (!online(user).on) {
      return offlineHint.replace("Черновик сохранён", "Курс сохранён").replace("module_status", "course_status");
    }
    // Большой курс Ноа забирает кусками — ждём дольше, чем модуль.
    const deadline = Date.now() + WAIT_MS + Math.min(120_000, (text.length / PART_CHARS) * 400);
    while (Date.now() < deadline) {
      await sleep(1500);
      const row = db.prepare("SELECT status, report FROM course_jobs WHERE id = ?").get(id);
      if (row.status === "passed") return row.report;
      if (row.status === "failed") throw new Error(row.report);
    }
    return `NOAH ещё принимает курс «${courseId}». Вызови course_status("${courseId}") через минуту.`;
  };
  const courseId = (value) => {
    const id = String(value ?? "").trim();
    if (!/^[A-Za-z0-9_-]{1,64}$/.test(id)) throw new Error("id курса — латиница, цифры, дефис или подчёркивание.");
    return id;
  };

  const tools = {
    module_format: () => standardText(),

    course_format: () => courseFormatText(),

    create_course: (user, args) => {
      const course = args.course;
      if (!course || typeof course !== "object" || Array.isArray(course)) throw new Error("course — объект курса по course_format.");
      if (!Array.isArray(course.topics) || !course.topics.length) throw new Error("В курсе нет тем (topics). Сначала прочитай course_format.");
      return courseJob(user, courseId(course.id), "course", course);
    },

    add_topic: (user, args) => {
      const topic = args.topic;
      if (!topic || typeof topic !== "object" || Array.isArray(topic)) throw new Error("topic — объект темы по course_format.");
      const id = courseId(args.course);
      return courseJob(user, id, "topic", { course: id, topic });
    },

    list_courses: (user) => {
      const { row, on } = online(user);
      const list = row?.courses || "";
      if (!row) return `NOAH ещё ни разу не подключался к этому аккаунту.\n${offlineHint}`;
      return `${on ? "" : "NOAH сейчас не на связи — список на момент последней связи.\n"}${list || "Курсов пока нет."}`;
    },

    course_status: (user, args) => {
      const row = db
        .prepare("SELECT status, report FROM course_jobs WHERE user_id = ? AND course_id = ? AND status != 'replaced' ORDER BY created DESC LIMIT 1")
        .get(user.id, String(args.id ?? ""));
      if (!row) return `Курс «${args.id}» сюда не отправляли.`;
      const state = { pending: "ждёт, пока NOAH его заберёт", checking: "NOAH проверяет", passed: "принят", failed: "не принят" }[row.status];
      return `Курс «${args.id}»: ${state}.${row.report ? `\n\n${row.report}` : ""}`;
    },

    environment: (user) => {
      const { row, on } = online(user);
      if (!row) return `NOAH ещё ни разу не подключался к этому аккаунту.\n${offlineHint}`;
      return `${on ? "NOAH на связи." : "NOAH сейчас не на связи."}\n${row.env || "Сведений об установленном пока нет."}`;
    },

    create_module: async (user, args) => {
      const manifest = args.module;
      const files = args.files ?? {};
      if (!manifest || typeof manifest !== "object") throw new Error("module — объект module.json.");
      if (typeof files !== "object" || Array.isArray(files)) throw new Error("files — объект «имя файла → содержимое».");
      const problems = lint(manifest, files);
      if (problems.length) {
        throw new Error(`Модуль не принят — нарушен регламент (module_format):\n${problems.map((p) => `  ✗ ${p}`).join("\n")}`);
      }
      const size = Buffer.byteLength(JSON.stringify(files));
      if (size > maxBody) throw new Error("Файлы модуля слишком большие.");
      const id = randomUUID();
      db.prepare("UPDATE drafts SET status = 'replaced' WHERE user_id = ? AND module_id = ? AND status = 'pending'").run(user.id, manifest.id);
      db.prepare("INSERT INTO drafts (id, user_id, module_id, manifest, files) VALUES (?, ?, ?, ?, ?)").run(
        id,
        user.id,
        manifest.id,
        JSON.stringify(manifest),
        JSON.stringify(files),
      );
      if (!online(user).on) return offlineHint;
      const deadline = Date.now() + WAIT_MS;
      while (Date.now() < deadline) {
        await sleep(1500);
        const row = db.prepare("SELECT status, report FROM drafts WHERE id = ?").get(id);
        if (row.status === "passed") return row.report;
        if (row.status === "failed") throw new Error(row.report);
      }
      return `NOAH ещё проверяет модуль «${manifest.id}». Вызови module_status("${manifest.id}") через минуту.`;
    },

    module_status: (user, args) => {
      const row = db
        .prepare("SELECT status, report FROM drafts WHERE user_id = ? AND module_id = ? AND status != 'replaced' ORDER BY created DESC LIMIT 1")
        .get(user.id, String(args.id ?? ""));
      if (!row) return `Черновика «${args.id}» нет.`;
      const state = { pending: "ждёт, пока NOAH его заберёт", checking: "NOAH проверяет", passed: "прошёл проверку", failed: "не прошёл проверку" }[row.status];
      return `Модуль «${args.id}»: ${state}.${row.report ? `\n\n${row.report}` : ""}`;
    },

    search_modules: (_user, args) => {
      const q = String(args.query ?? "").trim().toLowerCase();
      const rows = db.prepare("SELECT id, title, about, author, installs FROM modules WHERE hidden = 0 ORDER BY installs DESC").all();
      const found = rows.filter((row) => !q || `${row.title} ${row.about} ${row.id}`.toLowerCase().includes(q)).slice(0, 20);
      if (!found.length) return "В библиотеке ничего не нашлось — можно собрать свой модуль.";
      return found.map((m) => `${m.title} (id: ${m.id}) — ${m.about}; автор ${m.author}, установок ${m.installs}`).join("\n");
    },

    list_my_modules: (user) => {
      const drafts = db
        .prepare(
          `SELECT module_id, status, MAX(created) AS created FROM drafts WHERE user_id = ? AND status != 'replaced'
           GROUP BY module_id ORDER BY created DESC LIMIT 30`,
        )
        .all(user.id);
      const published = db.prepare("SELECT id, title, installs FROM modules WHERE owner_id = ?").all(user.id);
      const lines = [];
      if (drafts.length) lines.push("Черновики:", ...drafts.map((d) => `  ${d.module_id} — ${d.status}`));
      if (published.length) lines.push("Опубликованы:", ...published.map((m) => `  ${m.title} (${m.id}) — установок ${m.installs}`));
      return lines.length ? lines.join("\n") : "Модулей пока нет.";
    },

    publish_module: (user, args) => {
      const row = db
        .prepare("SELECT manifest, files, tools, status FROM drafts WHERE user_id = ? AND module_id = ? AND status != 'replaced' ORDER BY created DESC LIMIT 1")
        .get(user.id, String(args.id ?? ""));
      if (!row) throw new Error(`Черновика «${args.id}» нет — сначала create_module.`);
      if (row.status !== "passed") throw new Error("Публикуется только модуль, прошедший проверку NOAH.");
      const result = publishModule(user, {
        manifest: JSON.parse(row.manifest),
        files: JSON.parse(row.files),
        tools: JSON.parse(row.tools),
        description: args.description,
        category: args.category,
      });
      return `Модуль опубликован в библиотеке NOAH: ${result.url}`;
    },
  };

  /* ── Протокол ──────────────────────────────────────────────────────────── */

  async function handle(message, user) {
    const { id, method, params = {} } = message ?? {};
    const reply = (result) => ({ jsonrpc: "2.0", id, result });
    const fail = (code, text) => ({ jsonrpc: "2.0", id, error: { code, message: text } });
    if (id === undefined || id === null) return null;
    switch (method) {
      case "initialize":
        return reply({
          protocolVersion: params.protocolVersion ?? PROTOCOL,
          capabilities: { tools: {} },
          serverInfo: { name: "noah-platform", version: "1.0.0" },
          instructions: INSTRUCTIONS,
        });
      case "ping":
        return reply({});
      case "tools/list":
        return reply({ tools: TOOLS });
      case "tools/call": {
        const tool = tools[params.name];
        if (!tool) return reply({ content: [{ type: "text", text: `Нет инструмента «${params.name}».` }], isError: true });
        try {
          const text = await tool(user, params.arguments ?? {});
          return reply({ content: [{ type: "text", text: String(text) }] });
        } catch (err) {
          return reply({ content: [{ type: "text", text: err instanceof Fail ? err.message : String(err.message ?? err) }], isError: true });
        }
      }
      default:
        return fail(-32601, "нет такого метода");
    }
  }

  return async function mcp(req, res, url) {
    const headers = { "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "content-type, authorization, mcp-session-id, mcp-protocol-version", "Access-Control-Allow-Methods": "POST, GET, OPTIONS", "Access-Control-Expose-Headers": "WWW-Authenticate, mcp-session-id" };
    if (req.method === "OPTIONS") {
      res.writeHead(204, headers);
      return res.end();
    }
    const bearer0 = /^Bearer\s+(\S+)$/.exec(String(req.headers.authorization ?? ""))?.[1];
    // Ключ в ссылке — прежний способ, вход OAuth — нынешний. Устаревший ключ в
    // ссылке не должен перебивать рабочий токен входа: пробуем оба.
    const who = () => userForKey(url.searchParams.get("key")) ?? userForKey(bearer0);
    const denied = { ...headers, "Content-Type": "application/json", "WWW-Authenticate": authChallenge(publicUrl) };
    // GET — необязательный по протоколу поток для сообщений от сервера. Мы их
    // не шлём — ответы и так уходят прямо на POST, — но часть клиентов (Qwen
    // и другие) открывает этот поток первым делом и обрывается на отказе.
    // Держим его молча открытым, чтобы не мешать таким клиентам.
    if (req.method === "GET") {
      // Адрес открыли как страницу — человек в браузере или нейросеть, которая
      // «читает сайт» вместо того, чтобы подключить его. Клиент MCP просит
      // поток событий; всем остальным — объяснение, куда этот адрес вставлять.
      if (!String(req.headers.accept ?? "").includes("text/event-stream")) {
        res.writeHead(200, { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" });
        return res.end(MCP_PAGE(publicUrl));
      }
      if (!who()) {
        res.writeHead(401, denied);
        return res.end(JSON.stringify({ error: "Нужен вход: подключите NOAH в нейросети по адресу …/mcp — она сама откроет вход на сайте" }));
      }
      res.writeHead(200, { ...headers, "Content-Type": "text/event-stream", "Cache-Control": "no-cache", Connection: "keep-alive" });
      res.write(": открыт\n\n");
      const ping = setInterval(() => res.write(": ping\n\n"), 20_000);
      req.on("close", () => clearInterval(ping));
      return;
    }
    if (req.method !== "POST") {
      res.writeHead(405, { ...headers, Allow: "POST, GET", "Content-Type": "application/json" });
      return res.end(JSON.stringify({ error: "MCP принимает POST и GET." }));
    }
    const user = who();
    let body;
    try {
      body = await readJson(req);
    } catch (err) {
      res.writeHead(400, { ...headers, "Content-Type": "application/json" });
      return res.end(JSON.stringify({ jsonrpc: "2.0", id: null, error: { code: -32700, message: err.message } }));
    }
    if (!user) {
      res.writeHead(401, denied);
      return res.end(
        JSON.stringify({ jsonrpc: "2.0", id: body?.id ?? null, error: { code: -32001, message: "Нужен вход: подключите NOAH в нейросети по адресу …/mcp — она сама откроет вход на сайте" } }),
      );
    }
    const batch = Array.isArray(body);
    const answers = (await Promise.all((batch ? body : [body]).map((message) => handle(message, user)))).filter(Boolean);
    if (!answers.length) {
      res.writeHead(202, headers);
      return res.end();
    }
    for (const [key, value] of Object.entries(headers)) res.setHeader(key, value);
    return send(res, 200, batch ? answers : answers[0]);
  };
}
