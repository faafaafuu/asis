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
    const saved = savedSession();
    if (saved) content.append(resumeCard(saved));
    paintHistory();

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
        "Разговором — как на созвоне с наставником: Ноа спрашивает и дослушивает (паузы на подумать можно), переспросить тоже можно. Неполный ответ разбираете вместе по шагам, пока не станет понятно. Закончил отвечать — «вот как-то так». «Повтори», «пропустим», «стоп» — голосом.",
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
        el("span", "interview-history__text", `${new Date(item.at).toLocaleDateString("ru-RU")} · ${item.level} · ${topic ?? ""}`),
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
        `Ты — интервьюер на реальном техническом собеседовании уровня ${level}. ` +
        `Составь ${count} вопросов по материалу ниже — ровно такие, какие на самом деле задают на собеседованиях на эту работу: ` +
        "на понимание, «чем отличается», «что будет, если», «как бы ты это сделал/починил на работе». " +
        "Вопрос — 1–2 коротких предложения, понятных с первого раза, как спросил бы живой человек. " +
        "Только жизненные ситуации, с которыми инженер реально сталкивается. Никаких выдуманных условий: без конкретных IP-адресов, " +
        "номеров портов и AS, надуманных схем, загадок и подвохов. Не проси пересказать определение. От простого к сложному, " +
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
    saveSession(0);
    if (talk) conversation();
    else ask();
  }

  /* ── Незаконченное собеседование ─────────────────────────────────────
     Каждый разобранный вопрос сохраняется: вышли посреди — при следующем
     заходе «Продолжить» с того же места, а не заново. */

  const SESSION = "interviewSession";
  const saveSession = (next) =>
    prefs.set(SESSION, { course: courseId, scope, level, talk, questions, results, at: next, when: Date.now() });
  const savedSession = () => {
    const saved = prefs.get(SESSION);
    return saved?.course === courseId && saved.questions?.length && saved.at < saved.questions.length ? saved : null;
  };

  function resumeCard(saved) {
    const box = el("div", "plate plate--active");
    const done = (saved.results ?? []).filter((r) => r?.score != null).length;
    box.append(
      label("// незаконченное собеседование"),
      el("span", "continue__title", `Вопрос ${saved.at + 1} из ${saved.questions.length}`),
      el("span", "continue__lead", `Разобрано ${done} · ${saved.level} · ${new Date(saved.when).toLocaleDateString("ru-RU")}`),
    );
    const row = el("div", "continue__foot");
    const fresh = el("button", "btn btn--plain", "Заново");
    fresh.addEventListener("click", () => {
      prefs.set(SESSION, null);
      setup();
    });
    const go = el("button", "btn btn--primary");
    go.append(icon("play", 16, 2.5), "Продолжить");
    go.addEventListener("click", () => {
      ({ questions, results, at, level, talk } = saved);
      scope = saved.scope ?? null;
      results = results ?? [];
      if (talk) conversation();
      else ask();
    });
    row.append(fresh, go);
    box.append(row);
    return box;
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
      "собеседовании: суть и точность важнее объёма. Ответь только JSON: " +
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
     закончил») или нажмёт «Я закончил»; замолчал надолго — Ноа мягко
     спросит, всё ли. Сначала можно переспросить. Неполный ответ — не
     «неправильно, дальше», а разбор по шагу: наводящий вопрос, маленькое
     объяснение, «скажи своими словами». Вопрос закрыт, когда суть понята, —
     тогда следующий. «Повтори» — ещё раз, «пропустим» — суть коротко и
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

  /** Указания интервьюеру-наставнику на один вопрос. */
  const mentorRules = (q) =>
    `Ты — Ноа, интервьюер и наставник на тренировочном техническом собеседовании уровня ${level}` +
    `${course?.title ? ` по курсу «${course.title}»` : ""}. Ты задал вопрос: «${q.q}». ` +
    `Хороший ответ раскрывает: ${(q.points ?? []).join("; ") || "суть, пример, подводные камни"}.\n` +
    "Цель — чтобы человек понял, запомнил и прошёл настоящее собеседование. Говоришь вслух, как живой человек на созвоне: " +
    "просто, тепло, на «ты». СТРОГО: не больше 2 коротких предложений (до 35 слов) и не больше одного вопроса за раз. " +
    "Без списков, разметки и лекций.\n" +
    "Человек отвечает голосом: речь распознаётся с ошибками (MTV вместо MTU, слова слитно), говорит просто, коряво, разговорно. " +
    "Засчитывай СУТЬ, сказанную своими словами, в любой момент разговора; не придирайся к терминам и формулировкам.\n" +
    "Как вести разговор:\n" +
    "— Переспрашивает — поясни вопрос одной фразой, ответ не подсказывай.\n" +
    "— Суть сказана (даже простыми словами) — подтверди, назови это правильным термином, если его не было, и закрой вопрос. " +
    "Не объясняй заново то, что человек уже сказал, и не придумывай примеров.\n" +
    "— Суть сказана частично — скажи, что верно, и задай ОДИН наводящий вопрос к главному пробелу.\n" +
    "— Не знает или запутался — одна мысль простыми словами на обычном рабочем примере, потом спроси, понятно ли.\n" +
    "— После двух наводящих вопросов суть так и не прозвучала — коротко скажи её сам и закрой.\n" +
    "— Просит пропустить — суть в двух предложениях и закрой.\n" +
    "— Раздражён или ругается — не извиняйся длинно: коротко согласись и двигайся дальше.\n" +
    "Никаких выдуманных ситуаций, IP-адресов, номеров портов и абсурдных примеров. Баллы вслух не называй. " +
    "К следующему вопросу сам не переходи — это сделает приложение.\n" +
    'Ответь только JSON: {"say": "что сказать вслух", "done": false, ' +
    '"score": при done — 0–100, насколько человек САМ показал понимание сути за весь разговор ' +
    "(85–100 — сказал суть сразу, 60–84 — с подсказкой, ниже — суть пришлось говорить за него); иначе null, " +
    '"gaps": ["чего ему не хватило, кратко"], "note": "при done — главное по вопросу одной фразой"}';

  /** Реплика наставника на сказанное; `thread` — разговор по этому вопросу. */
  async function mentor(q, thread, said) {
    const rules = mentorRules(q);
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
      "Отвечай спокойно — короткие паузы не страшны, можно сначала переспросить. Закончил — скажи «вот как-то так», нажми «Я закончил» или просто помолчи несколько секунд.",
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
      ?.then?.((handle) => (unhear = () => Promise.resolve(handle?.unregister?.()).catch(() => {})))
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
        // Уже что-то сказал — после паузы ждём продолжения недолго; ещё молчит — дольше.
        const got = await segment(parts.length ? 3.5 : 12);
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
        // Сказал и замолчал — ответ принят (паузы на подумать уже позади).
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
            const turn = await mentor(q, thread, said);
            reply = turn.reply;
            // В памяти разговора — только сказанное: короче запрос, быстрее ответ.
            thread.push({ q: said, a: JSON.stringify({ say: reply.say ?? "", done: Boolean(reply.done) }) });
            if (thread.length > 8) thread.shift();
          } catch (err) {
            if (!alive) break;
            line("bad", `${String(err)} Скажи ещё раз — или «Итог».`);
            continue;
          }
          if (!alive) break;
          // Балл — итог по вопросу: ставится при закрытии (раньше — запасной).
          const score = Number(reply.score);
          if (reply.score != null && Number.isFinite(score) && (reply.done || result.score == null)) result.score = Math.max(0, Math.min(100, Math.round(score)));
          for (const gap of reply.gaps ?? []) if (gap && !result.gaps.includes(gap)) result.gaps.push(gap);
          const say = String(reply.say ?? "").trim() || (reply.done ? "Хорошо, с этим разобрались." : "Расскажи чуть подробнее?");
          line("noa", say);
          await speak(say);
          last = say;
          // Разбор затянулся — закрываем, чтобы не ходить по кругу.
          if (reply.done || result.answers.length >= 8) {
            result.note = String(reply.note ?? "").trim();
            if (result.score == null) result.score = 0;
            result.verdict = { good: "", missing: result.gaps, ideal: result.note };
            line(result.score >= PASS ? "good" : "bad", result.note ? `Главное: ${result.note}` : "Вопрос разобрали.", `${result.score}%`);
            if (at + 1 < questions.length) saveSession(at + 1);
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
    if (at + 1 < questions.length) saveSession(at + 1);
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
    const scored = results.filter((r) => r && r.score != null);
    const total = scored.length ? Math.round(scored.reduce((sum, r) => sum + (r.score ?? 0), 0) / scored.length) : 0;
    prefs.set(SESSION, null);
    const all = history();
    if (scored.length) all.push({ at: Date.now(), course: courseId, topic: scope, level, n: scored.length, score: total });
    prefs.set("interviews", all.slice(-50));

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
      alive = false;
      voice.stop();
      voice.cancel();
      unhear?.();
    },
  };
});
