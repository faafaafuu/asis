// Модель для Ноа в браузере: запросы идут из страницы прямо к провайдеру.
//
// Ключ живёт только в этом браузере (localStorage) и уходит только туда, куда
// человек его вписал: на сервер сайта он не попадает. Поэтому здесь нет
// «нашего» ключа и нет прокси — каждый подключает свою модель, как в программе.
//
// Провайдеров три, и у всех один формат — chat/completions:
//   openrouter — сотни моделей по одному ключу, есть бесплатные;
//   openai     — любой совместимый адрес: OpenAI, Groq, DeepSeek, Gemini, LM Studio;
//   ollama     — модель на своём компьютере, без ключа.

import { AiError } from "../ai-client.js";
import { request } from "./net.js";
import { siteFetch } from "./tunnel.js";

const KEY = "noa.model";

export const PROVIDERS = {
  openrouter: { title: "OpenRouter", base: "https://openrouter.ai/api/v1", needsKey: true, keyHint: "sk-or-…" },
  openai: { title: "Свой адрес (OpenAI-совместимый)", base: "", needsKey: false, keyHint: "ключ, если нужен" },
  ollama: { title: "Ollama на этом компьютере", base: "http://localhost:11434/v1", needsKey: false, keyHint: "" },
  // Мост к подпискам владельца (Claude Code, Codex, Gemini, Qwen) на сервере
  // сайта. Ключа нет: пускает вход на сайт, и только владельца.
  bridge: { title: "Мост (мои подписки)", base: "/api/noa/bridge", needsKey: false, keyHint: "", ownerOnly: true },
};

/** Пускает ли сервер этого человека к мосту. */
export async function bridgeAvailable() {
  try {
    const response = await request("/api/noa/bridge/models", { timeout: 10_000, retries: 1 });
    return response.ok;
  } catch {
    return false;
  }
}

export function loadModel() {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (saved && PROVIDERS[saved.kind]) return { base: "", key: "", model: "", ...saved };
  } catch {
    // Испорченная запись — как будто модели нет.
  }
  return null;
}

export function saveModel(model) {
  if (model) localStorage.setItem(KEY, JSON.stringify(model));
  else localStorage.removeItem(KEY);
}

const baseOf = (model) => (model.base || PROVIDERS[model.kind]?.base || "").replace(/\/+$/, "");

function headers(model) {
  const out = { "Content-Type": "application/json" };
  if (model.key) out.Authorization = `Bearer ${model.key}`;
  if (model.kind === "openrouter") {
    // OpenRouter показывает в статистике, откуда запросы.
    out["HTTP-Referer"] = location.origin;
    out["X-Title"] = "NOAH";
  }
  return out;
}

/** Текст ошибки провайдера по-человечески: что сделать, а не код ответа. */
function explainFailure(model, status, body) {
  const said = body?.error?.message ?? body?.message ?? "";
  if (model.kind === "bridge" && (status === 401 || status === 403)) return said || "Мост доступен только владельцу — войдите на сайт своим аккаунтом.";
  if (model.kind === "bridge" && status === 502) return `Мост: ${said || "сбой"}. Если это «вход истёк» — войдите в Claude Code на сервере заново.`;
  if (status === 401 || status === 403) return "Ключ не подошёл — проверьте его в настройках модели.";
  if (status === 402) return "На счёте провайдера закончились деньги — пополните или выберите бесплатную модель.";
  if (status === 404) return `Модель «${model.model}» не найдена у провайдера — выберите другую.`;
  if (status === 429) return "Провайдер просит подождать: слишком много запросов. Попробуйте через минуту.";
  return said ? `Провайдер ответил: ${String(said).slice(0, 200)}` : `Провайдер ответил ошибкой ${status}.`;
}

/** Иероглифы, кана, хангыль: в русском ответе их быть не должно. */
const FOREIGN = /[぀-ヿ㐀-鿿가-힯＀-￯]/;
/** Чужой кусок до конца фразы: половина предложения на китайском бесполезна. */
const FOREIGN_RUN = /[぀-ヿ㐀-鿿가-힯＀-￯][^\n.!?]*[.!?]?/g;

/**
 * Один запрос к модели. `json` — просить ответ-объект (где провайдер умеет);
 * `maxTokens` — потолок длины.
 */
export async function chat(model, messages, { json = false, maxTokens = 700, signal, timeoutMs = 90_000, retried = false } = {}) {
  if (!model?.model) throw new AiError("Модель не выбрана — выберите её в настройках.", { kind: "config" });
  const base = baseOf(model);
  if (!base) throw new AiError("Не указан адрес модели.", { kind: "config" });
  const body = { model: model.model, messages, temperature: 0.2, max_tokens: maxTokens };
  // Мост зовёт программу подписки на сервере: холодный старт и ответ — до трёх минут.
  if (model.kind === "bridge") timeoutMs = Math.max(timeoutMs, 200_000);
  if (json) body.response_format = { type: "json_object" };

  const timeout = new AbortController();
  const timer = setTimeout(() => timeout.abort(), timeoutMs);
  const abort = () => timeout.abort();
  signal?.addEventListener("abort", abort);
  try {
    let response;
    try {
      response = await siteFetch(`${base}/chat/completions`, {
        method: "POST",
        headers: headers(model),
        body: JSON.stringify(body),
        signal: timeout.signal,
        credentials: "same-origin",
      });
    } catch (err) {
      if (signal?.aborted) throw new AiError("Запрос отменён", { kind: "abort" });
      if (timeout.signal.aborted) throw new AiError("Модель не успела ответить.", { kind: "timeout" });
      throw new AiError(
        model.kind === "bridge"
          ? "Мост не отвечает — попробуйте ещё раз через минуту."
          : model.kind === "ollama"
          ? "Ollama не отвечает. Запустите её и разрешите сайту к ней обращаться — как, написано в настройках модели."
          : "Не достучаться до провайдера — проверьте адрес и интернет.",
        { kind: "backend" },
      );
    }
    const data = await response.json().catch(() => null);
    // Не все провайдеры понимают response_format — тогда без него.
    if (!response.ok && json && response.status === 400) {
      return chat(model, messages, { json: false, maxTokens, signal, timeoutMs, retried });
    }
    if (!response.ok) throw new AiError(explainFailure(model, response.status, data), { kind: "http", status: response.status });
    const text = data?.choices?.[0]?.message?.content;
    if (typeof text !== "string") throw new AiError("Модель прислала ответ, который не разобрать.", { kind: "parse" });
    // Рассуждающие модели присылают ход мысли в <think> — человеку он не нужен.
    const clean = text.replace(/<think>[\s\S]*?<\/think>/g, "").trim();
    // Маленькие модели срываются на китайский посреди русского ответа. Как в
    // программе: переспросить один раз, а если снова — вычистить чужие знаки.
    if (FOREIGN.test(clean) && !messages.some((m) => FOREIGN.test(m.content))) {
      if (!retried) return chat(model, messages, { json, maxTokens, signal, timeoutMs, retried: true });
      return clean.replace(FOREIGN_RUN, "").replace(/[ \t]{2,}/g, " ").trim();
    }
    return clean;
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener("abort", abort);
  }
}

/** Какие модели есть у провайдера — для списка в настройках. */
export async function listModels(model) {
  const base = baseOf(model);
  if (!base) return [];
  const response = await fetch(`${base}/models`, { headers: headers(model), credentials: "same-origin" });
  if (!response.ok) throw new AiError(explainFailure(model, response.status, null), { kind: "http", status: response.status });
  const data = await response.json();
  // У моста у каждой модели есть близнец «:free» — в списке он лишний.
  const items = (data?.data ?? []).filter((m) => model.kind !== "bridge" || !String(m.id).endsWith(":free")).map((m) => ({
    id: String(m.id),
    free: model.kind === "ollama" || model.kind === "bridge" || String(m.id).endsWith(":free") || (m.pricing && Number(m.pricing.prompt) === 0 && Number(m.pricing.completion) === 0),
  }));
  // Бесплатные — вверху: с них удобно начать.
  return items.sort((a, b) => Number(b.free) - Number(a.free) || a.id.localeCompare(b.id));
}

/* ── Словарь: то же, что объяснение выделенного в программе ─────────────── */

const EXPLAIN_RULES =
  "Ты объясняешь термин, который выделил пользователь. Ответь одним-двумя предложениями обычным текстом, " +
  "по-русски — даже если сам термин на другом языке. Объясняй в том смысле, в каком термин стоит в контексте " +
  "(текст вокруг): «ядро» в статье про Linux — ядро системы, а не ядро ореха. Если выделена фраза или " +
  "предложение — объясни её смысл в этом тексте. Только объяснение: без вступлений, без списков, без разметки и без JSON.";

/**
 * Клиент для окна объяснения (popup-view.js): explain и ask, как у программы.
 * Модель берётся в момент запроса — сменили в настройках, и следующий вопрос
 * уже идёт к новой.
 */
export function dictionaryClient(name = "Ноа") {
  const current = () => {
    const model = loadModel();
    if (!model) throw new AiError("Модель не подключена — подключите её на главной странице Ноа.", { kind: "config" });
    return model;
  };
  return {
    async explain(term, context, { signal } = {}) {
      const def = await chat(
        current(),
        [
          { role: "system", content: EXPLAIN_RULES },
          { role: "user", content: `Термин: «${term}».\nКонтекст: ${context}` },
        ],
        { signal, maxTokens: 300 },
      );
      return { def, simple: "", examples: [] };
    },
    async ask(term, context, thread, question, { signal } = {}) {
      const persona = term.trim()
        ? `Тебя зовут ${name}. Пользователь уточняет то, о чём шла речь, — «${term}». Отвечай одной-двумя короткими фразами, разговорно, обычным текстом, без JSON и без предложений помочь ещё. Отвечай по-русски, даже если сам термин на другом языке.`
        : `Тебя зовут ${name}. Отвечай по делу и коротко, живым языком, по-русски.`;
      const messages = [{ role: "system", content: context ? `${persona}\nКонтекст: ${context}` : persona }];
      for (const item of thread ?? []) messages.push({ role: "user", content: item.q }, { role: "assistant", content: item.a });
      messages.push({ role: "user", content: question });
      return chat(current(), messages, { signal, maxTokens: 500 });
    },
  };
}
