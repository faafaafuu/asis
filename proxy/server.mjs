// Прокси к языковой модели для веб-демонстрации.
//
// Зачем он нужен: страница в браузере не может ходить к API напрямую — ключ был бы
// виден каждому посетителю в исходниках и в панели разработчика. Прокси держит ключ
// у себя, наружу отдаёт только ответ модели.
//
// В самом приложении такого посредника нет: там роль прокси играет Rust-бэкенд,
// а ключ лежит в config.json на компьютере пользователя.
//
// Настройка — переменными окружения:
//   SUFLER_AI_ENDPOINT  адрес совместимого с OpenAI API
//   SUFLER_AI_KEY       ключ
//   SUFLER_AI_MODEL     название модели
//   SUFLER_ALLOW_ORIGIN источник, которому разрешены запросы (по умолчанию любой)
//   PORT                порт (по умолчанию 5174)

import { createServer } from "node:http";

const ENDPOINT = process.env.SUFLER_AI_ENDPOINT ?? "";
const KEY = process.env.SUFLER_AI_KEY ?? "";
const MODEL = process.env.SUFLER_AI_MODEL ?? "";
const ORIGIN = process.env.SUFLER_ALLOW_ORIGIN ?? "*";
const PORT = Number(process.env.PORT ?? 5174);

/** Ограничение размера запроса: демонстрация открыта всем, кто знает адрес. */
const MAX_BODY = 32 * 1024;

/** Простое ограничение частоты по IP — чтобы бесплатный лимит не выжгли за час. */
const RATE_WINDOW_MS = 60_000;
const RATE_LIMIT = 20;
const hits = new Map();

function rateLimited(ip) {
  const now = Date.now();
  const list = (hits.get(ip) ?? []).filter((t) => now - t < RATE_WINDOW_MS);
  list.push(now);
  hits.set(ip, list);
  return list.length > RATE_LIMIT;
}

function cors(res) {
  res.setHeader("access-control-allow-origin", ORIGIN);
  res.setHeader("access-control-allow-headers", "content-type");
  res.setHeader("access-control-allow-methods", "POST, OPTIONS");
}

const server = createServer(async (req, res) => {
  cors(res);

  if (req.method === "OPTIONS") {
    res.writeHead(204).end();
    return;
  }

  if (req.url === "/health") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ ok: Boolean(ENDPOINT && KEY), model: MODEL }));
    return;
  }

  if (req.method !== "POST" || !req.url.startsWith("/api/chat")) {
    res.writeHead(404).end("404");
    return;
  }

  if (!ENDPOINT || !KEY) {
    res.writeHead(503, { "content-type": "application/json" });
    res.end(JSON.stringify({ error: "Модель не настроена: не заданы SUFLER_AI_ENDPOINT и SUFLER_AI_KEY" }));
    return;
  }

  const ip = req.headers["x-forwarded-for"]?.split(",")[0]?.trim() || req.socket.remoteAddress;
  if (rateLimited(ip)) {
    res.writeHead(429, { "content-type": "application/json" });
    res.end(JSON.stringify({ error: "Слишком часто — подождите минуту" }));
    return;
  }

  let raw = "";
  for await (const chunk of req) {
    raw += chunk;
    if (raw.length > MAX_BODY) {
      res.writeHead(413).end("too large");
      return;
    }
  }

  let payload;
  try {
    payload = JSON.parse(raw);
  } catch {
    res.writeHead(400, { "content-type": "application/json" });
    res.end(JSON.stringify({ error: "Некорректный JSON" }));
    return;
  }

  try {
    const upstream = await fetch(ENDPOINT, {
      method: "POST",
      headers: { "content-type": "application/json", authorization: `Bearer ${KEY}` },
      // Модель задаётся здесь, а не приходит от клиента: иначе адрес прокси стал бы
      // бесплатным доступом к любой модели за наш счёт.
      body: JSON.stringify({ ...payload, model: MODEL, stream: false }),
      signal: AbortSignal.timeout(30_000),
    });

    const text = await upstream.text();
    res.writeHead(upstream.status, { "content-type": "application/json" });
    res.end(text);
  } catch (err) {
    console.error("ошибка обращения к модели:", err.message);
    res.writeHead(502, { "content-type": "application/json" });
    res.end(JSON.stringify({ error: "Модель не ответила" }));
  }
});

server.listen(PORT, () => {
  console.log(
    `Прокси к модели: http://localhost:${PORT}/api/chat` +
      (ENDPOINT && KEY ? ` → ${new URL(ENDPOINT).host} (${MODEL || "модель по умолчанию"})` : " — ключ не задан"),
  );
});
