// Запросы к сайту, которые не висят вечно.
//
// По мобильной сети в РФ соединение с сервером иногда замирает: запрос ушёл,
// ответ не пришёл, и браузер ждёт его бесконечно. Хуже того — такие запросы
// занимают соединения, и за ними встают в очередь все следующие: страница
// «висит как неавторизованная». Поэтому у каждого запроса есть предел
// ожидания, зависший обрывается (освобождая соединение) и повторяется.

export class NetError extends Error {}

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * fetch с пределом ожидания и повтором. Повторяются только сбои сети и
 * зависания; ответ сервера с ошибкой (4xx, 5xx) отдаётся как есть — его
 * разбирает вызывающий. Тело (`body`) повторяется, только если оно строка.
 */
export async function request(url, { timeout = 12_000, retries = 1, ...init } = {}) {
  let last;
  for (let attempt = 0; attempt <= retries; attempt++) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeout);
    const outer = init.signal;
    const relay = () => controller.abort();
    outer?.addEventListener("abort", relay);
    try {
      return await fetch(url, { credentials: "same-origin", ...init, signal: controller.signal });
    } catch (err) {
      if (outer?.aborted) throw err;
      last = err;
      if (attempt < retries) await pause(800 * (attempt + 1));
    } finally {
      clearTimeout(timer);
      outer?.removeEventListener("abort", relay);
    }
  }
  throw new NetError(last?.name === "AbortError" ? "Сервер не ответил — проверьте связь." : "Нет связи с сервером.");
}
