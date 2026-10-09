// «Собеседование» — мок-интервью по курсу или теме. Ноа — интервьюер:
// задаёт вопросы вслух (на понимание, сравнение, кейсы, траблшутинг, а не
// пересказ определений), слушает ответ, на неполный — один уточняющий
// вопрос, как на живом собеседовании. После каждого — балл, что сказано
// верно, чего не хватило, образцовый ответ. В конце — итог и что повторить.

import { el, icon, label, loading, nav, register, iconButton, call, voice, toast, prefs, steps, markdown } from "../core.js";
import { store, topicCard } from "../store.js";
import { coachJson } from "../coach.js";

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

export function lastInterview(courseId) {
  return history().filter((item) => item.course === courseId).at(-1) ?? null;
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
    const last = lastInterview(courseId);
    if (last) box.append(el("span", "small dim", `Прошлый раз: ${last.score}% · ${last.n} вопросов · ${new Date(last.at).toLocaleDateString("ru-RU")}`));
    content.replaceChildren(box);

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

  /* ── Вопросы ────────────────────────────────────────────────────────── */

  async function start() {
    prefs.set("interviewLevel", level);
    prefs.set("interviewCount", count);
    prefs.set("interviewAloud", aloud);
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
    ask();
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
      const rules =
        `Ты — репетитор и интервьюер на техническом собеседовании уровня ${level}. Оцени ответ кандидата строго, как на реальном ` +
        "собеседовании: суть и точность важнее объёма. Ответь только JSON: " +
        '{"score": 0-100, "good": "что сказано верно, одной фразой", "missing": ["чего не хватило, кратко"], ' +
        '"followup": "один уточняющий вопрос, если ответ неполный, иначе пусто", "ideal": "образцовый ответ в 2–4 предложениях", ' +
        '"say": "короткая реакция интервьюера вслух, одна фраза"}';
      const dialog = result.answers.map((item) => `Интервьюер: ${item.asked}\nКандидат: ${item.said}`).join("\n");
      verdict = await coachJson(rules, `Вопрос: ${q.q}\nКлючевые пункты: ${(q.points ?? []).join("; ")}\n\n${dialog}`);
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
    const scored = results.filter(Boolean);
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
