// «Собеседование» — мок-интервью по курсу или теме. Ноа — интервьюер:
// задаёт вопросы вслух (на понимание, сравнение, кейсы, траблшутинг, а не
// пересказ определений), слушает ответ, на неполный — один уточняющий
// вопрос, как на живом собеседовании. После каждого — балл, что сказано
// верно, чего не хватило, образцовый ответ. В конце — итог и что повторить.

import { el, icon, label, loading, nav, register, iconButton, call, voice, toast, prefs, steps, markdown, ring } from "../core.js";
import { store, topicCard } from "../store.js";
import { coachJson } from "../coach.js";
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
      el("span", "small dim", "Разговором — как на звонке: Ноа спрашивает, слушает, уточняет и идёт дальше сама. «Дальше» — пропустить, «повтори» — ещё раз, «стоп» — итог."),
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
        `Ты — репетитор и интервьюер на техническом собеседовании уровня ${level}. ` +
        `Составь ${count} вопросов по материалу ниже — такие, какие задают живые интервьюеры: на понимание, «чем отличается», ` +
        "«что будет, если», практические кейсы и поиск неисправностей. Не проси пересказать определение. От простого к сложному, " +
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
      "собеседовании: суть и точность важнее объёма. Ответь только JSON: " +
      '{"score": 0-100, "good": "что сказано верно, одной фразой", "missing": ["чего не хватило, кратко"], ' +
      '"followup": "один уточняющий вопрос, если ответ неполный, иначе пусто", "ideal": "образцовый ответ в 2–4 предложениях", ' +
      '"say": "короткая реакция интервьюера вслух, одна фраза"}';
    const dialog = result.answers.map((item) => `Интервьюер: ${item.asked}\nКандидат: ${item.said}`).join("\n");
    return coachJson(rules, `Вопрос: ${q.q}\nКлючевые пункты: ${(q.points ?? []).join("; ")}\n\n${dialog}`);
  }

  /* ── Разговором ─────────────────────────────────────────────────────
     Как на звонке: Ноа задаёт вопрос голосом и сама слушает ответ — конец
     ответа по паузе; неполный — уточняет; коротко говорит, засчитан ли, и
     сама переходит к следующему. Голосом же: «дальше» / «не знаю» —
     пропустить, «повтори» — ещё раз вопрос, «стоп» — к итогу. Можно и
     написать — поле внизу. */

  const SKIP = Symbol("skip");
  const REPEAT = Symbol("repeat");
  const STOP = Symbol("stop");

  function command(text) {
    const said = String(text).trim().toLowerCase().replace(/[.!?,]/g, "");
    if (/^(дальше|пропусти|пропустить|следующий|не знаю|я не знаю|без понятия)$/.test(said)) return SKIP;
    if (/^(повтори|повторите|ещё раз|еще раз|повтори вопрос)$/.test(said)) return REPEAT;
    if (/^(стоп|хватит|закончим|закончить|достаточно|итог)$/.test(said)) return STOP;
    return null;
  }

  function conversation() {
    voice.stop();
    const orb = ring("look", 56);
    const status = el("span", "talk__status", "Ноа готовится");
    const stage = el("div", "talk__stage");
    stage.append(orb, status);
    const feed = el("div", "list talk__feed");
    content.replaceChildren(stage, feed);

    const typed = el("input", "ask");
    typed.placeholder = "Или напишите ответ — Enter";
    typed.enterKeyHint = "send";
    const pause = el("button", "btn btn--secondary");
    pause.style.cssText = "height:44px;padding:0 12px";
    pause.textContent = "Пауза";
    const stop = el("button", "btn btn--plain", "Итог");
    stop.style.cssText = "height:44px;padding:0 10px";
    dock.replaceChildren(typed, pause, stop);

    let paused = false;
    let resume = null;
    /** Ответ, которого ждём: голос или поле ввода — что раньше. */
    let waiting = null;
    let current = null;
    let stopped = false;

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
    };
    const speak = async (text) => {
      if (!text || !alive || stopped) return;
      mode("speak", "Ноа говорит");
      await voice.speak(text);
    };
    const finish = (value) => {
      const done = waiting;
      waiting = null;
      done?.(value);
    };

    typed.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" || !typed.value.trim() || !waiting) return;
      const text = typed.value.trim();
      typed.value = "";
      typed.blur();
      voice.cancel();
      finish(command(text) ?? text);
    });
    pause.addEventListener("click", () => {
      paused = !paused;
      pause.textContent = paused ? "Продолжить" : "Пауза";
      if (paused) {
        voice.stop();
        voice.cancel();
        mode("look", "Пауза — нажмите «Продолжить»");
      } else resume?.();
    });
    stop.addEventListener("click", () => {
      stopped = true;
      voice.stop();
      voice.cancel();
      finish(STOP);
    });

    /** Слушать ответ: текст, команда или STOP. Тишина — ещё раз, третий раз — «дальше». */
    const hear = async () => {
      for (let quiet = 0; alive && !stopped; ) {
        if (paused) await new Promise((done) => (resume = done));
        if (!alive || stopped) return STOP;
        mode("listen", "Слушаю — отвечайте");
        const answer = new Promise((done) => (waiting = done));
        // Свой билет у каждого слушания: запоздалый ответ прошлого не
        // засчитывается тишиной в этом.
        const ticket = {};
        current = ticket;
        voice
          .listen({ pause: 2.8 })
          .then((text) => current === ticket && finish(text))
          .catch(() => current === ticket && finish(""));
        const got = await answer;
        if (got === STOP || stopped) return STOP;
        if (paused) continue;
        if (typeof got === "symbol") return got;
        const text = String(got ?? "").trim();
        if (text) return command(text) ?? text;
        quiet += 1;
        if (quiet >= 3) return SKIP;
        await speak("Не расслышала. Повторите — или скажите «дальше».");
      }
      return STOP;
    };

    (async () => {
      for (; at < questions.length && alive && !stopped; at += 1) {
        paintProgress();
        const q = questions[at];
        const result = { q: q.q, topic: q.topic, answers: [] };
        results[at] = result;
        line("noa", q.q);
        await speak(q.q);
        let asked = q.q;
        for (let round = 0; round < 2 && alive && !stopped; round += 1) {
          const said = await hear();
          if (said === STOP || !alive) break;
          if (said === REPEAT) {
            await speak(asked);
            round -= 1;
            continue;
          }
          const answer = said === SKIP ? "" : said;
          line("me", answer || "Не знаю — дальше");
          result.answers.push({ asked, said: answer || "(не знаю)" });
          mode("look", "Ноа оценивает ответ…");
          let verdict = null;
          try {
            verdict = await judge(q, result);
          } catch (err) {
            line("bad", String(err));
          }
          if (!verdict || !alive) break;
          result.verdict = verdict;
          result.score = Math.max(0, Math.min(100, Math.round(Number(verdict.score) || 0)));
          // Неполный ответ — одно уточнение, как на живом собеседовании.
          const followup = String(verdict.followup ?? "").trim();
          if (round === 0 && answer && result.score < PASS && followup) {
            asked = followup;
            line("noa", `${verdict.say ? `${verdict.say} ` : ""}${followup}`);
            await speak(`${verdict.say ?? ""} ${followup}`);
            continue;
          }
          const missing = (verdict.missing ?? []).filter(Boolean).slice(0, 2).join(", ");
          const spoken = `${verdict.say ?? ""}${result.score < PASS && missing ? ` Не хватило: ${missing}.` : ""}`.trim();
          line(result.score >= PASS ? "good" : "bad", spoken || (result.score >= PASS ? "Засчитано." : "Слабо."), `${result.score}%`);
          await speak(spoken);
          break;
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
    const all = history();
    all.push({ at: Date.now(), course: courseId, topic: scope, level, n: scored.length, score: total });
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
    },
  };
});
