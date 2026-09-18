// Площадка NOAH: библиотека модулей, аккаунты, публикация из Ноа.
//
// Без зависимостей: node:http, node:sqlite, node:crypto. Код модулей сервер не
// запускает никогда — модуль проверяет Ноа у автора перед публикацией и Ноа у
// каждого пользователя перед установкой.

import http from "node:http";
import { readFile, stat } from "node:fs/promises";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash, randomBytes, scrypt as scryptCb, timingSafeEqual } from "node:crypto";
import { promisify } from "node:util";
import { brotliCompressSync, constants as zlibConstants, gzipSync } from "node:zlib";
import { DatabaseSync } from "node:sqlite";

import { CATEGORIES, FORMAT, lint, validId } from "./standard.mjs";
import { mountRemote } from "./remote.mjs";
import { mountOAuth } from "./oauth.mjs";

const scrypt = promisify(scryptCb);
const HERE = dirname(fileURLToPath(import.meta.url));
const WEB = normalize(join(HERE, "..", "web"));
const PORT = Number(process.env.NOAH_PORT ?? 8795);
const HOST = process.env.NOAH_HOST ?? "0.0.0.0";
const DATA = process.env.NOAH_DATA ?? join(HERE, "..", "data");
const SEED = process.env.NOAH_SEED ?? join(HERE, "..", "..", "modules", "index.json");
const BUILTIN = process.env.NOAH_BUILTIN ?? join(HERE, "..", "..", "modules", "builtin.json");
const SECURE = process.env.NOAH_SECURE === "1";
const SESSION_DAYS = 30;
const MAX_BODY = 8 * 1024 * 1024;

mkdirSync(DATA, { recursive: true });
const db = new DatabaseSync(join(DATA, "noah.db"));
db.exec(`
  PRAGMA journal_mode = WAL;
  PRAGMA foreign_keys = ON;
  CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY,
    email TEXT NOT NULL UNIQUE COLLATE NOCASE,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    pass TEXT NOT NULL,
    created TEXT NOT NULL DEFAULT (datetime('now'))
  );
  CREATE TABLE IF NOT EXISTS sessions (
    hash TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires TEXT NOT NULL
  );
  CREATE TABLE IF NOT EXISTS tokens (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    hash TEXT NOT NULL UNIQUE,
    created TEXT NOT NULL DEFAULT (datetime('now')),
    used TEXT
  );
  CREATE TABLE IF NOT EXISTS modules (
    id TEXT PRIMARY KEY,
    owner_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    author TEXT NOT NULL,
    title TEXT NOT NULL,
    icon TEXT NOT NULL,
    about TEXT NOT NULL,
    voice TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'other',
    version TEXT NOT NULL DEFAULT '1.0.0',
    description TEXT NOT NULL DEFAULT '',
    manifest TEXT NOT NULL,
    files TEXT NOT NULL DEFAULT '{}',
    tools TEXT NOT NULL DEFAULT '[]',
    installs INTEGER NOT NULL DEFAULT 0,
    hidden INTEGER NOT NULL DEFAULT 0,
    created TEXT NOT NULL DEFAULT (datetime('now')),
    updated TEXT NOT NULL DEFAULT (datetime('now'))
  );
`);

/* ── Библиотека ядра ─────────────────────────────────────────────────────── */

const CORE_CATEGORY = { memory: "work", files: "work", fetch: "work", browser: "work", docs: "dev", thinking: "work" };
/** Модули ядра, которым не нужна сеть. */
const CORE_LOCAL = ["memory", "files", "thinking"];
function seed() {
  if (!existsSync(SEED)) return;
  const body = JSON.parse(readFileSync(SEED, "utf8"));
  const upsert = db.prepare(`
    INSERT INTO modules (id, owner_id, author, title, icon, about, voice, category, manifest)
    VALUES (?, NULL, 'noah-core', ?, ?, ?, ?, ?, ?)
    ON CONFLICT(id) DO UPDATE SET title = excluded.title, icon = excluded.icon, about = excluded.about,
      voice = excluded.voice, manifest = excluded.manifest, updated = datetime('now')
    WHERE modules.owner_id IS NULL`);
  for (const m of body.modules ?? []) {
    if (!validId(m.id)) continue;
    const manifest = { ...m, local: CORE_LOCAL.includes(m.id) };
    upsert.run(m.id, m.title, m.icon, m.about, m.voice, CORE_CATEGORY[m.id] ?? "other", JSON.stringify(manifest));
  }
}
seed();

/** Модули, встроенные в само приложение: в библиотеке для обзора, ставятся вместе с NOAH. */
function seedBuiltin() {
  if (!existsSync(BUILTIN)) return;
  const body = JSON.parse(readFileSync(BUILTIN, "utf8"));
  const upsert = db.prepare(`
    INSERT INTO modules (id, owner_id, author, title, icon, about, voice, category, description, manifest, tools)
    VALUES (?, NULL, 'noah', ?, ?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(id) DO UPDATE SET title = excluded.title, icon = excluded.icon, about = excluded.about,
      voice = excluded.voice, category = excluded.category, description = excluded.description,
      manifest = excluded.manifest, tools = excluded.tools, updated = datetime('now')
    WHERE modules.owner_id IS NULL`);
  for (const m of body.modules ?? []) {
    if (!validId(m.id)) continue;
    const manifest = { id: m.id, title: m.title, icon: m.icon, about: m.about, voice: m.voice, builtin: true, local: true };
    upsert.run(m.id, m.title, m.icon, m.about, m.voice, m.category ?? "other", m.description ?? "", JSON.stringify(manifest), JSON.stringify(m.tools ?? []));
  }
}
seedBuiltin();

/* ── Пароли, сессии, токены ──────────────────────────────────────────────── */

const sha = (text) => createHash("sha256").update(text).digest("hex");

async function hashPassword(password) {
  const salt = randomBytes(16);
  const key = await scrypt(password, salt, 64, { N: 16384, r: 8, p: 1 });
  return `scrypt$${salt.toString("base64")}$${key.toString("base64")}`;
}

async function checkPassword(password, stored) {
  const [kind, salt, key] = String(stored).split("$");
  if (kind !== "scrypt") return false;
  const expected = Buffer.from(key, "base64");
  const actual = await scrypt(password, Buffer.from(salt, "base64"), expected.length, { N: 16384, r: 8, p: 1 });
  return timingSafeEqual(expected, actual);
}

function cookies(req) {
  const out = {};
  for (const part of String(req.headers.cookie ?? "").split(";")) {
    const at = part.indexOf("=");
    if (at > 0) out[part.slice(0, at).trim()] = decodeURIComponent(part.slice(at + 1).trim());
  }
  return out;
}

function sessionCookie(value, maxAge) {
  return [`noah_session=${value}`, "Path=/", "HttpOnly", "SameSite=Lax", `Max-Age=${maxAge}`, SECURE ? "Secure" : ""]
    .filter(Boolean)
    .join("; ");
}

function openSession(userId) {
  const token = randomBytes(32).toString("base64url");
  const expires = new Date(Date.now() + SESSION_DAYS * 86400e3).toISOString();
  db.prepare("INSERT INTO sessions (hash, user_id, expires) VALUES (?, ?, ?)").run(sha(token), userId, expires);
  return sessionCookie(token, SESSION_DAYS * 86400);
}

function sessionUser(req) {
  const token = cookies(req).noah_session;
  if (!token) return null;
  return (
    db
      .prepare(
        `SELECT users.id, users.email, users.name FROM sessions JOIN users ON users.id = sessions.user_id
         WHERE sessions.hash = ? AND sessions.expires > ?`,
      )
      .get(sha(token), new Date().toISOString()) ?? null
  );
}

function tokenUser(req) {
  const match = /^Bearer\s+(noah_[A-Za-z0-9_-]{20,})$/.exec(String(req.headers.authorization ?? ""));
  return match ? userForKey(match[1]) : null;
}

/** Владелец ключа площадки: `noah_…`. */
function userForKey(key) {
  if (!/^noah_[A-Za-z0-9_-]{20,}$/.test(String(key ?? ""))) return null;
  const row = db
    .prepare("SELECT users.id, users.email, users.name, tokens.id AS token_id FROM tokens JOIN users ON users.id = tokens.user_id WHERE tokens.hash = ?")
    .get(sha(key));
  if (row) db.prepare("UPDATE tokens SET used = datetime('now') WHERE id = ?").run(row.token_id);
  return row ?? null;
}

/** Попытки входа: не больше 8 за 10 минут с одного адреса. */
const attempts = new Map();
function throttled(ip) {
  const now = Date.now();
  const list = (attempts.get(ip) ?? []).filter((at) => now - at < 600e3);
  list.push(now);
  attempts.set(ip, list);
  return list.length > 8;
}

/* ── Ответы ─────────────────────────────────────────────────────────────── */

const SECURITY_HEADERS = {
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "same-origin",
  "X-Frame-Options": "DENY",
  "Content-Security-Policy":
    "default-src 'self'; style-src 'self' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
};

class Fail extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

/** Сжатые файлы сайта: жмутся один раз, пока файл не изменился. */
const packCache = new Map();

/**
 * Сжатие текста: ответы меньше и быстрее доходят через медленные сети. Brotli
 * — где браузер его понимает: на треть меньше gzip, а из части сетей
 * соединение с сервером замирает после ~16 КБ, и каждый килобайт на счету.
 * `key` — файл и время его изменения, чтобы не жать одно и то же заново.
 */
function packed(req, body, key) {
  const accepts = String(req.headers["accept-encoding"] ?? "");
  const encoding = /(^|[\s,])br(;|,|$)/.test(accepts) ? "br" : /gzip/.test(accepts) ? "gzip" : "";
  if (!encoding || body.length < 1024) return { body, headers: {} };
  const cacheKey = key && `${encoding}:${key}`;
  let out = cacheKey && packCache.get(cacheKey);
  if (!out) {
    out =
      encoding === "br"
        ? brotliCompressSync(body, { params: { [zlibConstants.BROTLI_PARAM_QUALITY]: key ? 11 : 5 } })
        : gzipSync(body, { level: 9 });
    if (cacheKey) packCache.set(cacheKey, out);
  }
  return { body: out, headers: { "Content-Encoding": encoding, Vary: "Accept-Encoding" } };
}

function send(res, status, body, headers = {}) {
  const out = packed(res.req, Buffer.from(JSON.stringify(body)));
  res.writeHead(status, {
    ...SECURITY_HEADERS,
    "Content-Type": "application/json; charset=utf-8",
    "Cache-Control": "no-store",
    "Content-Length": out.body.length,
    ...out.headers,
    ...headers,
  });
  res.end(out.body);
}

async function readJson(req) {
  if (!String(req.headers["content-type"] ?? "").startsWith("application/json")) throw new Fail(415, "Нужен JSON.");
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > MAX_BODY) throw new Fail(413, "Слишком большой запрос.");
    chunks.push(chunk);
  }
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}");
  } catch {
    throw new Fail(400, "Запрос не разобрался.");
  }
}

/** Изменяющие запросы из браузера — только со своей страницы. */
function sameOrigin(req) {
  const origin = req.headers.origin;
  if (!origin) return true;
  try {
    return new URL(origin).host === req.headers.host;
  } catch {
    return false;
  }
}

function card(row) {
  return {
    id: row.id,
    title: row.title,
    icon: row.icon,
    about: row.about,
    voice: row.voice,
    author: row.author,
    category: row.category,
    version: row.version,
    installs: row.installs,
    updated: row.updated,
    core: row.owner_id === null,
    builtin: row.author === "noah" && row.owner_id === null,
  };
}

/* ── Маршруты ───────────────────────────────────────────────────────────── */

const routes = [];
const route = (method, pattern, handler) => routes.push({ method, pattern, handler });

route("GET", /^\/api\/health$/, () => ({ status: "ok" }));

route("GET", /^\/api\/me$/, ({ user }) => {
  if (!user) return { user: null };
  const row = db.prepare("SELECT pass FROM users WHERE id = ?").get(user.id);
  return { user: { ...user, hasPassword: String(row?.pass).startsWith("scrypt$") } };
});

route("POST", /^\/api\/auth\/register$/, async ({ req, res, ip }) => {
  if (throttled(ip)) throw new Fail(429, "Слишком много попыток. Подождите несколько минут.");
  const { email, name, password } = await readJson(req);
  if (!/^[^\s@]{1,64}@[^\s@]{1,190}\.[^\s@]{2,}$/.test(String(email ?? ""))) throw new Fail(400, "Проверьте почту.");
  if (!/^[a-z0-9][a-z0-9._-]{2,23}$/i.test(String(name ?? ""))) throw new Fail(400, "Имя автора — 3–24 знака: латиница, цифры, точка, дефис.");
  if (String(password ?? "").length < 10) throw new Fail(400, "Пароль — от 10 знаков.");
  if (String(name).toLowerCase() === "noah-core") throw new Fail(400, "Это имя занято.");
  const taken = db.prepare("SELECT email, name FROM users WHERE email = ? OR name = ?").get(email, name);
  if (taken) throw new Fail(409, taken.email.toLowerCase() === String(email).toLowerCase() ? "Эта почта уже зарегистрирована." : "Это имя уже занято.");
  const info = db.prepare("INSERT INTO users (email, name, pass) VALUES (?, ?, ?)").run(email.trim(), name.trim(), await hashPassword(password));
  res.setHeader("Set-Cookie", openSession(Number(info.lastInsertRowid)));
  return { user: { id: Number(info.lastInsertRowid), email: email.trim(), name: name.trim() } };
});

route("POST", /^\/api\/auth\/login$/, async ({ req, res, ip }) => {
  if (throttled(ip)) throw new Fail(429, "Слишком много попыток. Подождите несколько минут.");
  const { email, password } = await readJson(req);
  const row = db.prepare("SELECT id, email, name, pass FROM users WHERE email = ?").get(String(email ?? "").trim());
  // Проверка идёт и для несуществующей почты: время ответа не выдаёт, есть ли аккаунт.
  const ok = await checkPassword(String(password ?? ""), row?.pass ?? "scrypt$AAAAAAAAAAAAAAAAAAAAAA==$" + "A".repeat(86) + "==");
  if (!row || !ok) throw new Fail(401, "Неверная почта или пароль.");
  res.setHeader("Set-Cookie", openSession(row.id));
  return { user: { id: row.id, email: row.email, name: row.name } };
});

route("POST", /^\/api\/auth\/logout$/, ({ req, res }) => {
  const token = cookies(req).noah_session;
  if (token) db.prepare("DELETE FROM sessions WHERE hash = ?").run(sha(token));
  res.setHeader("Set-Cookie", sessionCookie("", 0));
  return { ok: true };
});

route("POST", /^\/api\/account\/password$/, async ({ req, user, ip }) => {
  if (!user) throw new Fail(401, "Войдите.");
  if (throttled(ip)) throw new Fail(429, "Слишком много попыток. Подождите несколько минут.");
  const { current, next } = await readJson(req);
  const row = db.prepare("SELECT pass FROM users WHERE id = ?").get(user.id);
  const hasPassword = String(row.pass).startsWith("scrypt$");
  if (hasPassword && !(await checkPassword(String(current ?? ""), row.pass))) throw new Fail(403, "Текущий пароль не подходит.");
  if (String(next ?? "").length < 10) throw new Fail(400, "Пароль — от 10 знаков.");
  db.prepare("UPDATE users SET pass = ? WHERE id = ?").run(await hashPassword(next), user.id);
  return { ok: true };
});

route("POST", /^\/api\/account\/logout-others$/, ({ req, user }) => {
  if (!user) throw new Fail(401, "Войдите.");
  const token = cookies(req).noah_session;
  db.prepare("DELETE FROM sessions WHERE user_id = ? AND hash != ?").run(user.id, sha(token ?? ""));
  return { ok: true };
});

route("DELETE", /^\/api\/account$/, async ({ req, res, user, ip }) => {
  if (!user) throw new Fail(401, "Войдите.");
  if (throttled(ip)) throw new Fail(429, "Слишком много попыток. Подождите несколько минут.");
  const { password } = await readJson(req);
  const row = db.prepare("SELECT pass FROM users WHERE id = ?").get(user.id);
  const hasPassword = String(row.pass).startsWith("scrypt$");
  if (hasPassword && !(await checkPassword(String(password ?? ""), row.pass))) throw new Fail(403, "Пароль не подходит.");
  db.prepare("DELETE FROM modules WHERE owner_id = ?").run(user.id);
  db.prepare("DELETE FROM users WHERE id = ?").run(user.id);
  res.setHeader("Set-Cookie", sessionCookie("", 0));
  return { ok: true };
});

route("GET", /^\/api\/stats$/, () => {
  const row = db
    .prepare("SELECT COUNT(*) AS modules, COUNT(DISTINCT author) AS authors, COALESCE(SUM(installs), 0) AS installs FROM modules WHERE hidden = 0")
    .get();
  // Мозги — источники, с которыми работает NOAH: Ollama, OpenRouter, Google AI Studio, Groq и любой OpenAI-совместимый.
  return { ...row, brains: 5, users: db.prepare("SELECT COUNT(*) AS n FROM users").get().n };
});

route("GET", /^\/api\/modules$/, ({ url }) => {
  const q = String(url.searchParams.get("q") ?? "").trim().toLowerCase();
  const category = url.searchParams.get("category");
  const sortKey = url.searchParams.get("sort");
  const sort = sortKey === "new" ? "created DESC, updated DESC" : "installs DESC, title";
  const rows = db.prepare(`SELECT * FROM modules WHERE hidden = 0 ORDER BY ${sort}`).all();
  return {
    modules: rows
      .filter((row) => {
        if (!category || category === "all" || category === "free") return true;
        if (category === "local") return JSON.parse(row.manifest).local === true;
        return row.category === category;
      })
      .filter((row) => !q || `${row.title} ${row.about} ${row.author} ${row.id}`.toLowerCase().includes(q))
      .map(card),
    categories: CATEGORIES,
  };
});

route("GET", /^\/api\/modules\/([a-z0-9-]+)$/, ({ match }) => {
  const row = db.prepare("SELECT * FROM modules WHERE id = ? AND hidden = 0").get(match[1]);
  if (!row) throw new Fail(404, "Модуль не найден.");
  const manifest = JSON.parse(row.manifest);
  return {
    ...card(row),
    description: row.description,
    tools: JSON.parse(row.tools),
    runtime: manifest.builtin ? "builtin" : (manifest.mcp?.command ?? ""),
    brain: typeof manifest.brain === "string" ? manifest.brain.slice(0, 40) : "",
    secrets: (manifest.secrets ?? []).map((s) => ({ title: s.title, hint: s.hint, optional: Boolean(s.optional) })),
    files: Object.keys(JSON.parse(row.files)),
  };
});

/** Пакет для установки в Ноа: описание и файлы. Ноа проверит его сама. */
route("GET", /^\/api\/modules\/([a-z0-9-]+)\/package$/, ({ match }) => {
  const row = db.prepare("SELECT * FROM modules WHERE id = ? AND hidden = 0").get(match[1]);
  if (!row) throw new Fail(404, "Модуль не найден.");
  if (JSON.parse(row.manifest).builtin) throw new Fail(409, `«${row.title}» уже встроен в NOAH и ставится вместе с приложением.`);
  db.prepare("UPDATE modules SET installs = installs + 1 WHERE id = ?").run(row.id);
  return { format: FORMAT, manifest: JSON.parse(row.manifest), files: JSON.parse(row.files) };
});

route("GET", /^\/api\/my\/modules$/, ({ user }) => {
  if (!user) throw new Fail(401, "Войдите.");
  return { modules: db.prepare("SELECT * FROM modules WHERE owner_id = ? ORDER BY updated DESC").all(user.id).map(card) };
});

route("DELETE", /^\/api\/my\/modules\/([a-z0-9-]+)$/, ({ user, match }) => {
  if (!user) throw new Fail(401, "Войдите.");
  const info = db.prepare("DELETE FROM modules WHERE id = ? AND owner_id = ?").run(match[1], user.id);
  if (!info.changes) throw new Fail(404, "Модуль не найден.");
  return { ok: true };
});

route("GET", /^\/api\/tokens$/, ({ user }) => {
  if (!user) throw new Fail(401, "Войдите.");
  return { tokens: db.prepare("SELECT id, label, created, used FROM tokens WHERE user_id = ? ORDER BY id DESC").all(user.id) };
});

route("POST", /^\/api\/tokens$/, async ({ req, user }) => {
  if (!user) throw new Fail(401, "Войдите.");
  const { label } = await readJson(req);
  const count = db.prepare("SELECT COUNT(*) AS n FROM tokens WHERE user_id = ?").get(user.id).n;
  if (count >= 10) throw new Fail(400, "Не больше 10 ключей — удалите ненужные.");
  const token = `noah_${randomBytes(24).toString("base64url")}`;
  db.prepare("INSERT INTO tokens (user_id, label, hash) VALUES (?, ?, ?)").run(
    user.id,
    String(label ?? "").trim().slice(0, 40) || "Ноа",
    sha(token),
  );
  return { token };
});

route("DELETE", /^\/api\/tokens\/(\d+)$/, ({ user, match }) => {
  if (!user) throw new Fail(401, "Войдите.");
  db.prepare("DELETE FROM tokens WHERE id = ? AND user_id = ?").run(Number(match[1]), user.id);
  return { ok: true };
});

/* ── Постоянная ссылка MCP ───────────────────────────────────────────────── */

// Одна ссылка на аккаунт, которую видно всегда. Раньше страница «Подключить
// ИИ» каждый раз выпускала новый ключ (прежний показать нельзя — хранится
// хеш), и нейросеть приходилось переподключать. Ключ этой ссылки хранится
// как есть: его видит только сам владелец, а сменить можно одной кнопкой.
db.exec(`
  CREATE TABLE IF NOT EXISTS mcp_links (
    user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    token_id INTEGER NOT NULL REFERENCES tokens(id) ON DELETE CASCADE,
    key TEXT NOT NULL
  );
`);

function issueMcpKey(userId) {
  const key = `noah_${randomBytes(24).toString("base64url")}`;
  const info = db.prepare("INSERT INTO tokens (user_id, label, hash) VALUES (?, ?, ?)").run(userId, "Ссылка MCP", sha(key));
  db.prepare("INSERT OR REPLACE INTO mcp_links (user_id, token_id, key) VALUES (?, ?, ?)").run(userId, Number(info.lastInsertRowid), key);
  return key;
}

route("GET", /^\/api\/my\/mcp$/, ({ user }) => {
  if (!user) throw new Fail(401, "Войдите.");
  const row = db.prepare("SELECT key FROM mcp_links WHERE user_id = ?").get(user.id);
  return { key: row?.key ?? issueMcpKey(user.id) };
});

route("POST", /^\/api\/my\/mcp\/rotate$/, ({ user }) => {
  if (!user) throw new Fail(401, "Войдите.");
  const row = db.prepare("SELECT token_id FROM mcp_links WHERE user_id = ?").get(user.id);
  if (row) db.prepare("DELETE FROM tokens WHERE id = ? AND user_id = ?").run(row.token_id, user.id);
  return { key: issueMcpKey(user.id) };
});

/** Публикация из Ноа: модуль уже прошёл живую проверку у автора. */
/** Публикует модуль от имени автора. Общий путь для Ноа и для MCP по ссылке. */
function publishModule(user, { manifest, files, tools, description, category }) {
  const problems = lint(manifest, files);
  if (problems.length) throw new Fail(422, `Модуль не соответствует стандарту:\n${problems.map((p) => `  ✗ ${p}`).join("\n")}`);
  if (!Array.isArray(tools) || !tools.length || tools.some((t) => typeof t?.name !== "string")) {
    throw new Fail(422, "Нужен список инструментов из проверки Ноа.");
  }
  const existing = db.prepare("SELECT owner_id FROM modules WHERE id = ?").get(manifest.id);
  if (existing && existing.owner_id !== user.id) throw new Fail(409, `Имя «${manifest.id}» занято другим автором — выберите другой id.`);
  const cat = CATEGORIES.includes(category) ? category : "other";
  const cleanTools = tools.slice(0, 25).map((t) => ({ name: String(t.name).slice(0, 48), about: String(t.about ?? "").slice(0, 200) }));
  db.prepare(
    `INSERT INTO modules (id, owner_id, author, title, icon, about, voice, category, version, description, manifest, files, tools)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(id) DO UPDATE SET title = excluded.title, icon = excluded.icon, about = excluded.about,
       voice = excluded.voice, category = excluded.category, version = excluded.version,
       description = excluded.description, manifest = excluded.manifest, files = excluded.files,
       tools = excluded.tools, updated = datetime('now')`,
  ).run(
    manifest.id,
    user.id,
    user.name,
    manifest.title,
    manifest.icon,
    manifest.about,
    manifest.voice,
    cat,
    String(manifest.version || "1.0.0").slice(0, 20),
    String(description ?? "").slice(0, 2000),
    JSON.stringify(manifest),
    JSON.stringify(files ?? {}),
    JSON.stringify(cleanTools),
  );
  return { ok: true, id: manifest.id, url: `/#/module/${manifest.id}` };
}

route("POST", /^\/api\/publish$/, async ({ req, tokenUser: user }) => {
  if (!user) throw new Fail(401, "Нужен ключ площадки: создайте его в кабинете и впишите в Ноа.");
  return publishModule(user, await readJson(req));
});

route("GET", /^\/api\/whoami$/, ({ tokenUser: user }) => {
  if (!user) throw new Fail(401, "Ключ не подошёл.");
  return { name: user.name };
});

/* ── Статика ────────────────────────────────────────────────────────────── */

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".webp": "image/webp",
  ".ico": "image/x-icon",
  ".json": "application/json; charset=utf-8",
  ".webmanifest": "application/manifest+json; charset=utf-8",
};

async function serveStatic(req, res, pathname, versioned) {
  let file = normalize(join(WEB, decodeURIComponent(pathname)));
  if (!file.startsWith(WEB)) return false;
  try {
    if ((await stat(file)).isDirectory()) file = join(file, "index.html");
  } catch {
    file = join(WEB, "index.html");
  }
  try {
    const type = TYPES[extname(file)] ?? "application/octet-stream";
    const raw = await readFile(file);
    const key = `${file}:${(await stat(file)).mtimeMs}`;
    const out = /^(text|application\/(json|manifest)|image\/svg)/.test(type) ? packed(req, raw, key) : { body: raw, headers: {} };
    res.writeHead(200, {
      ...SECURITY_HEADERS,
      "Content-Type": type,
      "Content-Length": out.body.length,
      // Файлы с версией в адресе (?v=) не меняются — их браузер берёт из кэша.
      // Остальное сверяется каждый раз, чтобы правки были видны сразу.
      "Cache-Control": versioned
        ? "public, max-age=31536000, immutable"
        : [".svg", ".png", ".webp", ".ico"].includes(extname(file))
          ? "public, max-age=86400"
          : "no-cache",
      ...out.headers,
    });
    res.end(req.method === "HEAD" ? undefined : out.body);
    return true;
  } catch {
    return false;
  }
}

/* ── Скачать NOAH: сразу установщик последнего релиза ───────────────────── */

const RELEASES = "https://github.com/faafaafuu/asis/releases/latest";
let installer = { url: "", at: 0 };

/** Прямая ссылка на установщик из последнего релиза; держится 10 минут. */
async function latestInstaller() {
  if (installer.url && Date.now() - installer.at < 10 * 60_000) return installer.url;
  try {
    const response = await fetch("https://api.github.com/repos/faafaafuu/asis/releases/latest", {
      headers: { Accept: "application/vnd.github+json", "User-Agent": "noah-platform" },
      signal: AbortSignal.timeout(8000),
    });
    const release = await response.json();
    const asset = (release.assets ?? []).find((a) => /setup\.exe$/i.test(a.name)) ?? (release.assets ?? []).find((a) => /\.(exe|msi)$/i.test(a.name));
    if (asset) installer = { url: asset.browser_download_url, at: Date.now() };
  } catch (err) {
    console.error("последний релиз не получен:", err.message);
  }
  return installer.url || RELEASES;
}

const oauthHandler = mountOAuth({ route, db, Fail, readJson, sessionUser, openSession, cookies });
const mcpHandler = mountRemote({ route, db, Fail, readJson, userForKey, publishModule, lint, validId, send, maxBody: MAX_BODY });

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, "http://local");
  // Заголовку прокси верим, только если запрос пришёл от него самого.
  const peer = String(req.socket.remoteAddress ?? "");
  // Свои: сам хост и сеть Docker, где стоит nginx с HTTPS для домена.
  const local = peer === "127.0.0.1" || peer === "::1" || /^(::ffff:)?(127\.|172\.(1[6-9]|2\d|3[01])\.)/.test(peer);
  const ip = local && req.headers["x-real-ip"] ? String(req.headers["x-real-ip"]) : peer;
  // По пути к серверу из некоторых сетей соединение замирает, когда через него
  // прошло около 30 КБ. Каждый ответ — в своём соединении, и замирать нечему.
  if (!local) res.shouldKeepAlive = false;
  try {
    // MCP по ссылке: нейросеть пользователя подключается сюда адресом с ключом.
    if (url.pathname === "/mcp") return await mcpHandler(req, res, url);
    if (url.pathname === "/download") {
      res.writeHead(302, { Location: await latestInstaller(), "Cache-Control": "no-store" });
      return res.end();
    }
    if (url.pathname.startsWith("/auth/") && (await oauthHandler(req, res, url))) return;
    if (url.pathname.startsWith("/api/")) {
      if (req.method !== "GET" && !sameOrigin(req)) throw new Fail(403, "Чужой источник запроса.");
      for (const r of routes) {
        const match = r.pattern.exec(url.pathname);
        if (!match || r.method !== req.method) continue;
        const body = await r.handler({ req, res, url, match, ip, user: sessionUser(req), tokenUser: tokenUser(req) });
        return send(res, 200, body);
      }
      throw new Fail(404, "Нет такого адреса.");
    }
    if (req.method !== "GET" && req.method !== "HEAD") throw new Fail(405, "Метод не поддерживается.");
    if (!(await serveStatic(req, res, url.pathname, url.searchParams.has("v")))) throw new Fail(404, "Не найдено.");
  } catch (err) {
    const status = err instanceof Fail ? err.status : 500;
    if (status === 500) console.error(err);
    if (!res.headersSent) send(res, status, { error: status === 500 ? "Ошибка сервера." : err.message });
    else res.end();
  }
});

setInterval(() => db.prepare("DELETE FROM sessions WHERE expires < ?").run(new Date().toISOString()), 3600e3).unref();

server.listen(PORT, HOST, () => console.log(`NOAH platform on http://${HOST}:${PORT}`));
