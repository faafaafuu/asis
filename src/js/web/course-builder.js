// Сборка курса моделью — прямо в Ноа, без внешней нейросети.
//
// Человек пишет, о чём курс и зачем; дальше — по методике course_format.md,
// той же, что читает нейросеть по MCP:
//   план → по каждой теме урок, потом понятия, карточки, задачи и экзамен →
//   финальный экзамен на стык тем.
// Урок и остальное пишутся двумя заходами: урок — обычным текстом (в JSON
// длинный текст с кодом ломается на кавычках), понятия и вопросы — по готовому
// уроку, чтобы спрашивалось то, что в нём объяснено.
//
// Каждая тема проверяется тем же validate, что курс из MCP, и сохраняется
// сразу: курс появляется у человека после первой темы и дорастает сам. Модуль
// без DOM и сети — модель и сохранение приходят снаружи, поэтому он работает
// и на сервере (через мост), и в браузере (через модель человека).

import { validate, normalizeCourse } from "./learn-core.js";

/* ── Мелочи ─────────────────────────────────────────────────────────────── */

const str = (value) => (typeof value === "string" ? value : "");
const list = (value) => (Array.isArray(value) ? value : []);
const chars = (text) => [...String(text ?? "")].length;

const TRANSLIT = {
  а: "a", б: "b", в: "v", г: "g", д: "d", е: "e", ё: "e", ж: "zh", з: "z", и: "i", й: "y", к: "k", л: "l",
  м: "m", н: "n", о: "o", п: "p", р: "r", с: "s", т: "t", у: "u", ф: "f", х: "h", ц: "ts", ч: "ch", ш: "sh",
  щ: "sch", ъ: "", ы: "y", ь: "", э: "e", ю: "yu", я: "ya",
};

/** id из чего угодно: латиница, цифры, дефис. Кириллица — транслитом. */
export function slug(text, fallback = "x") {
  const out = [...String(text ?? "").toLowerCase()]
    .map((ch) => TRANSLIT[ch] ?? ch)
    .join("")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40)
    .replace(/-+$/g, "");
  return out || fallback;
}

/** Короче `max` знаков: по концу предложения, а если его нет — с многоточием. */
export function cut(text, max) {
  const clean = str(text).trim();
  if (chars(clean) <= max) return clean;
  const head = [...clean].slice(0, max).join("");
  const end = Math.max(head.lastIndexOf(". "), head.lastIndexOf("! "), head.lastIndexOf("? "));
  if (end > max * 0.5) return head.slice(0, end + 1);
  return `${[...clean].slice(0, max - 1).join("").trimEnd()}…`;
}

/** Первый объект JSON в ответе модели: его любят обернуть текстом и ```. */
export function jsonIn(raw) {
  const text = String(raw ?? "");
  const start = text.indexOf("{");
  const end = text.lastIndexOf("}");
  if (start < 0 || end <= start) return null;
  const body = text.slice(start, end + 1);
  for (const candidate of [body, body.replace(/,\s*([}\]])/g, "$1")]) {
    try {
      return JSON.parse(candidate);
    } catch {
      /* следующая попытка — без висячих запятых */
    }
  }
  return null;
}

/**
 * Раздел методички по заголовку — модели нужен не весь текст, а свой кусок.
 * `shallow` — без подразделов: для плана из «Темы» нужен только скелет.
 */
export function formatPart(format, title, { shallow = false } = {}) {
  const lines = String(format ?? "").replace(/\r/g, "").split("\n");
  const start = lines.findIndex((line) => /^#{2,3} /.test(line) && line.replace(/^#+\s*/, "").trim() === title);
  if (start < 0) return "";
  const level = lines[start].match(/^#+/)[0].length;
  let end = lines.length;
  let fence = false;
  for (let at = start + 1; at < lines.length; at++) {
    if (lines[at].startsWith("```")) fence = !fence;
    const heading = !fence && lines[at].match(/^(#+) /);
    if (heading && (shallow || heading[1].length <= level)) {
      end = at;
      break;
    }
  }
  return lines.slice(start, end).join("\n").trim();
}

/** Урок без обёртки: модели иногда кладут весь ответ в ```markdown. */
function cleanLesson(text) {
  let lesson = String(text ?? "").replace(/\r/g, "").trim();
  if (/^```/.test(lesson)) lesson = lesson.replace(/^```[a-z]*\n?/i, "").replace(/\n?```\s*$/, "");
  return lesson.trim();
}

const sectionCount = (lesson) => lesson.split("\n").filter((line) => line.startsWith("## ")).length;

/* ── Подсказки модели ───────────────────────────────────────────────────── */

function planRules(format) {
  return [
    "Ты — методист NOAH: собираешь учебный курс для приложения Ноа по методике ниже. Сейчас нужен только план курса.",
    formatPart(format, "Порядок работы"),
    formatPart(format, "Курс"),
    formatPart(format, "Тема", { shallow: true }),
    [
      "Составь план по просьбе человека:",
      "- 3–10 тем по объёму: узкая область — 3–4 темы, профессия с нуля — 6–10. Тема — одна связная область на 6–15 понятий; от простого к сложному, каждая следующая опирается на прошлые.",
      '- id курса и тем — латиница, цифры и дефис, коротко и по смыслу: "cabling", "cctv-basics".',
      "- summary темы — одна строка: что человек поймёт и сможет после неё.",
      "- aliases — как курс и тему назовут голосом, 1–3 варианта по-русски.",
      "- description — для кого курс и что внутри, два предложения.",
      "Пиши по-русски. Верни только JSON, без пояснений:",
      '{"id":"…","title":"…","aliases":["…"],"description":"…","topics":[{"id":"…","title":"…","aliases":["…"],"summary":"…"}]}',
    ].join("\n"),
  ]
    .filter(Boolean)
    .join("\n\n");
}

function outline(plan, current) {
  return plan.topics.map((t, at) => `${at + 1}. ${t.title} — ${t.summary}${t.id === current ? "   ← эта тема" : ""}`).join("\n");
}

function lessonRules(format, plan, topic, earlier) {
  const known = earlier.length
    ? `\n\nПонятия прошлых тем — их не объясняй заново, опирайся на них и ссылайся:\n${earlier.map((c) => `- ${c.term}`).join("\n")}`
    : "";
  return [
    "Ты — методист NOAH: пишешь урок одной темы учебного курса по методике ниже.",
    formatPart(format, "Урок"),
    `Курс «${plan.title}». ${plan.description}\n\nТемы курса по порядку:\n${outline(plan, topic.id)}${known}`,
    [
      "Требования к уроку этой темы:",
      "- Перед первым разделом — 2–3 предложения вступления: о чём тема и зачем она.",
      "- 4–7 разделов «## …», в каждом одна мысль: что это → зачем → как устроено внутри (кто что делает и в каком порядке) → живой пример → где ошибаются.",
      "- 1500–2500 слов. Ключевые термины — **жирным**, команды и конфиги — в блоках ```.",
      "- Всё, о чём потом спросят, в разделе объяснено, а не только названо.",
      "- Не повторяй прошлые темы — продолжай с того места, где они закончились.",
      "Пиши по-русски. Верни только текст урока в Markdown — без вступлений вроде «Вот урок» и без ``` вокруг всего ответа.",
    ].join("\n"),
  ]
    .filter(Boolean)
    .join("\n\n");
}

function conceptRules(format, topic, earlier) {
  const known = earlier.length
    ? `Понятия прошлых тем — для связей related вида "тема/id" (бери только отсюда):\n${earlier.map((c) => `- ${c.key} — ${c.term}`).join("\n")}`
    : "Прошлых тем нет — related только внутри этой темы.";
  return [
    "Ты — методист NOAH: по готовому уроку составляешь понятия и карточки темы по методике ниже.",
    formatPart(format, "Понятие"),
    formatPart(format, "Карточка"),
    `Тема: id "${topic.id}", «${topic.title}».\n${known}`,
    [
      "Требования:",
      "- concepts: 6–12 понятий, объяснённых в уроке; id — латиница и дефис; term — точно как в тексте урока; definition — одна мысль до 300 знаков; mnemonic или analogy — у каждого; pitfall — хотя бы у половины; related — 2–4 связи: id понятия этой темы или \"тема/id\" из списка выше.",
      "- cards: 2–4 карточки — сравнения, последовательности, команды; back до 300 знаков; concept — id понятия этой темы.",
      "Пиши по-русски. Верни только JSON, без пояснений:",
      '{"concepts":[{"id":"…","term":"…","definition":"…","mnemonic":"…","analogy":"…","example":"…","pitfall":"…","related":["…"]}],"cards":[{"front":"…","back":"…","concept":"…"}]}',
    ].join("\n"),
  ]
    .filter(Boolean)
    .join("\n\n");
}

function questionRules(format, topic, concepts) {
  return [
    "Ты — методист NOAH: по готовому уроку составляешь практические задачи и мини-экзамен темы по методике ниже.",
    formatPart(format, "Вопрос"),
    formatPart(format, "Что проверяет Ноа"),
    `Тема: id "${topic.id}", «${topic.title}». Понятия темы (id — термин):\n${concepts.map((c) => `- ${c.id} — ${c.term}`).join("\n")}`,
    [
      "Требования:",
      "- tasks: 3–5 практических задач — сценарии, «что сделаешь», «найди ошибку»; из них 2–3 open.",
      "- exam: 6–8 вопросов мини-экзамена, не меньше половины open.",
      "- У каждого вопроса concept — id понятия из списка выше.",
      "- choice: 3–4 правдоподобных варианта, answer — номер верного, СЧИТАЯ С НУЛЯ; explain — почему так.",
      "- open: points — 2–4 пункта-мысли, reference — образцовый ответ.",
      "Пиши по-русски. Верни только JSON, без пояснений:",
      '{"tasks":[{"kind":"choice","q":"…","options":["…"],"answer":0,"explain":"…","concept":"…"},{"kind":"open","q":"…","points":["…"],"reference":"…","concept":"…"}],"exam":[…]}',
    ].join("\n"),
  ]
    .filter(Boolean)
    .join("\n\n");
}

/** Замечание проверки — про вопросы (иначе про понятия и карточки). */
const aboutQuestions = (problem, tid) =>
  problem.startsWith(`${tid}-t`) || problem.startsWith(`${tid}-e`) || /\((tasks|exam)\)/.test(problem);

function finalRules(format, plan, all) {
  return [
    "Ты — методист NOAH. Составь финальный экзамен курса — вопросы на стык тем.",
    formatPart(format, "Вопрос"),
    `Курс «${plan.title}». Понятия курса ("тема/id — термин"):\n${all.map((c) => `- ${c.key} — ${c.term}`).join("\n")}`,
    [
      "Требования:",
      "- 4–6 вопросов; каждый связывает две-три темы: «как X и Y вместе…», «что будет с Z, если…».",
      "- Не меньше половины open.",
      '- concept — "тема/id" из списка выше.',
      "Пиши по-русски. Верни только JSON, без пояснений:",
      '{"final":[{"kind":"open","q":"…","points":["…"],"reference":"…","concept":"тема/id"}]}',
    ].join("\n"),
  ]
    .filter(Boolean)
    .join("\n\n");
}

/* ── Приведение ответа модели к формату ─────────────────────────────────── */

/**
 * Вопрос в формате курса или null, если его не спасти: у choice нет верного
 * варианта, у open — пунктов или образца.
 */
function shapeQuestion(q, id, resolveConcept) {
  const options = list(q?.options).map((o) => String(o).trim()).filter(Boolean);
  const kind = q?.kind === "choice" || q?.kind === "open" ? q.kind : options.length >= 2 ? "choice" : "open";
  const text = str(q?.q).trim();
  if (!text) return null;
  const base = { id, kind, q: text, options: [], answer: null, explain: "", points: [], reference: "", concept: resolveConcept(str(q?.concept)) };
  if (kind === "choice") {
    let answer = q?.answer;
    if (typeof answer === "string") answer = /^\d+$/.test(answer.trim()) ? Number(answer) : options.indexOf(answer.trim());
    if (options.length < 2 || !Number.isInteger(answer) || answer < 0 || answer >= options.length) return null;
    return { ...base, options, answer, explain: str(q?.explain).trim() };
  }
  const points = list(q?.points).map((p) => String(p).trim()).filter(Boolean);
  const reference = str(q?.reference).trim();
  if (!points.length || !reference) return null;
  return { ...base, points, reference };
}

/**
 * Тема из урока и ответа модели. id приводятся к формату и делаются
 * уникальными, связи — только в существующие понятия, длинное — укорачивается.
 * `known` — понятия прошлых тем: ключ «тема/id».
 */
export function shapeTopic(planTopic, lesson, raw, known) {
  const tid = planTopic.id;
  const used = new Set();
  const byRaw = new Map();
  const concepts = [];
  for (const c of list(raw?.concepts)) {
    const term = str(c?.term).trim();
    const definition = cut(str(c?.definition), 320);
    if (!term || !definition) continue;
    let id = slug(str(c?.id) || term, `c${concepts.length + 1}`);
    while (used.has(id)) id = `${id}-2`;
    used.add(id);
    if (str(c?.id).trim()) {
      byRaw.set(str(c.id).trim(), id);
      byRaw.set(slug(c.id), id);
    }
    byRaw.set(term.toLowerCase(), id);
    concepts.push({
      id,
      term,
      definition,
      mnemonic: str(c?.mnemonic).trim(),
      analogy: str(c?.analogy).trim(),
      example: str(c?.example).trim(),
      pitfall: str(c?.pitfall).trim(),
      related: list(c?.related).map(String),
    });
  }
  const local = (ref) => {
    const clean = String(ref ?? "").trim();
    if (!clean) return null;
    return byRaw.get(clean) ?? byRaw.get(clean.toLowerCase()) ?? (used.has(slug(clean)) ? slug(clean) : null);
  };
  /** Связь или понятие вопроса: своё — по id, чужое — «тема/id» из прошлых тем. */
  const resolve = (ref) => {
    const clean = String(ref ?? "").trim();
    if (!clean) return null;
    if (clean.includes("/")) {
      const [t, x] = clean.split(/\/(.*)/s);
      if (t === tid || slug(t) === tid) return local(x);
      if (known.has(clean)) return clean;
      const guess = `${slug(t)}/${slug(x)}`;
      return known.has(guess) ? guess : null;
    }
    return local(clean);
  };
  for (const c of concepts) {
    c.related = [...new Set(c.related.map(resolve).filter((ref) => ref && ref !== c.id))].slice(0, 4);
  }
  const conceptOf = (ref) => resolve(ref) ?? "";
  const cards = list(raw?.cards)
    .map((card, at) => ({
      id: `k${at + 1}`,
      front: str(card?.front).trim(),
      back: cut(str(card?.back), 320),
      concept: local(card?.concept) ?? "",
    }))
    .filter((card) => card.front && card.back);
  const tasks = list(raw?.tasks)
    .map((q, at) => shapeQuestion(q, `${tid}-t${at + 1}`, conceptOf))
    .filter(Boolean);
  const exam = list(raw?.exam)
    .map((q, at) => shapeQuestion(q, `${tid}-e${at + 1}`, conceptOf))
    .filter(Boolean);
  return {
    id: tid,
    title: planTopic.title,
    aliases: list(planTopic.aliases).map(String).slice(0, 4),
    summary: planTopic.summary,
    lesson,
    concepts,
    cards,
    tasks,
    exam,
    cheatsheet: "",
  };
}

/** План из ответа модели: id по формату и без повторов, 3–10 тем. */
export function shapePlan(raw, taken = new Set()) {
  const title = str(raw?.title).trim();
  const topics = [];
  const ids = new Set();
  for (const t of list(raw?.topics)) {
    const topicTitle = str(t?.title).trim();
    if (!topicTitle) continue;
    let id = slug(str(t?.id) || topicTitle, `topic-${topics.length + 1}`);
    while (ids.has(id)) id = `${id}-2`;
    ids.add(id);
    topics.push({ id, title: topicTitle, summary: str(t?.summary).trim() || topicTitle, aliases: list(t?.aliases).map(String).slice(0, 4) });
    if (topics.length >= 10) break;
  }
  if (!title || topics.length < 2) return null;
  let id = slug(str(raw?.id) || title, "course");
  const base = id;
  for (let n = 2; taken.has(id); n++) id = `${base}-${n}`;
  return {
    id,
    title,
    aliases: list(raw?.aliases).map(String).slice(0, 4),
    description: str(raw?.description).trim() || title,
    topics,
  };
}

/* ── Сборка ─────────────────────────────────────────────────────────────── */

export class Stopped extends Error {}

/**
 * Собирает курс, сохраняя после каждой темы.
 *
 * @param {{
 *   goal: string,
 *   format: string,
 *   chat: (messages: {role: string, content: string}[], opts?: {maxTokens?: number, json?: boolean, long?: boolean}) => Promise<string>,
 *   save: (course: object) => Promise<void> | void,
 *   progress?: (state: {stage: string, done: number, total: number, title?: string, courseId?: string, message: string}) => void,
 *   stopped?: () => boolean,
 *   resume?: object,
 *   taken?: Set<string>,
 * }} deps
 * `long` у запроса — ответ большой (урок, понятия, вопросы): ждать его
 * дольше обычного.
 * `resume` — сохранённый недособранный курс (с полем building): сборка
 * продолжится со следующей темы. `taken` — id курсов, которые уже есть.
 */
export async function buildCourse({ goal, format, chat, save, progress = () => {}, stopped = () => false, resume = null, taken = new Set() }) {
  const check = () => {
    if (stopped()) throw new Stopped("Сборка остановлена.");
  };
  /** Модель с одной повторной попыткой: мост иногда не успевает. */
  const ask = async (messages, opts) => {
    try {
      return await chat(messages, opts);
    } catch (err) {
      check();
      return chat(messages, opts);
    }
  };

  let plan;
  let built = [];
  let failed = [];
  if (resume?.building?.plan) {
    plan = { id: resume.id, title: resume.title, aliases: list(resume.aliases), description: resume.description, topics: resume.building.plan };
    built = list(resume.topics);
    failed = list(resume.building.failed);
    goal = resume.building.goal || goal;
  } else {
    progress({ stage: "plan", done: 0, total: 0, message: "Составляю план курса" });
    for (let attempt = 0; attempt < 2 && !plan; attempt++) {
      check();
      const raw = jsonIn(await ask([{ role: "system", content: planRules(format) }, { role: "user", content: goal }], { json: true, maxTokens: 2500 }));
      plan = shapePlan(raw, taken);
    }
    if (!plan) throw new Error("Модель не прислала план курса — попробуйте сформулировать тему иначе.");
  }

  const total = plan.topics.length;
  const building = () => ({ goal, plan: plan.topics, total, done: built.length, failed });
  const course = (extra = {}) => ({ id: plan.id, title: plan.title, aliases: plan.aliases, description: plan.description, final: [], topics: built, ...extra });
  const knownConcepts = () =>
    built.flatMap((t) => list(t.concepts).map((c) => ({ key: `${t.id}/${c.id}`, term: c.term, definition: c.definition })));

  for (const [at, planTopic] of plan.topics.entries()) {
    if (built.some((t) => t.id === planTopic.id) || failed.includes(planTopic.title)) continue;
    const step = `${at + 1} из ${total}`;
    const earlier = knownConcepts();
    const known = new Set(earlier.map((c) => c.key));

    // Урок.
    let lesson = "";
    for (let attempt = 0; attempt < 2; attempt++) {
      check();
      progress({ stage: "lesson", done: built.length, total, title: plan.title, courseId: plan.id, message: `Пишу урок «${planTopic.title}» (${step})` });
      const text = cleanLesson(
        await ask(
          [
            { role: "system", content: lessonRules(format, plan, planTopic, earlier) },
            { role: "user", content: `Напиши урок темы «${planTopic.title}»: ${planTopic.summary}\n\nЧто человек хотел от курса: ${goal}` },
          ],
          { maxTokens: 8000, long: true },
        ),
      );
      if (sectionCount(text) >= 3 && chars(text) >= 1500) {
        lesson = text;
        break;
      }
      if (sectionCount(text) >= 3 && chars(text) > chars(lesson)) lesson = text;
    }
    if (!lesson) {
      failed.push(planTopic.title);
      continue;
    }

    // Понятия с карточками и вопросы — отдельными заходами: одним ответом
    // модель не укладывается в отведённое время. Не прошла проверка — заново
    // только та часть, к которой замечания.
    let topic = null;
    let conceptsRaw = null;
    let questionsRaw = null;
    let problems = [];
    for (let attempt = 0; attempt < 3 && !topic; attempt++) {
      const retry = (list) =>
        list.length ? `\n\nПрошлый ответ не прошёл проверку Ноа — исправь это:\n- ${list.slice(0, 15).join("\n- ")}` : "";
      const questionIssues = problems.filter((p) => aboutQuestions(p, planTopic.id));
      const conceptIssues = problems.filter((p) => !aboutQuestions(p, planTopic.id));
      if (!conceptsRaw || conceptIssues.length) {
        check();
        progress({ stage: "body", done: built.length, total, title: plan.title, courseId: plan.id, message: `Составляю понятия «${planTopic.title}» (${step})` });
        conceptsRaw = jsonIn(
          await ask(
            [
              { role: "system", content: conceptRules(format, planTopic, earlier) },
              { role: "user", content: `Урок темы «${planTopic.title}»:\n\n${lesson}${retry(conceptIssues)}` },
            ],
            { json: true, maxTokens: 6000, long: true },
          ),
        );
        questionsRaw = null;
        if (!conceptsRaw) {
          problems = ["ответ — не JSON; верни только объект {\"concepts\":…,\"cards\":…}"];
          continue;
        }
      }
      const concepts = shapeTopic(planTopic, lesson, conceptsRaw, known).concepts;
      if (!questionsRaw || questionIssues.length) {
        check();
        progress({ stage: "body", done: built.length, total, title: plan.title, courseId: plan.id, message: `Составляю задачи и экзамен «${planTopic.title}» (${step})` });
        questionsRaw = jsonIn(
          await ask(
            [
              { role: "system", content: questionRules(format, planTopic, concepts) },
              { role: "user", content: `Урок темы «${planTopic.title}»:\n\n${lesson}${retry(questionIssues)}` },
            ],
            { json: true, maxTokens: 6000, long: true },
          ),
        );
        if (!questionsRaw) {
          problems = [`${planTopic.id}-t: ответ — не JSON; верни только объект {"tasks":…,"exam":…}`];
          continue;
        }
      }
      const candidate = shapeTopic(planTopic, lesson, { ...conceptsRaw, ...questionsRaw }, known);
      problems = validate(course({ topics: [...built, candidate] }));
      if (!problems.length) topic = candidate;
    }
    if (!topic) {
      failed.push(planTopic.title);
      progress({ stage: "body", done: built.length, total, title: plan.title, courseId: plan.id, message: `Тема «${planTopic.title}» не прошла проверку — пропускаю` });
      continue;
    }
    built = [...built, topic];
    check();
    await save(course({ building: building() }));
    progress({ stage: "topic", done: built.length, total, title: plan.title, courseId: plan.id, message: `Готова тема «${planTopic.title}» (${step})` });
  }

  if (!built.length) throw new Error("Ни одна тема не прошла проверку — попробуйте ещё раз или другую формулировку.");

  // Финальный экзамен на стык тем. Не вышел — курс работает и без него.
  let final = [];
  if (built.length >= 2) {
    check();
    progress({ stage: "final", done: built.length, total, title: plan.title, courseId: plan.id, message: "Составляю финальный экзамен" });
    const all = knownConcepts();
    const keys = new Set(all.map((c) => c.key));
    try {
      const raw = jsonIn(await ask([{ role: "system", content: finalRules(format, plan, all) }, { role: "user", content: "Составь финальный экзамен." }], { json: true, maxTokens: 4000, long: true }));
      final = list(raw?.final)
        .map((q, at) => shapeQuestion(q, `final-${at + 1}`, (ref) => (keys.has(String(ref).trim()) ? String(ref).trim() : "")))
        .filter(Boolean);
      if (validate(course({ final })).length) final = [];
    } catch {
      final = [];
    }
  }

  const done = course({ final });
  await save(failed.length ? { ...done, skipped: failed } : done);
  progress({
    stage: "done",
    done: built.length,
    total,
    title: plan.title,
    courseId: plan.id,
    message: failed.length ? `Курс готов: тем ${built.length} из ${total}; не вышли: ${failed.join(", ")}` : `Курс готов: тем ${built.length}`,
  });
  return normalizeCourse(done);
}
