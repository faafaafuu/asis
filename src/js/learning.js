// Окно обучения: курс из тем, в теме — урок, задачи и мини-экзамен, в конце
// курса — финальный экзамен.
//
// Материал и проверка — в Rust (learning.rs). Вопросы экзамена приходят сюда
// без ответов: подсмотреть их в окне нельзя, проверка идёт на стороне Rust.

import { tauri, appWindow, applyTheme } from "./bridge.js";

const api = tauri();
const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

/** Состояние окна. */
let courses = [];
let course = null;
let view = { kind: "home", topic: null, step: "lesson" };
let topicView = null;
let exam = null;
let busy = false;

/* ── Мелочи ────────────────────────────────────────────────────────────── */

function el(tag, className = "", text = "") {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  if (tag === "button") node.type = "button";
  return node;
}

function button(text, onClick, quiet = false) {
  const node = el("button", quiet ? "button button--quiet" : "button", text);
  node.addEventListener("click", onClick);
  return node;
}

function escapeHtml(text) {
  return text.replace(/[&<>"']/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch]);
}

/** Строка Markdown: код, жирный, курсив. Всё остальное экранировано. */
function inline(text) {
  return escapeHtml(text)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[\s(])\*([^*\s][^*]*)\*/g, "$1<em>$2</em>");
}

/**
 * Небольшой Markdown для уроков: заголовки, абзацы, списки (с вложенностью
 * одного уровня), блоки кода. Уроки пишутся в программе, а не приходят из
 * сети, но текст всё равно экранируется — привычка дешевле уязвимости.
 */
function markdown(source) {
  const lines = source.replace(/\r/g, "").split("\n");
  const out = [];
  let paragraph = [];
  let list = null;
  let code = null;

  const flushParagraph = () => {
    if (paragraph.length) out.push(`<p>${inline(paragraph.join(" "))}</p>`);
    paragraph = [];
  };
  const flushList = () => {
    if (!list) return;
    const render = (items, tag) =>
      `<${tag}>${items
        .map((item) => `<li>${inline(item.text)}${item.children.length ? render(item.children, "ul") : ""}</li>`)
        .join("")}</${tag}>`;
    out.push(render(list.items, list.tag));
    list = null;
  };

  for (const line of lines) {
    if (code) {
      if (line.startsWith("```")) {
        out.push(`<pre><code>${escapeHtml(code.join("\n"))}</code></pre>`);
        code = null;
      } else {
        code.push(line);
      }
      continue;
    }
    if (line.startsWith("```")) {
      flushParagraph();
      flushList();
      code = [];
      continue;
    }
    const heading = /^(#{1,3})\s+(.*)$/.exec(line);
    if (heading) {
      flushParagraph();
      flushList();
      out.push(`<h${heading[1].length}>${inline(heading[2])}</h${heading[1].length}>`);
      continue;
    }
    const item = /^(\s*)([-*]|\d+\.)\s+(.*)$/.exec(line);
    if (item) {
      flushParagraph();
      const nested = item[1].length >= 2;
      const tag = /\d/.test(item[2]) ? "ol" : "ul";
      if (!list) list = { tag, items: [] };
      if (nested && list.items.length) {
        list.items[list.items.length - 1].children.push({ text: item[3], children: [] });
      } else {
        list.items.push({ text: item[3], children: [] });
      }
      continue;
    }
    if (!line.trim()) {
      flushParagraph();
      flushList();
      continue;
    }
    flushList();
    paragraph.push(line.trim());
  }
  flushParagraph();
  flushList();
  if (code) out.push(`<pre><code>${escapeHtml(code.join("\n"))}</code></pre>`);
  return out.join("\n");
}

/** Доля темы: урок — пятая часть, задачи — треть, экзамен — половина. */
function topicPercent(topic) {
  const read = topic.read ? 20 : 0;
  const tasks = topic.tasksTotal ? (30 * topic.tasksDone) / topic.tasksTotal : 30;
  const exam = 50 * Math.min((topic.examBest ?? 0) / course.topicPass, 1);
  return Math.round(read + tasks + exam);
}

const MARKS = { done: "✓", practice: "◑", reading: "◔", new: "·" };

/* ── Боковая колонка ───────────────────────────────────────────────────── */

function renderSide() {
  ui.courseTitle.textContent = course ? course.title : "";
  ui.coursePercent.textContent = course ? `${course.percent}%` : "";
  ui.courseBar.style.width = `${course?.percent ?? 0}%`;
  ui.home.setAttribute("aria-current", String(view.kind === "home"));
  ui.final.setAttribute("aria-current", String(view.kind === "final"));
  ui.final.dataset.locked = String(!course?.finalUnlocked);
  ui.final.textContent =
    course?.finalBest != null ? `Финальный экзамен · ${course.finalBest}%` : "Финальный экзамен";

  ui.topics.replaceChildren(
    ...(course?.topics ?? []).map((topic, at) => {
      const item = el("li");
      const node = el("button", "topic");
      node.setAttribute("aria-current", String(view.kind === "topic" && view.topic === topic.id));
      const line = el("span", "topic__line");
      line.append(
        el("span", `topic__mark topic__mark--${topic.status}`, MARKS[topic.status] ?? "·"),
        el("span", "topic__title", `${at + 1}. ${topic.title}`),
        el("span", "topic__score", topic.examBest != null ? `${topic.examBest}%` : ""),
      );
      const bar = el("span", "bar bar--small");
      const fill = el("span", "bar__fill");
      fill.style.width = `${topicPercent(topic)}%`;
      bar.append(fill);
      node.append(line, bar);
      node.title = topic.summary;
      node.addEventListener("click", () => openTopic(topic.id, topic.read ? "tasks" : "lesson"));
      item.append(node);
      return item;
    }),
  );
}

/* ── Страницы ──────────────────────────────────────────────────────────── */

function page(title, lead = "") {
  const node = el("div", "page");
  node.append(el("h2", "page__title", title));
  if (lead) node.append(el("p", "page__lead", lead));
  ui.main.replaceChildren(node);
  ui.main.scrollTop = 0;
  return node;
}

function renderHome() {
  view = { kind: "home", topic: null, step: "lesson" };
  renderSide();
  if (!course) {
    page("Курсов пока нет");
    return;
  }
  const root = page(course.title, course.description);
  const done = course.topics.filter((t) => t.status === "done").length;
  const mistakes = course.topics.reduce((sum, t) => sum + t.mistakes, 0);
  const cards = el("div", "cards");
  const card = (value, label) => {
    const node = el("div", "card");
    node.append(el("div", "card__value", value), el("div", "card__label", label));
    return node;
  };
  cards.append(
    card(`${course.percent}%`, "пройдено"),
    card(`${done}/${course.topics.length}`, "тем сдано"),
    card(course.finalBest != null ? `${course.finalBest}%` : "—", "финальный экзамен"),
    card(String(mistakes), "ошибок на повторение"),
  );
  root.append(cards);

  const next =
    course.topics.find((t) => t.id === course.current && t.status !== "done") ??
    course.topics.find((t) => t.status !== "done");
  const actions = el("div", "actions");
  if (next) {
    actions.append(
      button(`Продолжить: ${next.title}`, () => openTopic(next.id, next.read ? "tasks" : "lesson")),
    );
  } else {
    actions.append(button("К финальному экзамену", () => openFinal()));
  }
  root.append(actions);

  root.append(el("h3", "", "Как устроено"));
  const how = el("ul", "list");
  for (const text of [
    "В каждой теме: урок → практические задачи → мини-экзамен. Экзамен сдан от " +
      `${course.topicPass}% — тема засчитана.`,
    "Вопросы с вариантами проверяются сразу, открытые ответы оценивает модель по ключевым пунктам эталона.",
    `Финальный экзамен открывается после всех тем, проходной балл — ${course.finalPass}%.`,
    "Ошибки копятся во вкладке «Ошибки» каждой темы — повторяйте, пока не исчезнут.",
    "Голосом и в Telegram: «Ноа, погоняй меня по докеру», «как мой прогресс по девопсу».",
  ]) {
    how.append(el("li", "", text));
  }
  root.append(how);

  const weak = course.topics.filter((t) => t.mistakes > 0);
  if (weak.length) {
    root.append(el("h3", "", "Слабые места"));
    const list = el("ul", "list");
    for (const topic of weak) {
      const item = el("li");
      const link = el("button", "button button--quiet", `${topic.title}: ошибок ${topic.mistakes}`);
      link.addEventListener("click", () => openTopic(topic.id, "mistakes"));
      item.append(link);
      list.append(item);
    }
    root.append(list);
  }
}

async function openTopic(id, step = "lesson") {
  if (!api || busy) return;
  try {
    topicView = await api.invoke("learn_topic", { course: course.id, topic: id });
  } catch (err) {
    page("Тема не открылась", String(err));
    return;
  }
  view = { kind: "topic", topic: id, step };
  exam = null;
  renderTopic();
}

function renderTopic() {
  renderSide();
  const card = course.topics.find((t) => t.id === view.topic);
  const topic = topicView.topic;
  const root = page(topic.title, topic.summary);

  const steps = el("div", "steps");
  const step = (key, label, badge = "") => {
    const node = el("button", "step", label);
    node.setAttribute("aria-selected", String(view.step === key));
    if (badge) node.append(el("span", "step__badge", badge));
    node.addEventListener("click", () => {
      view.step = key;
      exam = null;
      renderTopic();
    });
    steps.append(node);
  };
  step("lesson", "Урок", card?.read ? "✓" : "");
  step("tasks", "Задачи", `${card?.tasksDone ?? 0}/${card?.tasksTotal ?? 0}`);
  step("exam", "Мини-экзамен", card?.examBest != null ? `${card.examBest}%` : "");
  if (topicView.mistakes.length) step("mistakes", "Ошибки", String(topicView.mistakes.length));
  root.append(steps);

  if (view.step === "lesson") renderLesson(root, topic);
  else if (view.step === "tasks") renderQuestions(root, topic.tasks, "Задача");
  else if (view.step === "mistakes") renderQuestions(root, topicView.mistakes, "Повтор");
  else renderExamIntro(root, view.topic, card?.examBest, course.topicPass);
}

function renderLesson(root, topic) {
  const lesson = el("div", "lesson");
  lesson.innerHTML = markdown(topic.lesson);
  root.append(lesson);
  const actions = el("div", "actions");
  actions.append(
    button("Прочитал — к задачам", async () => {
      await api?.invoke("learn_read", { course: course.id, topic: topic.id }).catch(() => {});
      await refreshOverview();
      view.step = "tasks";
      renderTopic();
    }),
  );
  root.append(actions);
}

/* ── Вопросы ───────────────────────────────────────────────────────────── */

/** Поле ответа: варианты или текст. Отдаёт узел и функцию чтения ответа. */
function answerField(q, name) {
  if (q.kind === "choice") {
    const box = el("div", "options");
    q.options.forEach((option, at) => {
      const label = el("label", "option");
      const input = el("input");
      input.type = "radio";
      input.name = name;
      input.value = String(at);
      label.append(input, el("span", "", option));
      box.append(label);
    });
    const read = () => {
      const checked = box.querySelector("input:checked");
      return checked ? Number(checked.value) : null;
    };
    return { node: box, read };
  }
  const area = el("textarea", "answer");
  area.placeholder = "Ваш ответ своими словами — как на собеседовании";
  return { node: area, read: () => area.value.trim() };
}

function renderQuestions(root, questions, label) {
  if (!questions.length) {
    root.append(el("p", "muted", "Здесь пусто."));
    return;
  }
  questions.forEach((q, at) => {
    const card = el("div", "question");
    const head = el("div", "question__head");
    head.append(el("span", "question__number", `${label} ${at + 1}`));
    const best = topicView.scores?.[q.id];
    if (best != null && label === "Задача") {
      head.append(el("span", `question__score ${best >= 60 ? "good" : "bad"}`, `лучший: ${best}/100`));
    }
    card.append(head, el("p", "question__text", q.q));
    const field = answerField(q, `q-${q.id}`);
    card.append(field.node);
    const place = el("div");
    const actions = el("div", "actions");
    const check = button("Проверить", async () => {
      const answer = field.read();
      if (answer === null || answer === "") {
        place.replaceChildren(el("p", "note", "Сначала ответьте."));
        return;
      }
      check.disabled = true;
      check.textContent = q.kind === "choice" ? "Проверяю…" : "Проверяю… (модель читает ответ)";
      try {
        const verdict = await api.invoke("learn_check", { course: course.id, question: q.id, answer });
        place.replaceChildren(renderVerdict(verdict, q, true));
        markOptions(field.node, verdict);
        await refreshOverview();
        renderSide();
      } catch (err) {
        place.replaceChildren(el("p", "bad", String(err)));
      } finally {
        check.disabled = false;
        check.textContent = "Проверить ещё раз";
      }
    });
    actions.append(check);
    card.append(actions, place);
    root.append(card);
  });
}

/** Подсветка вариантов после проверки. */
function markOptions(box, verdict) {
  if (!box.classList?.contains("options")) return;
  box.querySelectorAll(".option").forEach((option, at) => {
    option.classList.toggle("option--right", at === verdict.answer);
    option.classList.toggle("option--wrong", at === verdict.chosen && at !== verdict.answer);
  });
}

/** Разбор ответа: балл, отзыв, пункты эталона, сам эталон. */
function renderVerdict(verdict, q, allowSelf) {
  const unknown = verdict.score == null;
  const box = el("div", `verdict ${unknown ? "" : verdict.right ? "verdict--right" : "verdict--wrong"}`);
  const title =
    q.kind === "choice"
      ? verdict.right
        ? "Верно"
        : "Неверно"
      : unknown
        ? "Не проверено"
        : `${verdict.score}/100 — ${verdict.right ? "засчитано" : "не засчитано"}`;
  box.append(el("p", `verdict__line ${verdict.right ? "good" : unknown ? "" : "bad"}`, title));
  if (verdict.feedback) box.append(el("p", "verdict__line", verdict.feedback));

  if (verdict.points?.length) {
    const list = el("ul", "verdict__points");
    verdict.points.forEach((point, at) => {
      const covered = verdict.covered?.includes(at + 1);
      list.append(el("li", covered ? "covered" : "missed", `${covered ? "✓" : "○"} ${point}`));
    });
    box.append(list);
  }
  if (verdict.reference) {
    const details = el("details");
    details.append(el("summary", "", q.kind === "choice" ? "Почему так" : "Эталонный ответ"));
    details.append(el("p", "verdict__reference", verdict.reference));
    if (unknown) details.open = true;
    box.append(details);
  }
  if (unknown && allowSelf) {
    const actions = el("div", "actions");
    const self = (knew) => async () => {
      await api?.invoke("learn_self_grade", { course: course.id, question: q.id, knew }).catch(() => {});
      await refreshOverview();
      renderSide();
      actions.replaceChildren(el("span", "note", knew ? "Засчитано." : "Добавлено в ошибки на повтор."));
    };
    actions.append(button("Знал", self(true)), button("Не знал", self(false), true));
    box.append(actions);
  }
  return box;
}

/* ── Экзамены ──────────────────────────────────────────────────────────── */

function renderExamIntro(root, scope, best, pass) {
  const lines = el("ul", "list");
  lines.append(
    el("li", "", `Проходной балл — ${pass}%.`),
    el("li", "", best != null ? `Лучший результат: ${best}%.` : "Ещё не сдавали."),
    el("li", "", "Ответы видны только после сдачи. Открытые ответы проверяет модель — это займёт до минуты."),
  );
  root.append(lines);
  const actions = el("div", "actions");
  actions.append(button("Начать", () => startExam(scope)));
  root.append(actions);
}

async function startExam(scope) {
  try {
    exam = await api.invoke("learn_exam", { course: course.id, scope });
  } catch (err) {
    ui.main.append(el("p", "bad", String(err)));
    return;
  }
  const root = page(exam.title, `${exam.questions.length} вопросов · проходной балл ${exam.pass}%`);
  const fields = new Map();
  exam.questions.forEach((q, at) => {
    const card = el("div", "question");
    const head = el("div", "question__head");
    head.append(el("span", "question__number", `Вопрос ${at + 1} из ${exam.questions.length}`));
    card.append(head, el("p", "question__text", q.q));
    const field = answerField(q, `exam-${q.id}`);
    fields.set(q.id, field);
    card.append(field.node);
    root.append(card);
  });
  const actions = el("div", "actions");
  const submit = button("Сдать экзамен", async () => {
    const answers = {};
    let empty = 0;
    for (const [id, field] of fields) {
      const value = field.read();
      if (value === null || value === "") empty += 1;
      answers[id] = value;
    }
    if (empty && !confirm(`Без ответа: ${empty}. Сдать так?`)) return;
    busy = true;
    submit.disabled = true;
    submit.textContent = "Проверяю ответы…";
    try {
      const result = await api.invoke("learn_submit", { course: course.id, scope, answers });
      await refreshOverview();
      renderResult(result, scope);
    } catch (err) {
      actions.append(el("p", "bad", String(err)));
      submit.disabled = false;
      submit.textContent = "Сдать экзамен";
    } finally {
      busy = false;
    }
  });
  const cancel = button("Отмена", () => (scope === "final" ? openFinal() : renderTopic()), true);
  actions.append(submit, cancel);
  root.append(actions);
}

function renderResult(result, scope) {
  renderSide();
  const root = page(exam?.title ?? "Результат");
  const box = el("div", "result");
  box.append(el("div", `result__score ${result.passed ? "good" : "bad"}`, `${result.score}%`));
  box.append(
    el(
      "p",
      "",
      result.passed
        ? scope === "final"
          ? "Финальный экзамен сдан. Курс пройден — к собеседованию готовы."
          : "Сдано! Тема засчитана."
        : `Не хватило до ${result.pass}%. Ошибки добавлены на повтор — разберите их и попробуйте снова.`,
    ),
  );
  if (result.unchecked) {
    box.append(el("p", "note", `Модель не проверила ответов: ${result.unchecked} — они посчитаны нулём.`));
  }
  root.append(box);

  const byId = new Map((exam?.questions ?? []).map((q) => [q.id, q]));
  result.items.forEach((verdict, at) => {
    const q = byId.get(verdict.id) ?? { kind: verdict.options?.length ? "choice" : "open", q: verdict.q };
    const card = el("div", "question");
    const head = el("div", "question__head");
    head.append(el("span", "question__number", `Вопрос ${at + 1}`));
    card.append(head, el("p", "question__text", verdict.q));
    if (verdict.options?.length) {
      const list = el("div", "options");
      verdict.options.forEach((option, index) => {
        const cls =
          index === verdict.answer ? "option option--right" : index === verdict.chosen ? "option option--wrong" : "option";
        list.append(el("div", cls, option));
      });
      card.append(list);
    }
    card.append(renderVerdict(verdict, q, false));
    root.append(card);
  });

  const actions = el("div", "actions");
  if (scope === "final") {
    actions.append(button("К обзору курса", () => renderHome()));
  } else {
    actions.append(button("Вернуться к теме", () => openTopic(scope, result.passed ? "exam" : "mistakes")));
    const next = course.topics.find((t) => t.status !== "done" && t.id !== scope);
    if (result.passed && next) actions.append(button(`Следующая тема: ${next.title}`, () => openTopic(next.id)));
  }
  root.append(actions);
}

function openFinal() {
  view = { kind: "final", topic: null, step: "exam" };
  exam = null;
  renderSide();
  const root = page(
    "Финальный экзамен",
    "Сквозные вопросы на стык тем и по два вопроса из каждой темы — как на настоящем собеседовании.",
  );
  if (!course.finalUnlocked) {
    const left = course.topics.filter((t) => t.status !== "done").map((t) => t.title);
    root.append(el("p", "", `Откроется, когда будут сданы все темы. Осталось: ${left.join(", ")}.`));
    const actions = el("div", "actions");
    actions.append(button("Всё равно попробовать", () => startExam("final"), true));
    root.append(actions);
    return;
  }
  renderExamIntro(root, "final", course.finalBest, course.finalPass);
}

/* ── Данные ────────────────────────────────────────────────────────────── */

async function refreshOverview() {
  if (!api) return;
  try {
    courses = (await api.invoke("learn_overview")) ?? [];
    course = courses.find((c) => c.id === course?.id) ?? courses[0] ?? null;
  } catch {
    /* окно покажет прежнее */
  }
}

ui.home.addEventListener("click", () => {
  if (!busy) renderHome();
});
ui.final.addEventListener("click", () => {
  if (!busy) openFinal();
});

/* ── Окно ──────────────────────────────────────────────────────────────── */

const win = appWindow();
// Свернуть — окно уходит на панель задач и возвращается оттуда или из трея.
ui.minimize?.addEventListener("click", () => win?.minimize());
ui.head.addEventListener("pointerdown", (event) => {
  if (event.button !== 0 || event.target.closest("button")) return;
  event.preventDefault();
  win?.startDragging();
});

// Esc в поле ответа — выйти из поля; вне поля — закрыть окно.
document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  if (document.activeElement?.matches("textarea, input")) {
    document.activeElement.blur();
    return;
  }
  api?.invoke("close_learning").catch(() => {});
});

ui.close.addEventListener("click", () => {
  api?.invoke("close_learning").catch(() => {});
});

api?.invoke("runtime_config").then((config) => applyTheme(config?.theme));
// Окно открыли голосом на другой теме — показать её.
document.addEventListener("visibilitychange", async () => {
  if (document.visibilityState !== "visible" || busy || view.kind === "final" || exam) return;
  await refreshOverview();
  renderSide();
});

refreshOverview().then(() => {
  const current = course?.topics.find((t) => t.id === course.current);
  if (current) openTopic(current.id, current.read ? "tasks" : "lesson");
  else renderHome();
});
