// 03 Тема — заголовок со свечением, шесть режимов плитками (как вкладки
// окна обучения в программе), пояснение режима, чипы понятий и закреплённая
// кнопка «Продолжить …» — в тот режим, где человек сейчас.

import { el, icon, label, loading, header, nav, register, iconButton, plural, sections, markdown } from "../core.js";
import { store, topicCard } from "../store.js";

const MODES = [
  { id: "lesson", name: "Урок с Ноа", icon: "chat", about: "Ноа ведёт урок по разделам: читает вслух, разбирает подробно и обсуждает с вами то, что непонятно." },
  { id: "practice", name: "Практика", icon: "window", about: "Живая задача на вашем сервере. Вы работаете в терминале — Ноа его видит, объясняет, замечает ошибки и подсказывает следующий шаг." },
  { id: "material", name: "Материал", icon: "learn", about: "Понятия темы: определение, зацепка для памяти, аналогия, пример и частая ошибка." },
  { id: "cards", name: "Карточки", icon: "cards", about: "Карточки по расписанию: вспомнили — вернутся через дни, забыли — сегодня же." },
  { id: "sheet", name: "Конспект", icon: "note", about: "Вся тема на одну страницу — для быстрого повторения перед практикой или собеседованием." },
  { id: "check", name: "Проверить себя", icon: "target", about: "Мини-экзамен по теме: варианты и ответы своими словами. Ноа разбирает каждый ответ." },
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
    const status = {
      lesson: card.read ? `${plural(parts.length, "раздел", "раздела", "разделов")} · пройдено` : `${plural(parts.length, "раздел", "раздела", "разделов")}`,
      practice: own ? (own.done ? "пройдена" : `шаг ${own.step + 1} из ${own.total}`) : "задача на сервере",
      material: plural(topic.concepts?.length ?? 0, "понятие", "понятия", "понятий"),
      cards: `${card.conceptsMature} из ${card.conceptsTotal}`,
      sheet: view.cheatsheet ? "1 страница" : "собирается из понятий",
      // Вопросы экзамена приходят отдельно (learn_exam), без ответов.
      check: card.examBest != null ? `лучший ${card.examBest}%` : `порог ${course.topicPass}%`,
    };
    const done = {
      lesson: card.read,
      practice: own?.done,
      cards: card.conceptsTotal > 0 && card.conceptsMature >= card.conceptsTotal,
      check: card.examBest != null && card.examBest >= course.topicPass,
    };
    // Где человек сейчас: начатая практика, иначе непрочитанный урок, иначе проверка.
    const active = own && !own.done ? "practice" : !card.read ? "lesson" : card.examBest == null ? "check" : "cards";

    content.replaceChildren();
    const head = el("div", "course-head");
    head.append(el("span", "display topic-title", topic.title));
    if (topic.summary) head.append(el("span", "muted", topic.summary));
    content.append(head);

    const grid = el("div", "modes");
    for (const mode of MODES) {
      const tile = el("button", `mode${mode.id === active ? " is-active" : ""}${done[mode.id] ? " is-done" : ""}`);
      tile.append(icon(mode.icon), el("span", "mode__name", mode.name), el("span", "mode__state", status[mode.id]));
      tile.addEventListener("click", () => open(mode.id));
      grid.append(tile);
    }
    content.append(grid);

    const about = MODES.find((m) => m.id === active);
    const note = el("div", "note");
    note.append(label(`// ${about.name.toLowerCase()}`), el("span", "", about.about));
    content.append(note);

    const terms = (topic.concepts ?? []).slice(0, 8).map((c) => c.term);
    if (terms.length) {
      const chips = el("div", "chips");
      for (const term of terms) chips.append(el("span", "chip", term));
      content.append(chips);
    }

    const cta = el("button", "btn btn--primary btn--big");
    const text = {
      practice: own ? `Продолжить практику · шаг ${own.step + 1}` : "Начать практику",
      lesson: (card.step === "lesson" || card.step === "talk") && card.section > 0 ? `Продолжить урок · раздел ${card.section + 1}` : "Начать урок с Ноа",
      check: "Проверить себя",
      cards: "Повторить карточки",
    }[active];
    cta.append(icon("play", 20, 2.25), text);
    cta.addEventListener("click", () => open(active));
    sticky.replaceChildren(cta);
  })();

  function open(mode) {
    const params = { course: courseId, topic: topicId };
    if (mode === "lesson") nav.push("lesson", params);
    else if (mode === "practice") nav.push("practice", params);
    else if (mode === "material") nav.push("material", params);
    else if (mode === "cards") nav.push("review-topic", params);
    else if (mode === "sheet") nav.push("sheet", params);
    else nav.push("check", { course: courseId, scope: topicId });
  }

  return { cleanup: () => (alive = false) };
});

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
