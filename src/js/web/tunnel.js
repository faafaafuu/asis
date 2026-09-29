// Запись через зеркало, которое пропускает только чтение.
//
// CDN перед зеркалом (m.noahlab.ru) пропускает к серверу лишь GET и HEAD —
// на POST, PUT и DELETE он сам отвечает «405». С зеркала запись уходит как
// GET: тело — кусками в адресе, затем команда «выполнить», и сервер
// проводит её через обычные маршруты (platform/server/tunnel.mjs). С
// основного адреса всё идёт обычным fetch.
//
// Долгая работа (вопрос к мосту, распознавание речи) идёт на сервере в
// фоне: CDN ждёт ответа около десяти секунд, и ответ забирается несколькими
// короткими запросами.

/** Байт тела на один запрос: в base64 это ~4 КБ адреса — в пределах CDN. */
const PART = 3000;
const PARALLEL = 4;

const onMirror = () => typeof location !== "undefined" && /^m\./.test(location.hostname);

/**
 * Метка против чужих сайтов: страница кладёт её в cookie и повторяет в
 * адресе. Чужая страница не может ни прочесть эту cookie, ни поставить свою.
 */
function mark() {
  const found = document.cookie.match(/(?:^|;\s*)noah_t=([\w-]{16,})/);
  if (found) return found[1];
  const fresh = crypto.randomUUID().replaceAll("-", "");
  document.cookie = `noah_t=${fresh}; path=/; max-age=31536000; samesite=strict; secure`;
  return fresh;
}

function base64url(bytes) {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

function contentType(headers) {
  if (!headers) return "";
  if (headers instanceof Headers) return headers.get("content-type") ?? "";
  const key = Object.keys(headers).find((name) => name.toLowerCase() === "content-type");
  return key ? String(headers[key]) : "";
}

/**
 * fetch для своего сайта. Чтение и всё, что не на зеркале, — обычный fetch;
 * запись с зеркала — через GET-туннель. Ответ — тот же, что дал бы POST.
 */
export async function siteFetch(url, init = {}) {
  const method = String(init.method ?? "GET").toUpperCase();
  if (method === "GET" || method === "HEAD" || !onMirror() || !String(url).startsWith("/")) return fetch(url, init);

  const bytes = init.body == null ? new Uint8Array() : new Uint8Array(await new Response(init.body).arrayBuffer());
  const k = mark();
  const id = crypto.randomUUID();
  const common = { credentials: "same-origin", cache: "no-store", signal: init.signal };
  const go = { k, id, m: method, p: String(url), t: contentType(init.headers) };

  if (bytes.length <= PART) {
    go.d = base64url(bytes);
  } else {
    const parts = [];
    for (let i = 0; i < bytes.length; i += PART) parts.push(bytes.subarray(i, i + PART));
    let next = 0;
    const send = async () => {
      while (next < parts.length) {
        const i = next++;
        const query = new URLSearchParams({ k, id, i: String(i), d: base64url(parts[i]) });
        const response = await fetch(`/api/tunnel/part?${query}`, common);
        // Отказ на куске — отдать его как ответ на весь запрос.
        if (!response.ok) throw Object.assign(new Error("part"), { response });
      }
    };
    try {
      await Promise.all(Array.from({ length: Math.min(PARALLEL, parts.length) }, send));
    } catch (err) {
      if (err.response) return err.response;
      throw err;
    }
    go.n = String(parts.length);
  }
  return settle(await fetch(`/api/tunnel/go?${new URLSearchParams(go)}`, common), { k, id }, common);
}

/** Ответ «ещё работаю» — наш, а не обработчика, который сам ответил 202. */
async function isPending(response) {
  if (response.status !== 202) return false;
  if (response.headers.get("X-Noah-Pending") === "1") return true;
  // CDN мог не пропустить заголовок — тогда по телу.
  const body = await response.clone().json().catch(() => null);
  return body?.tunnel === "pending";
}

/**
 * Дожидается ответа долгой работы. Сервер держит каждый запрос до семи
 * секунд (дольше CDN не ждёт) и отвечает «ещё работаю»; тогда спрашиваем
 * снова. Сбой сети посреди ожидания работу не губит — она идёт на сервере, —
 * поэтому спрашиваем ещё, пока не отменили снаружи.
 */
async function settle(response, { k, id }, common) {
  let failures = 0;
  while (await isPending(response)) {
    try {
      response = await fetch(`/api/tunnel/wait?${new URLSearchParams({ k, id })}`, common);
      failures = 0;
    } catch (err) {
      if (common.signal?.aborted || ++failures > 5) throw err;
      await new Promise((resolve) => setTimeout(resolve, 1000 * failures));
    }
  }
  return response;
}
