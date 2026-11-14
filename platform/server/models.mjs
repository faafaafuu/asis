// Какие модели посоветовать человеку, который подключает OpenRouter.
//
// Каталог OpenRouter — почти 500 моделей и полмегабайта JSON: телефону из
// России его не скачать (канал до зарубежных серверов рвёт ответы на 32 КБ),
// да и выбирать из сотни названий — не дело человека. Сервер раз в несколько
// часов забирает каталог и отдаёт короткий список: свежая версия каждого
// хорошего семейства, цена и примерная цена курса.

const CATALOG = "https://openrouter.ai/api/v1/models";
const FRESH_MS = 6 * 60 * 60_000;

/** Семейства, которые хорошо пишут по-русски и держат формат курса, — по убыванию качества. */
const FAMILIES = [
  { re: /^anthropic\/claude-haiku-[\d.]+$/, label: "Claude Haiku", hint: "лучше всех пишет уроки и держит тему курса" },
  { re: /^google\/gemini-[\d.]+-flash-lite$/, label: "Gemini Flash Lite", hint: "быстрый и дешёвый" },
  { re: /^deepseek\/deepseek-v[\d.]+-flash$/, label: "DeepSeek Flash", hint: "почти бесплатно, пишет неплохо" },
  { re: /^openai\/gpt-[\d.]+-mini$/, label: "GPT mini", hint: "от OpenAI, надёжный" },
];
/** Бесплатные — для простых тем; слабее, могут путать тему. */
const FREE = [/^google\/gemma-\d+-31b-it:free$/, /^nvidia\/nemotron-[\d.]+-lightning:free$/, /^google\/gemma-[\d.]+-\d+b-it:free$/];

/** Сколько токенов уходит на курс из пяти–шести тем: запросы и ответы. */
const COURSE_IN = 300_000;
const COURSE_OUT = 200_000;

let cache = { at: 0, data: null };

/** Номер версии из названия: «gemini-3.1-flash-lite» → [3, 1]. */
const version = (id) => (id.match(/\d+(?:\.\d+)*/g) ?? []).join(".").split(".").map(Number);
const newer = (a, b) => {
  const [x, y] = [version(a), version(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (y[i] ?? 0) - (x[i] ?? 0);
  return 0;
};

/** Короткий список по каталогу OpenRouter. */
export function recommend(catalog) {
  const models = (Array.isArray(catalog) ? catalog : []).filter((m) => typeof m?.id === "string" && !/:(batch|thinking|extended)$/.test(m.id));
  const price = (m) => ({
    prompt: Number(m.pricing?.prompt ?? 0) * 1_000_000,
    completion: Number(m.pricing?.completion ?? 0) * 1_000_000,
  });
  const out = [];
  for (const family of FAMILIES) {
    const best = models.filter((m) => family.re.test(m.id)).sort((a, b) => newer(a.id, b.id))[0];
    if (!best) continue;
    const { prompt, completion } = price(best);
    const course = (prompt * COURSE_IN + completion * COURSE_OUT) / 1_000_000;
    out.push({ id: best.id, label: family.label, hint: family.hint, prompt, completion, course: Math.round(course * 100) / 100, free: false });
  }
  for (const re of FREE) {
    const found = models.find((m) => re.test(m.id));
    if (found) {
      out.push({ id: found.id, label: "Бесплатная", hint: "без оплаты, но слабее: для простых тем, может путать тему", prompt: 0, completion: 0, course: 0, free: true });
      break;
    }
  }
  return out;
}

export function mountModels({ route }) {
  // Открыто без входа: это только цены и названия моделей.
  route("GET", /^\/api\/qrack\/models\/openrouter$/, async () => {
    if (cache.data && Date.now() - cache.at < FRESH_MS) return { models: cache.data };
    try {
      const response = await fetch(CATALOG, { signal: AbortSignal.timeout(20_000) });
      const data = await response.json();
      const list = recommend(data?.data);
      if (list.length) cache = { at: Date.now(), data: list };
    } catch (err) {
      console.error("каталог OpenRouter не пришёл:", err?.message ?? err);
    }
    return { models: cache.data ?? [] };
  });
}
