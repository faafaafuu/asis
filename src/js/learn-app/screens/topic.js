// 03 Тема — заголовок и путь темы из пяти шагов: ролик, урок, аудиодрама,
// собеседование, команды для практики. Остальные режимы — в «Ещё».
// Закреплённая кнопка — следующий шаг, а пройдена тема — следующая тема.

import { el, icon, label, loading, header, nav, register, iconButton, plural, sections, markdown, isWeb, sheet, prefs, api } from "../core.js";
import { store, topicCard } from "../store.js";
import { dramaBox } from "../../drama-panel.js";
import { videoBox } from "../../video-panel.js";
import { commandsBox } from "../../lesson-commands.js";

const MODES = [
  { id: "lesson", name: "Урок с Ноа", icon: "chat", about: "Ноа ведёт урок по разделам: читает вслух, разбирает подробно и обсуждает с вами то, что непонятно." },
  { id: "practice", name: "Практика", icon: "window", about: "Живая задача на вашем сервере. Вы работаете в терминале — Ноа его видит, объясняет, замечает ошибки и подсказывает следующий шаг." },
  { id: "material", name: "Материал", icon: "learn", about: "Понятия темы: определение, зацепка для памяти, аналогия, пример и частая ошибка." },
  { id: "cards", name: "Карточки", icon: "cards", about: "Карточки по расписанию: вспомнили — вернутся через дни, забыли — сегодня же." },
  { id: "sheet", name: "Конспект", icon: "note", about: "Вся тема на одну страницу — для быстрого повторения перед практикой или собеседованием." },
  { id: "check", name: "Проверить себя", icon: "target", about: "Мини-экзамен по теме: варианты и ответы своими словами. Ноа разбирает каждый ответ." },
  { id: "drill", name: "Наизусть", icon: "flame", about: "Понятия темы до автоматизма: запомнить, вспомнить пропуски, рассказать по памяти — пока не вспомните дважды подряд." },
  { id: "interview", name: "Собеседование", icon: "user", about: "Мок-интервью: Ноа задаёт вопросы вслух, слушает ответ, уточняет и разбирает, чего не хватило." },
];

register("topic", (screen, { course: courseId, topic: topicId }) => {
  let course = store.course(courseId);
  let card = topicCard(course, topicId);
  if (!course || !card) {
    nav.back();
    return {};
  }
  const index = course.topics.indexOf(card) + 1;
  screen.append(header({ title: `${course.title} · тема ${index} из ${course.topics.length}`, right: iconButton("more", "Ещё", () => nav.push("topics-more", { course: courseId })) }));
  const content = el("div", "content");
  content.append(loading());
  const sticky = el("div", "sticky");
  screen.append(content, sticky);

  let alive = true;
  (async () => {
    let view;
    try {
      // Прогресс свежий: урок или проверка только что могли его поменять.
      [view] = await Promise.all([store.topic(courseId, topicId), store.practice(true), store.courses(true)]);
      course = store.course(courseId) ?? course;
      card = topicCard(course, topicId) ?? card;
    } catch (err) {
      content.replaceChildren(el("span", "error", String(err)));
      return;
    }
    if (!alive) return;
    const topic = view.topic;
    const own = store.practiceOf(courseId, topicId);
    const parts = sections(topic.lesson);
    const drilled = prefs.get("drilled", []).includes(`${courseId}/${topicId}`);
    const interviewed = prefsInterview(courseId, topicId);
    const status = {
      lesson: card.read ? `${plural(parts.length, "раздел", "раздела", "разделов")} · пройдено` : `${plural(parts.length, "раздел", "раздела", "разделов")}`,
      practice: isWeb ? "в приложении" : own ? (own.done ? "пройдена" : `шаг ${own.step + 1} из ${own.total}`) : "задача на сервере",
      material: plural(topic.concepts?.length ?? 0, "понятие", "понятия", "понятий"),
      cards: `${card.conceptsMature} из ${card.conceptsTotal}`,
      sheet: view.cheatsheet ? "1 страница" : "собирается из понятий",
      // Вопросы экзамена приходят отдельно (learn_exam), без ответов.
      check: card.examBest != null ? `лучший ${card.examBest}%` : `порог ${course.topicPass}%`,
      drill: drilled ? "выучено · повторить" : "6 понятий за подход",
      interview: interviewed ? `прошлый раз ${interviewed.score}%` : "мок-интервью по теме",
    };
    const done = {
      drill: drilled,
      interview: interviewed && interviewed.score >= 70,
      lesson: card.read,
      practice: own?.done,
      cards: card.conceptsTotal > 0 && card.conceptsMature >= card.conceptsTotal,
      check: card.examBest != null && card.examBest >= course.topicPass,
    };
    content.replaceChildren();
    const head = el("div", "course-head");
    head.append(el("span", "display topic-title", topic.title));
    if (topic.summary) head.append(el("span", "muted", topic.summary));
    content.append(head);

    // Путь темы — пять шагов по порядку: посмотреть, прочитать, послушать,
    // ответить вслух, попробовать руками. Остальное — в «Ещё».
    const seen = prefs.get("flow", {});
    const mark = (step) => {
      const all = prefs.get("flow", {});
      all[`${courseId}/${topicId}/${step}`] = Date.now();
      prefs.set("flow", all);
    };
    const was = (step) => Boolean(seen[`${courseId}/${topicId}/${step}`]);
    const button = (text, onClick) => {
      const node = el("button", "btn btn--secondary", text);
      node.addEventListener("click", onClick);
      return node;
    };
    const STEPS = [
      { id: "video", name: "Ролик", hint: "схема всей темы за пару минут", done: was("video"), panel: () => videoBox({ api, el, button, course: courseId, topic: topicId, section: null }) },
      { id: "lesson", name: "Урок", hint: status.lesson, done: card.read, open: () => open("lesson") },
      { id: "drama", name: "Аудиодрама", hint: "история по уроку на три голоса", done: was("drama"), panel: () => dramaBox({ api, el, button, course: courseId, topic: topicId }) },
      { id: "interview", name: "Собеседование", hint: status.interview, done: Boolean(done.interview), open: () => open("interview") },
      {
        id: "commands",
        name: "Команды для практики",
        hint: "что попробовать руками и что делает каждая",
        done: was("commands"),
        panel: () => {
          const wrap = el("div", "");
          wrap.append(commandsBox({ api, el, course: courseId, topic: topicId, lesson: topic.lesson }));
          if (!isWeb) wrap.append(button("Практика в терминале", () => open("practice")));
          return wrap;
        },
      },
    ];
    const now = STEPS.find((step) => !step.done) ?? null;
    const flow = el("div", "flow");
    for (const [n, step] of STEPS.entries()) {
      const row = el("button", `flow__step${step.done ? " is-done" : ""}${step === now ? " is-now" : ""}`);
      const body = el("span", "flow__body");
      body.append(el("span", "flow__name", step.name), el("span", "flow__hint", step.hint));
      row.append(el("span", "flow__n", step.done ? "✓" : String(n + 1)), body, el("span", "flow__go", step.panel ? "▾" : "›"));
      const panel = el("div", "flow__panel");
      panel.hidden = true;
      row.addEventListener("click", () => {
        if (!step.panel) return step.open();
        if (!panel.firstChild) panel.append(step.panel());
        panel.hidden = !panel.hidden;
        if (!panel.hidden) mark(step.id);
      });
      flow.append(row, panel);
    }
    content.append(flow);

    // Остальные режимы — для тех, кому нужно больше: материал, карточки,
    // конспект, мини-экзамен, наизусть, практика.
    const more = el("details", "more-modes");
    more.append(el("summary", "", "Ещё: материал, карточки, конспект, мини-экзамен"));
    const grid = el("div", "modes");
    for (const mode of MODES.filter((m) => !["lesson", "interview"].includes(m.id))) {
      const tile = el("button", `mode${done[mode.id] ? " is-done" : ""}`);
      tile.append(icon(mode.icon), el("span", "mode__name", mode.name), el("span", "mode__state", status[mode.id]));
      tile.addEventListener("click", () => open(mode.id));
      grid.append(tile);
    }
    more.append(grid);
    content.append(more);

    // Внизу — следующий шаг, а пройдена тема — следующая тема.
    const next = course.topics[course.topics.indexOf(card) + 1] ?? null;
    const cta = el("button", "btn btn--primary btn--big");
    if (now) {
      cta.append(icon("play", 20, 2.25), now.id === "lesson" && card.section > 0 ? `Продолжить урок · раздел ${card.section + 1}` : `Дальше: ${now.name.toLowerCase()}`);
      cta.addEventListener("click", () => {
        const at = flow.children[STEPS.indexOf(now) * 2];
        if (now.panel) at.scrollIntoView({ block: "center", behavior: "smooth" });
        at.click();
      });
    } else if (next) {
      cta.append(icon("play", 20, 2.25), `Следующая тема: ${next.title}`);
      cta.addEventListener("click", () => nav.replace("topic", { course: courseId, topic: next.id }));
    } else {
      cta.append(icon("play", 20, 2.25), "Финальный экзамен курса");
      cta.addEventListener("click", () => nav.push("check", { course: courseId, scope: "final" }));
    }
    sticky.replaceChildren(cta);
  })();

  function open(mode) {
    const params = { course: courseId, topic: topicId };
    if (mode === "lesson") nav.push("lesson", params);
    else if (mode === "practice") {
      if (isWeb) practiceElsewhere();
      else nav.push("practice", params);
    }
    else if (mode === "material") nav.push("material", params);
    else if (mode === "cards") nav.push("review-topic", params);
    else if (mode === "sheet") nav.push("sheet", params);
    else if (mode === "drill") nav.push("drill", params);
    else if (mode === "interview") nav.push("interview", params);
    else nav.push("check", { course: courseId, scope: topicId });
  }

  return { cleanup: () => (alive = false) };
});

/** Последнее собеседование именно по этой теме. */
function prefsInterview(courseId, topicId) {
  return prefs.get("interviews", []).filter((item) => item.course === courseId && item.topic === topicId).at(-1) ?? null;
}

/** Практика в браузере: терминала здесь нет — она в программе и на iPhone. */
function practiceElsewhere() {
  sheet("Практика", (box) => {
    box.append(
      el(
        "span",
        "muted",
        "Практика — живая задача в терминале вашего сервера: Ноа видит вывод и ведёт по шагам. В браузере своего терминала нет, поэтому она — в приложении NOAH на iPhone и в программе на компьютере. Прогресс общий: шаг, сделанный там, виден здесь.",
      ),
    );
    const go = el("a", "btn btn--secondary", "Скачать программу NOAH");
    go.href = "/download";
    go.target = "_blank";
    go.rel = "noopener";
    box.append(go);
  });
}

/* ── Материал — понятия темы ───────────────────────────────────────────── */

register("material", (screen, { course: courseId, topic: topicId }) => {
  const card = topicCard(store.course(courseId), topicId);
  screen.append(header({ title: `${card?.title ?? ""} · материал` }));
  const content = el("div", "content");
  content.append(loading());
  screen.append(content);
  (async () => {
    try {
      const view = await store.topic(courseId, topicId);
      content.replaceChildren();
      const concepts = view.topic.concepts ?? [];
      if (!concepts.length) {
        content.append(el("div", "empty", "У этой темы нет понятий — весь материал в уроке."));
      }
      for (const concept of concepts) {
        const box = el("div", "plate concept");
        box.append(el("span", "concept__term", concept.term), el("span", "concept__def", concept.definition));
        for (const [key, title] of [
          ["mnemonic", "Зацепка"],
          ["analogy", "Похоже на"],
          ["example", "Пример"],
          ["pitfall", "Частая ошибка"],
        ]) {
          if (!concept[key]) continue;
          const line = el("span", "concept__extra");
          line.append(el("b", "", `${title}: `), concept[key]);
          box.append(line);
        }
        content.append(box);
      }
      const whole = el("button", "btn btn--secondary");
      whole.append(icon("learn", 18, 2), "Урок целиком");
      whole.addEventListener("click", () => nav.push("lesson", { course: courseId, topic: topicId }));
      content.append(whole);
    } catch (err) {
      content.replaceChildren(el("span", "error", String(err)));
    }
  })();
  return {};
});

/* ── Конспект ──────────────────────────────────────────────────────────── */

register("sheet", (screen, { course: courseId, topic: topicId }) => {
  const card = topicCard(store.course(courseId), topicId);
  screen.append(header({ title: `${card?.title ?? ""} · конспект` }));
  const content = el("div", "content content--read");
  content.append(loading());
  screen.append(content);
  (async () => {
    try {
      const view = await store.topic(courseId, topicId);
      content.replaceChildren(label(`// конспект · ${view.topic.title}`), markdown(view.cheatsheet || "Конспекта пока нет.", "md md--small"));
    } catch (err) {
      content.replaceChildren(el("span", "error", String(err)));
    }
  })();
  return {};
});
