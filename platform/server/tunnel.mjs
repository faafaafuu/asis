// Запись через зеркало, которое пропускает только чтение.
//
// CDN перед зеркалом (m.noahlab.ru) пропускает к серверу лишь GET и HEAD,
// а на POST, PUT и DELETE сам отвечает «405». Поэтому с зеркала страница
// шлёт запись как GET: тело — кусками в адресе, затем команда «выполнить».
// Сервер собирает тело и проводит запрос через те же маршруты, что и
// обычный POST: для обработчиков разницы нет.
//
// Долгое — в фоне. Ответа сервера CDN ждёт около десяти секунд, потом
// обрывает запрос и повторяет его сам. Вопрос к мосту идёт дольше, и раньше
// он уходил на сервер по три раза, а до телефона не доходил ни один ответ.
// Теперь «выполнить» запускает работу и ждёт её не дольше WAIT; не успела —
// страница забирает ответ запросами /api/tunnel/wait. Повтор от CDN узнаётся
// по номеру и работу второй раз не запускает.
//
// Чужой сайт так не сделает: в каждом запросе метка, которую страница сама
// кладёт в cookie (SameSite=Strict) и повторяет в адресе. Чужая страница
// не может ни прочесть эту cookie, ни поставить свою.

import { EventEmitter } from "node:events";
import { Readable } from "node:stream";

const METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);
const LIFETIME = 5 * 60_000;
const MAX_PENDING = 500;

/** Сколько держать запрос, ожидая работу: меньше, чем ждёт CDN. */
const WAIT = 7_000;

/** Сколько хранить готовый ответ: его могут забрать ещё раз — повтором CDN. */
const KEEP_DONE = 2 * 60_000;

/** Недособранные тела: id → { parts, size, at, mark }. */
const pending = new Map();

/** Запущенные работы: id → { mark, reply, at }. */
const jobs = new Map();

setInterval(() => {
  const now = Date.now();
  for (const [id, entry] of pending) if (entry.at < now - LIFETIME) pending.delete(id);
  for (const [id, job] of jobs) {
    if ((job.reply.finished && job.reply.finishedAt < now - KEEP_DONE) || job.at < now - 30 * 60_000) jobs.delete(id);
  }
}, 30_000).unref();

const decode = (text) => Buffer.from(String(text ?? ""), "base64url");

/**
 * Ответ, который пишется не в сеть, а в память: обработчики пишут в него как
 * в обычный, а отдаётся он потом — одним куском, тому запросу, что за ним
 * пришёл.
 */
class Reply extends EventEmitter {
  constructor() {
    super();
    this.statusCode = 200;
    this.headers = {};
    this.chunks = [];
    this.headersSent = false;
    this.finished = false;
    this.done = new Promise((resolve) => (this.resolve = resolve));
  }

  setHeader(name, value) {
    this.headers[String(name).toLowerCase()] = value;
    return this;
  }

  getHeader(name) {
    return this.headers[String(name).toLowerCase()];
  }

  getHeaders() {
    return { ...this.headers };
  }

  hasHeader(name) {
    return String(name).toLowerCase() in this.headers;
  }

  removeHeader(name) {
    delete this.headers[String(name).toLowerCase()];
  }

  writeHead(status, message, headers) {
    const given = typeof message === "object" && message ? message : headers;
    this.statusCode = status;
    for (const [name, value] of Object.entries(given ?? {})) this.setHeader(name, value);
    this.headersSent = true;
    return this;
  }

  write(chunk) {
    if (chunk != null) this.chunks.push(Buffer.from(chunk));
    this.headersSent = true;
    return true;
  }

  end(chunk) {
    if (this.finished) return this;
    if (chunk != null && typeof chunk !== "function") this.chunks.push(Buffer.from(chunk));
    this.headersSent = true;
    this.finished = true;
    this.finishedAt = Date.now();
    this.emit("finish");
    this.resolve();
    return this;
  }

  /** Отдаёт записанное настоящему ответу. */
  replay(res) {
    const body = Buffer.concat(this.chunks);
    const headers = { ...this.headers };
    for (const name of ["content-length", "transfer-encoding", "connection", "keep-alive"]) delete headers[name];
    res.writeHead(this.statusCode, { ...headers, "Content-Length": body.length, "Cache-Control": "no-store" });
    res.end(body);
  }
}

/** Ждёт работу не дольше WAIT: готова — отдаёт ответ, нет — «ещё работаю». */
async function deliver(job, res, wait) {
  let timer;
  await Promise.race([job.reply.done, new Promise((resolve) => (timer = setTimeout(resolve, wait)))]);
  clearTimeout(timer);
  if (job.reply.finished) return job.reply.replay(res);
  const body = JSON.stringify({ tunnel: "pending" });
  res.writeHead(202, { "Content-Type": "application/json; charset=utf-8", "Content-Length": Buffer.byteLength(body), "Cache-Control": "no-store", "X-Noah-Pending": "1" });
  res.end(body);
}

/**
 * Обрабатывает /api/tunnel/part, /api/tunnel/go и /api/tunnel/wait.
 * `handle` — общий обработчик сервера: ему отдаётся собранный запрос.
 */
export async function tunnel({ req, res, url, cookies, handle, Fail, maxBody, wait = WAIT }) {
  const mark = cookies(req).noah_t;
  if (!mark || mark.length < 16 || url.searchParams.get("k") !== mark) throw new Fail(403, "Чужой источник запроса.");
  if (req.headers["sec-fetch-site"] === "cross-site") throw new Fail(403, "Чужой источник запроса.");
  const id = String(url.searchParams.get("id") ?? "");
  if (!/^[\w-]{8,64}$/.test(id)) throw new Fail(400, "Запрос не разобрался.");

  if (url.pathname === "/api/tunnel/part") {
    const index = Number(url.searchParams.get("i"));
    if (!Number.isInteger(index) || index < 0 || index > 10_000) throw new Fail(400, "Запрос не разобрался.");
    // Кусок, повторённый CDN после того, как работа уже запущена, не нужен.
    if (jobs.has(id)) {
      res.writeHead(204, { "Cache-Control": "no-store" });
      return res.end();
    }
    let entry = pending.get(id);
    if (!entry) {
      if (pending.size >= MAX_PENDING) throw new Fail(503, "Сервер занят — попробуйте через минуту.");
      entry = { parts: [], size: 0, at: Date.now(), mark };
      pending.set(id, entry);
    }
    if (entry.mark !== mark) throw new Fail(403, "Чужой источник запроса.");
    const part = decode(url.searchParams.get("d"));
    entry.size += part.length;
    if (entry.size > maxBody) {
      pending.delete(id);
      throw new Fail(413, "Слишком большой запрос.");
    }
    entry.parts[index] = part;
    res.writeHead(204, { "Cache-Control": "no-store" });
    return res.end();
  }

  if (url.pathname === "/api/tunnel/wait") {
    const job = jobs.get(id);
    if (!job || job.mark !== mark) throw new Fail(410, "Ответ не сохранился — повторите.");
    return deliver(job, res, wait);
  }

  if (url.pathname !== "/api/tunnel/go") throw new Fail(404, "Нет такого адреса.");

  // Повтор той же команды — от CDN или от страницы: работа уже идёт.
  const running = jobs.get(id);
  if (running) {
    if (running.mark !== mark) throw new Fail(403, "Чужой источник запроса.");
    return deliver(running, res, wait);
  }

  const method = String(url.searchParams.get("m") ?? "").toUpperCase();
  const path = String(url.searchParams.get("p") ?? "");
  if (!METHODS.has(method)) throw new Fail(400, "Запрос не разобрался.");
  if (!/^\/(api|auth)\//.test(path) || path.startsWith("/api/tunnel/")) throw new Fail(400, "Запрос не разобрался.");

  let body;
  const count = Number(url.searchParams.get("n") ?? 0);
  if (count > 0) {
    const entry = pending.get(id);
    pending.delete(id);
    if (!entry || entry.mark !== mark) throw new Fail(400, "Запрос дошёл не целиком — повторите.");
    const parts = entry.parts.slice(0, count);
    if (parts.length !== count || parts.includes(undefined) || entry.parts.length !== count) {
      throw new Fail(400, "Запрос дошёл не целиком — повторите.");
    }
    body = Buffer.concat(parts);
  } else {
    body = decode(url.searchParams.get("d"));
    if (body.length > maxBody) throw new Fail(413, "Слишком большой запрос.");
  }

  // Собранный запрос: тот же сокет и cookie, свой метод, адрес и тело.
  const inner = Readable.from(body.length ? [body] : []);
  inner.method = method;
  inner.url = path;
  inner.socket = req.socket;
  inner.headers = { ...req.headers, "content-length": String(body.length) };
  const type = url.searchParams.get("t");
  if (type) inner.headers["content-type"] = type;
  else delete inner.headers["content-type"];

  const job = { mark, reply: new Reply(), at: Date.now() };
  // Как у настоящего ответа: по запросу, например, выбирается сжатие.
  job.reply.req = inner;
  jobs.set(id, job);
  Promise.resolve(handle(inner, job.reply)).catch((err) => {
    console.error(err);
    if (!job.reply.finished) {
      job.reply.statusCode = 500;
      job.reply.setHeader("Content-Type", "application/json; charset=utf-8");
      job.reply.end(JSON.stringify({ error: "Ошибка сервера." }));
    }
  });
  return deliver(job, res, wait);
}
