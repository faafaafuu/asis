// Обучение без программы: то же, что learning.rs, recall.rs, srs.rs, focus.rs
// и tutor.rs делают на компьютере, но на JS — для Ноа в браузере.
//
// Ни DOM, ни сети здесь нет: курсы, хранилище прогресса и модель приходят
// снаружи. Поэтому модуль работает и в браузере, и в Node (сайт проверяет им
// курсы, пришедшие по MCP).
//
// Прогресс хранится в том же виде, что learning.json программы: ключи, поля и
// единицы те же. Так прогресс можно перенести между компьютером и сайтом.
// Поведение повторяет Rust-версию; где правило описано там подробно, здесь
// только ссылка на него.

/* ── Константы — как в learning.rs и recall.rs ─────────────────────────── */

export const TOPIC_PASS = 70;
export const FINAL_PASS = 75;
const FINAL_PER_TOPIC = 2;
const RIGHT = 60;
const MIN_CONCEPTS = 6;
const MAX_DEFINITION = 320;
const LESSON_WORDS = 1200;
const NEW_PER_DAY = 20;
const KEEP_SESSIONS = 200;
/** Сколько обменов обсуждения помнить (tutor.rs, DEPTH). */
const DEPTH = 6;
const MAX_SECTION = 6000;

/* ── Время ─────────────────────────────────────────────────────────────── */

const pad = (n) => String(n).padStart(2, "0");
const dayKey = (date) => `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
const chars = (text) => [...String(text ?? "")].length;
const lower = (text) => String(text ?? "").toLowerCase().replaceAll("ё", "е");

/* ── Курс: значения по умолчанию ───────────────────────────────────────── */

const str = (value) => (typeof value === "string" ? value : "");
const list = (value) => (Array.isArray(value) ? value : []);

function normalizeQuestion(q) {
  return {
    id: str(q?.id),
    kind: str(q?.kind),
    q: str(q?.q),
    options: list(q?.options).map(String),
    answer: Number.isInteger(q?.answer) ? q.answer : null,
    explain: str(q?.explain),
    points: list(q?.points).map(String),
    reference: str(q?.reference),
    concept: str(q?.concept),
  };
}

function normalizeTopic(t) {
  return {
    id: str(t?.id),
    title: str(t?.title),
    aliases: list(t?.aliases).map(String),
    summary: str(t?.summary),
    lesson: str(t?.lesson),
    tasks: list(t?.tasks).map(normalizeQuestion),
    exam: list(t?.exam).map(normalizeQuestion),
    concepts: list(t?.concepts).map((c) => ({
      id: str(c?.id),
      term: str(c?.term),
      definition: str(c?.definition),
      mnemonic: str(c?.mnemonic),
      analogy: str(c?.analogy),
      example: str(c?.example),
      pitfall: str(c?.pitfall),
      related: list(c?.related).map(String),
    })),
    cards: list(t?.cards).map((c) => ({ id: str(c?.id), front: str(c?.front), back: str(c?.back), concept: str(c?.concept) })),
    cheatsheet: str(t?.cheatsheet),
  };
}

/**
 * Курс ещё собирается: сколько тем готово из скольких. Сборка сохраняет курс
 * после каждой темы, и человек учится по готовым, пока пишутся остальные.
 */
function normalizeBuilding(b) {
  if (!b || typeof b !== "object") return null;
  const total = Math.max(Number.parseInt(b.total, 10) || 0, 0);
  return { total, done: Math.min(Math.max(Number.parseInt(b.done, 10) || 0, 0), total), failed: list(b.failed).map(String) };
}

/** Курс в полном виде: пропущенные поля — пустыми, как `#[serde(default)]`. */
export function normalizeCourse(c) {
  return {
    id: str(c?.id),
    title: str(c?.title),
    aliases: list(c?.aliases).map(String),
    description: str(c?.description),
    final: list(c?.final).map(normalizeQuestion),
    topics: list(c?.topics).map(normalizeTopic),
    building: normalizeBuilding(c?.building),
  };
}

/* ── Проверка курса — learning.rs, validate и advice ───────────────────── */

const idOk = (id) => id.trim() !== "" && /^[A-Za-z0-9_-]+$/.test(id);

/** Объяснено ли понятие в тексте (learning.rs, explained_in). */
export function explainedIn(text, definition) {
  const words = (value) =>
    lower(value)
      .split(/[^\p{L}\p{N}]+/u)
      .filter((word) => chars(word) >= 4)
      .map((word) => [...word].slice(0, 5).join(""));
  const known = new Set(words(text));
  const wanted = new Set(words(definition));
  if (!wanted.size) return true;
  return [...wanted].filter((word) => known.has(word)).length * 2 >= wanted.size;
}

/** Что не так с курсом. Пусто — курс годится. */
export function validate(raw) {
  const course = normalizeCourse(raw);
  const problems = [];
  if (!idOk(course.id)) problems.push("id курса — латиница, цифры, дефис или подчёркивание");
  if (!course.title.trim()) problems.push("у курса нет названия (title)");
  if (!course.topics.length) problems.push("в курсе нет тем (topics)");
  const topicIds = new Set();
  for (const topic of course.topics) {
    if (!idOk(topic.id)) problems.push(`тема «${topic.title}»: id — латиница, цифры, дефис`);
    if (topicIds.has(topic.id)) problems.push(`тема ${topic.id} повторяется`);
    topicIds.add(topic.id);
    if (chars(topic.lesson.trim()) < 200) problems.push(`${topic.id}: урок слишком короткий (меньше 200 знаков)`);
    const sections = topic.lesson.split("\n").filter((line) => line.startsWith("## ")).length;
    if (sections < 3) problems.push(`${topic.id}: в уроке меньше трёх разделов «## …» — урок читают кусками`);
    if (topic.concepts.length < MIN_CONCEPTS) {
      problems.push(`${topic.id}: понятий ${topic.concepts.length} — нужно не меньше ${MIN_CONCEPTS}: из них карточки, карта и шпаргалка`);
    }
    const conceptIds = new Set();
    for (const concept of topic.concepts) {
      if (!idOk(concept.id)) problems.push(`${topic.id}: у понятия «${concept.term}» id — латиница, цифры, дефис`);
      if (conceptIds.has(concept.id)) problems.push(`${topic.id}: понятие ${concept.id} повторяется`);
      conceptIds.add(concept.id);
      if (!concept.term.trim() || !concept.definition.trim()) problems.push(`${topic.id}/${concept.id}: нужны term и definition`);
      if (chars(concept.definition) > MAX_DEFINITION) {
        problems.push(`${topic.id}/${concept.id}: определение длиннее ${MAX_DEFINITION} знаков — сократи до одной мысли, детали — в урок или example`);
      }
    }
    for (const concept of topic.concepts) {
      for (const reference of concept.related) {
        const [t, c] = reference.includes("/") ? reference.split(/\/(.*)/s) : [topic.id, reference];
        const found = course.topics.find((known) => known.id === t)?.concepts.some((known) => known.id === c);
        if (!found) problems.push(`${topic.id}/${concept.id}: связь «${reference}» никуда не ведёт — id понятия этой темы или «тема/id»`);
      }
    }
    for (const card of topic.cards) {
      if (!card.front.trim() || !card.back.trim()) problems.push(`${topic.id}/${card.id}: у карточки нужны front и back`);
      if (chars(card.back) > MAX_DEFINITION) problems.push(`${topic.id}/${card.id}: ответ карточки длиннее ${MAX_DEFINITION} знаков`);
      if (card.concept && !topic.concepts.some((c) => c.id === card.concept)) {
        problems.push(`${topic.id}/${card.id}: понятия «${card.concept}» в теме нет`);
      }
    }
    if (!topic.tasks.length) problems.push(`${topic.id}: нет практических задач (tasks)`);
    if (!topic.exam.length) problems.push(`${topic.id}: нет вопросов мини-экзамена (exam)`);
  }
  const cardIds = new Set();
  for (const topic of course.topics) {
    for (const card of topic.cards) {
      const key = `${topic.id}/${card.id}`;
      if (cardIds.has(key)) problems.push(`${topic.id}: карточка ${card.id} повторяется`);
      cardIds.add(key);
    }
  }
  const ids = new Set();
  const questions = [
    ...course.topics.flatMap((topic) => [...topic.tasks, ...topic.exam].map((q) => [topic.id, q])),
    ...course.final.map((q) => [null, q]),
  ];
  for (const [home, q] of questions) {
    if (q.concept) {
      const [t, c] = q.concept.includes("/") ? q.concept.split(/\/(.*)/s) : [home, q.concept];
      const found = t && course.topics.find((known) => known.id === t)?.concepts.some((known) => known.id === c);
      if (!found) problems.push(`${q.id}: понятия «${q.concept}» нет — id понятия темы или «тема/id»`);
    }
    if (ids.has(q.id)) problems.push(`номер вопроса ${q.id} повторяется`);
    ids.add(q.id);
    if (!q.q.trim()) problems.push(`${q.id}: пустой вопрос`);
    if (q.kind === "choice") {
      if (q.options.length < 2) problems.push(`${q.id}: меньше двух вариантов`);
      if (!(q.answer !== null && q.answer < q.options.length)) problems.push(`${q.id}: answer — номер верного варианта, считая с нуля`);
    } else if (q.kind === "open") {
      if (!q.points.length) problems.push(`${q.id}: нет ключевых пунктов (points)`);
      if (!q.reference.trim()) problems.push(`${q.id}: нет образцового ответа (reference)`);
    } else {
      problems.push(`${q.id}: kind «${q.kind}» — нужен choice или open`);
    }
  }
  return problems;
}

/** Замечания о качестве: курс принимается, но может быть лучше. */
export function advice(raw) {
  const course = normalizeCourse(raw);
  const notes = [];
  for (const topic of course.topics) {
    const total = topic.concepts.length;
    if (!total) continue;
    const hooked = topic.concepts.filter((c) => c.mnemonic.trim() || c.analogy.trim()).length;
    if (hooked * 2 < total) notes.push(`${topic.id}: зацепка (mnemonic или analogy) есть у ${hooked} из ${total} понятий — добавь хотя бы половине`);
    const pitfalls = topic.concepts.filter((c) => c.pitfall.trim()).length;
    if (pitfalls * 3 < total) notes.push(`${topic.id}: частых ошибок (pitfall) мало — ${pitfalls} из ${total}`);
    const cross = topic.concepts.flatMap((c) => c.related).filter((r) => r.includes("/")).length;
    if (course.topics.length > 1 && cross < 2) {
      notes.push(`${topic.id}: связей с другими темами ${cross} — знание держится связями, свяжи хотя бы два понятия с другими темами`);
    }
    const words = topic.lesson.split(/\s+/).filter(Boolean).length;
    if (words < LESSON_WORDS) {
      notes.push(`${topic.id}: урок — ${words} слов; раскрой каждый раздел: как устроено, кто что делает, пример, ошибки — от ${LESSON_WORDS} слов`);
    }
    const unexplained = topic.concepts.filter((c) => !explainedIn(topic.lesson, c.definition)).map((c) => c.term);
    if (unexplained.length) {
      notes.push(`${topic.id}: в уроке названы, но не объяснены: ${unexplained.join(", ")} — Ноа спросит их после раздела, а ответа в тексте нет`);
    }
    const linked = [...topic.exam, ...topic.tasks].filter((q) => q.concept).length;
    if (linked * 2 < topic.exam.length + topic.tasks.length) {
      notes.push(`${topic.id}: у вопросов не указано concept — ошибка на экзамене не вернёт понятие в повторение`);
    }
  }
  return notes;
}

/* ── Интервальное повторение — srs.rs ──────────────────────────────────── */

const START_EASE = 2.5;
const MIN_EASE = 1.3;
/* ── Прогресс с разных устройств ──────────────────────────────────────── */

/**
 * Сливает прогресс двух устройств: у каждого курса берётся тот, что изменён
 * позже (поле `updated`, «ГГГГ-ММ-ДД ЧЧ:ММ» — строки сравниваются как даты).
 * При равенстве побеждает `incoming` — то, что прислали только что.
 *
 * Раньше выигрывал целиком тот, кто сохранил последним: занимались курсом на
 * телефоне, потом открыли программу со старым прогрессом — и телефонный
 * пропадал.
 */
export function mergeProgress(stored, incoming) {
  const a = stored?.courses ?? {};
  const b = incoming?.courses ?? {};
  const courses = {};
  for (const id of new Set([...Object.keys(a), ...Object.keys(b)])) {
    const mine = a[id];
    const theirs = b[id];
    if (!mine) courses[id] = theirs;
    else if (!theirs) courses[id] = mine;
    else courses[id] = String(theirs.updated ?? "") >= String(mine.updated ?? "") ? theirs : mine;
  }
  return { ...(stored ?? {}), ...(incoming ?? {}), courses, deep: { ...(stored?.deep ?? {}), ...(incoming?.deep ?? {}) } };
}

export const MATURE_DAYS = 21;

export const newCardState = () => ({ due: 0, interval: 0, ease: START_EASE, streak: 0, lapses: 0, seen: 0, last: 0 });

export function levelOf(state) {
  if (!state || state.seen === 0) return "new";
  if (state.interval < 3) return "learning";
  if (state.interval < MATURE_DAYS) return "young";
  return "mature";
}

const isDue = (state, now) => state.seen > 0 && state.due <= now;

/** Ответ на карточку: SM-2, как srs.rs. `now` — секунды. */
export function answerCard(state, grade, now) {
  const DAY = 86_400;
  state.seen += 1;
  state.last = now;
  if (grade === "again") {
    state.lapses += 1;
    state.streak = 0;
    state.ease = Math.max(state.ease - 0.2, MIN_EASE);
    state.interval = state.interval > 0 ? Math.max(state.interval * 0.2, 0) : 0;
    state.due = now + 600;
    return state;
  }
  if (grade === "hard") {
    state.ease = Math.max(state.ease - 0.15, MIN_EASE);
    state.interval = state.streak === 0 ? 1 : Math.max(state.interval * 1.2, state.interval + 1);
  } else if (grade === "good") {
    state.interval = state.streak === 0 ? 1 : state.streak === 1 ? 3 : Math.max(state.interval * state.ease, state.interval + 1);
  } else {
    state.ease += 0.15;
    state.interval = state.streak === 0 ? 3 : state.streak === 1 ? 6 : Math.max(state.interval * state.ease * 1.3, state.interval + 2);
  }
  state.streak += 1;
  const jitter = 1 + ((now % 97) - 48) / 1000;
  state.interval = Math.min(state.interval * jitter, 365);
  state.due = now + Math.trunc(state.interval * DAY);
  return state;
}

export function parseGrade(text) {
  const value = String(text ?? "").trim().toLowerCase();
  if (["again", "1", "снова", "забыл"].includes(value)) return "again";
  if (["hard", "2", "трудно"].includes(value)) return "hard";
  if (["good", "3", "хорошо"].includes(value)) return "good";
  if (["easy", "4", "легко"].includes(value)) return "easy";
  return null;
}

/* ── Урок по разделам — tutor.rs, sections ─────────────────────────────── */

export function sections(lesson) {
  const parts = [];
  let current = [];
  for (const raw of String(lesson).split("\n")) {
    const line = raw.replace(/\r$/, "");
    if (line.startsWith("## ") && current.some((l) => l.startsWith("## "))) {
      parts.push(current.join("\n"));
      current = [];
    }
    current.push(line);
  }
  if (current.join("").trim()) parts.push(current.join("\n"));
  if (!parts.length) parts.push(String(lesson));
  return parts;
}

const heading = (section) => {
  const line = section.split("\n").find((l) => l.startsWith("## "));
  return line ? line.slice(3).trim() : null;
};

/* ── Слова ─────────────────────────────────────────────────────────────── */

function plural(n, one, few, many) {
  const tens = n % 100;
  const units = n % 10;
  if (tens >= 11 && tens <= 14) return many;
  if (units === 1) return one;
  if (units >= 2 && units <= 4) return few;
  return many;
}

function when(seconds) {
  const minutes = Math.trunc(seconds / 60);
  if (minutes < 60) return "через несколько минут";
  const hours = Math.trunc(minutes / 60);
  if (hours < 24) return `через ${hours} ч`;
  const days = Math.round(seconds / 86_400);
  if (days === 1) return "завтра";
  if (days < 30) return `через ${days} ${plural(days, "день", "дня", "дней")}`;
  const months = Math.round(days / 30);
  return `через ${months} ${plural(months, "месяц", "месяца", "месяцев")}`;
}

/** Отпечаток текста — узнать, что раздел поменялся (FNV-1a). */
function fingerprint(text) {
  let hash = 0x811c9dc5;
  for (let at = 0; at < text.length; at++) {
    hash ^= text.charCodeAt(at);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16);
}

/** Вариант по сказанному: «второй», «2», «б» или слова варианта. */
export function choiceOf(said, options) {
  const ORDINALS = [
    ["1", "первый", "первое", "один", "а", "a"],
    ["2", "второй", "второе", "два", "б", "b"],
    ["3", "третий", "третье", "три", "в", "c"],
    ["4", "четвертый", "четвертое", "четыре", "г", "d"],
    ["5", "пятый", "пятое", "пять", "д", "e"],
  ];
  const words = lower(said).split(/[^\p{L}\p{N}]+/u).filter(Boolean);
  for (let at = 0; at < Math.min(ORDINALS.length, options.length); at++) {
    if (words.some((word) => ORDINALS[at].includes(word))) return at;
  }
  let best = null;
  let most = 0;
  options.forEach((option, at) => {
    const text = option.toLowerCase();
    const hits = words.filter((word) => chars(word) > 2 && text.includes(word)).length;
    if (hits > most) {
      most = hits;
      best = at;
    }
  });
  return best;
}

function openScore(points, covered, partial, wrong) {
  const total = Math.max(points, 1);
  let score = Math.round(((covered + partial * 0.5) / total) * 100);
  if (wrong) score -= 10;
  return Math.min(Math.max(score, 0), 100);
}

/** Первый объект JSON в ответе модели: модели любят обернуть его текстом. */
function jsonIn(raw) {
  const start = raw.indexOf("{");
  const end = raw.lastIndexOf("}");
  if (start < 0 || end <= start) return null;
  try {
    return JSON.parse(raw.slice(start, end + 1));
  } catch {
    return null;
  }
}

/* ── Обучение целиком ──────────────────────────────────────────────────── */

/**
 * @param {{
 *   courses: () => object[],
 *   store: { data: object, save: () => void },
 *   ai?: { chat: (messages: {role: string, content: string}[], opts?: {json?: boolean, maxTokens?: number}) => Promise<string> },
 *   name?: string,
 *   clock?: () => Date,
 * }} deps
 *
 * `store.data` — объект вида learning.json (`{ courses: {…}, deep: {…} }`);
 * модуль меняет его и зовёт `save()`.
 */
export function createLearning({ courses, store, ai = null, name = "Ноа", clock = () => new Date() }) {
  const nowSec = () => Math.trunc(clock().getTime() / 1000);
  const today = () => dayKey(clock());
  const dayAgo = (offset) => {
    const date = clock();
    date.setDate(date.getDate() - offset);
    return dayKey(date);
  };

  store.data.courses ??= {};
  store.data.deep ??= {};

  const all = () => courses().map(normalizeCourse);
  const course = (id) => {
    const found = all().find((c) => c.id === id);
    if (!found) throw new Error("Такого курса нет.");
    return found;
  };
  const topicOf = (c, id) => {
    const found = c.topics.find((t) => t.id === id);
    if (!found) throw new Error("Такой темы нет.");
    return found;
  };
  const question = (c, id) => {
    for (const t of c.topics) {
      const found = [...t.tasks, ...t.exam].find((q) => q.id === id);
      if (found) return [found, t];
    }
    const final = c.final.find((q) => q.id === id);
    return final ? [final, null] : null;
  };

  const emptyTopic = () => ({ read: false, tasks: {}, examBest: null, examAttempts: 0, mistakes: [], step: "", section: 0 });
  const emptyCourse = () => ({
    topics: {},
    finalBest: null,
    finalAttempts: 0,
    current: null,
    updated: null,
    cards: {},
    newDay: "",
    newCount: 0,
    days: {},
    sessions: [],
  });
  const progress = (id) => {
    const own = store.data.courses[id] ?? emptyCourse();
    return { ...emptyCourse(), ...own };
  };
  const ownTopic = (p, id) => {
    p.topics[id] = { ...emptyTopic(), ...(p.topics[id] ?? {}) };
    return p.topics[id];
  };
  /** Меняет прогресс курса и сохраняет — `with` из learning.rs. */
  const change = (id, fn) => {
    const p = progress(id);
    const value = fn(p);
    const at = clock();
    p.updated = `${dayKey(at)} ${pad(at.getHours())}:${pad(at.getMinutes())}`;
    store.data.courses[id] = p;
    store.save();
    return value;
  };

  const noteAnswer = (p, topicId, id, score) => {
    if (!topicId) return;
    const own = ownTopic(p, topicId);
    own.mistakes = own.mistakes.filter((known) => known !== id);
    if (score < RIGHT) own.mistakes.push(id);
  };

  /* ── Колода и повторение — recall.rs ── */

  const deck = (c) => {
    const out = [];
    for (const t of c.topics) {
      const byId = new Map(t.concepts.map((x) => [x.id, x]));
      for (const x of t.concepts) {
        out.push({
          key: `${t.id}/c-${x.id}`,
          topic: t.id,
          topicTitle: t.title,
          concept: x.id,
          front: `Что такое ${x.term}?`,
          back: x.definition,
          mnemonic: x.mnemonic,
          analogy: x.analogy,
          example: x.example,
          pitfall: x.pitfall,
        });
      }
      for (const card of t.cards) {
        const x = byId.get(card.concept);
        out.push({
          key: `${t.id}/${card.id}`,
          topic: t.id,
          topicTitle: t.title,
          concept: card.concept,
          front: card.front,
          back: card.back,
          mnemonic: x?.mnemonic ?? "",
          analogy: "",
          example: "",
          pitfall: x?.pitfall ?? "",
        });
      }
    }
    return out;
  };

  const mastery = (c) => {
    const own = progress(c.id);
    const at = nowSec();
    const out = { total: 0, new: 0, learning: 0, young: 0, mature: 0, due: 0, newLeft: 0 };
    for (const card of deck(c)) {
      out.total += 1;
      const state = own.cards[card.key];
      if (!state) {
        out.new += 1;
        continue;
      }
      out[levelOf(state)] += 1;
      if (isDue(state, at)) out.due += 1;
    }
    const taken = own.newDay === today() ? own.newCount : 0;
    out.newLeft = Math.min(Math.max(NEW_PER_DAY - taken, 0), out.new);
    return out;
  };

  const topicMastery = (c) => {
    const own = progress(c.id);
    const out = {};
    for (const card of deck(c)) {
      out[card.topic] ??= [0, 0];
      out[card.topic][0] += 1;
      if (levelOf(own.cards[card.key]) === "mature" && own.cards[card.key]) out[card.topic][1] += 1;
    }
    return out;
  };

  const relapse = (courseId, topicId, concept) => {
    let t = topicId;
    let x = concept;
    if (concept.includes("/")) [t, x] = concept.split(/\/(.*)/s);
    if (!t || !x.trim()) return;
    const key = `${t}/c-${x}`;
    const at = nowSec();
    change(courseId, (p) => {
      const state = p.cards[key];
      if (state) {
        state.due = at;
        state.interval = Math.min(state.interval * 0.5, state.interval);
        state.streak = 0;
      }
    });
  };

  const resolve = (c, from, reference) => {
    const [t, x] = reference.includes("/") ? reference.split(/\/(.*)/s) : [from.id, reference];
    const topic = c.topics.find((known) => known.id === t);
    const concept = topic?.concepts.find((known) => known.id === x);
    return topic && concept ? [topic, concept] : null;
  };
  const conceptLevel = (courseId, topicId, conceptId) => levelOf(progress(courseId).cards[`${topicId}/c-${conceptId}`]);

  const linksOf = (c, topic, concept) => {
    const out = [];
    const push = (t, x) => {
      if ((t.id === topic.id && x.id === concept.id) || out.some((l) => l.topic === t.id && l.concept === x.id)) return;
      out.push({ topic: t.id, concept: x.id, term: x.term, cross: t.id !== topic.id });
    };
    for (const reference of concept.related) {
      const found = resolve(c, topic, reference);
      if (found) push(...found);
    }
    for (const other of c.topics) {
      for (const x of other.concepts) {
        const namesUs = x.related.some((reference) => {
          const found = resolve(c, other, reference);
          return found && found[0].id === topic.id && found[1].id === concept.id;
        });
        if (namesUs) push(other, x);
      }
    }
    return out;
  };

  const cheatsheet = (topic) => {
    if (topic.cheatsheet.trim()) return topic.cheatsheet;
    if (!topic.concepts.length) return "";
    let out = `## ${topic.title} — на одном листе\n\n`;
    for (const x of topic.concepts) {
      out += `- **${x.term}** — ${x.definition.trim()}`;
      if (x.mnemonic.trim()) out += ` *Запомнить:* ${x.mnemonic.trim()}`;
      out += "\n";
    }
    const pitfalls = topic.concepts.filter((x) => x.pitfall.trim());
    if (pitfalls.length) {
      out += "\n## Где ошибаются\n\n";
      for (const x of pitfalls) out += `- **${x.term}**: ${x.pitfall.trim()}\n`;
    }
    return out;
  };

  /* ── Фокус — focus.rs ── */

  const focusStats = (days, sessions) => {
    const todayMinutes = days[dayAgo(0)] ?? 0;
    let week = 0;
    for (let offset = 0; offset < 7; offset++) week += days[dayAgo(offset)] ?? 0;
    // Сегодня ещё не занимались — серия не прервана, пока не кончился день.
    const start = Object.hasOwn(days, dayAgo(0)) ? 0 : 1;
    let streak = 0;
    while (Object.hasOwn(days, dayAgo(start + streak))) streak++;
    const midnight = new Date(clock());
    midnight.setHours(0, 0, 0, 0);
    const since = Math.trunc(midnight.getTime() / 1000);
    return { today: todayMinutes, week, streak, sessionsToday: sessions.filter((s) => s.at >= since).length };
  };

  /* ── Обзор ── */

  const percent = (topics, finalBest) => {
    if (!topics.length) return 0;
    const perTopic =
      topics
        .map((t) => {
          const read = t.read ? 0.2 : 0;
          const tasks = t.tasksTotal === 0 ? 0.3 : (0.3 * t.tasksDone) / t.tasksTotal;
          const exam = 0.5 * Math.min((t.examBest ?? 0) / TOPIC_PASS, 1);
          return read + tasks + exam;
        })
        .reduce((a, b) => a + b, 0) / topics.length;
    const last = finalBest !== null && finalBest >= FINAL_PASS ? 0.1 : 0;
    return Math.round((perTopic * 0.9 + last) * 100);
  };

  const card = (c) => {
    const p = progress(c.id);
    const byTopic = topicMastery(c);
    const topics = c.topics.map((t) => {
      const own = { ...emptyTopic(), ...(p.topics[t.id] ?? {}) };
      const tasksDone = t.tasks.filter((task) => (own.tasks[task.id] ?? -1) >= RIGHT).length;
      const passed = own.examBest !== null && own.examBest >= TOPIC_PASS;
      const status = passed ? "done" : tasksDone > 0 || own.examAttempts > 0 ? "practice" : own.read ? "reading" : "new";
      return {
        id: t.id,
        title: t.title,
        summary: t.summary,
        status,
        read: own.read,
        tasksDone,
        tasksTotal: t.tasks.length,
        examBest: own.examBest,
        mistakes: own.mistakes.length,
        conceptsTotal: byTopic[t.id]?.[0] ?? 0,
        conceptsMature: byTopic[t.id]?.[1] ?? 0,
        step: own.step,
        section: own.section,
      };
    });
    return {
      id: c.id,
      title: c.title,
      description: c.description,
      percent: percent(topics, p.finalBest),
      finalUnlocked: topics.every((t) => t.status === "done"),
      finalBest: p.finalBest,
      current: p.current,
      topics,
      topicPass: TOPIC_PASS,
      finalPass: FINAL_PASS,
      building: c.building,
      mastery: mastery(c),
      focus: focusStats(p.days, p.sessions),
    };
  };

  /* ── Проверка ответов ── */

  const needAi = () => {
    if (!ai) throw new Error("Модель не подключена — подключите её на главной странице Ноа.");
    return ai;
  };

  const gradeOpen = async (q, answer) => {
    const points = q.points.map((point, at) => `${at + 1}. ${point}`).join("\n");
    const reference = q.reference ? `\n\nПример сильного ответа (один из возможных): ${q.reference}` : "";
    const rules =
      "Ты — доброжелательный экзаменатор на собеседовании. Оцени, понимает ли ученик суть, по ключевым пунктам ниже. Пункты — ориентир, а не слова, которые надо повторить.\n" +
      "- Пункт раскрыт (covered), если ученик передал его смысл: своими словами, другими терминами, равноценной командой или инструментом (htop вместо top, ss вместо netstat), другим верным путём к той же цели.\n" +
      "- Наполовину (partial), если направление верное, но не хватает важной детали или пункт следует из названного лишь частично.\n" +
      "- Не раскрыт — только если ничего близкого по смыслу нет.\n" +
      "- wrong — только существенная фактическая ошибка, которая выдаёт непонимание; оговорки и неточные слова ошибкой не считай.\n" +
      "- Не придирайся к формулировкам, порядку и краткости.\n" +
      'Верни только JSON: {"covered":[номера],"partial":[номера],"wrong":"ошибка или пустая строка","feedback":"одно-два предложения по-русски: что верно и чего не хватило"}.' +
      `\n\nВопрос: ${q.q}\n\nКлючевые пункты:\n${points}${reference}`;
    let raw;
    try {
      raw = await needAi().chat([{ role: "system", content: rules }, { role: "user", content: answer }], { json: true });
    } catch {
      return null;
    }
    const parsed = jsonIn(String(raw ?? ""));
    if (!parsed) return null;
    const numbers = (key) =>
      [...new Set(list(parsed[key]).map((item) => Number.parseInt(item, 10)).filter((at) => at >= 1 && at <= q.points.length))].sort((a, b) => a - b);
    const covered = numbers("covered");
    const partial = numbers("partial").filter((at) => !covered.includes(at));
    const text = (key) => (typeof parsed[key] === "string" ? parsed[key].trim() : "");
    return { covered, partial, wrong: text("wrong"), feedback: text("feedback") };
  };

  const grade = async (q, answer) => {
    const verdict = {
      id: q.id,
      q: q.q,
      score: 0,
      right: false,
      feedback: "",
      reference: q.kind === "choice" ? q.explain : q.reference,
      covered: [],
      partial: [],
      points: q.points,
      options: q.options,
      chosen: null,
      answer: q.answer,
    };
    if (q.kind === "choice") {
      const chosen = Number.isInteger(answer) ? answer : typeof answer === "string" ? choiceOf(answer, q.options) : null;
      verdict.chosen = chosen;
      const right = chosen !== null && chosen === q.answer;
      verdict.score = right ? 100 : 0;
      verdict.right = right;
      const correct = q.answer !== null ? q.options[q.answer] : undefined;
      verdict.feedback = right ? "Верно." : correct !== undefined ? `Неверно. Правильно: ${correct}.` : "Неверно.";
      return verdict;
    }
    const text = String(typeof answer === "string" ? answer : "").trim();
    if (chars(text) < 3) {
      verdict.feedback = "Ответа нет.";
      return verdict;
    }
    const graded = ai ? await gradeOpen(q, text) : null;
    if (!graded) {
      verdict.score = null;
      verdict.feedback = ai
        ? "Модель не ответила — сверьтесь с эталоном и оцените себя сами."
        : "Модель не подключена — сверьтесь с эталоном и оцените себя сами.";
      return verdict;
    }
    const score = openScore(q.points.length, graded.covered.length, graded.partial.length, Boolean(graded.wrong));
    verdict.score = score;
    verdict.right = score >= RIGHT;
    verdict.covered = graded.covered;
    verdict.partial = graded.partial;
    if (!graded.wrong) verdict.feedback = !graded.feedback && score >= RIGHT ? "Хороший ответ." : graded.feedback;
    else verdict.feedback = graded.feedback ? `Ошибка: ${graded.wrong} ${graded.feedback}` : `Ошибка: ${graded.wrong}`;
    return verdict;
  };

  const blank = (q) => ({ ...q, answer: null, explain: "", points: [], reference: "" });

  const exam = (courseId, scope) => {
    const c = course(courseId);
    if (scope === "final") {
      const questions = c.final.map(blank);
      const seed = nowSec();
      c.topics.forEach((t, at) => {
        const pool = t.exam;
        for (let step = 0; step < Math.min(FINAL_PER_TOPIC, pool.length); step++) {
          const pick = (Math.trunc(seed / 7) + at * 3 + step * 5) % pool.length;
          const q = pool[(pick + step) % pool.length];
          if (!questions.some((known) => known.id === q.id)) questions.push(blank(q));
        }
      });
      return { scope: "final", title: `Финальный экзамен: ${c.title}`, pass: FINAL_PASS, questions };
    }
    const t = topicOf(c, scope);
    return { scope: t.id, title: `Мини-экзамен: ${t.title}`, pass: TOPIC_PASS, questions: t.exam.map(blank) };
  };

  /* ── Репетитор — tutor.rs ── */

  let discussion = null;

  const conceptsIn = (topic, text) => {
    const where = text.toLowerCase();
    return topic.concepts
      .filter((x) => {
        const term = x.term.toLowerCase();
        return [term, ...term.split(/[,()]/).flatMap((part) => part.split(" и "))]
          .map((part) => part.trim())
          .some((part) => chars(part) >= 3 && where.includes(part));
      })
      .map((x) => `- ${x.term} — ${x.definition}`)
      .join("\n");
  };

  const sectionMaterial = (c, topic, at) => {
    const parts = sections(topic.lesson);
    const index = Math.min(at ?? 0, parts.length - 1);
    const section = [...parts[index]].slice(0, MAX_SECTION).join("");
    const outline = parts
      .map((part, n) => {
        const h = heading(part);
        return h ? `${n + 1}. ${h}${n === index ? " ← сейчас" : ""}` : null;
      })
      .filter(Boolean)
      .join("\n");
    const concepts = conceptsIn(topic, section);
    let material = `Курс «${c.title}», тема «${topic.title}». Человек читает раздел ${index + 1} из ${parts.length}.\n`;
    if (outline) material += `\nРазделы урока:\n${outline}\n`;
    material += `\nТекст раздела:\n${section}\n`;
    if (concepts) material += `\nПонятия из раздела:\n${concepts}\n`;
    return [material, heading(section) ?? topic.title, index];
  };

  const questionMaterial = (c, id, answer) => {
    const found = question(c, id);
    if (!found) throw new Error("Такого вопроса нет.");
    const [q, topic] = found;
    let material = `Курс «${c.title}», тема «${topic?.title ?? "итоговый экзамен"}».\nВопрос: ${q.q}\n`;
    if (q.options.length) material += `Варианты: ${q.options.join("; ")}\n`;
    if (answer.trim()) material += `Ответ человека: ${answer.trim()}\n`;
    if (q.answer !== null && q.options[q.answer] !== undefined) material += `Верный вариант: ${q.options[q.answer]}\n`;
    if (q.reference) material += `Пример сильного ответа (не единственно верный): ${q.reference}\n`;
    if (q.explain) material += `Пояснение: ${q.explain}\n`;
    if (q.points.length) material += `Что важно раскрыть: ${q.points.join("; ")}\n`;
    return material;
  };

  const sameTarget = (a, b) =>
    a.course === b.course &&
    (a.topic ?? null) === (b.topic ?? null) &&
    (a.section ?? null) === (b.section ?? null) &&
    (a.question ?? null) === (b.question ?? null) &&
    (a.answer ?? null) === (b.answer ?? null);

  const openDiscussion = (target) => {
    const c = course(target.course);
    let material;
    if (target.question) material = questionMaterial(c, target.question, target.answer ?? "");
    else if (target.topic) material = sectionMaterial(c, topicOf(c, target.topic), target.section)[0];
    else throw new Error("Не сказано, что обсуждать.");
    if (!discussion || !sameTarget(discussion.target, target)) discussion = { target: { ...target }, material, thread: [] };
  };

  const tutorRules = (material) =>
    `Ты — репетитор ${name}: помогаешь человеку разобраться в уроке. Ниже — то, что у него сейчас перед глазами; «тут», «это», «первый пункт» относятся к этому.\n\n` +
    `${material}\n` +
    "Как отвечать:\n" +
    "- Объясняй сразу и по существу, простыми словами: что это, зачем нужно, кто что делает и в каком порядке, что будет, если этого нет.\n" +
    "- Опирайся на живой пример или бытовую аналогию. Термин, которого нет в материале, объясни одной фразой.\n" +
    "- Человек говорит «не понял» — не переспрашивай, что именно: объясни главное ещё раз, проще и с другой стороны. Переспроси, только если вопрос совсем не разобрать.\n" +
    "- Вопрос шире материала, но по теме курса — отвечай.\n" +
    "- Ответ читают в окне: до 150 слов; можно короткий список и `команды` в обратных кавычках.\n" +
    "- Без вступлений, похвалы вопросу и предложений помочь ещё. В конце можно одним коротким вопросом проверить, понятно ли.\n" +
    "Отвечай по-русски.";

  /* ── Устный зачёт — learning.rs, start_quiz и quiz_answer ── */

  let quiz = null;

  const spoken = (q) => {
    if (q.kind !== "choice") return q.q;
    const names = ["первый", "второй", "третий", "четвёртый", "пятый"];
    const options = q.options.slice(0, names.length).map((option, at) => `${names[at]}: ${option}`).join("; ");
    return `${q.q} Варианты — ${options}.`;
  };

  /** Следующий вопрос: сперва ошибки, затем случайный из экзаменов и задач. */
  const nextQuestion = (c, topicId, after) => {
    const own = progress(c.id);
    const topics = c.topics.filter((t) => !topicId || t.id === topicId);
    const mistakes = topics
      .flatMap((t) => own.topics[t.id]?.mistakes ?? [])
      .filter((id) => question(c, id) && id !== after);
    if (mistakes.length) return question(c, mistakes[0])[0];
    const pool = topics.flatMap((t) => [...t.exam, ...t.tasks]).filter((q) => q.id !== after);
    return pool.length ? pool[Math.floor(Math.random() * pool.length)] : null;
  };

  /** Прогресс словами — learning.rs, summary. */
  const summary = (c) => {
    const view = card(c);
    const memory = view.mastery;
    const remembered = memory.total ? ` Уверенно держится ${memory.mature} из ${memory.total} понятий; повторить сегодня — ${memory.due}.` : "";
    const done = view.topics.filter((t) => t.status === "done").length;
    const mistakes = view.topics.reduce((sum, t) => sum + t.mistakes, 0);
    const pending = view.topics.find((t) => t.status !== "done");
    const next = pending
      ? ` Дальше — «${pending.title}».`
      : view.finalBest !== null && view.finalBest >= FINAL_PASS
        ? " Финальный экзамен сдан."
        : " Остался финальный экзамен.";
    const weak = mistakes ? ` Ошибок на повторение: ${mistakes}.` : "";
    return `${c.title}: пройдено ${view.percent}%, тем сдано ${done} из ${view.topics.length}.${remembered}${next}${weak}`;
  };

  const oralStop = () => {
    if (!quiz) return null;
    const { course: id, asked, right } = quiz;
    quiz = null;
    return asked ? `Закончили: верно ${right} из ${asked}. ${summary(course(id))}` : "Закончили опрос.";
  };

  return {
    courses: all,
    validate,
    advice,

    /** Начать устный зачёт: первая фраза Ноа — с вопросом. */
    oralStart(courseId, topicId) {
      const c = course(courseId);
      const topic = topicId ? topicOf(c, topicId) : null;
      const q = nextQuestion(c, topic?.id ?? null, null);
      if (!q) return "Вопросов по этой теме нет.";
      quiz = { course: c.id, topic: topic?.id ?? null, question: q.id, asked: 0, right: 0 };
      const intro = topic
        ? `Устный зачёт по теме «${topic.title}». Скажите «хватит», чтобы закончить. `
        : `Устный зачёт по курсу «${c.title}». Скажите «хватит», чтобы закончить. `;
      return intro + spoken(q);
    },

    oralActive: () => Boolean(quiz),
    oralStop,

    /** Ответ голосом. Отдаёт, что сказать; `done` — зачёт кончился. */
    async oralAnswer(said) {
      if (!quiz) return { text: "Зачёт не начат.", done: true };
      const c = course(quiz.course);
      const found = question(c, quiz.question);
      if (!found) return { text: oralStop() ?? "Закончили.", done: true };
      const q = found[0];
      const text = lower(said);
      const words = text.split(/[^\p{L}\p{N}]+/u).filter(Boolean);
      const STOP = ["хватит", "стоп", "закончим", "заканчиваем", "достаточно", "устал"];
      if (words.length <= 4 && words.some((w) => STOP.includes(w))) return { text: oralStop(), done: true };

      let reply;
      let correct = false;
      if (text.includes("не знаю") || text.includes("пропус") || text.includes("дальше")) {
        const answer = (q.kind === "choice" ? (q.answer !== null ? q.options[q.answer] : "") : q.reference).trimEnd();
        this.selfGrade(c.id, q.id, false);
        reply = `Правильный ответ: ${answer}${/[.!?]$/.test(answer) ? "" : "."}`;
      } else {
        const verdict = await this.check(c.id, q.id, said);
        if (verdict.score === null) reply = `Не смог проверить. Эталон: ${verdict.reference}`;
        else if (q.kind === "choice") {
          const why = q.explain ? ` ${q.explain}` : "";
          reply = verdict.score >= RIGHT ? `Верно.${why}` : `${verdict.feedback}${why}`;
        } else {
          reply =
            verdict.score >= RIGHT
              ? `Засчитано, ${verdict.score} из 100. ${verdict.feedback}`
              : `${verdict.score} из 100. ${verdict.feedback} Эталон: ${verdict.reference}`;
        }
        correct = verdict.right;
      }
      quiz.asked += 1;
      if (correct) quiz.right += 1;
      const next = nextQuestion(c, quiz.topic, q.id);
      if (!next) {
        const end = oralStop();
        return { text: `${reply} Вопросы кончились. ${end}`, done: true };
      }
      quiz.question = next.id;
      return { text: `${reply} Следующий вопрос: ${spoken(next)}`, done: false };
    },

    summary: (courseId) => summary(course(courseId)),

    overview: () => all().map(card),

    topicView(courseId, topicId) {
      const c = course(courseId);
      const t = topicOf(c, topicId);
      change(courseId, (p) => (p.current = topicId));
      const own = { ...emptyTopic(), ...(progress(courseId).topics[topicId] ?? {}) };
      return {
        cheatsheet: cheatsheet(t),
        mistakes: own.mistakes.map((id) => question(c, id)?.[0]).filter(Boolean),
        scores: own.tasks,
        topic: { ...t, exam: [] },
      };
    },

    place(courseId, topicId, step, section) {
      topicOf(course(courseId), topicId);
      if (!["talk", "lesson", "concepts", "map", "tasks", "sheet", "mistakes"].includes(step)) return null;
      change(courseId, (p) => {
        const own = ownTopic(p, topicId);
        own.step = step;
        own.section = Number(section) || 0;
        p.current = topicId;
      });
      return null;
    },

    read(courseId, topicId) {
      topicOf(course(courseId), topicId);
      change(courseId, (p) => {
        ownTopic(p, topicId).read = true;
        p.current = topicId;
      });
      return null;
    },

    async check(courseId, questionId, answer) {
      const c = course(courseId);
      const found = question(c, questionId);
      if (!found) throw new Error("Такого вопроса нет.");
      const [q, topic] = found;
      const verdict = await grade(q, answer);
      if (verdict.score !== null) {
        const isTask = topic?.tasks.some((task) => task.id === q.id);
        change(courseId, (p) => {
          if (isTask) {
            const own = ownTopic(p, topic.id);
            own.tasks[q.id] = Math.max(own.tasks[q.id] ?? 0, verdict.score);
          }
          noteAnswer(p, topic?.id, q.id, verdict.score);
        });
        if (verdict.score < RIGHT) relapse(courseId, topic?.id ?? null, q.concept);
      }
      return verdict;
    },

    selfGrade(courseId, questionId, knew) {
      const c = course(courseId);
      const found = question(c, questionId);
      if (!found) throw new Error("Такого вопроса нет.");
      const [q, topic] = found;
      const score = knew ? 100 : 0;
      change(courseId, (p) => {
        if (topic?.tasks.some((task) => task.id === q.id)) {
          const own = ownTopic(p, topic.id);
          own.tasks[q.id] = Math.max(own.tasks[q.id] ?? 0, score);
        }
        noteAnswer(p, topic?.id, q.id, score);
      });
      return null;
    },

    exam,

    async submit(courseId, scope, answers) {
      const c = course(courseId);
      const given = exam(courseId, scope);
      const asked = scope === "final" ? Object.keys(answers ?? {}) : given.questions.map((q) => q.id);
      const items = [];
      for (const id of asked) {
        const found = question(c, id);
        if (!found) continue;
        items.push(await grade(found[0], answers?.[id] ?? null));
      }
      if (!items.length) throw new Error("Ответов нет.");
      const unchecked = items.filter((item) => item.score === null).length;
      const score = Math.round(items.reduce((sum, item) => sum + (item.score ?? 0), 0) / items.length);
      change(courseId, (p) => {
        for (const item of items) {
          const found = question(c, item.id);
          if (item.score !== null && found) noteAnswer(p, found[1]?.id, item.id, item.score);
        }
        if (scope === "final") {
          p.finalAttempts += 1;
          p.finalBest = Math.max(p.finalBest ?? 0, score);
        } else {
          const own = ownTopic(p, scope);
          own.examAttempts += 1;
          own.examBest = Math.max(own.examBest ?? 0, score);
        }
      });
      for (const item of items) {
        if (item.score !== null && item.score < RIGHT) {
          const found = question(c, item.id);
          if (found) relapse(courseId, found[1]?.id ?? null, found[0].concept);
        }
      }
      return { score, passed: score >= given.pass, pass: given.pass, unchecked, items };
    },

    review(courseId, topicId) {
      const c = course(courseId);
      const own = progress(courseId);
      const at = nowSec();
      const cards = deck(c).filter((x) => !topicId || x.topic === topicId);
      const due = cards
        .filter((x) => own.cards[x.key] && isDue(own.cards[x.key], at))
        .sort((a, b) => own.cards[a.key].due - own.cards[b.key].due)
        .map((x) => ({ ...x, level: levelOf(own.cards[x.key]), fresh: false }));
      const taken = own.newDay === today() ? own.newCount : 0;
      const allowed = topicId ? Infinity : Math.max(NEW_PER_DAY - taken, 0);
      const read = Object.entries(own.topics).filter(([, t]) => t.read).map(([id]) => id);
      const fresh = cards
        .filter((x) => !own.cards[x.key])
        .filter((x) => topicId || read.includes(x.topic))
        .slice(0, allowed)
        .map((x) => ({ ...x, level: "new", fresh: true }));
      return [...due, ...fresh];
    },

    gradeCard(courseId, key, gradeText) {
      const g = parseGrade(gradeText);
      if (!g) throw new Error("Оценка — again, hard, good или easy.");
      const c = course(courseId);
      if (!deck(c).some((x) => x.key === key)) throw new Error("Такой карточки в курсе нет.");
      const at = nowSec();
      const state = change(courseId, (p) => {
        if (!p.cards[key]) {
          if (p.newDay !== today()) {
            p.newDay = today();
            p.newCount = 0;
          }
          p.newCount += 1;
        }
        p.days[today()] ??= 0;
        p.cards[key] = answerCard({ ...newCardState(), ...(p.cards[key] ?? {}) }, g, at);
        return p.cards[key];
      });
      return { level: levelOf(state), next: when(state.due - at), again: g === "again" };
    },

    concepts(courseId, topicId) {
      const c = course(courseId);
      const t = topicOf(c, topicId);
      return t.concepts.map((x) => ({ ...x, level: conceptLevel(courseId, t.id, x.id), links: linksOf(c, t, x) }));
    },

    map(courseId) {
      const c = course(courseId);
      const topics = c.topics.map((t) => ({
        id: t.id,
        title: t.title,
        concepts: t.concepts.map((x) => ({ id: x.id, term: x.term, level: conceptLevel(courseId, t.id, x.id) })),
      }));
      const edges = [];
      for (const t of c.topics) {
        for (const x of t.concepts) {
          for (const reference of x.related) {
            const found = resolve(c, t, reference);
            if (!found) continue;
            const a = `${t.id}/${x.id}`;
            const b = `${found[0].id}/${found[1].id}`;
            const [from, to] = a < b ? [a, b] : [b, a];
            if (!edges.some((e) => e.from === from && e.to === to)) edges.push({ from, to, cross: found[0].id !== t.id });
          }
        }
      }
      return { topics, edges };
    },

    focusDone(courseId, session) {
      course(courseId);
      const clean = {
        at: nowSec(),
        minutes: Math.min(Math.max(Math.trunc(Number(session?.minutes) || 0), 0), 240),
        goal: [...str(session?.goal).trim()].slice(0, 300).join(""),
        recall: [...str(session?.recall).trim()].slice(0, 4000).join(""),
        parked: list(session?.parked).map(String).filter((note) => note.trim()),
      };
      return change(courseId, (p) => {
        p.days[today()] = (p.days[today()] ?? 0) + clean.minutes;
        p.sessions.push(clean);
        p.sessions = p.sessions.slice(-KEEP_SESSIONS);
        return focusStats(p.days, p.sessions);
      });
    },

    async ask(target, text) {
      const said = String(text ?? "").trim();
      if (!said) throw new Error("Напишите вопрос.");
      openDiscussion(target);
      const messages = [{ role: "system", content: tutorRules(discussion.material) }];
      for (const item of discussion.thread) messages.push({ role: "user", content: item.q }, { role: "assistant", content: item.a });
      messages.push({ role: "user", content: said });
      const reply = String((await needAi().chat(messages, { maxTokens: 700 })) ?? "").trim();
      if (!reply) throw new Error("Модель прислала пустой ответ.");
      discussion.thread.push({ q: said, a: reply });
      discussion.thread = discussion.thread.slice(-DEPTH);
      return reply;
    },

    async deep(courseId, topicId, at, cachedOnly = false) {
      const c = course(courseId);
      const t = topicOf(c, topicId);
      const parts = sections(t.lesson);
      const index = Math.min(Number(at) || 0, parts.length - 1);
      const key = `${courseId}/${topicId}#${index}`;
      const source = fingerprint(parts[index]);
      const saved = store.data.deep[key];
      if (saved && saved.source === source) return saved.text;
      if (cachedOnly) return null;
      const [material, title] = sectionMaterial(c, t, index);
      const rules =
        `Ты — репетитор ${name}. Разбери раздел урока подробно — для человека, который видит тему впервые и хочет её понять, а не выучить слова.\n\n${material}\n` +
        "Как писать:\n" +
        "- Каждую мысль раздела раскрой: что это, как устроено внутри — кто что делает и в каком порядке, — зачем оно нужно и что сломается без него.\n" +
        "- Перечисление (уровни, этапы, компоненты, команды) разбирай по пунктам: у каждого своё назначение, кто или что в нём работает, пример из практики.\n" +
        "- Пример — живой: команда с выводом, случай на сервере, бытовая аналогия.\n" +
        "- В конце — «Где ошибаются»: две-три типичные ошибки или путаницы.\n" +
        "- Не пересказывай раздел теми же словами — объясняй глубже.\n" +
        "- Markdown: подзаголовки ###, списки, `команды`, блоки кода. 400–900 слов.\n" +
        "- Без вступлений и без заключения «надеюсь, стало понятно».\n" +
        "Пиши по-русски.";
      const text = String(
        (await needAi().chat([{ role: "system", content: rules }, { role: "user", content: `Разбери раздел «${title}».` }], { maxTokens: 2800 })) ?? "",
      ).trim();
      if (chars(text) <= 200) throw new Error("Модель ответила слишком коротко — попробуйте ещё раз.");
      store.data.deep[key] = { source, text };
      store.save();
      return text;
    },
  };
}
