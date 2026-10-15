// «Собеседование» — мок-интервью по курсу или теме. Ноа — интервьюер:
// задаёт вопросы вслух (на понимание, сравнение, кейсы, траблшутинг, а не
// пересказ определений), слушает ответ, на неполный — один уточняющий
// вопрос, как на живом собеседовании. После каждого — балл, что сказано
// верно, чего не хватило, образцовый ответ. В конце — итог и что повторить.

import { el, icon, label, loading, nav, register, iconButton, call, voice, toast, prefs, steps, markdown, ring, api } from "../core.js";
import { store, topicCard } from "../store.js";
import { coach, coachJson, parseJson } from "../coach.js";
import { swipeRow, pressMenu, collapse, confirmAction } from "../gestures.js";

const LEVELS = [
  { id: "junior", name: "Junior" },
  { id: "middle", name: "Middle" },
  { id: "senior", name: "Senior" },
];
const COUNTS = [5, 8, 12];
/** Балл, с которым на реальном собеседовании ответ засчитали бы. */
const PASS = 70;

/** Материал для вопросов: темы и их понятия — коротко, чтобы влезло в запрос. */
async function material(course, topicId) {
  const topics = topicId ? course.topics.filter((t) => t.id === topicId) : course.topics;
  const perTopic = topicId ? 16 : 5;
  const views = await Promise.all(topics.map((t) => store.topic(course.id, t.id).catch(() => null)));
  return topics
    .map((t, i) => {
      const concepts = (views[i]?.topic?.concepts ?? []).slice(0, perTopic);
      const lines = concepts.map((c) => `- ${c.term}: ${String(c.definition).slice(0, 160)}`);
      return `## ${t.title} (id: ${t.id})\n${lines.join("\n")}`;
    })
    .join("\n\n");
}

/** История собеседований — балл по времени: виден рост. */
const history = () => prefs.get("interviews", []);

/** Вопрос разобран до конца (оценён), а не только начат. */
const closed = (r) => Boolean(r && r.verdict && r.score != null);

/** Средний балл по разобранным вопросам. */
const average = (list) => {
  const done = list.filter(closed);
  return done.length ? Math.round(done.reduce((sum, r) => sum + r.score, 0) / done.length) : 0;
};

/** Записать собеседование в историю; то же `at` — обновить запись (продолжили незаконченное). */
function remember(entry) {
  const all = history().filter((one) => one.at !== entry.at);
  all.push(entry);
  prefs.set("interviews", all.slice(-50));
}


register("interview", (screen, { course: courseId, topic: topicId = null }) => {
  const course = store.course(courseId);
  const meta = el("span", "bar__meta");
  meta.style.marginRight = "8px";
  const bar = el("header", "bar");
  bar.append(iconButton("close", "Закрыть", () => nav.back()), el("span", "bar__title", "Собеседование"), meta);
  const progressRow = el("div", "pbar-steps");
  const content = el("div", "content");
  content.style.cssText = "padding:16px;gap:14px";
  const dock = el("div", "dock");
  screen.append(bar, progressRow, content, dock, el("div", "homebar"));

  let alive = true;
  let scope = topicId;
  let level = prefs.get("interviewLevel", "middle");
  let count = prefs.get("interviewCount", 8);
  let aloud = prefs.get("interviewAloud", true);
  /** Разговором: Ноа спрашивает голосом и сама слушает — без кнопок. */
  let talk = prefs.get("interviewTalk", true);
  let questions = [];
  let results = [];
  let at = 0;
  /** Когда начато это собеседование — ключ черновика и записи в истории. */
  let startedAt = 0;
  /** Итог показан — дальше только просмотр, черновик больше не пишется. */
  let finished = false;
  /** Незаконченное собеседование по курсу: вышли — можно продолжить с того же вопроса. */
  const draftKey = `interviewDraft:${courseId}`;
  /** Снять подписку на распознаваемую речь (разговором). */
  let unhear = null;

  const seg = (options, current, pick) => {
    const box = el("div", "seg");
    for (const option of options) {
      const btn = el("button", "", option.name);
      btn.setAttribute("aria-pressed", String(option.id === current));
      btn.addEventListener("click", () => {
        pick(option.id);
        for (const other of box.children) other.setAttribute("aria-pressed", String(other === btn));
      });
      box.append(btn);
    }
    return box;
  };

  /* ── Подготовка ─────────────────────────────────────────────────────── */

  function setup() {
    meta.textContent = "";
    progressRow.replaceChildren();
    const box = el("div", "plate plate--active");
    box.append(label("// мок-интервью"), el("span", "continue__title", course?.title ?? "Курс"));
    box.append(
      el(
        "span",
        "continue__lead",
        "Ноа ведёт собеседование как живой интервьюер: вопросы на понимание, сравнение и кейсы, уточняет, если ответ неполный. Отвечайте голосом — как на настоящем собеседовании.",
      ),
    );
    content.replaceChildren(box);
    paintHistory();
    paintDraft();

    if (topicId) {
      content.append(label("// о чём"));
      const card = topicCard(course, topicId);
      content.append(
        seg(
          [
            { id: topicId, name: "Эта тема" },
            { id: "", name: "Весь курс" },
          ],
          scope ?? "",
          (id) => (scope = id || null),
        ),
      );
      if (card) content.append(el("span", "small dim", card.title));
    }
    content.append(
      label("// формат"),
      seg(
        [
          { id: "talk", name: "Разговором" },
          { id: "buttons", name: "С кнопками" },
        ],
        talk ? "talk" : "buttons",
        (id) => (talk = id === "talk"),
      ),
      el(
        "span",
        "small dim",
        "Разговором — как на созвоне: Ноа спрашивает и дослушивает, переспросить можно. Замолчал на пару секунд — ответ ушёл; сразу — «вот как-то так». Верно — следующий вопрос, неполно — одно-два уточнения. «Повтори», «пропустим», «стоп» — голосом.",
      ),
    );
    content.append(label("// уровень"), seg(LEVELS, level, (id) => (level = id)));
    content.append(
      label("// вопросов"),
      seg(
        COUNTS.map((n) => ({ id: n, name: String(n) })),
        count,
        (n) => (count = n),
      ),
    );
    const switchRow = el("button", "setting");
    const text = el("span", "setting__text");
    text.append(el("span", "setting__name", "Вопросы вслух"), el("span", "setting__hint", "Ноа задаёт вопрос голосом, как на звонке"));
    const toggle = el("span", "switch");
    toggle.setAttribute("aria-checked", String(aloud));
    switchRow.append(text, toggle);
    switchRow.addEventListener("click", () => {
      aloud = !aloud;
      toggle.setAttribute("aria-checked", String(aloud));
    });
    content.append(switchRow);

    const go = el("button", "btn btn--primary btn--big");
    go.append(icon("play", 20, 2.25), "Начать собеседование");
    go.addEventListener("click", start);
    dock.replaceChildren(go);
  }

  /** Незаконченное собеседование — продолжить с того же вопроса или начать заново. */
  function paintDraft() {
    const draft = prefs.get(draftKey, null);
    if (!draft?.questions?.length) return;
    const done = draft.results.filter(closed).length;
    if (done >= draft.questions.length) return prefs.set(draftKey, null);
    const plate = el("div", "plate plate--active");
    plate.append(
      label(`// не закончено · ${done} из ${draft.questions.length}`),
      el("span", "continue__lead", done ? `Разобрано ${done}, пока ${average(draft.results)}%. Продолжим с ${done + 1}-го вопроса.` : "Вопросы готовы, начнём с первого."),
    );
    const go = el("button", "btn btn--primary");
    go.style.cssText = "height:44px;margin-top:6px";
    go.append(icon("play", 18, 2.25), "Продолжить");
    go.addEventListener("click", () => resume(draft));
    const drop = el("button", "btn btn--plain", "Начать заново");
    drop.style.cssText = "height:36px;padding:0;align-self:flex-start;font-size:12.5px";
    drop.addEventListener("click", async () => {
      prefs.set(draftKey, null);
      await collapse(plate);
    });
    plate.append(go, drop);
    content.children[0]?.after(plate);
  }

  /** Сохранить ход собеседования: после каждого вопроса и при выходе. */
  function saveDraft() {
    if (!questions.length || !startedAt || finished) return;
    const done = results.filter(closed);
    if (done.length >= questions.length) return prefs.set(draftKey, null);
    prefs.set(draftKey, {
      at: startedAt, scope, level, talk, aloud, questions,
      // Начатый, но не разобранный вопрос при продолжении задаётся заново.
      results: results.map((r) => (closed(r) ? r : null)),
    });
    // Разобранное уже видно в истории — не пропадёт, даже если не продолжать.
    if (done.length) remember({ at: startedAt, course: courseId, topic: scope, level, n: done.length, of: questions.length, score: average(results), partial: true });
  }

  function resume(draft) {
    ({ scope, level, talk, aloud, questions } = draft);
    startedAt = draft.at;
    finished = false;
    results = draft.results.map((r) => (closed(r) ? r : undefined));
    at = results.findIndex((r) => !closed(r));
    if (at < 0) at = questions.length;
    dock.replaceChildren();
    if (at >= questions.length) return report();
    if (talk) conversation();
    else ask();
  }

  /** Прошлые собеседования по курсу — смахнуть влево или удержать — удалить. */
  function paintHistory() {
    const mine = history().filter((item) => item.course === courseId);
    if (!mine.length) return;
    const block = el("div", "list interview-history");
    const head = label(`// прошлые собеседования · ${mine.length}`);
    block.append(head);
    let shown = Math.min(mine.length, 8);
    const forget = async (item, wrap) => {
      prefs.set("interviews", history().filter((one) => one.at !== item.at));
      await collapse(wrap);
      shown -= 1;
      const left = history().filter((one) => one.course === courseId).length;
      if (!left || !shown) collapse(block);
      else head.textContent = `// прошлые собеседования · ${left}`;
    };
    for (const item of mine.slice(-8).reverse()) {
      const row = el("div", "interview-history__row");
      const topic = item.topic ? topicCard(course, item.topic)?.title : "весь курс";
      row.append(
        el("span", `interview-history__score ${item.score >= PASS ? "is-pass" : ""}`, `${item.score}%`),
        el("span", "interview-history__text", `${new Date(item.at).toLocaleDateString("ru-RU")} · ${item.level} · ${topic ?? ""}${item.partial ? ` · ${item.n} из ${item.of}, не до конца` : ""}`),
      );
      const wrap = swipeRow(row, [{ text: "Удалить", icon: "trash", danger: true, run: () => forget(item, wrap) }]);
      pressMenu(row, () => [{ text: "Удалить из истории", icon: "trash", danger: true, run: () => forget(item, wrap) }]);
      block.append(wrap);
    }
    block.append(el("span", "small dim", "Смахните строку влево или удержите — удалить."));
    const clear = el("button", "btn btn--plain", "Очистить историю");
    clear.style.cssText = "height:36px;padding:0;align-self:flex-start;font-size:12.5px";
    clear.addEventListener("click", async () => {
      const sure = await confirmAction({ title: "Очистить историю собеседований?", text: "Баллы прошлых собеседований по этому курсу пропадут.", ok: "Очистить" });
      if (!sure) return;
      prefs.set("interviews", history().filter((one) => one.course !== courseId));
      collapse(block);
    });
    block.append(clear);
    // Над настройками: сразу видно, как менялся балл.
    content.children[0]?.after(block);
  }

  /* ── Вопросы ────────────────────────────────────────────────────────── */

  async function start() {
    prefs.set("interviewLevel", level);
    prefs.set("interviewCount", count);
    prefs.set("interviewAloud", aloud);
    prefs.set("interviewTalk", talk);
    dock.replaceChildren();
    content.replaceChildren(loading("Ноа готовит вопросы…"));
    try {
      const text = await material(course, scope);
      const rules =
        `Ты — репетитор и интервьюер на техническом собеседовании уровня ${level}. ` +
        `Составь ${count} вопросов по материалу ниже — такие, какие реально задают на собеседованиях на эту должность: на понимание, ` +
        "«чем отличается», «что будет, если», поиск неисправностей. Каждый вопрос — 1–2 коротких предложения про одну вещь, его легко " +
        "понять на слух. Кейсы — только из обычной рабочей практики (сервер, сеть, деплой, инцидент), без выдуманных сюжетов, " +
        "искусственных ограничений и цифр, которые надо держать в голове. Не проси пересказать определение. От простого к сложному, " +
        "разные темы вперемешку. Ответь только JSON-массивом: " +
        '[{"q": "вопрос", "topic": "id темы", "points": ["ключевой пункт хорошего ответа", "… 3–5 пунктов"]}]';
      const list = await coachJson(rules, text, { long: true });
      questions = (Array.isArray(list) ? list : list.questions ?? []).filter((q) => q?.q).slice(0, count);
      if (!questions.length) throw new Error("Вопросы не составились — попробуйте ещё раз.");
    } catch (err) {
      if (!alive) return;
      content.replaceChildren(el("span", "error", String(err)));
      const back = el("button", "btn btn--secondary btn--big", "Назад");
      back.addEventListener("click", setup);
      dock.replaceChildren(back);
      return;
    }
    if (!alive) return;
    results = [];
    at = 0;
    finished = false;
    startedAt = Date.now();
    saveDraft();
    if (talk) conversation();
    else ask();
  }

  function paintProgress() {
    meta.textContent = `${Math.min(at + 1, questions.length)} / ${questions.length}`;
    progressRow.replaceChildren(steps(questions.length, at, at, { ok: true, thin: true }));
  }

  /** Вопрос (и, если нужно, уточнение) — поле ответа, микрофон, «Не знаю». */
  function ask(followup = null) {
    voice.stop();
    paintProgress();
    const q = questions[at];
    const topic = topicCard(course, q.topic);
    const box = el("div", "plate plate--active");
    box.append(label(`// вопрос ${at + 1} из ${questions.length}${topic ? ` · ${topic.title}` : ""}`), el("span", "question", q.q));
    content.replaceChildren(box);
    if (followup) {
      const more = el("div", "explain");
      more.append(el("span", "explain__who", "Интервьюер уточняет"), el("span", "", followup));
      content.append(more);
    }
    const said = followup ?? q.q;
    if (aloud) voice.speak(said);

    const area = el("textarea", "answer-box");
    area.placeholder = "Ответ — голосом (кнопка с микрофоном) или текстом";
    content.append(area);
    content.scrollTop = 0;

    const mic = el("button", "square");
    mic.title = "Ответить голосом";
    mic.append(icon("mic", 22, 1.9));
    mic.addEventListener("click", async () => {
      voice.stop();
      if (mic.getAttribute("aria-pressed") === "true") {
        voice.cancel();
        return;
      }
      mic.setAttribute("aria-pressed", "true");
      try {
        // Ответ на собеседовании — с паузами на подумать: ждём подольше.
        const heard = await voice.listen({ pause: 2.8 });
        if (heard) area.value = `${area.value} ${heard}`.trim();
      } catch (err) {
        toast(String(err));
      }
      mic.setAttribute("aria-pressed", "false");
    });
    const skip = el("button", "btn btn--plain", "Не знаю");
    skip.style.cssText = "height:52px;padding:0 12px";
    skip.addEventListener("click", () => grade(q, followup, ""));
    const send = el("button", "btn btn--primary");
    send.style.cssText = "flex:1;height:52px";
    send.append("Ответить", icon("chevron-right", 18, 2.25));
    send.addEventListener("click", () => {
      const text = area.value.trim();
      if (!text) return toast("Сначала ответьте — голосом или текстом");
      grade(q, followup, text);
    });
    dock.replaceChildren(mic, skip, send);
  }

  /** Оценка ответа моделью: балл, что верно, чего не хватило, уточнение. */
  async function judge(q, result) {
    const rules =
      `Ты — репетитор и интервьюер на техническом собеседовании уровня ${level}. Оцени ответ кандидата строго, как на реальном ` +
      "собеседовании: суть и точность важнее объёма. Ответ часто надиктован голосом — разговорный, с распознанными с ошибкой терминами " +
      "(«MTV» вместо MTU): оценивай смысл, не формулировки. Ответь только JSON: " +
      '{"score": 0-100, "good": "что сказано верно, одной фразой", "missing": ["чего не хватило, кратко"], ' +
      '"followup": "один уточняющий вопрос, если ответ неполный, иначе пусто", "ideal": "образцовый ответ в 2–4 предложениях", ' +
      '"say": "короткая реакция интервьюера вслух, одна фраза"}';
    const dialog = result.answers.map((item) => `Интервьюер: ${item.asked}\nКандидат: ${item.said}`).join("\n");
    return coachJson(rules, `Вопрос: ${q.q}\nКлючевые пункты: ${(q.points ?? []).join("; ")}\n\n${dialog}`);
  }

  /* ── Разговором ─────────────────────────────────────────────────────
     Живой разговор с интервьюером-наставником. Ноа задаёт вопрос и
     слушает, не перебивая: паузы на подумать — не конец ответа. Ответ
     закончен, когда человек сам так скажет («вот как-то так», «я
     закончил»), нажмёт «Я закончил» или помолчит пару секунд после
     сказанного. Сначала можно переспросить. Суть сказана — вопрос закрыт;
     неполный ответ — не больше двух наводящих вопросов, потом коротко суть.
     Речь распознана с ошибками — судим по смыслу. «Повтори» — ещё раз, «пропустим» — суть коротко и
     дальше, «стоп» — итог. Кольцо — перебить Ноа и ответить. */

  const SKIP = Symbol("skip");
  const REPEAT = Symbol("repeat");
  const STOP = Symbol("stop");
  const DONE = Symbol("done");

  /** Без регистра, «ё» и знаков — для сравнения фраз. */
  const plain = (text) =>
    String(text ?? "")
      .toLowerCase()
      .replace(/ё/g, "е")
      .replace(/[^a-zа-я0-9]+/g, " ")
      .trim();

  /** Конец ответа словами — так заканчивают на живом собеседовании. */
  const ENDINGS = ["как то так", "вот так вот", "вот и все", "это все", "у меня все", "я все", "на этом все", "я закончил", "я закончила", "закончил", "закончила", "ответ готов", "вот мой ответ"];
  /** Начала коротких встречных вопросов — их отдаём сразу, конца ответа не ждём. */
  const ASKS = ["что значит", "что имеется в виду", "что ты имеешь в виду", "что вы имеете в виду", "в смысле", "уточни", "уточните", "можно уточнить", "не понял", "не поняла", "непонятно", "не понятно", "переформулируй", "поясни", "что такое", "можешь пояснить", "можете пояснить"];
  /** «Это всё?» — «да, всё». */
  const YES = /^(да|ага|угу|все|да все|закончил|закончила|да закончил|да закончила|это все|готово|да готово)$/;

  const ended = (text) => {
    const said = plain(text);
    return !/не закончила?$/.test(said) && ENDINGS.some((end) => said.endsWith(end));
  };
  const asking = (text) => {
    const said = plain(text);
    return said.split(" ").length <= 14 && ASKS.some((start) => said.startsWith(start) || said.startsWith(`а ${start}`));
  };

  function command(text) {
    const said = plain(text);
    if (/^(пропусти|пропустим|пропустить|следующий вопрос|давай дальше|дальше)$/.test(said)) return SKIP;
    if (/^(повтори|повторите|еще раз|повтори вопрос|повторите вопрос)$/.test(said)) return REPEAT;
    if (/^(стоп|хватит|закончим|закончим собеседование|достаточно|итог)$/.test(said)) return STOP;
    return null;
  }

  /** Сколько реплик человека по вопросу, после которых Ноа обязана его закрыть. */
  const MAX_TURNS = 3;

  /** Указания интервьюеру-наставнику на один вопрос; `turn` — какая это реплика человека по нему. */
  const mentorRules = (q, turn) =>
    `Ты — Ноа, интервьюер и наставник на тренировочном техническом собеседовании уровня ${level}` +
    `${course?.title ? ` по курсу «${course.title}»` : ""}. Ты задал вопрос: «${q.q}». ` +
    `Хороший ответ раскрывает: ${(q.points ?? []).join("; ") || "суть, пример, подводные камни"}.\n` +
    "Человек отвечает голосом, ты получаешь распознанную речь: разговорную, со словами-паразитами, иногда с матом и с терминами, " +
    "распознанными с ошибкой («MTV» — это MTU, «TCPIP FPP» — TCP/IP). Суди только по смыслу, к словам не придирайся.\n" +
    "Говоришь вслух, как живой интервьюер на созвоне: просто, на «ты», без канцелярита. За раз — 1–3 коротких предложения, " +
    "без списков и разметки. Не произноси баллы и проценты.\n" +
    "Как вести разговор:\n" +
    "— Суть сказана, пусть коротко и своими словами, — это верный ответ. Подтверди одной фразой, при желании добавь одну деталь, " +
    "которой не хватало, и закрой вопрос (done: true). Не переспрашивай то, что уже сказано, и не объясняй это заново.\n" +
    "— Ответ неполный — скажи, что верно, и задай ОДИН наводящий вопрос к самому важному пробелу. Не больше двух наводящих за вопрос.\n" +
    "— Не знает или просит пропустить — объясни суть в 2–3 простых предложениях и закрой вопрос.\n" +
    "— Переспрашивает вопрос — поясни, что имеешь в виду, без подсказки ответа (done: false).\n" +
    "— Раздражён или говорит, что уже ответил, — признай это одной фразой и закрой вопрос.\n" +
    "— Никаких выдуманных ситуаций, аналогий и «представь, что…»: примеры — только из реальной работы и только если человек не знает ответа.\n" +
    `Это ${turn}-я реплика человека по вопросу.` +
    (turn >= MAX_TURNS ? " Обязательно закрой вопрос сейчас (done: true): коротко скажи главное и всё." : "") +
    "\nПри done: true последняя фраза — короткий итог, что главное запомнить. К следующему вопросу сам не переходи.\n" +
    'Ответь только JSON: {"say": "что сказать вслух", "done": false, ' +
    '"score": 0–100 — как реальный интервьюер оценил бы понимание, которое человек показал САМ за весь разговор по вопросу ' +
    "(подсказанное тобой — вполовину). Балл должен совпадать с тем, что ты говоришь: «верно», «ровно», «отлично» — 75 и выше; " +
    'объяснил за человека — не выше 40, ' +
    '"gaps": ["чего не хватало в ответе, кратко"], "note": "при done — главное по вопросу одной фразой"}';

  /** Реплика наставника на сказанное; `thread` — разговор по этому вопросу. */
  async function mentor(q, thread, said, turn) {
    const rules = mentorRules(q, turn);
    let raw = await coach(rules, said, { thread });
    try {
      return { reply: parseJson(raw), raw };
    } catch {
      raw = await coach(`${rules}\nСТРОГО: только JSON, без слов вокруг.`, said, { thread });
      return { reply: parseJson(raw), raw };
    }
  }

  function conversation() {
    voice.stop();
    const orb = ring("look", 56);
    const orbBtn = el("button", "talk__orb");
    orbBtn.title = "Перебить и ответить";
    orbBtn.append(orb);
    const status = el("span", "talk__status grow", "Ноа готовится");
    const pause = el("button", "btn btn--plain talk__tool", "Пауза");
    const stop = el("button", "btn btn--plain talk__tool", "Итог");
    const stage = el("div", "talk__stage");
    stage.append(orbBtn, status, pause, stop);
    const hint = el(
      "span",
      "small dim talk__hint",
      "Отвечай своими словами. Замолчал на пару секунд — ответ ушёл Ноа; сразу — «вот как-то так» или «Я закончил».",
    );
    const feed = el("div", "list talk__feed");
    content.replaceChildren(stage, hint, feed);

    const typed = el("input", "ask");
    typed.placeholder = "Или напиши — Enter";
    typed.enterKeyHint = "send";
    const finishBtn = el("button", "btn btn--primary");
    finishBtn.style.cssText = "height:44px;padding:0 14px;white-space:nowrap";
    finishBtn.textContent = "Я закончил";
    finishBtn.hidden = true;
    dock.replaceChildren(typed, finishBtn);

    let paused = false;
    let resume = null;
    let stopped = false;
    /** Кусок речи, которого ждём: голос, набранное, кнопка — что раньше. */
    let waiting = null;
    let current = null;
    /** «Я закончил» нажато, пока микрофон слушал: дослушанный кусок — последний. */
    let wantDone = false;
    /** Пузырь со своей речью — заполняется по мере распознавания. */
    let live = null;
    let heardBefore = "";

    const mode = (name, text) => {
      orb.dataset.ring = name;
      status.textContent = text;
    };
    const line = (who, text, extra = "") => {
      const node = el("div", who === "me" ? "say say--me" : who === "bad" ? "say say--bad" : who === "good" ? "say say--good" : "say");
      node.textContent = text;
      if (extra) node.append(el("span", "talk__score", extra));
      feed.append(node);
      content.scrollTop = content.scrollHeight;
      return node;
    };
    const speak = async (text) => {
      if (!text || !alive || stopped) return;
      mode("speak", "Ноа говорит · кольцо — перебить");
      // Чуть медленнее обычного: на слух разбор должен успевать доходить.
      await voice.speak(text, { pace: 0.92 });
    };
    const finish = (value) => {
      const done = waiting;
      waiting = null;
      done?.(value);
    };

    // Слова появляются в пузыре по мере речи — видно, что Ноа слышит.
    globalThis.__TAURI__?.core
      ?.addPluginListener?.("sufler", "hearing", (event) => {
        if (!live || !alive) return;
        const now = String(event?.text ?? "").trim();
        live.textContent = [heardBefore, now].filter(Boolean).join(" ") || "…";
        content.scrollTop = content.scrollHeight;
      })
      ?.then?.((handle) => (unhear = () => handle?.unregister?.()?.catch?.(() => {})))
      ?.catch?.(() => {});

    orbBtn.addEventListener("click", () => {
      if (orb.dataset.ring === "speak") voice.stop();
    });
    typed.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" || !typed.value.trim() || !waiting) return;
      const text = typed.value.trim();
      typed.value = "";
      typed.blur();
      current = null;
      voice.cancel();
      finish({ typed: text });
    });
    finishBtn.addEventListener("click", () => {
      if (!waiting) return;
      wantDone = true;
      // Микрофон отдаёт то, что успел расслышать; не ответил — просто «закончил».
      api?.invoke("plugin:sufler|stopListening").catch(() => {});
      setTimeout(() => waiting && wantDone && ((current = null), finish(DONE)), 1500);
    });
    pause.addEventListener("click", () => {
      paused = !paused;
      pause.textContent = paused ? "Дальше" : "Пауза";
      if (paused) {
        voice.stop();
        voice.cancel();
        mode("look", "Пауза");
      } else resume?.();
    });
    stop.addEventListener("click", () => {
      stopped = true;
      voice.stop();
      voice.cancel();
      finish(STOP);
    });

    /** Кусок речи до паузы на подумать: текст, набранное, «Я закончил» или STOP. */
    const segment = (idle) => {
      const answer = new Promise((done) => (waiting = done));
      // Свой билет у каждого слушания: запоздалый ответ прошлого не
      // засчитывается в этом.
      const ticket = {};
      current = ticket;
      voice
        .listen({ pause: 3.5, idle, limit: 120, ends: ENDINGS, asks: ASKS })
        .then((text) => current === ticket && finish(text))
        .catch(() => current === ticket && finish(""));
      return answer;
    };

    /** Реплика человека целиком: куски речи копятся, пока он не закончит. */
    const hearTurn = async () => {
      const parts = [];
      let quiet = 0;
      let fails = 0;
      wantDone = false;
      const bubble = () => {
        if (!live) live = line("me", "");
        live.classList.add("is-live");
        live.textContent = parts.join(" ") || "…";
        content.scrollTop = content.scrollHeight;
      };
      const done = (value) => {
        finishBtn.hidden = true;
        if (live) {
          if (parts.length) live.classList.remove("is-live");
          else live.remove();
        }
        live = null;
        heardBefore = "";
        return value;
      };
      while (alive && !stopped) {
        if (paused) await new Promise((go) => (resume = go));
        if (!alive || stopped) break;
        mode("listen", parts.length ? "Слушаю…" : "Слушаю — отвечай или переспроси");
        finishBtn.hidden = false;
        heardBefore = parts.join(" ");
        bubble();
        const from = Date.now();
        // Уже что-то сказал — ждём продолжения недолго: молчит — значит, всё.
        const got = await segment(parts.length ? 4 : 10);
        if (got === STOP || stopped) return done(STOP);
        if (paused) continue;
        if (got === DONE) {
          wantDone = false;
          if (parts.length) return done(parts.join(" "));
          continue;
        }
        if (got && typeof got === "object") {
          parts.push(got.typed);
          bubble();
          const cmd = parts.length === 1 ? command(got.typed) : null;
          if (cmd) parts.length = 0;
          return done(cmd ?? parts.join(" "));
        }
        const text = String(got ?? "").trim();
        if (text) {
          quiet = 0;
          fails = 0;
          if (parts.length && YES.test(plain(text))) return done(parts.join(" "));
          parts.push(text);
          bubble();
          const cmd = parts.length === 1 ? command(text) : null;
          if (cmd) {
            parts.length = 0;
            return done(cmd);
          }
          if (wantDone || ended(parts.join(" ")) || (parts.length === 1 && asking(text))) return done(parts.join(" "));
          // Пауза на подумать — слушаем дальше.
          continue;
        }
        if (wantDone) {
          wantDone = false;
          if (parts.length) return done(parts.join(" "));
          continue;
        }
        // Микрофон отказал сразу (нет доступа) — не крутимся впустую.
        if (Date.now() - from < 1500) {
          fails += 1;
          if (fails >= 3) {
            mode("look", "Микрофон не отвечает — напиши ответ внизу");
            const got2 = await new Promise((go) => (waiting = go));
            if (got2 === STOP) return done(STOP);
            if (got2 && typeof got2 === "object") {
              parts.push(got2.typed);
              return done(command(got2.typed) ?? parts.join(" "));
            }
            if (got2 === DONE && parts.length) return done(parts.join(" "));
            continue;
          }
          await new Promise((go) => setTimeout(go, 1500));
          continue;
        }
        quiet += 1;
        if (parts.length) return done(parts.join(" "));
        if (quiet === 2) {
          await speak("Не тороплю. Можно подумать вслух или переспросить.");
        }
      }
      return done(STOP);
    };

    (async () => {
      for (; at < questions.length && alive && !stopped; at += 1) {
        paintProgress();
        const q = questions[at];
        const result = { q: q.q, topic: q.topic, answers: [], gaps: [] };
        results[at] = result;
        const thread = [];
        line("noa", q.q);
        await speak(`${at === 0 ? "Начнём." : "Следующий вопрос."} ${q.q}`);
        let last = q.q;
        while (alive && !stopped) {
          let said = await hearTurn();
          if (said === STOP || !alive || stopped) break;
          if (said === REPEAT) {
            await speak(last);
            continue;
          }
          if (said === SKIP) {
            line("me", "Давай пропустим");
            said = "Давай пропустим этот вопрос.";
          }
          result.answers.push({ asked: last, said });
          mode("look", "Ноа думает…");
          let reply;
          try {
            const turn = await mentor(q, thread, said, result.answers.length);
            reply = turn.reply;
            thread.push({ q: said, a: turn.raw });
          } catch (err) {
            if (!alive) break;
            line("bad", `${String(err)} Скажи ещё раз — или «Итог».`);
            continue;
          }
          if (!alive) break;
          const score = Number(reply.score);
          if (reply.score != null && Number.isFinite(score)) result.score = Math.max(0, Math.min(100, Math.round(score)));
          for (const gap of reply.gaps ?? []) if (gap && !result.gaps.includes(gap)) result.gaps.push(gap);
          const say = String(reply.say ?? "").trim() || (reply.done ? "Хорошо, с этим разобрались." : "Расскажи чуть подробнее?");
          line("noa", say);
          await speak(say);
          last = say;
          // Разбор затянулся — закрываем, чтобы не ходить по кругу.
          if (reply.done || result.answers.length >= MAX_TURNS + 1) {
            result.note = String(reply.note ?? "").trim();
            if (result.score == null) result.score = 0;
            result.verdict = { good: "", missing: result.gaps, ideal: result.note };
            line(result.score >= PASS ? "good" : "bad", result.note ? `Главное: ${result.note}` : "Вопрос разобрали.", `${result.score}%`);
            saveDraft();
            break;
          }
        }
      }
      if (alive) report();
    })();
  }

  /** Оценка ответа; на неполный — одно уточнение, как на живом собеседовании. */
  async function grade(q, followup, answer) {
    voice.stop();
    voice.cancel();
    dock.replaceChildren();
    const result = results[at] ?? { q: q.q, topic: q.topic, answers: [] };
    result.answers.push({ asked: followup ?? q.q, said: answer || "(не знаю)" });
    results[at] = result;
    content.append(loading("Ноа оценивает ответ…"));
    let verdict;
    try {
      verdict = await judge(q, result);
    } catch (err) {
      if (!alive) return;
      content.lastChild?.remove();
      content.append(el("span", "error", String(err)));
      const retry = el("button", "btn btn--primary btn--big", "Оценить ещё раз");
      retry.addEventListener("click", () => {
        result.answers.pop();
        grade(q, followup, answer);
      });
      dock.replaceChildren(retry);
      return;
    }
    if (!alive) return;
    const score = Math.max(0, Math.min(100, Math.round(Number(verdict.score) || 0)));
    result.score = score;
    result.verdict = verdict;
    // Неполный ответ — одно уточнение, дальше — следующий вопрос.
    if (!followup && answer && score < PASS && String(verdict.followup ?? "").trim()) {
      if (aloud && verdict.say) await voice.speak(verdict.say);
      if (!alive) return;
      return ask(String(verdict.followup).trim());
    }
    feedback(result);
  }

  function feedback(result) {
    saveDraft();
    const v = result.verdict ?? {};
    content.replaceChildren();
    const head = el("div", `plate interview__score ${result.score >= PASS ? "is-pass" : "is-weak"}`);
    head.append(label(result.score >= PASS ? "// засчитано" : "// слабо"), el("span", "score", `${result.score}%`));
    if (v.good) head.append(el("span", "muted", v.good));
    content.append(head);
    const missing = (v.missing ?? []).filter(Boolean);
    if (missing.length) {
      const box = el("div", "warn");
      box.append(label("Не хватило"));
      const list = el("ul", "md");
      for (const item of missing) list.append(el("li", "", item));
      box.append(list);
      content.append(box);
    }
    if (v.ideal) {
      const ideal = el("div", "plate");
      ideal.append(label("// как ответил бы сильный кандидат"), markdown(v.ideal, "md md--small"));
      content.append(ideal);
    }
    content.scrollTop = 0;
    if (aloud && v.say) voice.speak(v.say);
    const nextBtn = el("button", "btn btn--primary btn--big");
    const last = at + 1 >= questions.length;
    nextBtn.append(last ? "Итог собеседования" : "Следующий вопрос", icon("chevron-right", 18, 2.25));
    nextBtn.addEventListener("click", () => {
      if (last) return report();
      at += 1;
      ask();
    });
    dock.replaceChildren(nextBtn);
  }

  /* ── Итог ───────────────────────────────────────────────────────────── */

  function report() {
    voice.stop();
    at = questions.length;
    progressRow.replaceChildren(steps(questions.length, questions.length, -1, { ok: true, thin: true }));
    meta.textContent = "итог";
    finished = true;
    const scored = results.filter(closed);
    const total = average(results);
    prefs.set(draftKey, null);
    if (scored.length) remember({ at: startedAt || Date.now(), course: courseId, topic: scope, level, n: scored.length, of: questions.length, score: total });

    content.replaceChildren();
    const head = el("div", "plate plate--active");
    head.append(label(`// итог · ${level}`), el("span", "score", `${total}%`));
    head.append(
      el(
        "span",
        "continue__lead",
        total >= 80
          ? "Уверенно: на таком собеседовании вы проходите дальше."
          : total >= PASS
            ? "На грани: базу знаете, но интервьюер заметит пробелы — их ниже."
            : "Пока рано: пробелы заметны. Выучите слабые темы наизусть и пройдите ещё раз.",
      ),
    );
    content.append(head);
    const rows = el("div", "rows");
    scored.forEach((r, i) => {
      const row = el("button", `row${r.score >= PASS ? " is-done" : ""}`);
      const text = el("span", "row__body");
      text.append(el("span", "row__title", `${i + 1}. ${r.q}`));
      row.append(el("span", "row__mark", r.score >= PASS ? "✓" : "!"), text, el("span", "row__pct", `${r.score}%`));
      row.addEventListener("click", () => {
        at = i;
        feedback(r);
        const back = el("button", "btn btn--secondary btn--big", "К итогу");
        back.addEventListener("click", report);
        dock.replaceChildren(back);
      });
      rows.append(row);
    });
    content.append(rows);

    // Слабые темы — сразу в «Наизусть».
    const weak = [...new Set(scored.filter((r) => r.score < PASS && r.topic).map((r) => r.topic))]
      .map((id) => topicCard(course, id))
      .filter(Boolean);
    if (weak.length) {
      content.append(label("// выучить наизусть"));
      for (const topic of weak.slice(0, 3)) {
        const btn = el("button", "setting");
        const text = el("span", "setting__text");
        text.append(el("span", "setting__name", topic.title), el("span", "setting__hint", "Понятия темы — до автоматизма"));
        btn.append(icon("flame", 20, 2), text, icon("chevron-right", 18, 2));
        btn.firstChild.style.color = "var(--c-label)";
        btn.addEventListener("click", () => nav.replace("drill", { course: courseId, topic: topic.id }));
        content.append(btn);
      }
    }
    const again = el("button", "btn btn--secondary", "Ещё раз");
    again.style.cssText = "flex:1;height:52px";
    again.addEventListener("click", setup);
    const done = el("button", "btn btn--primary", "Готово");
    done.style.cssText = "flex:1.3;height:52px";
    done.addEventListener("click", () => nav.back());
    dock.replaceChildren(again, done);
  }

  if (!course) {
    content.append(el("span", "error", "Курс не найден."));
  } else setup();
  return {
    full: true,
    cleanup: () => {
      // Вышли посреди собеседования — разобранное и место, где остановились, сохраняются.
      if (at < questions.length) saveDraft();
      alive = false;
      voice.stop();
      voice.cancel();
      unhear?.();
    },
  };
});
