// AI-клиент фронтенда (SPEC §10).
//
// Единая абстракция, не завязанная на конкретного провайдера. Любая реализация
// предоставляет два метода:
//
//   explain(term, contextText, { signal })            → { def, simple, examples[] }
//   ask(term, contextText, thread, question, {signal}) → string
//
// В комплекте три реализации:
//   MockProvider  — детерминированные ответы (демо-режим и разработка без сети);
//   HttpProvider  — заготовка под реальный HTTP API, ключ/endpoint только из конфигурации;
//   TauriProvider — проксирует запрос в Rust (в приложении сеть ходит из бэкенда,
//                   потому что CSP окна попапа запрещает внешние connect-src).

/** Тип ошибки важен для UI: всё, что не `abort`, показывается состоянием Error. */
export class AiError extends Error {
  /** @param {string} message @param {{kind: 'timeout'|'network'|'http'|'parse'|'abort'|'config'|'backend', status?: number}} meta */
  constructor(message, meta) {
    super(message);
    this.name = "AiError";
    this.kind = meta.kind;
    this.status = meta.status;
  }

  /**
   * Текст для состояния Error, если он осмысленнее общей фразы про сбой сети.
   * `null` — показывать настроенный текст по умолчанию: «fetch failed» или
   * «NetworkError» пользователю ничего не объясняют.
   */
  get userText() {
    const speaking = ["backend", "http", "parse", "config"];
    return speaking.includes(this.kind) && this.message ? this.message : null;
  }
}

export const DEFAULT_ERROR_TEXT = "Сбой сети — нет ответа";

/* ─────────────────────────────── Mock ─────────────────────────────────────
   Словарь и морфологический подбор — ровно как в дизайн-референсе
   `AI Popup.dc.html`, чтобы демо-документ вёл себя один в один. */

const ANSWERS = {
  излучение: {
    def: "Перенос энергии волнами или частицами — в этом тексте речь о солнечном свете и тепле, приходящих на поверхность.",
    simple:
      "Это энергия, которая летит от источника сама, без нагретого воздуха или воды-посредника. Долетела — нагрела то, во что попала.",
    examples: [
      "солнечный свет, нагревающий снег",
      "тепло от костра, которое чувствуешь на расстоянии",
      "инфракрасный поток, уходящий с Земли обратно в космос",
    ],
  },
  альбедо: {
    def: "Отражательная способность поверхности: доля падающего света, которая уходит обратно.",
    simple:
      "Насколько поверхность «светлая» для солнца. Светлая отражает и остаётся холодной, тёмная поглощает и греется.",
    examples: ["свежий снег — 0.8–0.9", "открытый океан — около 0.06", "белая крыша летом прохладнее чёрной"],
  },
  криоконит: {
    def: "Тёмный осадок из минеральной пыли, сажи и микроорганизмов на поверхности ледника.",
    simple: "Грязь на льду. Она темнее льда, потому сильнее нагревается и проплавляет себе ямку.",
    examples: [
      "криоконитовые колодцы глубиной в несколько сантиметров",
      "пыль от лесных пожаров, осевшая на ледник",
    ],
  },
  абляция: {
    def: "Убыль массы льда: таяние, испарение, сублимация и механический отрыв.",
    simple: "Всё, из-за чего ледник теряет лёд. Противоположность накоплению снега.",
    examples: ["стаявший за лето слой на поверхности", "откол айсбергов от языка ледника"],
  },
  изостазия: {
    def: "Равновесие литосферы на пластичной мантии: снимите нагрузку — кора поднимется.",
    simple: "Земная кора плавает, как плот. Убрали с него груз льда — плот всплывает, но очень медленно.",
    examples: [
      "Скандинавия поднимается ~8 мм в год после последнего оледенения",
      "прогиб коры под Гренландским щитом",
    ],
  },
  криосфера: {
    def: "Все формы льда в системе Земли: морской лёд, ледники и щиты, снежный покров, мерзлота.",
    simple: "Вся замёрзшая часть планеты, вместе взятая.",
    examples: ["арктический морской лёд", "мерзлота Сибири", "сезонный снежный покров"],
  },
  литосфера: {
    def: "Жёсткая внешняя оболочка Земли: кора и верхняя часть мантии.",
    simple: "Твёрдая «скорлупа» планеты, которая лежит на более вязком слое под ней.",
    examples: ["толщина под океаном — около 70 км", "континентальная литосфера — до 150 км"],
  },
};

const STEMS = Object.keys(ANSWERS);

/** Грубое отсечение русских окончаний — достаточно для демо-словаря. */
export function lookup(raw) {
  const w = String(raw ?? "")
    .toLowerCase()
    .replace(/[^a-zа-яё\- ]/gi, "")
    .trim();
  if (ANSWERS[w]) return { term: w, data: ANSWERS[w] };
  const base = w.replace(/(ами|ями|ах|ях|ов|ей|ий|ие|ия|ию|ем|ом|ой|ы|и|е|у|ю|а|я)$/u, "");
  for (const k of STEMS) {
    const kb = k.replace(/(а|я|е|о)$/u, "");
    if (base.length > 3 && (k.startsWith(base) || base.startsWith(kb))) return { term: k, data: ANSWERS[k] };
  }
  return { term: raw, data: null };
}

const sleep = (ms, signal) =>
  new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(new AiError("Запрос отменён", { kind: "abort" }));
    const t = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(t);
        reject(new AiError("Запрос отменён", { kind: "abort" }));
      },
      { once: true },
    );
  });

export class MockProvider {
  /** @param {{latencyMs?: number, forceState?: 'auto'|'loading'|'error'}} [opts] */
  constructor(opts = {}) {
    this.latencyMs = opts.latencyMs ?? 900;
    this.forceState = opts.forceState ?? "auto";
  }

  async explain(term, _contextText, { signal } = {}) {
    // «Загрузка» из демо-панели референса: состояние Loading, которое никогда не сменяется.
    if (this.forceState === "loading") return new Promise(() => {});
    await sleep(this.latencyMs, signal);
    if (this.forceState === "error") throw new AiError(DEFAULT_ERROR_TEXT, { kind: "network" });
    const hit = lookup(term);
    return (
      hit.data ?? {
        def: `Определения для «${term}» нет — выделите одно слово или термин.`,
        simple: "",
        examples: [],
      }
    );
  }

  async ask(term, _contextText, thread, _question, { signal } = {}) {
    await sleep(this.latencyMs, signal);
    if (this.forceState === "error") throw new AiError(DEFAULT_ERROR_TEXT, { kind: "network" });
    const d = lookup(term).data ?? { def: "", simple: "", examples: [] };
    const variants = [
      `Если совсем коротко: ${String(d.def).replace(/\.$/, "")}.`,
      `Иначе говоря: ${d.simple || "определение выше — самое короткое, что тут есть."}`,
      `Пример по делу: ${d.examples[0] ?? "—"}.`,
    ];
    return variants[Math.max(0, thread.length - 1) % variants.length];
  }
}

/* ─────────────────────────────── HTTP ─────────────────────────────────── */

const SYSTEM_PROMPT =
  "Ты объясняешь выделенный пользователем термин. Отвечай по-русски, по сути, без служебных " +
  'фраз вроде «это фрагмент из абзаца». Верни строгий JSON: {"def": "одно-два предложения", ' +
  '"simple": "то же максимально просто", "examples": ["2–3 коротких примера"]}.';

/**
 * Заготовка реального провайдера. Формат запроса намеренно нейтральный
 * (messages + JSON-ответ) — подстраивается под конкретный API одной правкой
 * `buildRequest`/`parseResponse`, без изменений в остальном приложении.
 */
export class HttpProvider {
  /** @param {{endpoint: string, apiKey?: string, model?: string, timeoutMs?: number, retries?: number, retryBackoffMs?: number, fetchImpl?: typeof fetch}} cfg */
  constructor(cfg) {
    if (!cfg?.endpoint) throw new AiError("Не задан endpoint AI-провайдера", { kind: "config" });
    this.endpoint = cfg.endpoint;
    this.apiKey = cfg.apiKey ?? "";
    this.model = cfg.model ?? "";
    this.timeoutMs = cfg.timeoutMs ?? 12000;
    this.retries = cfg.retries ?? 1;
    this.retryBackoffMs = cfg.retryBackoffMs ?? 400;
    this.fetch = cfg.fetchImpl ?? globalThis.fetch.bind(globalThis);
  }

  async explain(term, contextText, { signal } = {}) {
    const raw = await this.#send(
      [
        { role: "system", content: SYSTEM_PROMPT },
        { role: "user", content: `Термин: «${term}».\nКонтекст: ${contextText ?? ""}` },
      ],
      signal,
    );
    return normalizeExplain(raw, term);
  }

  async ask(term, contextText, thread, question, { signal } = {}) {
    const history = thread.flatMap((m) => [
      { role: "user", content: m.q },
      { role: "assistant", content: m.a },
    ]);
    const raw = await this.#send(
      [
        {
          role: "system",
          content: `Пользователь уточняет ранее объяснённый термин «${term}». Отвечай коротко, обычным текстом, без JSON.`,
        },
        { role: "user", content: `Исходный контекст: ${contextText ?? ""}` },
        ...history,
        { role: "user", content: question },
      ],
      signal,
    );
    return typeof raw === "string" ? raw : String(raw?.text ?? "");
  }

  async #send(messages, outerSignal) {
    let lastError;
    for (let attempt = 0; attempt <= this.retries; attempt++) {
      try {
        return await this.#once(messages, outerSignal);
      } catch (err) {
        if (err.kind === "abort") throw err;
        lastError = err;
        if (!isRetryable(err) || attempt === this.retries) break;
        await sleep(this.retryBackoffMs * 2 ** attempt, outerSignal);
      }
    }
    throw lastError;
  }

  async #once(messages, outerSignal) {
    const ctrl = new AbortController();
    const onAbort = () => ctrl.abort();
    outerSignal?.addEventListener("abort", onAbort, { once: true });
    const timer = setTimeout(() => ctrl.abort("timeout"), this.timeoutMs);
    try {
      const res = await this.fetch(this.endpoint, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          ...(this.apiKey ? { authorization: `Bearer ${this.apiKey}` } : {}),
        },
        body: JSON.stringify({ model: this.model || undefined, messages }),
        signal: ctrl.signal,
      });
      if (!res.ok) {
        throw new AiError(`HTTP ${res.status}`, { kind: "http", status: res.status });
      }
      return await res.json();
    } catch (err) {
      if (err instanceof AiError) throw err;
      if (outerSignal?.aborted) throw new AiError("Запрос отменён", { kind: "abort" });
      if (err?.name === "AbortError") throw new AiError("Таймаут запроса", { kind: "timeout" });
      throw new AiError(err?.message ?? "Сетевая ошибка", { kind: "network" });
    } finally {
      clearTimeout(timer);
      outerSignal?.removeEventListener("abort", onAbort);
    }
  }
}

/** Повторяем только то, что имеет шанс пройти со второй попытки. */
export function isRetryable(err) {
  if (err?.kind === "timeout" || err?.kind === "network") return true;
  if (err?.kind === "http") return err.status === 408 || err.status === 429 || err.status >= 500;
  return false;
}

/** Ответ модели приводим к контракту {def, simple, examples[]} и не доверяем его форме. */
export function normalizeExplain(raw, term) {
  let data = raw;
  if (typeof raw === "string") {
    try {
      data = JSON.parse(raw);
    } catch {
      data = { def: raw };
    }
  }
  // Типовая обёртка chat-completions: вытаскиваем текст ответа и парсим его как JSON.
  const inner = data?.choices?.[0]?.message?.content ?? data?.content?.[0]?.text;
  if (typeof inner === "string") {
    try {
      data = JSON.parse(inner);
    } catch {
      data = { def: inner };
    }
  }
  const def = String(data?.def ?? data?.definition ?? "").trim();
  if (!def) throw new AiError(`Пустой ответ для «${term}»`, { kind: "parse" });
  return {
    def,
    simple: String(data?.simple ?? "").trim(),
    examples: Array.isArray(data?.examples) ? data.examples.map(String).filter(Boolean).slice(0, 3) : [],
  };
}

/* ─────────────────────────────── Tauri ────────────────────────────────── */

/** Внутри приложения сеть ходит из Rust: команды `ai_explain` / `ai_ask`. */
export class TauriProvider {
  constructor(invoke) {
    this.invoke = invoke;
  }

  async explain(term, contextText, { signal } = {}) {
    return this.#call("ai_explain", { term, context: contextText ?? "" }, signal);
  }

  async ask(term, contextText, thread, question, { signal } = {}) {
    const answer = await this.#call(
      "ai_ask",
      { term, context: contextText ?? "", thread, question },
      signal,
    );
    return typeof answer === "string" ? answer : String(answer?.answer ?? "");
  }

  async #call(cmd, args, signal) {
    if (signal?.aborted) throw new AiError("Запрос отменён", { kind: "abort" });
    try {
      return await this.invoke(cmd, args);
    } catch (err) {
      // Rust отдаёт готовый текст: сетевые ошибки он уже заменил на настроенную
      // фразу, а всё остальное («Сервис ответил ошибкой 401») стоит показать как есть.
      const message = typeof err === "string" ? err : (err?.message ?? DEFAULT_ERROR_TEXT);
      throw new AiError(message, { kind: "backend" });
    }
  }
}

/**
 * @param {{provider?: 'mock'|'http'|'tauri', latencyMs?: number, forceState?: string, invoke?: Function} & Record<string, any>} cfg
 */
export function createAiClient(cfg = {}) {
  switch (cfg.provider ?? "mock") {
    case "http":
      return new HttpProvider(cfg);
    case "tauri":
      return new TauriProvider(cfg.invoke);
    default:
      return new MockProvider(cfg);
  }
}
