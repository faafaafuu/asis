// Путеводители на сервере: «Деревья Москвы», «Как построить дом самому»,
// «Всё про умный дом» — разделы, виды, цифры, советы и картинки, без задач.
//
// Собирает модель через мост, раздел за разделом: сначала план, потом каждый
// раздел своим вызовом, и путеводитель сохраняется после каждого — читать
// можно, пока он дописывается. Картинки — из Википедии: модель называет
// статью, сервер берёт её иллюстрацию и хранит рядом с путеводителем.
//
// Путеводитель живёт в аккаунте: его видят программа на компьютере,
// приложение на телефоне и Ноа онлайн. Прежде он жил на устройстве, где его
// собрали, и собирался там же — на телефоне по каналу, который рвётся.

import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { jsonIn, passing, plainError } from "../../src/js/web/course-builder.js";

const MIN_SECTIONS = 4;
const MAX_SECTIONS = 10;
const MAX_ITEM_PICTURES = 8;
const MAX_PICTURE = 4 * 1024 * 1024;
const MAX_GUIDE_CHARS = 400_000;
const PART_CHARS = 5000;
const STUCK_MS = 10 * 60_000;
/** Ширины — из стандартного ряда Wikimedia: другие она отвергает (400). */
export const WIDE = 960;
export const NARROW = 500;
/** Википедия отвечает 403 без внятного User-Agent — её правило для программ. */
const USER_AGENT = "NOAH/1.0 (https://noahlab.ru; guides)";
const RETRY_PAUSES = [5_000, 20_000, 60_000];

const str = (value) => (typeof value === "string" ? value : value == null ? "" : String(value));
const list = (value) => (Array.isArray(value) ? value : []);
const clean = (value) => str(value).trim();

export function shorten(text, limit) {
  const value = clean(text);
  return [...value].length <= limit ? value : `${[...value].slice(0, limit).join("").trimEnd()}…`;
}

export const validId = (id) => /^[A-Za-z0-9-]{1,64}$/.test(str(id));
const validFile = (file) => /^[\w-]{1,40}\.(jpg|png|webp|gif)$/.test(str(file));

/** Раздел дописан: есть текст или карточки. */
export const written = (section) => list(section?.text).length > 0 || list(section?.items).length > 0;

/** Карточка для полки — без текста разделов. */
export function cardOf(guide) {
  return {
    id: guide.id,
    title: guide.title,
    subtitle: guide.subtitle ?? "",
    emoji: guide.emoji ?? "📖",
    cover: guide.cover ?? null,
    sections: list(guide.sections).length,
    written: list(guide.sections).filter(written).length,
    status: guide.status,
    message: guide.message ?? "",
    created: guide.created ?? 0,
  };
}

/* ── Ответы модели ──────────────────────────────────────────────────────── */

export function planRules(name) {
  return `Ты — ${name}, автор увлекательных путеводителей. Человек просит путеводитель — составь его план.
Путеводитель — не курс: без задач и экзаменов. Его листают с интересом, как хороший научно-популярный журнал или энциклопедию для взрослых: разделы, особенности, виды, сравнения, цифры, советы, картинки. Не сухо, но и без воды.
Ответь строго JSON, без текста вокруг:
{"title": "…", "subtitle": "…", "emoji": "…", "intro": "…", "cover": "…", "cover_en": "…", "sections": [{"title": "…", "emoji": "…", "about": "…"}]}
- title — название путеводителя, 2–6 слов; subtitle — о чём он, до 12 слов.
- emoji — один значок темы.
- intro — 2–3 предложения, которые хочется дочитать: чем тема интересна и что человек узнает.
- cover — точное название статьи русской Википедии, чья иллюстрация подойдёт для обложки; cover_en — та же статья в английской Википедии.
- sections — ${MIN_SECTIONS}–8 разделов, от общего к частному: title — 2–5 слов, emoji — один значок, about — что в разделе, до 25 слов. Если тема — набор видов (деревья, породы, устройства), отведи им разделы по группам. Последний раздел может быть практическим: с чего начать, где посмотреть, сколько стоит.
Пиши по-русски.`;
}

/** План из ответа модели — в путеводитель. `null`, если плана нет. */
export function applyPlan(guide, raw) {
  const sections = list(raw?.sections)
    .filter((s) => clean(s?.title))
    .slice(0, MAX_SECTIONS)
    .map((s, n) => ({ id: `s${n + 1}`, title: clean(s.title), emoji: clean(s.emoji) || "•", about: clean(s.about) }));
  if (sections.length < 2) return null;
  return {
    ...guide,
    title: clean(raw.title) || guide.title,
    subtitle: clean(raw.subtitle),
    emoji: clean(raw.emoji) || guide.emoji,
    intro: clean(raw.intro),
    coverHint: clean(raw.cover),
    coverHintEn: clean(raw.cover_en),
    sections,
  };
}

export function sectionRules(name, guide, at, added) {
  const others = guide.sections
    .filter((_, n) => n !== at)
    .map((s) => `- ${s.title} — ${s.about}`)
    .join("\n");
  const rename = added ? "- title и emoji — название раздела (2–5 слов) и значок: человек описал раздел своими словами.\n" : "";
  return `Ты — ${name}, автор путеводителя «${guide.title}» (${guide.subtitle}). Пишешь один его раздел.
Другие разделы — их содержание не повторяй:
${others}

Ответь строго JSON, без текста вокруг:
{"title": "…", "emoji": "…", "lead": "…", "image": "…", "image_en": "…", "text": ["…"], "facts": [{"label": "…", "value": "…"}], "items": [{"name": "…", "text": "…", "tags": ["…"], "image": "…", "image_en": "…"}], "tips": ["…"], "wow": "…"}
${rename}- lead — одна фраза-подводка, до 25 слов.
- text — 2–4 абзаца по 40–90 слов. Можно выделить главное **жирным**.
- facts — 0–6 коротких характеристик: цифры, сроки, размеры, цены (label до 4 слов, value до 8). Пусто, если цифр по делу нет.
- items — 0–8 карточек, если раздел перечисляет виды, породы, устройства, этапы, места: name — название, text — чем примечателен и как узнать, до 35 слов, tags — 1–3 коротких признака, image — точное название статьи русской Википедии именно про этот предмет, image_en — та же статья в английской Википедии. Нет такой статьи — "" (лучше без картинки, чем чужая).
- tips — 0–4 практических совета, до 20 слов каждый.
- wow — один удивительный факт по теме раздела, до 30 слов, или "".
- image — точное название статьи русской Википедии, чья иллюстрация подойдёт разделу; image_en — та же статья в английской Википедии. Не статьи о людях, если раздел не о них.
Пиши живо и точно, без канцелярита. Не выдумывай факты, цифры и названия: не уверен — не пиши. По-русски.`;
}

/** Раздел из ответа модели. `null` — пустой или не JSON. */
export function applySection(section, raw, added) {
  if (!raw) return null;
  const text = list(raw.text).map(clean).filter(Boolean);
  const items = list(raw.items)
    .filter((i) => clean(i?.name))
    .slice(0, 12)
    .map((i) => ({ name: clean(i.name), text: clean(i.text), tags: list(i.tags).map(clean).filter(Boolean).slice(0, 3), imageHint: clean(i.image), imageHintEn: clean(i.image_en), picture: null }));
  if (!text.length && !items.length) return null;
  return {
    ...section,
    ...(added && clean(raw.title) ? { title: clean(raw.title), emoji: clean(raw.emoji) || section.emoji } : {}),
    lead: clean(raw.lead),
    imageHint: clean(raw.image),
    imageHintEn: clean(raw.image_en),
    text,
    facts: list(raw.facts)
      .filter((f) => clean(f?.label) && clean(f?.value))
      .slice(0, 8)
      .map((f) => ({ label: clean(f.label), value: clean(f.value) })),
    items,
    tips: list(raw.tips).map(clean).filter(Boolean).slice(0, 5),
    wow: clean(raw.wow),
  };
}

/* ── Картинки ───────────────────────────────────────────────────────────── */

/** «…/330px-Tilia.jpg» → «…/960px-Tilia.jpg». */
export function resized(thumb, width) {
  const slash = thumb.lastIndexOf("/");
  const name = thumb.slice(slash + 1);
  const px = name.indexOf("px-");
  if (slash < 0 || px <= 0 || !/^\d+$/.test(name.slice(0, px))) return null;
  return `${thumb.slice(0, slash + 1)}${width}${name.slice(px)}`;
}

/** Картинка шириной `width`: уменьшенная копия, а если оригинал меньше — он сам. */
export function imageUrl(page, width) {
  const originalWidth = Number(page?.originalimage?.width) || 0;
  const thumb = page?.thumbnail?.source;
  if (thumb && originalWidth > width) {
    const sized = resized(thumb, width);
    if (sized) return sized;
  }
  const original = page?.originalimage?.source;
  // Оригинал SVG Википедия отдаёт огромным — берём уменьшенную копию.
  if (original && !original.endsWith(".svg") && originalWidth <= width) return original;
  return thumb ?? null;
}

const getJson = (url) =>
  fetch(url, { headers: { "User-Agent": USER_AGENT }, signal: AbortSignal.timeout(20_000) })
    .then((r) => (r.ok ? r.json() : null))
    .catch(() => null);

const title = (text) => encodeURIComponent(clean(text).replace(/ /g, "_"));
const norm = (text) => clean(text).toLowerCase().replace(/ё/g, "е");

/**
 * Корень слова: длинному — длиннее. По пяти буквам «электропроводка»
 * совпадала с «электромобилем»; без окончания — «лианозовский» с «Лианозово».
 */
const stemOf = (word) => {
  const letters = [...word];
  return letters.slice(0, Math.max(5, letters.length - 4)).join("");
};
const significant = (text) => norm(text).split(/[^\p{L}\d]+/u).filter((word) => [...word].length >= 4);

export function stems(...texts) {
  return [...new Set(texts.flatMap(significant).map(stemOf))];
}

/**
 * О том ли статья: для какой-то из подсказок в названии есть корни
 * большинства её слов. Уточнение в скобках («Балка (рельеф)») должно
 * подходить к теме (`context`), иначе это другое значение слова.
 */
export function relevant(name, hints, context = "") {
  const found = norm(name);
  const match = hints.some((hint) => {
    const words = significant(hint);
    if (!words.length) return false;
    const hit = words.filter((word) => found.includes(stemOf(word))).length;
    return hit / words.length >= 0.6;
  });
  if (!match) return false;
  const qualifier = /\(([^)]+)\)/.exec(clean(name))?.[1];
  if (!qualifier || !clean(context)) return true;
  // «Рыжик (гриб)» в путеводителе о грибах — да, «Балка (рельеф)» о стройке — нет.
  const around = norm(`${context} ${hints.join(" ")}`);
  return significant(qualifier).some((word) => around.includes(stemOf(word)));
}

// Статьи о людях — только если путеводитель о них: «Электропроводка» однажды
// получила портрет Илона Маска — первый результат поиска.
const PERSON = /(предпринимател|политик|актёр|актер|актрис|певец|певица|писател|футболист|режиссёр|режиссер|рэпер|блогер|журналист|инвестор|businessman|businesswoman|politician|actor|actress|singer|writer|footballer|entrepreneur|rapper|journalist|investor)/i;
export const aboutPerson = (page, hints) => PERSON.test(clean(page?.description)) && !relevant(page?.title, hints);

/** Говорит ли статья о теме путеводителя: в её описании — корень слова темы. */
export function onTopic(page, context) {
  const topic = significant(context);
  if (!topic.length) return true;
  const text = norm(`${page?.title ?? ""} ${page?.description ?? ""} ${page?.extract ?? ""}`);
  return topic.some((word) => text.includes(stemOf(word)));
}

async function summaryOf(lang, hint) {
  if (!clean(hint)) return null;
  const page = await getJson(`https://${lang}.wikipedia.org/api/rest_v1/page/summary/${title(hint)}`);
  // Страница неоднозначности — не та статья, и картинки у неё нет.
  if (!page || page.type === "disambiguation" || !imageUrl(page, NARROW)) return null;
  return page;
}

async function searchOf(lang, text) {
  if (!clean(text)) return [];
  const found = await getJson(`https://${lang}.wikipedia.org/w/api.php?action=query&list=search&srlimit=3&format=json&srsearch=${encodeURIComponent(clean(text))}`);
  return list(found?.query?.search).map((hit) => hit.title);
}

/** Файл с Wikimedia Commons — когда у статей картинки нет. */
async function commonsFile(query, hints, width, context = "") {
  if (!clean(query)) return null;
  const found = await getJson(
    `https://commons.wikimedia.org/w/api.php?action=query&format=json&generator=search&gsrnamespace=6&gsrlimit=8&prop=imageinfo&iiprop=url|mime&iiurlwidth=${width}&gsrsearch=${encodeURIComponent(`${clean(query)} filetype:bitmap`)}`,
  );
  const pages = Object.values(found?.query?.pages ?? {}).sort((a, b) => (a.index ?? 0) - (b.index ?? 0));
  for (const page of pages) {
    const info = page.imageinfo?.[0];
    const name = clean(page.title).replace(/^File:/, "").replace(/\.\w+$/, "");
    if (!info?.thumburl || !/^image\/(jpeg|png)$/.test(info.mime ?? "") || !relevant(name, hints, context)) continue;
    return { url: info.thumburl, source: info.descriptionurl ?? "", caption: name };
  }
  return null;
}

/**
 * Где взять картинку: статья русской Википедии, английской, поиск по ним —
 * только если найденное о том же, — и файлы Commons. Не нашли уверенно —
 * без картинки: пустая лучше чужой.
 */
export async function findPicture(wanted, width) {
  const hint = typeof wanted === "string" ? { ru: wanted } : wanted ?? {};
  const ru = clean(hint.ru);
  const en = clean(hint.en);
  const name = clean(hint.name);
  const context = clean(hint.context);
  // Тема путеводителя — для проверки смысла: разделы («Кофе мира») шумят.
  const topic = clean(hint.topic) || context;
  const hints = [ru, en, name].filter(Boolean);
  if (!hints.length) return null;
  const page = (found) => found && !aboutPerson(found, hints) && { url: imageUrl(found, width), source: found.content_urls?.desktop?.page ?? "", caption: found.title };
  let picked = page(await summaryOf("ru", ru)) || page(await summaryOf("en", en));
  if (!picked) {
    for (const other of await searchOf("ru", ru || name)) {
      if (!relevant(other, [ru, name].filter(Boolean), context)) continue;
      // Найдено поиском — статья должна быть о теме путеводителя: «Гейша» в
      // путеводителе о кофе — сорт, а не японские артистки.
      const found = await summaryOf("ru", other);
      if (found && onTopic(found, topic) && (picked = page(found))) break;
    }
  }
  if (!picked && en) {
    for (const other of await searchOf("en", en)) {
      if (relevant(other, [en], context) && (picked = page(await summaryOf("en", other)))) break;
    }
  }
  // Файлы Commons — только по названной статье: одно слово («Балка») там
  // находит овраг на Украине.
  if (!picked && (en || ru)) picked = await commonsFile(en || ru, [en, ru].filter(Boolean), width, context);
  return picked?.url ? picked : null;
}

const KINDS = { "image/png": "png", "image/webp": "webp", "image/gif": "gif", "image/jpeg": "jpg" };

/** Иллюстрация — в папку путеводителя. Нет — путеводитель без неё. */
export async function fetchPicture(dir, wanted, base, width) {
  const picked = await findPicture(wanted, width);
  if (!picked) return null;
  try {
    const response = await fetch(picked.url, { headers: { "User-Agent": USER_AGENT }, signal: AbortSignal.timeout(20_000) });
    const ext = KINDS[(response.headers.get("content-type") ?? "").split(";")[0]];
    if (!response.ok || !ext) return null;
    const bytes = Buffer.from(await response.arrayBuffer());
    if (!bytes.length || bytes.length > MAX_PICTURE) return null;
    await mkdir(dir, { recursive: true });
    const file = `${base}.${ext}`;
    await writeFile(join(dir, file), bytes);
    return { file, source: picked.source, caption: picked.caption };
  } catch {
    return null;
  }
}

/* ── Хранение, сборка, адреса ───────────────────────────────────────────── */

export class Stopped extends Error {}

export function mountGuides({ route, db, Fail, readJson, chat, owner, need, dataDir, name = "Ноа" }) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS noa_guides (
      user_id TEXT NOT NULL,
      id TEXT NOT NULL,
      body TEXT NOT NULL,
      status TEXT NOT NULL,
      created INTEGER NOT NULL,
      updated TEXT NOT NULL DEFAULT (datetime('now')),
      PRIMARY KEY (user_id, id)
    );
  `);
  const root = join(dataDir, "guides");
  const folder = (userId, id) => join(root, String(userId), id);

  const load = (userId, id) => {
    const row = db.prepare("SELECT body FROM noa_guides WHERE user_id = ? AND id = ?").get(String(userId), id);
    return row ? JSON.parse(row.body) : null;
  };
  const save = (userId, guide) => {
    db.prepare(
      `INSERT INTO noa_guides (user_id, id, body, status, created) VALUES (?, ?, ?, ?, ?)
       ON CONFLICT(user_id, id) DO UPDATE SET body = excluded.body, status = excluded.status, updated = datetime('now')`,
    ).run(String(userId), guide.id, JSON.stringify(guide), guide.status, guide.created ?? Date.now());
  };
  /** Записать изменения поверх свежей копии: путеводитель могли удалить. */
  const patch = (userId, id, change) => {
    const fresh = load(userId, id);
    if (!fresh) throw new Stopped("Путеводитель удалили.");
    const next = change(fresh);
    save(userId, next);
    return next;
  };
  const version = (userId) => {
    const row = db.prepare("SELECT COUNT(*) AS n, COALESCE(MAX(updated), '') AS at FROM noa_guides WHERE user_id = ?").get(String(userId));
    return `${row.n}:${row.at}`;
  };

  /** Идущие сборки: человек → { id, stop }. Одна за раз на человека. */
  const running = new Map();

  const ask = async (control, rules, said) => {
    for (let attempt = 0; ; attempt++) {
      if (control.stop) throw new Stopped("Сборку остановили — готовые разделы на месте.");
      try {
        const text = await chat("claude-code-bridge", [{ role: "system", content: rules }, { role: "user", content: said }], true, "sonnet");
        if (clean(text)) return text;
        throw new Error("Модель прислала пустой ответ.");
      } catch (err) {
        if (attempt >= RETRY_PAUSES.length || !passing(err)) throw new Error(plainError(err));
        await new Promise((done) => setTimeout(done, RETRY_PAUSES[attempt]));
      }
    }
  };

  async function build(userId, id, added, control) {
    let guide = load(userId, id);
    const dir = folder(userId, id);
    if (!guide.sections?.length) {
      let planned = null;
      for (let attempt = 0; attempt < 2 && !planned; attempt++) {
        planned = applyPlan(guide, jsonIn(await ask(control, planRules(name), `Путеводитель: ${guide.goal}`)));
      }
      if (!planned) throw new Error("План путеводителя не сложился — попробуйте ещё раз или опишите тему иначе.");
      guide = patch(userId, id, (fresh) => ({ ...fresh, ...planned, status: "building", message: "" }));
      const cover = await fetchPicture(dir, { ru: guide.coverHint, en: guide.coverHintEn, name: guide.title }, "cover", WIDE);
      if (cover) guide = patch(userId, id, (fresh) => ({ ...fresh, cover }));
    }
    const total = guide.sections.length;
    for (let at = 0; at < total; at++) {
      if (written(guide.sections[at])) continue;
      const last = added && at === total - 1;
      let section = null;
      // Две попытки: длинный JSON изредка обрывается или ломается.
      for (let attempt = 0; attempt < 2 && !section; attempt++) {
        const said = `Раздел «${guide.sections[at].title}»: ${guide.sections[at].about}`;
        section = applySection(guide.sections[at], jsonIn(await ask(control, sectionRules(name, guide, at, last), said)), last);
      }
      if (!section) throw new Error("Раздел не сложился. Попробуйте дописать путеводитель ещё раз.");
      const base = `s${at + 1}`;
      const context = `${guide.title} ${guide.sections[at].title}`;
      section.picture = await fetchPicture(dir, { ru: section.imageHint, en: section.imageHintEn, context, topic: guide.title }, base, WIDE);
      let pictured = 0;
      for (const [n, item] of section.items.entries()) {
        if (pictured >= MAX_ITEM_PICTURES || control.stop) break;
        item.picture = await fetchPicture(dir, { ru: item.imageHint, en: item.imageHintEn, name: item.name, context, topic: guide.title }, `${base}-${n + 1}`, NARROW);
        if (item.picture) pictured += 1;
      }
      guide = patch(userId, id, (fresh) => {
        const sections = [...fresh.sections];
        sections[at] = section;
        return { ...fresh, sections, status: "building" };
      });
    }
  }

  function run(userId, id, added) {
    const control = { id, stop: false };
    running.set(String(userId), control);
    build(userId, id, added, control)
      .then(() => patch(userId, id, (fresh) => ({ ...fresh, status: "ready", message: "", picturesVersion: 4 })))
      .catch((err) => {
        if (!(err instanceof Stopped)) console.error("путеводитель:", err?.message ?? err);
        try {
          patch(userId, id, (fresh) => ({
            ...fresh,
            status: fresh.sections?.length ? "stopped" : "failed",
            message: str(err?.message ?? err),
          }));
        } catch {
          /* путеводитель удалили посреди сборки */
        }
      })
      .finally(() => {
        if (running.get(String(userId)) === control) running.delete(String(userId));
      });
  }

  const busy = (userId) => {
    const control = running.get(String(userId));
    if (control) {
      const other = load(userId, control.id);
      throw new Fail(409, `Уже пишу путеводитель${other ? ` «${other.title}»` : ""} — дождитесь его или остановите.`);
    }
  };

  /**
   * Картинки готовых путеводителей — заново, где их нет или они не о том:
   * до 21.10 поиск брал первую попавшуюся статью. Подсказки, которых модель
   * не дала, она называет одним коротким вызовом. Новые файлы — с новыми
   * именами: старая неверная картинка не всплывёт из копии на устройстве.
   */
  const PICTURES_VERSION = 4;
  async function repairPictures(userId, id) {
    const guide = load(userId, id);
    if (!guide || guide.status === "building" || guide.picturesVersion >= PICTURES_VERSION) return;
    const dir = folder(userId, id);
    const entries = [];
    entries.push({ path: ["cover"], name: guide.title, ru: guide.coverHint, en: guide.coverHintEn, picture: guide.cover, width: WIDE, base: "cover" });
    list(guide.sections).forEach((section, at) => {
      entries.push({ path: ["sections", at], name: section.title, ru: section.imageHint, en: section.imageHintEn, picture: section.picture, width: WIDE, base: `s${at + 1}` });
      list(section.items).forEach((item, n) => {
        entries.push({ path: ["sections", at, "items", n], name: item.name, ru: item.imageHint, en: item.imageHintEn, picture: item.picture, width: NARROW, base: `s${at + 1}-${n + 1}`, section: section.title });
      });
    });
    // Найденная прямой статьёй (в том числе переадресацией: «Робуста» — «Кофе
    // канефора») — верная; по поиску и не о том — меняем.
    const doubtful = async (entry) => {
      if (!entry.picture) return true;
      // Найденное прошлыми починками — заново, по строгим правилам.
      if (/-v[23]\./.test(entry.picture.file ?? "")) return true;
      if (relevant(entry.picture.caption, [entry.ru, entry.en, entry.name].filter(Boolean), `${guide.title} ${entry.section ?? ""}`)) return false;
      const direct = await summaryOf("ru", entry.ru);
      return !direct || direct.title !== entry.picture.caption;
    };
    const redo = [];
    for (const entry of entries) if (await doubtful(entry)) redo.push(entry);
    if (redo.length) {
      const unnamed = redo.filter((entry) => !entry.ru || !entry.en);
      if (unnamed.length) {
        try {
          const asked = unnamed.map((entry, i) => `${i + 1}. ${entry.name}${entry.section ? ` (раздел «${entry.section}»)` : ""}`).join("\n");
          const reply = await chat(
            "claude-code-bridge",
            [
              {
                role: "system",
                content: `Путеводитель «${guide.title}». Для каждого пункта назови точное название статьи русской Википедии именно про этот предмет и ту же статью в английской Википедии. Нет такой статьи — "". Ответ строго JSON: {"items": [{"n": 1, "ru": "…", "en": "…"}]}`,
              },
              { role: "user", content: asked },
            ],
            false,
            "haiku",
          );
          const named = jsonIn(reply);
          for (const hit of Array.isArray(named) ? named : list(named?.items)) {
            const entry = unnamed[Number(hit?.n) - 1];
            if (!entry) continue;
            entry.ru ||= clean(hit.ru);
            entry.en ||= clean(hit.en);
          }
        } catch (err) {
          console.error("путеводитель: подсказки картинок:", err?.message ?? err);
        }
      }
      for (const entry of redo) {
        entry.fresh = await fetchPicture(dir, { ru: entry.ru, en: entry.en, name: entry.name, context: `${guide.title} ${entry.section ?? entry.name}`, topic: guide.title }, `${entry.base}-v${PICTURES_VERSION}`, entry.width);
      }
    }
    patch(userId, id, (fresh) => {
      const next = structuredClone(fresh);
      for (const entry of redo) {
        let target = next;
        for (const key of entry.path.slice(0, -1)) target = target?.[key];
        const last = entry.path[entry.path.length - 1];
        if (!target) continue;
        if (entry.path.length === 1) {
          next.cover = entry.fresh ?? null;
          next.coverHint ||= entry.ru ?? "";
          next.coverHintEn ||= entry.en ?? "";
        } else {
          const holder = target[last];
          if (!holder) continue;
          holder.picture = entry.fresh ?? null;
          holder.imageHint ||= entry.ru ?? "";
          holder.imageHintEn ||= entry.en ?? "";
        }
      }
      return { ...next, picturesVersion: PICTURES_VERSION };
    });
    console.log(`путеводитель ${id}: картинки проверены — заменено ${redo.length}, найдено ${redo.filter((e) => e.fresh).length}`);
  }

  // Готовые путеводители — проверить картинки, по одному, в фоне.
  setTimeout(async () => {
    for (const row of db.prepare("SELECT user_id, id FROM noa_guides").all()) {
      try {
        await repairPictures(row.user_id, row.id);
      } catch (err) {
        if (!(err instanceof Stopped)) console.error("путеводитель: починка картинок:", err?.message ?? err);
      }
    }
  }, 15_000);

  // Сервер перезапустили посреди сборки — недописанное останавливается,
  // дописать можно кнопкой.
  for (const row of db.prepare("SELECT user_id, id FROM noa_guides WHERE status = 'building'").all()) {
    try {
      patch(row.user_id, row.id, (fresh) => ({ ...fresh, status: fresh.sections?.length ? "stopped" : "failed", message: "Сборку прервал перезапуск сервера — нажмите «Дописать»." }));
    } catch {
      /* нечего чинить */
    }
  }

  route("GET", /^\/api\/noa\/guides$/, ({ user: who, tokenUser }) => {
    const user = need(who ?? tokenUser);
    const rows = db.prepare("SELECT body FROM noa_guides WHERE user_id = ? ORDER BY created DESC").all(user.id);
    return { guides: rows.map((row) => cardOf(JSON.parse(row.body))), version: version(user.id) };
  });

  route("GET", /^\/api\/noa\/guides\/([A-Za-z0-9-]{1,64})$/, ({ user: who, tokenUser, match }) => {
    const user = need(who ?? tokenUser);
    const guide = load(user.id, match[1]);
    if (!guide) throw new Fail(404, "Нет такого путеводителя.");
    return { guide };
  });

  route("GET", /^\/api\/noa\/guides\/([A-Za-z0-9-]{1,64})\/pictures\/([\w.-]{1,48})$/, async ({ res, user: who, tokenUser, match }) => {
    const user = need(who ?? tokenUser);
    if (!validFile(match[2])) throw new Fail(404, "Нет такой картинки.");
    let bytes;
    try {
      bytes = await readFile(join(folder(user.id, match[1]), match[2]));
    } catch {
      throw new Fail(404, "Нет такой картинки.");
    }
    const ext = match[2].split(".").pop();
    res.writeHead(200, {
      "Content-Type": { png: "image/png", webp: "image/webp", gif: "image/gif" }[ext] ?? "image/jpeg",
      "Content-Length": bytes.length,
      "Cache-Control": "private, max-age=86400",
    });
    res.end(bytes);
    return undefined;
  });

  // Новый путеводитель: id сразу, дальше он пишется сам.
  route("POST", /^\/api\/noa\/guides$/, async ({ req, user: who, tokenUser }) => {
    const user = owner(who ?? tokenUser);
    const { goal } = await readJson(req);
    const text = clean(goal);
    if ([...text].length < 3) throw new Fail(400, "Напишите, о чём путеводитель: например, «деревья Москвы» или «умный дом».");
    if (text.length > 1000) throw new Fail(400, "Слишком длинно — хватит пары предложений.");
    busy(user.id);
    const now = Date.now();
    const guide = { id: `g${now}`, title: shorten(text, 60), goal: text, emoji: "📖", sections: [], status: "building", message: "Составляю план", created: now };
    save(user.id, guide);
    run(user.id, guide.id, false);
    return { id: guide.id };
  });

  // Дописать прерванный или добавить раздел по просьбе.
  route("POST", /^\/api\/noa\/guides\/([A-Za-z0-9-]{1,64})\/resume$/, async ({ req, user: who, tokenUser, match }) => {
    const user = owner(who ?? tokenUser);
    const { wish } = await readJson(req);
    if (!load(user.id, match[1])) throw new Fail(404, "Нет такого путеводителя.");
    busy(user.id);
    const extra = clean(wish);
    patch(user.id, match[1], (fresh) => ({
      ...fresh,
      sections: extra
        ? [...fresh.sections, { id: `s${fresh.sections.length + 1}`, title: shorten(extra, 40), emoji: "✨", about: extra }]
        : fresh.sections,
      status: "building",
      message: "",
    }));
    run(user.id, match[1], Boolean(extra));
    return { ok: true };
  });

  route("POST", /^\/api\/noa\/guides\/stop$/, ({ user: who, tokenUser }) => {
    const user = need(who ?? tokenUser);
    const control = running.get(user.id);
    if (control) control.stop = true;
    return { ok: true };
  });

  route("POST", /^\/api\/noa\/guides\/([A-Za-z0-9-]{1,64})\/remove$/, async ({ user: who, tokenUser, match }) => {
    const user = need(who ?? tokenUser);
    const control = running.get(user.id);
    if (control?.id === match[1]) control.stop = true;
    db.prepare("DELETE FROM noa_guides WHERE user_id = ? AND id = ?").run(user.id, match[1]);
    await rm(folder(user.id, match[1]), { recursive: true, force: true });
    return { ok: true };
  });

  // Путеводитель, собранный на устройстве до переезда на сервер: текстом,
  // кусками (канал рвёт длинные запросы). Картинки сервер берёт заново по
  // тем же статьям Википедии.
  const uploads = new Map();
  route("POST", /^\/api\/noa\/guides\/([A-Za-z0-9-]{1,64})\/part$/, async ({ req, user: who, tokenUser, match }) => {
    const user = need(who ?? tokenUser);
    const { at, text, size } = await readJson(req, PART_CHARS * 4 + 1000);
    const total = Number(size) | 0;
    if (total <= 0 || total > MAX_GUIDE_CHARS) throw new Fail(400, "Путеводитель слишком большой.");
    const key = `${user.id}/${match[1]}`;
    const start = Number(at) | 0;
    if (start === 0) uploads.set(key, { text: "", size: total, at: Date.now() });
    const upload = uploads.get(key);
    if (!upload || upload.text.length !== start || upload.size !== total) {
      uploads.delete(key);
      throw new Fail(409, "Куски путеводителя пришли не по порядку — начните заново.");
    }
    upload.text += str(text);
    upload.at = Date.now();
    for (const [other, item] of uploads) if (Date.now() - item.at > STUCK_MS) uploads.delete(other);
    if (upload.text.length < total) return { at: upload.text.length };
    uploads.delete(key);
    let guide;
    try {
      guide = JSON.parse(upload.text);
    } catch {
      throw new Fail(400, "Путеводитель пришёл битым.");
    }
    if (guide?.id !== match[1] || !Array.isArray(guide.sections)) throw new Fail(400, "Путеводитель пришёл битым.");
    if (load(user.id, guide.id)) return { ok: true, existed: true };
    const status = guide.status === "building" ? "stopped" : ["ready", "stopped", "failed"].includes(guide.status) ? guide.status : "ready";
    save(user.id, { ...guide, status });
    refetchPictures(user.id, guide.id).catch((err) => console.error("путеводитель: картинки:", err?.message ?? err));
    return { ok: true };
  });

  /** Картинки перенесённого путеводителя — заново из Википедии. */
  async function refetchPictures(userId, id) {
    const dir = folder(userId, id);
    const guide = load(userId, id);
    if (!guide) return;
    const hint = (picture, fallback) => clean(fallback) || clean(picture?.caption);
    const cover = guide.cover || guide.coverHint ? await fetchPicture(dir, { ru: hint(guide.cover, guide.coverHint), name: guide.title }, "cover", WIDE) : null;
    const sections = [];
    for (const [at, section] of guide.sections.entries()) {
      const base = `s${at + 1}`;
      const picture = section.picture || section.imageHint ? await fetchPicture(dir, { ru: hint(section.picture, section.imageHint) }, base, WIDE) : null;
      const items = [];
      for (const [n, item] of list(section.items).entries()) {
        const wanted = item.picture || item.imageHint;
        items.push({ ...item, picture: wanted ? await fetchPicture(dir, { ru: hint(item.picture, item.imageHint), name: item.name }, `${base}-${n + 1}`, NARROW) : null });
      }
      sections.push({ ...section, picture, items });
    }
    try {
      patch(userId, id, (fresh) => ({ ...fresh, cover, sections }));
    } catch {
      /* удалили, пока качались картинки */
    }
  }
}
