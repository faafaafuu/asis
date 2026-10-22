// Выученное произношение. Словарь программы (src-tauri/assets/pronounce.json)
// знает частые термины, а новое слово — «Velero», «Karpenter», «Cilium» —
// голос прочёл бы по буквам. Сервер собирает такие слова из того, что
// читают голосом (сайт, телефон, программа присылает свои), раз в несколько
// минут спрашивает модель, как их произносят по-русски, и раздаёт
// выученное: голосовому серверу — файлом, устройствам — по /api/noa/pronounce.

import { readFileSync, writeFileSync, renameSync } from "node:fs";
import { join } from "node:path";

import { jsonIn } from "../../src/js/web/course-builder.js";

const BASE = process.env.NOAH_PRONOUNCE ?? "/opt/noah-voice/pronounce.json";
const BATCH = 40;
const STUDY_MS = 5 * 60_000;
/** Произношение — только кириллица, дефис и пробел: иначе голос снова споткнётся. */
const SPOKEN = /^[а-яё][а-яё\- ]{0,40}$/i;

const RULES = `Ты помогаешь синтезатору речи, который читает только кириллицу. Для каждого слова из списка напиши, как его произносит вслух русскоязычный инженер.
- Кириллицей, без латиницы и цифр, строчными.
- Аббревиатуры — по английским буквам через дефис: SSH — «эс-эс-эйч», etcd — «и-ти-си-ди».
- Названия продуктов и утилит — как принято в русской речи: kubectl — «кубкатл», nginx — «энджинкс».
- Обычное английское слово — как его говорят по-русски: deploy — «деплой».
- Опечатку, мусор и случайный набор букв пропусти.
Ответ — только JSON-объект {"слово": "произношение"}, без текста вокруг.`;

let base = {};
let learned = {};
let file = "";
let chat = null;
const queue = new Set();

const known = (word) => word in base || word in learned;

function save() {
  if (!file) return;
  try {
    writeFileSync(`${file}.tmp`, JSON.stringify(learned, null, 1), { mode: 0o644 });
    renameSync(`${file}.tmp`, file);
  } catch (err) {
    console.error("произношение: не сохранилось:", err.message);
  }
}

/** Латинские слова из прочитанного голосом — в очередь на разучивание. */
export function note(text) {
  for (const word of String(text ?? "").match(/[A-Za-z][A-Za-z0-9]{2,30}/g) ?? []) {
    // Прописные — аббревиатура, их голос читает по буквам и так верно.
    if (/^[A-Z0-9]+$/.test(word)) continue;
    const lower = word.toLowerCase();
    if (!known(lower) && queue.size < 500) queue.add(lower);
  }
}

/** Термин с произношением — от модели или из аудиодрамы. */
export function teach(term, spoken) {
  const word = String(term ?? "").trim().toLowerCase();
  const said = String(spoken ?? "").trim().toLowerCase();
  if (!/^[a-z][a-z0-9.+#-]{1,40}$/.test(word) || !SPOKEN.test(said) || word in base) return false;
  if (learned[word] === said) return true;
  learned[word] = said;
  save();
  return true;
}

async function study() {
  if (!chat || !queue.size) return;
  const words = [...queue].slice(0, BATCH);
  for (const word of words) queue.delete(word);
  try {
    const reply = await chat("claude-code-bridge", [{ role: "system", content: RULES }, { role: "user", content: words.join("\n") }], false, "haiku");
    const answer = jsonIn(reply) ?? {};
    let taught = 0;
    for (const [word, spoken] of Object.entries(answer)) if (words.includes(word.toLowerCase()) && teach(word, spoken)) taught += 1;
    console.log(`произношение: выучено ${taught} из ${words.length}`);
  } catch (err) {
    console.error("произношение: модель не ответила:", err?.message ?? err);
  }
}

export function mountPronounce({ route, readJson, need, chat: ask, dataDir }) {
  chat = ask;
  file = join(dataDir, "pronounce-learned.json");
  try {
    base = Object.fromEntries(Object.entries(JSON.parse(readFileSync(BASE, "utf8")).words ?? {}).map(([k, v]) => [k.toLowerCase(), v]));
  } catch {
    base = {};
  }
  try {
    learned = JSON.parse(readFileSync(file, "utf8"));
  } catch {
    learned = {};
  }
  setInterval(study, STUDY_MS).unref();

  // Выученное — устройствам: программа и телефон читают голосом сами.
  route("GET", /^\/api\/noa\/pronounce$/, ({ user: who, tokenUser }) => {
    need(who ?? tokenUser);
    return { words: learned };
  });

  // Незнакомые слова, которые устройство прочло по звучанию.
  route("POST", /^\/api\/noa\/pronounce\/unknown$/, async ({ req, user: who, tokenUser }) => {
    need(who ?? tokenUser);
    const { words } = await readJson(req, 20_000);
    note((Array.isArray(words) ? words : []).slice(0, 200).join(" "));
    return { ok: true };
  });
}
