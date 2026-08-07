// Провайдер на Википедии: определения без ключей, без регистрации и без своего сервера.
//
// Зачем он есть рядом с моделью: для «что это за слово» энциклопедия точнее и честнее
// генерации — она не выдумывает. Модель нужна там, где словаря не хватает: объяснить
// простыми словами, привести примеры, ответить на уточняющий вопрос.
//
// Ограничение, которое видно пользователю: раздел «Примеры» и follow-up-тред здесь
// не работают — энциклопедия их не даёт. Поле «Спросить ещё…» честно об этом говорит.

import { AiError } from "./ai-client.js";

const TIMEOUT_MS = 8000;

/** Латиница — ищем в английской Википедии, кириллица — в русской. */
function wikiHost(term) {
  return /[а-яё]/i.test(term) ? "ru.wikipedia.org" : "en.wikipedia.org";
}

/** Первое-второе предложение: в шапке попапа нужен короткий ответ, а не абзац. */
function firstSentences(text, count = 2) {
  const parts = String(text).match(/[^.!?]+[.!?]+(\s|$)/g);
  if (!parts) return String(text).trim();
  return parts.slice(0, count).join("").trim();
}

export class WikipediaProvider {
  constructor(opts = {}) {
    this.fetch = opts.fetchImpl ?? globalThis.fetch.bind(globalThis);
    this.timeoutMs = opts.timeoutMs ?? TIMEOUT_MS;
  }

  async explain(term, _context, { signal } = {}) {
    const page = (await this.#summary(term, signal)) ?? (await this.#viaSearch(term, signal));

    if (!page?.extract) {
      // Не сетевая ошибка, а отсутствие статьи — и сказать надо именно это.
      throw new AiError(`В Википедии нет статьи о «${term}»`, { kind: "backend" });
    }

    const def = firstSentences(page.extract);
    return {
      def,
      // Полная выжимка попадает под «?», если она содержательнее первой фразы.
      simple: page.extract.length > def.length + 40 ? page.extract : "",
      examples: [],
    };
  }

  async ask(term) {
    // Не притворяемся, что умеем отвечать: у энциклопедии нет диалога.
    return (
      `Уточняющие вопросы умеет только языковая модель — сейчас определения берутся ` +
      `из Википедии. Подключить модель можно в config.json; статья целиком: ` +
      `https://${wikiHost(term)}/wiki/${encodeURIComponent(term)}`
    );
  }

  /** Прямое попадание по названию статьи. */
  async #summary(term, signal) {
    const url = `https://${wikiHost(term)}/api/rest_v1/page/summary/${encodeURIComponent(term)}`;
    const data = await this.#get(url, signal);
    if (!data) return null;
    // Страница-разрешение неоднозначностей определением не является.
    if (data.type === "disambiguation") return null;
    return data;
  }

  /** Если статьи с таким названием нет — ищем ближайшую по смыслу. */
  async #viaSearch(term, signal) {
    const url =
      `https://${wikiHost(term)}/w/api.php?action=query&list=search&srlimit=1` +
      `&srsearch=${encodeURIComponent(term)}&format=json&origin=*`;
    const data = await this.#get(url, signal);
    const title = data?.query?.search?.[0]?.title;
    if (!title) return null;
    return this.#summary(title, signal);
  }

  async #get(url, outerSignal) {
    const ctrl = new AbortController();
    const onAbort = () => ctrl.abort();
    outerSignal?.addEventListener("abort", onAbort, { once: true });
    const timer = setTimeout(() => ctrl.abort(), this.timeoutMs);
    try {
      const res = await this.fetch(url, {
        signal: ctrl.signal,
        headers: { accept: "application/json" },
      });
      // 404 — обычное дело: статьи просто нет. Это не повод показывать сбой сети.
      if (res.status === 404) return null;
      if (!res.ok) throw new AiError(`Википедия ответила ошибкой ${res.status}`, { kind: "http", status: res.status });
      return await res.json();
    } catch (err) {
      if (err instanceof AiError) throw err;
      if (outerSignal?.aborted) throw new AiError("Запрос отменён", { kind: "abort" });
      throw new AiError("Не удалось получить ответ", { kind: "network" });
    } finally {
      clearTimeout(timer);
      outerSignal?.removeEventListener("abort", onAbort);
    }
  }
}
