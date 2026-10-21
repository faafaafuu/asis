// Окно обучения: курс из тем, в теме — урок, понятия, карта, задачи и
// мини-экзамен, в конце курса — финальный экзамен. Сквозь всё идёт
// повторение по расписанию: понятия курса приходят карточками тогда, когда
// вот-вот забудутся.
//
// Материал и проверка — в Rust (learning.rs). Вопросы экзамена приходят сюда
// без ответов: подсмотреть их в окне нельзя, проверка идёт на стороне Rust.

import { tauri, appWindow } from "./bridge.js";
import { renderLessonAgent } from "./lesson-agent.js";
import { attachReader } from "./lesson-reader.js";
import { guideCard, guidePage, tint } from "./guides.js";
import { videoBox } from "./video-panel.js";

const api = tauri();
const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

// В шапке — версия, которая стоит: в программе её знает Rust, в браузере это Ноа онлайн.
api
  ?.invoke("app_version")
  .then((version) => (ui.appVersion.title = `NOAH ${version}`))
  .catch(() => (ui.appVersion.title = document.documentElement.classList.contains("is-web") ? "NOAH · Ноа онлайн" : "NOAH"));

// Значки — спрайт NOAH в начале страницы: на него ссылаются <use href="#i-…">.
fetch(new URL("../styles/noah/icons.svg", import.meta.url))
  .then((response) => response.text())
  .then((text) => {
    const box = document.createElement("div");
    box.hidden = true;
    box.innerHTML = text;
    document.body.prepend(box);
  })
  .catch(() => {
    /* без значков — подписи всё равно есть */
  });

/** Состояние окна. */
let courses = [];
let course = null;
let view = { kind: "home", topic: null, step: "lesson" };
let topicView = null;
let exam = null;
let busy = false;
/** Идёт повторение карточек: очередь и счёт. */
let review = null;
/** Путеводители: карточки для лобби и открытый сейчас. В браузере их нет — null. */
let guides = null;
let guide = null;
/** Какое понятие спрашивали после какого раздела урока: «тема#раздел» → id. */
const recalled = new Map();

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

/**
 * Телефон: список тем не помещается сбоку — он выдвигается кнопкой «Темы»
 * в заголовке и прячется, когда тему выбрали.
 */
function phoneTopics() {
  if (!document.documentElement.classList.contains("is-phone") || document.querySelector(".phone-topics")) return;
  const toggle = el("button", "phone-topics", "☰ Темы");
  const side = document.querySelector(".side");
  toggle.addEventListener("click", () => side?.classList.toggle("is-open"));
  side?.addEventListener("click", (event) => {
    if (event.target.closest("button")) side.classList.remove("is-open");
  });
  ui.head.prepend(toggle);
}

const NS = "http://www.w3.org/2000/svg";

/** Значок из спрайта NOAH (styles/noah/icons.svg). */
function icon(name, size = 16, stroke = 2) {
  const node = document.createElementNS(NS, "svg");
  node.setAttribute("width", String(size));
  node.setAttribute("height", String(size));
  node.style.strokeWidth = String(stroke);
  const use = document.createElementNS(NS, "use");
  use.setAttribute("href", `#i-${name}`);
  node.append(use);
  return node;
}

/** Хлебные крошки в заголовке окна: где ты. */
function crumbs() {
  const parts = ["Обучение"];
  if (view.kind === "guides" || view.kind === "guide") parts.push("Путеводители");
  if (view.kind === "guide" && guide) parts.push(guide.title);
  if (!["lobby", "guides", "guide"].includes(view.kind) && course) {
    parts.push(course.title);
    const topic = view.kind === "topic" ? course.topics.find((t) => t.id === view.topic) : null;
    if (topic) parts.push(topic.title);
    if (view.kind === "final") parts.push("Финальный экзамен");
  }
  ui.crumbs.textContent = parts.join(" / ");
  ui.crumbs.title = ui.crumbs.textContent;
}

/** Меню 4 × 36: Главная · Повторение · Фокус · Путеводители. */
function renderMenu() {
  const due = course?.mastery ? course.mastery.due + course.mastery.newLeft : 0;
  const items = [
    ["Главная", "learn", "", view.kind === "lobby", () => renderLobby()],
    ["Повторение", "cards", due ? String(due) : "", view.kind === "review", () => course && startReview(null)],
    ["Фокус", "timer", "", false, () => (focus ? openFocusMenu() : openFocusSetup())],
    ["Путеводители", "note", guides ? String(guides.length) : "", view.kind === "guides" || view.kind === "guide", () => renderGuides()],
  ];
  ui.menu.replaceChildren(
    ...items.map(([text, name, badge, on, go]) => {
      const node = el("button", "menu-item");
      node.setAttribute("aria-current", String(on));
      node.append(icon(name), el("span", "", text), el("span", "menu-item__badge", badge));
      node.addEventListener("click", () => !busy && go());
      return node;
    }),
  );
}

function sideLabel(text) {
  return el("li", "side__label", text);
}

function renderSide() {
  phoneTopics();
  crumbs();
  renderMenu();
  ui.sideFoot.replaceChildren();

  // Путеводитель — его разделы.
  if (view.kind === "guide") {
    ui.topics.replaceChildren(
      sideLabel("Разделы"),
      ...(guide?.sections ?? []).map((section, at) => {
        const item = el("li");
        const node = el("button", "side-topic");
        const ready = section.text.length || section.items.length;
        if (ready) node.classList.add("is-done");
        node.title = section.title;
        node.append(
          el("span", "side-topic__mark", section.emoji || "·"),
          el("span", "side-topic__n", String(at + 1)),
          el("span", "side-topic__title", section.title),
          el("span", "side-topic__pct", ready ? "" : "…"),
        );
        node.addEventListener("click", () =>
          document.getElementById(`guide-${section.id}`)?.scrollIntoView({ behavior: "smooth", block: "start" }),
        );
        item.append(node);
        return item;
      }),
    );
    return;
  }

  // Главная и путеводители — курсы строками 52, внизу «Собрать курс».
  if (view.kind === "lobby" || view.kind === "guides") {
    ui.topics.replaceChildren(
      ...(courses.length ? [sideLabel(`Курсы · ${courses.length}`)] : []),
      ...courses.map((other) => {
        const item = el("li");
        const node = el("button", "side-course");
        node.title = other.title;
        const line = el("span", "side-course__line");
        line.append(
          el("span", "side-course__title", other.title),
          el("span", `side-course__pct${other.percent ? " is-on" : ""}`, `${other.percent}%`),
        );
        node.append(line, bar(other.percent, "lh-bar lh-bar--thin"));
        node.addEventListener("click", () => {
          if (busy) return;
          chooseCourse(other);
          renderHome();
        });
        item.append(node);
        return item;
      }),
    );
    const build = el("button", "side-build");
    build.append(icon("plus", 16, 2.25), el("span", "", "Собрать курс"));
    build.addEventListener("click", () => {
      if (busy) return;
      const open = () => {
        const box = ui.main.querySelector(".builder");
        if (!box) return false;
        box.hidden = false;
        box.open = true;
        box.scrollIntoView({ behavior: "smooth", block: "center" });
        box.querySelector("textarea")?.focus();
        return true;
      };
      if (!open()) renderLobby().then(open);
    });
    ui.sideFoot.append(build);
    return;
  }

  // Курс: «Обзор курса», темы строками 32, внизу финальный экзамен.
  const overview = el("button", "side-overview");
  overview.setAttribute("aria-current", String(view.kind === "home"));
  overview.append(icon("target"), el("span", "", "Обзор курса"));
  overview.addEventListener("click", () => !busy && renderHome());
  const now = nextTopic();
  ui.topics.replaceChildren(
    el("li"),
    ...(course?.topics ?? []).map((topic, at) => {
      const item = el("li");
      const node = el("button", "side-topic");
      const done = topic.status === "done";
      const current = view.kind === "topic" ? view.topic === topic.id : now?.id === topic.id;
      if (done) node.classList.add("is-done");
      if (current) node.classList.add("is-now");
      node.setAttribute("aria-current", String(view.kind === "topic" && view.topic === topic.id));
      const percent = topicPercent(topic);
      node.append(
        el("span", "side-topic__mark", done ? "✓" : current ? "▸" : "·"),
        el("span", "side-topic__n", String(at + 1)),
        el("span", "side-topic__title", topic.title),
        el("span", "side-topic__pct", percent ? `${percent}%` : "—"),
      );
      // Из чего складывается и чего не хватает: урок — 20, задачи — 30, экзамен — 50.
      const parts = [
        `Урок: ${topic.read ? "прочитан ✓" : "не отмечен прочитанным"}`,
        topic.tasksTotal ? `Задачи: ${topic.tasksDone} из ${topic.tasksTotal}` : "",
        `Мини-экзамен: ${topic.examBest != null ? `${topic.examBest}%${topic.examBest >= course.topicPass ? " ✓" : ` (нужно ${course.topicPass}%)`}` : "не сдавали"}`,
        topic.conceptsTotal ? `Уверенно: ${topic.conceptsMature} из ${topic.conceptsTotal} понятий` : "",
      ].filter(Boolean);
      node.title = `${at + 1}. ${topic.title}\n${topic.summary ?? ""}\n\n${parts.join("\n")}`;
      node.addEventListener("click", () => resumeTopic(topic.id));
      item.append(node);
      return item;
    }),
  );
  ui.topics.firstChild.append(overview);
  if (course) {
    const final = el("button", "side-final");
    final.dataset.locked = String(!course.finalUnlocked);
    final.setAttribute("aria-current", String(view.kind === "final"));
    const done = course.topics.filter((t) => t.status === "done").length;
    final.append(
      icon(course.finalUnlocked ? "target" : "lock"),
      el("span", "", course.finalBest != null ? `Финальный экзамен · ${course.finalBest}%` : "Финальный экзамен"),
      el("span", "side-final__count", `${done} / ${course.topics.length}`),
    );
    final.title = course.finalUnlocked ? "Все темы сданы — экзамен открыт" : "Откроется, когда все темы будут сданы";
    final.addEventListener("click", () => !busy && openFinal());
    ui.sideFoot.append(final);
  }
  // Тема из конца курса не должна прятаться под прокруткой колонки.
  ui.topics.querySelector(".is-now")?.scrollIntoView({ block: "nearest" });
}

/** Полоса прогресса: дорожка и заливка. */
function bar(percent, className = "lh-bar") {
  const track = el("span", className);
  const fill = el("span");
  fill.style.width = `${Math.max(0, Math.min(100, percent))}%`;
  track.append(fill);
  return track;
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

/**
 * «Собрать курс» прямо в окне обучения: курс собирается на сервере через
 * мост (как в Ноа онлайн) и приходит сюда сам — программа забирает готовые
 * курсы с сайта. Нужен ключ площадки в настройках. В браузере кнопка своя —
 * в Ноа онлайн.
 */
function courseBuilder(open = false) {
  const box = el("details", "builder");
  box.open = open;
  box.append(el("summary", "builder__title", "＋ Собрать новый курс"));
  const goal = el("textarea", "answer");
  goal.rows = 3;
  goal.placeholder = "О чём курс и зачем. Например: «Kubernetes с нуля до продакшена — чтобы пройти собеседование на DevOps»";
  const quality = el("select", "builder__quality");
  for (const [value, label] of [
    ["sonnet", "Sonnet — точнее, около минуты на тему"],
    ["haiku", "Haiku — бережёт лимит подписки"],
  ]) {
    const option = el("option", "", label);
    option.value = value;
    quality.append(option);
  }
  const status = el("p", "builder__status");
  const go = button("Собрать курс", async () => {
    const text = goal.value.trim();
    if (text.length < 10) {
      status.textContent = "Опишите курс подробнее: о чём он и для чего — хотя бы одним предложением.";
      return;
    }
    go.disabled = true;
    status.textContent = "Отправляю…";
    try {
      await api.invoke("learn_build", { goal: text, quality: quality.value });
      goal.value = "";
      watchBuilds(status, stop);
    } catch (err) {
      status.textContent = String(err);
    }
    go.disabled = false;
  });
  const stop = button("Остановить", async () => {
    if (stop.dataset.id) await api.invoke("learn_build_stop", { id: stop.dataset.id }).catch(() => {});
  }, true);
  stop.hidden = true;
  const row = el("div", "actions");
  row.append(go, stop);
  box.append(
    el("p", "muted", "Ноа составит план и напишет курс тема за темой на сервере через ваш мост: урок, понятия, карточки, задачи, мини-экзамен. Курс появится здесь сам — по первой теме можно учиться сразу."),
    goal,
    quality,
    row,
    status,
  );
  watchBuilds(status, stop);
  return box;
}

/**
 * Вход в аккаунт через Telegram — без ключа вручную. После входа курсы
 * аккаунта приходят сами за несколько секунд: окно ждёт их и показывает.
 */
function accountCard() {
  const box = el("section", "builder account");
  box.append(el("h2", "account__title", "Войдите в аккаунт NOAH"));
  box.append(el("p", "muted", "Подтянутся ваши курсы, прогресс и мост — то же, что у вас на компьютере и в Ноа онлайн."));
  const status = el("p", "builder__status");
  const go = button("Войти через Telegram", async () => {
    go.disabled = true;
    status.textContent = "Открываю Telegram — нажмите там Start и вернитесь сюда.";
    try {
      const name = await api.invoke("account_login");
      status.textContent = `Вы вошли${name ? ` как ${name}` : ""}. Подтягиваю курсы…`;
      for (let i = 0; i < 20 && !courses.length; i++) {
        await new Promise((resolve) => setTimeout(resolve, 3000));
        await refreshOverview();
      }
      if (courses.length) renderHome();
      else status.textContent = "Вход выполнен. Курсов в аккаунте пока нет — соберите первый ниже.";
    } catch (err) {
      status.textContent = String(err);
      go.disabled = false;
    }
  });
  box.append(go, status);
  return box;
}

/** Ход сборки — пока окно на месте; готовая тема — сразу в списке. */
let buildTimer = 0;
function watchBuilds(status, stop) {
  clearTimeout(buildTimer);
  const tick = async () => {
    if (!status.isConnected) return;
    const builds = await api.invoke("learn_builds").catch(() => []);
    const running = (Array.isArray(builds) ? builds : []).find((b) => b.status === "running");
    const last = Array.isArray(builds) ? builds[0] : null;
    if (running) {
      const where = running.total ? `тема ${Math.min(running.done + 1, running.total)} из ${running.total}` : "план курса";
      status.textContent = `Собирается${running.title ? ` «${running.title}»` : ""}: ${where}. ${running.message ?? ""}`;
      stop.hidden = false;
      stop.dataset.id = running.id;
      // Готовые темы уже пришли — показать их, не дожидаясь конца.
      const before = courses.length;
      await refreshOverview();
      if (courses.length !== before && view.kind === "home") renderSide();
      buildTimer = setTimeout(tick, 10_000);
    } else {
      stop.hidden = true;
      if (last && status.textContent.startsWith("Собирается")) {
        status.textContent = last.status === "done" ? `Курс «${last.title}» собран.` : last.message ?? "";
        await refreshOverview();
        renderSide();
      }
    }
  };
  tick();
}

/**
 * Главная обучения по хендоффу: приветствие и итоги, «Продолжить», плитки
 * «Повторение» и «Фокус», мои курсы 2 × 2 и путеводители. Блоки одного ряда —
 * одна высота и одна структура.
 */
async function renderLobby() {
  view = { kind: "lobby", topic: null, step: "lesson" };
  review = null;
  guide = null;
  await Promise.all([refreshOverview(), refreshGuides()]);
  renderSide();
  const root = lhPage();

  // 1. Приветствие и три итога.
  const current = courses.find((c) => c.id === lastCourse()) ?? course;
  const f = current?.focus ?? { today: 0, week: 0, streak: 0 };
  const hello = el("div", "lh-hello");
  const text = el("div", "lh-hello__text");
  const date = new Date().toLocaleDateString("ru-RU", { weekday: "long", day: "numeric", month: "long" });
  text.append(el("span", "lh-hello__date", date[0].toUpperCase() + date.slice(1)), el("span", "lh-h lh-hello__title", greeting()));
  const stats = el("div", "lh-stats");
  const doneAll = courses.reduce((sum, c) => sum + c.topics.filter((t) => t.status === "done").length, 0);
  const stat = (label, value, color) => {
    const box = el("span", "lh-stat");
    const number = el("span", "lh-stat__value", value);
    if (color) number.style.color = color;
    box.append(el("span", "lh-stat__label", label), number);
    return box;
  };
  stats.append(
    stat("Серия", plural(f.streak, "день", "дня", "дней"), "var(--lh-streak)"),
    stat("Неделя", `${f.week} мин`),
    stat("Сдано", plural(doneAll, "тема", "темы", "тем"), "var(--lh-done)"),
  );
  hello.append(text, stats);
  root.append(hello);

  // 2. «Продолжить»
  const next = current ? nextOf(current) : null;
  if (current && next) root.append(continueCard(current, next));

  // 3. Плитки
  if (current) root.append(tilesRow(current));

  // 4. Мои курсы
  const mine = el("section", "lh-block");
  const mineHead = el("div", "lh-block__head");
  mineHead.append(el("span", "lh-h lh-block__title", "Мои курсы"), el("span", "lh-block__note", String(courses.length)));
  mine.append(mineHead);
  const grid = el("div", "lh-courses");
  for (const other of courses) grid.append(courseCard(other));
  // Сборка курса прячется, пока не нажали «Новый курс»: в макете её нет.
  const builder = courseBuilder(!courses.length);
  builder.hidden = Boolean(courses.length);
  const add = el("button", "lh-course lh-course--add");
  const plus = el("span", "lh-course__plus");
  plus.append(icon("plus", 20, 2.25));
  add.append(plus, el("span", "lh-course__title", "Новый курс"), el("span", "lh-course__about", "Любая тема — уроки, задачи, экзамены"));
  add.addEventListener("click", () => {
    builder.hidden = false;
    builder.open = true;
    builder.scrollIntoView({ behavior: "smooth", block: "center" });
    builder.querySelector("textarea")?.focus();
  });
  grid.append(add);
  mine.append(grid, builder);
  root.append(mine);

  // 5. Путеводители — только в программе: пишет их модель на этом компьютере.
  if (guides) root.append(guidesBlock(false));
}

/** Страница новой разметки: колонка блоков с полями 24/28. */
function lhPage() {
  const node = el("div", "lh");
  ui.main.replaceChildren(node);
  ui.main.scrollTop = 0;
  return node;
}

function greeting() {
  const hour = new Date().getHours();
  return hour < 5 ? "Доброй ночи" : hour < 12 ? "Доброе утро" : hour < 18 ? "Добрый день" : "Добрый вечер";
}

function plural(n, one, few, many) {
  const tens = n % 100;
  const last = n % 10;
  const word = tens >= 11 && tens <= 14 ? many : last === 1 ? one : last >= 2 && last <= 4 ? few : many;
  return `${n} ${word}`;
}

/** Тема, с которой продолжать курс: где остановились, иначе первая несданная. */
function nextOf(of) {
  return of.topics.find((t) => t.id === of.current && t.status !== "done") ?? of.topics.find((t) => t.status !== "done") ?? null;
}

/** «Продолжить» — grid 1fr 200px: текст и прогресс · три кнопки столбиком. */
function continueCard(of, next) {
  const card = el("section", "lh-active lh-continue");
  const text = el("div", "lh-continue__text");
  const at = of.topics.indexOf(next) + 1;
  text.append(
    el("span", "lh-label", "// продолжить"),
    el("span", "lh-continue__meta", `${of.title} · тема ${at} из ${of.topics.length}`),
    el("span", "lh-continue__title", next.title),
    el("span", "lh-continue__grow"),
  );
  const progress = el("div", "lh-continue__progress");
  progress.append(bar(of.percent), el("span", "lh-continue__pct", `${of.percent}% курса`));
  text.append(progress);
  const actions = el("div", "lh-continue__actions");
  const place = placeOf(next.id);
  const started = next.read || next.status !== "new" || place.section > 0;
  const enter = (step) => {
    if (busy) return;
    chooseCourse(of);
    step ? openTopic(next.id, step) : resumeTopic(next.id);
  };
  actions.append(
    lhButton(started ? `Раздел ${place.section + 1}` : "Начать", "play", () => enter(null), "lh-btn--primary", 2.5),
    lhButton("Урок с Ноа", "mic", () => enter("talk")),
    lhButton("Практика", "window", () => enter("practice")),
  );
  card.append(text, actions);
  return card;
}

function lhButton(text, name, onClick, extra = "", stroke = 2) {
  const node = el("button", `lh-btn ${extra}`.trim());
  node.append(icon(name, 16, stroke), el("span", "", text));
  node.addEventListener("click", onClick);
  return node;
}

/** Две плитки одинаковой структуры: Повторение и Фокус. */
function tilesRow(of) {
  const row = el("div", "lh-tiles");
  const m = of.mastery ?? { total: 0, due: 0, newLeft: 0, mature: 0, young: 0, learning: 0, new: 0 };
  const count = (m.due ?? 0) + (m.newLeft ?? 0);
  const minutes = Math.max(1, Math.round(count * 0.25));
  const review = tile({
    label: "Повторение",
    meta: count ? `≈ ${minutes} мин` : "",
    value: String(count),
    unit: "карточек",
    note: m.total ? `уверенно ${m.mature} · держится ${m.young} · учится ${m.learning}` : "в курсе нет понятий",
    segs: shares(
      [
        [m.mature, "var(--lh-done)"],
        [m.young, "#8ab4ff"],
        [m.learning, "#ffd98a"],
      ],
      m.total,
    ),
    cta: count ? "Повторить сейчас" : "На сегодня всё",
    icon: "cards",
    go: count ? () => !busy && startReview(null) : null,
  });
  const f = of.focus ?? { today: 0, week: 0, streak: 0 };
  const focusTile = tile({
    label: "Фокус",
    meta: f.streak ? `${plural(f.streak, "день", "дня", "дней")} подряд` : "",
    value: String(f.today),
    unit: "мин сегодня",
    note: `${f.week} мин за неделю`,
    segs: Array.from({ length: 7 }, (_, i) => (i < Math.min(7, f.streak) ? "var(--lh-accent)" : "")),
    cta: focus ? "Фокус идёт — меню" : "Фокус-сессия",
    icon: "timer",
    go: () => !busy && (focus ? openFocusMenu() : openFocusSetup()),
  });
  row.append(review, focusTile);
  return row;
}

/** Семь отрезков полоски поровну долям: уверенно, держится, учится, остальное. */
function shares(parts, total) {
  const out = [];
  if (!total) return Array(7).fill("");
  let left = 7;
  for (const [n, color] of parts) {
    const k = Math.min(left, Math.round((n / total) * 7));
    for (let i = 0; i < k; i++) out.push(color);
    left -= k;
  }
  while (out.length < 7) out.push("");
  return out;
}

function tile({ label, meta, value, unit, note, segs, cta, icon: name, go }) {
  const box = el("div", "lh-tile");
  const head = el("span", "lh-tile__head");
  head.append(el("span", "", label), el("span", "lh-tile__meta", meta));
  const number = el("span", "lh-tile__value");
  number.append(el("span", "lh-tile__num", value), el("span", "lh-tile__unit", unit));
  const strip = el("span", "lh-segs");
  for (const color of segs) {
    const seg = el("span");
    if (color) seg.style.background = color;
    strip.append(seg);
  }
  const button = lhButton(cta, name, () => go?.(), "lh-btn--soft", 2.25);
  button.disabled = !go;
  box.append(head, number, el("span", "lh-tile__note", note), strip, button);
  return box;
}

function chooseCourse(other) {
  course = other;
  try {
    localStorage.setItem("noa.lastCourse", other.id);
  } catch {
    /* не запомнится — откроется первый */
  }
}

const TILES = ["#2b5bc4", "#b5342e", "#5f8c4c", "#8a4fc4", "#c4842b", "#2b8fa8"];

/** Буквы курса для плитки-монограммы: первые буквы двух значимых слов. */
function monogram(title) {
  const words = title.split(/[\s:—,/-]+/).filter((w) => (w.length > 2 || /^\p{Lu}{2}$/u.test(w)) && /^[\p{L}\d]/u.test(w));
  const letters = words.slice(0, 2).map((w) => w[0].toUpperCase());
  // Название начинается латиницей — и вторая буква латинская: «DevOps и
  // инфраструктура» — «DI», а не «DИ».
  if (/^[A-Z]/.test(letters[0] ?? "")) letters[1] = LATIN[letters[1]] ?? letters[1];
  return (letters.join("") || title.slice(0, 2)).toUpperCase();
}

const LATIN = { А: "A", Б: "B", В: "V", Г: "G", Д: "D", Е: "E", Ж: "Z", З: "Z", И: "I", Й: "I", К: "K", Л: "L", М: "M", Н: "N", О: "O", П: "P", Р: "R", С: "S", Т: "T", У: "U", Ф: "F", Х: "H", Ц: "C", Ч: "C", Ш: "S", Щ: "S", Э: "E", Ю: "U", Я: "Y" };

/** Карточка курса 184: монограмма · мета · % / название / описание / полоса. */
function courseCard(other) {
  const card = el("button", `lh-course${other.id === course?.id ? " is-current" : ""}`);
  const top = el("span", "lh-course__top");
  const mono = el("span", "lh-mono", monogram(other.title));
  mono.style.setProperty("--tile", TILES[tint(other.title) % TILES.length]);
  const done = other.topics.filter((t) => t.status === "done").length;
  const due = other.mastery ? other.mastery.due + other.mastery.newLeft : 0;
  const meta = other.building
    ? `собирается · ${other.building.done} из ${other.building.total}`
    : due
      ? `повторить ${due}`
      : `${plural(other.topics.length, "тема", "темы", "тем")}`;
  top.append(mono, el("span", "lh-course__meta", meta), el("span", `lh-course__pct${other.percent ? " is-on" : ""}`, `${other.percent}%`));
  const foot = el("span", "lh-course__foot");
  foot.append(bar(other.percent, "lh-bar lh-bar--card"), el("span", "lh-course__count", `${done} / ${other.topics.length} тем`));
  card.append(top, el("span", "lh-course__title", other.title), el("span", "lh-course__about", other.description || ""), el("span"), foot);
  card.title = other.title;
  card.addEventListener("click", () => {
    if (busy) return;
    chooseCourse(other);
    renderHome();
  });
  return card;
}

const GUIDE_IDEAS = ["Деревья Москвы", "Как построить дом", "Всё про умный дом", "Грибы средней полосы", "Кофе: от зерна до чашки"];

/**
 * Путеводители: поле 40 + «Собрать» 120, чипы-подсказки 28 и готовые
 * путеводители строками. `shelf` — со страницы «Путеводители»: ещё и обложки.
 */
function guidesBlock(shelf) {
  const box = el("section", "lh-block");
  box.dataset.shelf = "guides";
  const head = el("div", "lh-block__head");
  head.append(el("span", "lh-h lh-block__title", "Путеводители"), el("span", "lh-block__note", "почитать для интереса — без задач и экзаменов"));
  box.append(head);
  const panel = el("div", "lh-guides");
  const row = el("div", "lh-guides__row");
  const input = el("input", "lh-input");
  input.type = "text";
  input.placeholder = "О чём путеводитель — например, «деревья Москвы»";
  const status = el("p", "lh-guides__status");
  const go = el("button", "lh-btn lh-guides__go", "Собрать");
  go.addEventListener("click", async () => {
    const goal = input.value.trim();
    if (goal.length < 3) {
      status.textContent = "Напишите тему хотя бы парой слов.";
      input.focus();
      return;
    }
    go.disabled = true;
    status.textContent = "Начинаю: сначала план, потом раздел за разделом…";
    try {
      const id = await api.invoke("guide_build", { goal });
      input.value = "";
      status.textContent = "";
      openGuide(id);
    } catch (err) {
      status.textContent = String(err);
    }
    go.disabled = false;
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") go.click();
  });
  row.append(input, go);
  const chips = el("div", "lh-chips");
  for (const idea of GUIDE_IDEAS) {
    const chip = el("button", "lh-chip", idea);
    chip.addEventListener("click", () => {
      input.value = idea;
      input.focus();
    });
    chips.append(chip);
  }
  panel.append(row, chips, status);
  box.append(panel);
  if (shelf) {
    const covers = el("div", "shelf");
    covers.dataset.el = "guideShelf";
    fillGuideShelf(covers);
    box.append(covers);
  } else if (guides?.length) {
    const list = el("div", "lh-table");
    list.dataset.el = "guideRows";
    fillGuideRows(list);
    box.append(list);
  }
  return box;
}

/** Готовые путеводители строками 48: значок · название · разделы. */
function fillGuideRows(list) {
  list.replaceChildren(
    ...(guides ?? []).map((card) => {
      const row = el("button", "lh-row lh-row--guide");
      const live = card.status === "building";
      row.append(
        el("span", "lh-row__n", card.emoji || "📖"),
        el("span", "lh-row__title", card.title),
        el("span", `lh-row__meta${live ? " is-live" : ""}`, live ? `пишется · ${card.written} из ${card.sections || "…"}` : plural(card.sections, "раздел", "раздела", "разделов")),
        icon("chevron-right", 20),
      );
      row.title = card.subtitle || card.title;
      row.addEventListener("click", () => !busy && openGuide(card.id));
      return row;
    }),
  );
}

/** Карточки путеводителей с обложками — на странице «Путеводители». */
function fillGuideShelf(shelf) {
  shelf.replaceChildren(...(guides ?? []).map((card) => guideCard(card, { api, el, onOpen: (id) => !busy && openGuide(id) })));
  if (!guides?.length) shelf.append(el("p", "shelf__empty", "Пока ни одного. Напишите тему выше — например, «деревья Москвы»."));
}

/** «Путеводители» из меню: сборка и полка с обложками. */
async function renderGuides() {
  view = { kind: "guides", topic: null, step: "lesson" };
  review = null;
  guide = null;
  await refreshGuides();
  renderSide();
  const root = lhPage();
  if (!guides) {
    root.append(el("p", "lh-block__note", "Путеводители собирает программа на компьютере — в браузере их нет."));
    return;
  }
  root.append(guidesBlock(true));
}

async function refreshGuides() {
  if (!api) return;
  try {
    guides = (await api.invoke("guides_list")) ?? [];
  } catch {
    guides = null;
  }
}

/** Открыть путеводитель. Пишется — перерисуется сам по `guides:changed`. */
async function openGuide(id) {
  try {
    guide = await api.invoke("guide_get", { id });
  } catch (err) {
    page("Путеводитель", String(err));
    return;
  }
  view = { kind: "guide", topic: null, step: "lesson", guide: id };
  review = null;
  renderGuide(true);
}

function renderGuide(top = false) {
  if (!guide) return;
  const scroll = ui.main.scrollTop;
  renderSide();
  const node = guidePage(guide, {
    api,
    el,
    button,
    inline,
    attachReader,
    onBack: () => !busy && renderGuides(),
    onDeleted: () => renderGuides(),
    onChanged: () => reloadGuide(),
  });
  const wrap = el("div", "page page--guide");
  wrap.append(node);
  ui.main.replaceChildren(wrap);
  ui.main.scrollTop = top ? 0 : scroll;
}

async function reloadGuide() {
  if (view.kind !== "guide" || !view.guide) return;
  try {
    guide = await api.invoke("guide_get", { id: view.guide });
  } catch {
    return renderGuides();
  }
  // Читают вслух — не перерисовывать под голосом: дорисуется, когда умолкнет.
  if (ui.main.querySelector(".reader-focus")) return;
  renderGuide();
}

function renderHome() {
  view = { kind: "home", topic: null, step: "lesson" };
  review = null;
  renderSide();
  if (!course) {
    // Курсов по умолчанию нет: курс по своей теме собирает нейросеть человека.
    const root = page(
      "Соберите свой курс",
      "Курс по любой теме собирает ваша нейросеть — Claude, ChatGPT, Cursor или другая с MCP. " +
        "Ноа проверит его и покажет здесь: уроки, задачи, экзамены и прогресс.",
    );
    const steps = el("ol", "list");
    // В браузере нет ни sufler.exe, ни «этого компьютера» — зато курс
    // собирается кнопкой прямо в Ноа онлайн.
    const inBrowser = document.documentElement.classList.contains("is-web");
    const texts = inBrowser
      ? [
          "В Ноа онлайн нажмите «Собрать курс» и напишите, о чём курс и зачем, — Ноа соберёт его сама, тема за темой.",
          "Или попросите свою нейросеть с MCP — адрес для неё на noahlab.ru в разделе «Подключить ИИ»: " +
            "«Собери мне курс Ноа по английскому для путешествий: пять тем, в каждой урок, задачи и мини-экзамен».",
          "Курс появится здесь сам — по первой теме уже можно учиться, остальные дорастают.",
        ]
      : [
          "Подключите нейросеть к Ноа: постоянная ссылка MCP — на noahlab.ru в разделе «Подключить ИИ»; " +
            "Claude Desktop и Cursor на этом компьютере подключаются командой sufler.exe --mcp.",
          "Попросите её, например: «Собери мне курс Ноа по английскому для путешествий: пять тем, " +
            "в каждой урок, задачи и мини-экзамен».",
          "Курс появится в этом окне сам. Дальше — «Ноа, погоняй меня по курсу» или «как мой прогресс».",
        ];
    // Не вошли в аккаунт — сначала вход: по нему придут курсы, прогресс и мост.
    if (!inBrowser) {
      const login = accountCard();
      root.append(login);
      api?.invoke("account_status").then((signed) => {
        if (signed) login.remove();
      }).catch(() => {});
    }
    // В программе (компьютер, телефон) — сборка прямо здесь, через мост.
    if (!inBrowser) root.append(courseBuilder(true));
    for (const text of texts) {
      steps.append(el("li", "", text));
    }
    root.append(steps);
    return;
  }
  // Обзор курса по хендоффу: заголовок и прогресс, «Дальше», путь курса,
  // плитки, слабые места. Карта и справка — свёрнуты внизу.
  const root = lhPage();
  const done = course.topics.filter((t) => t.status === "done").length;
  const head = el("div", "lh-course-head");
  head.append(el("span", "lh-h lh-course-head__title", course.title));
  if (course.description) head.append(el("span", "lh-course-head__about", course.description));
  const progress = el("div", "lh-course-head__progress");
  progress.append(
    el("span", "lh-course-head__pct", `${course.percent}%`),
    bar(course.percent),
    el("span", "lh-course-head__count", `${done} из ${course.topics.length} тем${course.finalBest != null ? ` · финал ${course.finalBest}%` : ""}`),
  );
  head.append(progress);
  if (course.building) {
    head.append(el("span", "lh-course-head__building", `Курс ещё собирается: готово ${course.building.done} из ${course.building.total} тем — новые появятся сами.`));
  }
  root.append(head);

  const next = nextTopic();
  root.append(nextCard(next));
  root.append(pathBlock(next));
  root.append(tilesRow(course));

  const weak = course.topics
    .map((topic, at) => ({ topic, at }))
    .filter(({ topic }) => topic.mistakes > 0)
    .sort((a, b) => b.topic.mistakes - a.topic.mistakes)
    .slice(0, 6);
  if (weak.length) {
    const box = el("section", "lh-block");
    const headRow = el("div", "lh-block__head lh-block__head--label");
    headRow.append(el("span", "lh-label", "Слабые места"), el("span", "lh-block__note", "где больше всего ошибок"));
    box.append(headRow);
    const table = el("div", "lh-table");
    for (const { topic, at } of weak) {
      const row = el("div", "lh-row lh-row--weak");
      const dots = el("span", "lh-dots");
      for (let i = 0; i < 5; i++) dots.append(el("span", i < topic.mistakes ? "is-on" : ""));
      const go = el("button", "lh-row__btn", `Разобрать · ${topic.mistakes}`);
      go.addEventListener("click", () => !busy && openTopic(topic.id, "mistakes"));
      row.append(el("span", "lh-row__n", String(at + 1).padStart(2, "0")), el("span", "lh-row__title", topic.title), dots, go);
      table.append(row);
    }
    box.append(table);
    root.append(box);
  }

  if (course.mastery?.total) {
    const map = el("details", "lh-more");
    map.append(el("summary", "", "Карта курса — темы и связи"));
    const holder = el("div", "map-holder");
    map.append(holder);
    let drawn = false;
    map.addEventListener("toggle", () => {
      if (map.open && !drawn) {
        drawn = true;
        drawCourseMap(holder);
      }
    });
    root.append(map);
  }

  const how = el("details", "lh-more");
  how.append(el("summary", "", "Как устроен курс"));
  const list = el("ul", "list");
  for (const text of [
    "В каждой теме: урок кусками → понятия → повторение → задачи → мини-экзамен. Экзамен сдан от " +
      `${course.topicPass}% — тема засчитана.`,
    "Повторение по расписанию: вспомнили — карточка вернётся через 1, 3, 8, 20 дней и дальше; " +
      "забыли — сегодня же. «Уверенно» — понятие держится три недели и дольше.",
    "Вопросы с вариантами проверяются сразу. Открытые ответы модель оценивает по смыслу: пункт, сказанный " +
      "своими словами или другой верной командой, засчитан, раскрытый наполовину — половиной.",
    "Под каждым разделом урока: «Разобрать подробно» — раздел по шагам, с примерами и ошибками; " +
      "«Обсудить» — вопросы о нём текстом или голосом.",
    `Финальный экзамен открывается после всех тем, проходной балл — ${course.finalPass}%.`,
    "Голосом: «Ноа, давай повторим», «погоняй меня по курсу», «как мой прогресс».",
  ]) {
    list.append(el("li", "", text));
  }
  how.append(list);
  root.append(how);
}

/** Тема, с которой продолжать: где остановились, иначе первая несданная. */
function nextTopic() {
  return course ? nextOf(course) : null;
}

/** «Дальше» 88 — текст · Начать 120 · урок с Ноа 40 · практика 40. */
function nextCard(next) {
  const card = el("section", "lh-active lh-next");
  const text = el("span", "lh-next__text");
  if (!next) {
    text.append(el("span", "lh-label", "// все темы сданы"), el("span", "lh-next__title", "Финальный экзамен"));
    card.append(text, lhButton("Начать", "play", () => !busy && openFinal(), "lh-btn--primary", 2.5));
    return card;
  }
  const at = course.topics.indexOf(next) + 1;
  const place = placeOf(next.id);
  const started = next.read || next.status !== "new" || place.section > 0;
  text.append(
    el("span", "lh-label", `// дальше · тема ${at}${started ? ` · раздел ${place.section + 1}` : ""}`),
    el("span", "lh-next__title", next.title),
  );
  text.title = next.title;
  const talk = lhButton("", "mic", () => !busy && openTopic(next.id, "talk"), "lh-btn--icon");
  talk.title = "Урок с Ноа";
  talk.setAttribute("aria-label", "Урок с Ноа");
  const practice = lhButton("", "window", () => !busy && openTopic(next.id, "practice"), "lh-btn--icon");
  practice.title = "Практика";
  practice.setAttribute("aria-label", "Практика");
  card.append(text, lhButton(started ? "Дальше" : "Начать", "play", () => !busy && resumeTopic(next.id), "lh-btn--primary", 2.5), talk, practice);
  return card;
}

/**
 * Путь курса: клетка на тему, заливка снизу — сколько темы пройдено, текущая
 * — в кольце. С одного взгляда видно, где ты и сколько осталось.
 */
function pathBlock(next) {
  const box = el("section", "lh-block");
  const head = el("div", "lh-block__head lh-block__head--label");
  const legend = (cls, text) => {
    const item = el("span", `lh-legend lh-legend--${cls}`);
    item.append(el("span"), document.createTextNode(text));
    return item;
  };
  head.append(el("span", "lh-label", "Путь курса"), legend("done", "сдано"), legend("now", "сейчас"), legend("rest", "впереди"));
  box.append(head);
  const path = el("div", "lh-path");
  path.style.setProperty("--cells", String(Math.min(24, Math.max(course.topics.length, 1))));
  course.topics.forEach((topic, at) => {
    const cell = el("button", `lh-cell${next && topic.id === next.id ? " is-now" : ""}`);
    const percent = topicPercent(topic);
    cell.title = `${at + 1}. ${topic.title} · ${percent}%`;
    cell.setAttribute("aria-label", cell.title);
    const fill = el("span");
    fill.style.height = `${topic.status === "done" ? 100 : percent}%`;
    cell.append(fill);
    cell.addEventListener("click", () => !busy && resumeTopic(topic.id));
    path.append(cell);
  });
  box.append(path);
  return box;
}

/**
 * Где в теме остановились: вкладка и раздел урока. Тему, которую ещё не
 * открывали, — с урока, прочитанную — с понятий.
 */
function placeOf(id) {
  const card = course?.topics.find((t) => t.id === id);
  return {
    // Без сохранённого места — разговором: так тема учится, а не читается.
    step: card?.step || "talk",
    section: card?.section ?? 0,
  };
}

/** Открывает тему там, где в ней остановились. */
function resumeTopic(id) {
  const place = placeOf(id);
  return openTopic(id, place.step, place.section);
}

async function openTopic(id, step = "lesson", section = 0) {
  if (!api || busy) return;
  try {
    topicView = await api.invoke("learn_topic", { course: course.id, topic: id });
  } catch (err) {
    page("Тема не открылась", String(err));
    return;
  }
  view = { kind: "topic", topic: id, step, section, whole: false };
  exam = null;
  review = null;
  renderTopic();
}

/** Запоминает место в теме — окно откроется на нём же. */
function savePlace() {
  if (view.kind !== "topic" || !course) return;
  const card = course.topics.find((t) => t.id === view.topic);
  const section = view.section ?? 0;
  if (card) {
    card.step = view.step;
    card.section = section;
  }
  api?.invoke("learn_place", { course: course.id, topic: view.topic, step: view.step, section }).catch(() => {});
}

function renderTopic() {
  renderSide();
  savePlace();
  const card = course.topics.find((t) => t.id === view.topic);
  const topic = topicView.topic;
  const root = page(topic.title, topic.summary);
  // Три раздела вместо девяти вкладок: учить, материал, проверить себя.
  const GROUPS = {
    talk: ["talk"],
    practice: ["practice"],
    material: ["lesson", "concepts", "map"],
    cards: ["review"],
    sheet: ["sheet"],
    check: ["tasks", "exam", "mistakes"],
  };
  const groupOf = (key) => Object.keys(GROUPS).find((g) => GROUPS[g].includes(key)) ?? "material";
  const current = groupOf(view.step);

  const steps = el("div", "steps");
  const step = (key, label, badge = "", into = steps, selected = view.step === key) => {
    const node = el("button", "step", label);
    node.setAttribute("aria-selected", String(selected));
    if (badge) node.append(el("span", "step__badge", badge));
    node.addEventListener("click", () => {
      view.step = key;
      exam = null;
      renderTopic();
    });
    into.append(node);
  };
  // Главный способ пройти тему — урок разговором с Ноа.
  step("talk", "🎙 Урок с Ноа", card?.read ? "✓" : "", steps, current === "talk");
  step("practice", "🖥 Практика", "", steps, current === "practice");
  step("lesson", "Материал", "", steps, current === "material");
  if (topic.concepts?.length) step("review", "🃏 Карточки", `${card?.conceptsMature ?? 0}/${topic.concepts.length}`, steps, current === "cards");
  if (topicView.cheatsheet) step("sheet", "📝 Конспект", "", steps, current === "sheet");
  step("tasks", "Проверить себя", card?.examBest != null ? `${card.examBest}%` : "", steps, current === "check");
  root.append(steps);

  // Внутри раздела — мелкая строка, только если в нём больше одного.
  const sub = el("div", "substeps");
  if (current === "material") {
    step("lesson", "Урок", "", sub);
    if (topic.concepts?.length) step("concepts", "Понятия", "", sub);
    if (topic.concepts?.length) step("map", "Карта", "", sub);
  } else if (current === "check") {
    step("tasks", "Задачи", `${card?.tasksDone ?? 0}/${card?.tasksTotal ?? 0}`, sub);
    step("exam", "Мини-экзамен", "", sub);
    if (topicView.mistakes.length) step("mistakes", "Ошибки", String(topicView.mistakes.length), sub);
  }
  if (sub.childElementCount > 1) root.append(sub);

  if (view.step === "talk") {
    renderLessonAgent(root, {
      api,
      course,
      topic,
      parts: sections(topic.lesson),
      start: view.section ?? 0,
      el,
      button,
      markdown,
      openStep: async (key) => {
        await refreshOverview();
        view.step = key;
        renderTopic();
      },
    });
  } else if (view.step === "practice") renderPractice(root, topic);
  else if (view.step === "lesson") renderLesson(root, topic);
  else if (view.step === "concepts") renderConcepts(root, topic);
  else if (view.step === "map") renderTopicMap(root, topic);
  else if (view.step === "review") startReview(topic.id);
  else if (view.step === "sheet") renderSheet(root);
  else if (view.step === "tasks") renderQuestions(root, topic.tasks, "Задача");
  else if (view.step === "mistakes") renderQuestions(root, topicView.mistakes, "Повтор");
  else renderExamIntro(root, view.topic, card?.examBest, course.topicPass);
}

/**
 * Практика в терминале: тема — руками на своём сервере, Ноа смотрит в
 * терминал и ведёт по шагам. Окно отдельное: терминалу нужно место.
 */
function renderPractice(root, topic) {
  const box = el("section", "practice-intro");
  box.append(
    el(
      "p",
      "",
      "Учитесь руками: живая задача по теме на вашем сервере. Вы работаете в терминале, Ноа его видит — объясняет, что происходит, замечает ошибки, подсказывает следующий шаг и показывает общую картину.",
    ),
  );
  if (document.documentElement.classList.contains("is-web")) {
    box.append(el("p", "muted", "Терминал браузеру недоступен — практика открывается в программе Ноа на компьютере."));
  } else {
    const failed = el("p", "muted");
    const open = button("🖥 Открыть практику по теме", () =>
      api?.invoke("practice_open", { course: course.id, topic: topic.id }).catch((err) => (failed.textContent = `Не открылось: ${err}`)),
    );
    box.append(open, failed);
    // У темы своя практика: начата — продолжается с того же шага.
    api
      ?.invoke("practice_progress")
      .then((list) => {
        const own = (list ?? []).find((p) => p.course === course.id && p.topic === topic.id);
        if (!own) return;
        open.textContent = own.done ? "🖥 Практика пройдена — открыть" : "🖥 Продолжить практику";
        failed.textContent = own.done ? "" : `Остановились на шаге ${own.step + 1} из ${own.total}.`;
      })
      .catch(() => {});
  }
  root.append(box);
}

/**
 * Урок по разделам «## …». Вступление до первого раздела идёт вместе с ним.
 */
function sections(lesson) {
  const parts = [];
  let current = [];
  for (const line of lesson.replace(/\r/g, "").split("\n")) {
    if (line.startsWith("## ") && current.some((l) => l.startsWith("## "))) {
      parts.push(current.join("\n"));
      current = [];
    }
    current.push(line);
  }
  if (current.join("").trim()) parts.push(current.join("\n"));
  return parts.length ? parts : [lesson];
}

/**
 * Понятие раздела: названное в его тексте и не спрошенное в других разделах.
 * У раздела оно одно и то же при возврате назад — вопрос не прыгает.
 *
 * Термин ищется целиком и по частям: «TCP и UDP» узнаётся по «TCP», «page
 * cache и available» — по «page cache».
 */
function conceptOf(section, at, topic) {
  const here = `${topic.id}#${at}`;
  const concepts = topic.concepts ?? [];
  if (recalled.has(here)) return concepts.find((c) => c.id === recalled.get(here));
  const asked = new Set(
    [...recalled].filter(([key]) => key.startsWith(`${topic.id}#`)).map(([, id]) => id),
  );
  const text = section.toLowerCase();
  let best = null;
  let bestLength = 0;
  for (const c of concepts) {
    if (asked.has(c.id)) continue;
    const term = c.term.toLowerCase();
    const parts = [term, ...term.split(/\s+и\s+|,\s*|\s*\(|\)/)].map((p) => p.trim()).filter((p) => p.length >= 3);
    const hit = parts.find((part) => text.includes(part));
    if (hit && hit.length > bestLength) {
      best = c;
      bestLength = hit.length;
    }
  }
  if (best) recalled.set(here, best.id);
  return best;
}

/**
 * Объяснено ли понятие в разделе: встречается ли в нём хотя бы половина
 * значимых слов определения. Так же проверяет Rust (`explained_in`).
 *
 * Упомянуть не значит объяснить: «маршруты объявляют по BGP» называет BGP, но
 * не говорит, что это. Спросить «что такое BGP» после такого раздела — спросить
 * то, чего человеку не рассказывали; такое понятие показывается как новое.
 */
function explainedIn(text, definition) {
  const words = (value) =>
    value
      .toLowerCase()
      .replace(/ё/g, "е")
      .split(/[^\p{L}\p{N}]+/u)
      .filter((word) => [...word].length >= 4)
      .map((word) => [...word].slice(0, 5).join(""));
  const known = new Set(words(text));
  const wanted = new Set(words(definition));
  if (!wanted.size) return true;
  let hits = 0;
  for (const word of wanted) if (known.has(word)) hits += 1;
  return hits * 2 >= wanted.size;
}

/**
 * Урок кусками. Раздел на экран и после него — одно понятие вспомнить.
 *
 * Прочитанное подряд создаёт ощущение, что всё понятно: текст знакомый, глаз
 * узнаёт. Проверяет его только попытка вспомнить без подсказки — и сделанная
 * сразу после раздела, она ещё и закрепляет его вдвое лучше перечитывания.
 */
function renderLesson(root, topic) {
  const parts = sections(topic.lesson);
  if (view.whole || parts.length < 2) {
    renderWholeLesson(root, topic);
    return;
  }
  const at = Math.min(view.section ?? 0, parts.length - 1);
  const head = el("div", "lesson__progress");
  head.append(el("span", "muted", `Раздел ${at + 1} из ${parts.length}`));
  const whole = el("button", "button button--quiet", "Весь урок целиком");
  whole.addEventListener("click", () => {
    view.whole = true;
    renderTopic();
  });
  head.append(whole);
  root.append(head);
  // Ролик по всей теме — над уроком; по разделу — под ним.
  root.append(videoBox({ api, el, button: (text, onClick) => button(text, onClick, true), course: course.id, topic: topic.id, section: null }));

  const lesson = el("div", "lesson");
  lesson.innerHTML = markdown(parts[at]);
  // Слушать раздел вместо чтения; дочитали — переход к следующему на виду.
  const reader = attachReader(lesson, {
    api,
    el,
    button,
    onFinish: () => root.querySelector(".actions .button:not(.button--quiet)")?.focus(),
  });
  root.append(reader);
  root.append(lesson);

  root.append(deepBox(topic, at));
  root.append(videoBox({ api, el, button: (text, onClick) => button(text, onClick, true), course: course.id, topic: topic.id, section: at }));

  const concept = conceptOf(parts[at], at, topic);
  if (concept) {
    root.append(explainedIn(parts[at], concept.definition) ? recallBox(topic, concept) : newConceptBox(concept));
  }

  const actions = el("div", "actions");
  if (at > 0) {
    actions.append(
      button("← Назад", () => {
        view.section = at - 1;
        renderTopic();
      }, true),
    );
  }
  if (at < parts.length - 1) {
    actions.append(
      button("Дальше →", () => {
        view.section = at + 1;
        renderTopic();
      }),
    );
  } else {
    actions.append(
      button(topic.concepts?.length ? "Прочитал — закрепить понятия" : "Прочитал — к задачам", async () => {
        await api?.invoke("learn_read", { course: course.id, topic: topic.id }).catch(() => {});
        await refreshOverview();
        view.section = 0;
        view.step = topic.concepts?.length ? "review" : "tasks";
        renderTopic();
      }),
    );
  }
  actions.append(discussButton(root, { course: course.id, topic: topic.id, section: at }));
  root.append(actions);
}

/** Понятие, которое раздел только называет: определение сразу, без вопроса. */
function newConceptBox(concept) {
  const box = el("div", "recall recall--new");
  box.append(el("div", "recall__label", "Новое понятие"));
  box.append(el("p", "recall__q", concept.term));
  box.append(el("p", "", concept.definition));
  if (concept.mnemonic) box.append(el("p", "hook", `🧠 ${concept.mnemonic}`));
  if (concept.analogy) box.append(el("p", "hook", `≈ ${concept.analogy}`));
  return box;
}

/**
 * «Разобрать подробно»: модель раскрывает раздел по шагам — механизм, кто что
 * делает, пример, типичные ошибки. Готовый разбор лежит на диске и
 * показывается сразу; новый пишется по щелчку, до минуты.
 */
function deepBox(topic, at) {
  const box = el("div", "deep");
  const body = el("div", "lesson deep__body");
  body.hidden = true;
  let loaded = false;
  // Разбор тоже можно слушать — так же, как урок, по абзацам.
  let reader = null;
  const showBody = (on) => {
    body.hidden = !on;
    if (reader) reader.hidden = !on;
  };
  const toggle = button("🔍 Разобрать подробно", () => {
    if (loaded) {
      showBody(body.hidden);
      toggle.textContent = body.hidden ? "🔍 Показать разбор" : "Свернуть разбор";
    } else {
      load(false);
    }
  }, true);
  const load = async (cachedOnly) => {
    if (!api) return;
    if (!cachedOnly) {
      toggle.disabled = true;
      toggle.textContent = "Разбираю раздел… (до минуты)";
    }
    try {
      const text = await api.invoke("learn_deep", { course: course.id, topic: topic.id, section: at, cached: cachedOnly });
      if (text) {
        loaded = true;
        body.innerHTML = markdown(text);
        reader?.remove();
        reader = attachReader(body, { api, el, button, label: "разбор", keys: false });
        box.insertBefore(reader, body);
        showBody(!cachedOnly);
        toggle.textContent = cachedOnly ? "🔍 Показать разбор" : "Свернуть разбор";
      }
    } catch (err) {
      toggle.textContent = "🔍 Разобрать ещё раз";
      body.replaceChildren(el("p", "bad", String(err)));
      body.hidden = false;
    } finally {
      toggle.disabled = false;
    }
  };
  box.append(toggle, body);
  load(true);
  return box;
}

/** Вспомнить понятие раздела: вопрос сразу, ответ — по щелчку. */
function recallBox(topic, concept) {
  const box = el("div", "recall");
  box.append(el("div", "recall__label", "Вспомни, не подглядывая"));
  box.append(el("p", "recall__q", `Что такое ${concept.term}?`));
  const answer = el("div", "recall__a");
  answer.hidden = true;
  answer.append(el("p", "", concept.definition));
  if (concept.mnemonic) answer.append(el("p", "hook", `🧠 ${concept.mnemonic}`));
  const show = button("Проверить себя", () => {
    answer.hidden = false;
    show.remove();
  }, true);
  box.append(show, answer);
  return box;
}

/** Открытая панель обсуждения: реплики, сказанные голосом, дописываются в неё. */
let talk = null;

/**
 * «Обсудить»: разговор о том, что на экране — разделе урока или вопросе.
 * Спросить можно текстом или голосом; это один разговор, и Ноа знает, о чём
 * он: раздел целиком с его понятиями или вопрос с эталоном и ответом.
 */
function discussButton(root, target) {
  return button("💬 Обсудить", () => {
    let panel = root.querySelector(":scope > .talk");
    if (!panel) {
      panel = talkPanel(target);
      root.append(panel);
    }
    panel.querySelector("textarea")?.focus();
    panel.scrollIntoView({ block: "nearest" });
  }, true);
}

/** Флажок в памяти браузера; по умолчанию — включён. */
function readFlag(key) {
  try {
    return localStorage.getItem(key) !== "off";
  } catch {
    return true;
  }
}

function writeFlag(key, on) {
  try {
    localStorage.setItem(key, on ? "on" : "off");
  } catch {
    /* не запомнится */
  }
}

function talkPanel(target) {
  const panel = el("div", "talk");
  const log = el("div", "talk__log");
  const hint = el(
    "p",
    "note",
    "Спросите о том, что на экране: «не понял, зачем тут iowait», «разбери первый пункт на примере». Enter — отправить.",
  );
  const area = el("textarea", "answer talk__input");
  area.rows = 2;
  area.placeholder = "Ваш вопрос";

  const line = (who, text) => {
    const node = el("div", `talk__line talk__line--${who}`);
    if (who === "noa") node.innerHTML = markdown(text);
    else node.textContent = text;
    log.append(node);
    node.scrollIntoView({ block: "nearest" });
    return node;
  };
  // Ответы обсуждения — и вслух, как у Ноа везде. Выключается кнопкой.
  let aloud = readFlag("noa.talkAloud");
  const aloudBtn = button("", () => {
    aloud = !aloud;
    writeFlag("noa.talkAloud", aloud);
    if (!aloud) api?.invoke("voice_stop").catch(() => {});
    paintAloud();
  }, true);
  const paintAloud = () => (aloudBtn.textContent = aloud ? "🔊 Вслух" : "🔇 Без голоса");
  paintAloud();
  const send = button("Отправить", async () => {
    const text = area.value.trim();
    if (!text || send.disabled || !api) return;
    area.value = "";
    api.invoke("voice_stop").catch(() => {});
    line("me", text);
    const wait = line("wait", "Ноа думает…");
    send.disabled = true;
    try {
      const reply = await api.invoke("learn_ask", { target, text });
      wait.remove();
      line("noa", reply);
      if (aloud) api.invoke("voice_speak", { text: reply }).catch(() => {});
    } catch (err) {
      wait.remove();
      line("error", String(err));
    } finally {
      send.disabled = false;
      area.focus();
    }
  });
  const voice = button("🎙 Голосом", async () => {
    try {
      await api?.invoke("learn_discuss", { target });
      hint.textContent = "Ноа слушает — спрашивай голосом. «Спроси меня» — вопрос по теме, «спасибо» — закончить.";
    } catch (err) {
      hint.textContent = String(err);
    }
  }, true);
  area.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      send.click();
    }
  });
  const actions = el("div", "actions talk__actions");
  actions.append(send, voice, aloudBtn);
  panel.append(log, area, actions, hint);
  talk = { panel, line };
  return panel;
}

// Курс дописывается — сборкой или нейросетью по MCP: новые темы появляются
// сами. Посреди экзамена и ответа окно не трогаем.
api?.listen("guides:changed", async () => {
  if (view.kind === "guide") return reloadGuide();
  if (view.kind !== "lobby" && view.kind !== "guides") return;
  await refreshGuides();
  const shelf = ui.main.querySelector('[data-el="guideShelf"]');
  if (shelf) fillGuideShelf(shelf);
  const rows = ui.main.querySelector('[data-el="guideRows"]');
  if (rows) fillGuideRows(rows);
  renderSide();
});

api?.listen("learn:changed", async () => {
  if (busy || exam) return;
  await refreshOverview();
  renderSide();
  if (view.kind === "home") renderHome();
});

// Сказанное голосом в обсуждении — в ту же ленту, что и напечатанное.
api?.listen("learn:talk", (event) => {
  if (!talk?.panel.isConnected) return;
  if (event.payload.q) talk.line("me", `🎙 ${event.payload.q}`);
  if (event.payload.a) talk.line("noa", event.payload.a);
});

function renderWholeLesson(root, topic) {
  const lesson = el("div", "lesson");
  lesson.innerHTML = markdown(topic.lesson);
  root.append(videoBox({ api, el, button: (text, onClick) => button(text, onClick, true), course: course.id, topic: topic.id, section: null }));
  root.append(attachReader(lesson, { api, el, button }));
  root.append(lesson);
  const actions = el("div", "actions");
  actions.append(discussButton(root, { course: course.id, topic: topic.id, section: 0 }));
  actions.append(
    button(topic.concepts?.length ? "Прочитал — закрепить понятия" : "Прочитал — к задачам", async () => {
      await api?.invoke("learn_read", { course: course.id, topic: topic.id }).catch(() => {});
      await refreshOverview();
      view.step = topic.concepts?.length ? "review" : "tasks";
      renderTopic();
    }),
  );
  if (sections(topic.lesson).length > 1) {
    actions.append(
      button("По разделам", () => {
        view.whole = false;
        renderTopic();
      }, true),
    );
  }
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
  const box = el("div", "answer-box");
  const area = el("textarea", "answer");
  area.placeholder = "Ваш ответ своими словами — как на собеседовании. Можно надиктовать.";
  box.append(area, dictateButton(area));
  return { node: box, read: () => area.value.trim() };
}

/** Запись идёт — вторая кнопка не начинает новую. */
let dictating = null;

/**
 * «Надиктовать»: первый щелчок начинает запись, второй — останавливает;
 * расшифровка дописывается в поле ответа.
 */
function dictateButton(area) {
  const node = button("🎙 Надиктовать", async () => {
    if (!api) return;
    if (dictating && dictating !== node) return;
    if (!dictating) {
      try {
        await api.invoke("learn_dictate_start");
        dictating = node;
        node.textContent = "■ Готово — распознать";
      } catch (err) {
        node.textContent = String(err);
      }
      return;
    }
    node.disabled = true;
    node.textContent = "Распознаю…";
    try {
      const text = await api.invoke("learn_dictate_stop");
      if (text) area.value = area.value ? `${area.value.trim()} ${text}` : text;
      node.textContent = "🎙 Надиктовать ещё";
    } catch (err) {
      node.textContent = `🎙 ${err}`;
    } finally {
      dictating = null;
      node.disabled = false;
    }
  }, true);
  node.classList.add("dictate");
  return node;
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
        place.replaceChildren(renderVerdict(verdict, q, true, answer));
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

/**
 * Разбор ответа: балл, отзыв, пункты эталона, сам эталон — и кнопка обсудить
 * вопрос голосом. `answer` — что ответил человек; без него обсуждать нечего.
 */
function renderVerdict(verdict, q, allowSelf, answer) {
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
      const partial = !covered && verdict.partial?.includes(at + 1);
      const [cls, mark] = covered ? ["covered", "✓"] : partial ? ["partial", "◐"] : ["missed", "○"];
      list.append(el("li", cls, `${mark} ${point}`));
    });
    box.append(list);
  }
  if (verdict.reference) {
    const details = el("details");
    details.append(el("summary", "", q.kind === "choice" ? "Почему так" : "Пример сильного ответа"));
    if (q.kind !== "choice") {
      details.append(el("p", "note", "Один из верных вариантов, а не единственный: засчитывается смысл, а не слова."));
    }
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
  if (answer !== undefined) {
    // Вариант уходит словами, а не номером: модели «ответил 2» ничего не скажет.
    const said =
      q.kind === "choice" && typeof answer === "number" ? (q.options?.[answer] ?? "") : String(answer ?? "");
    const actions = el("div", "actions");
    actions.append(discussButton(box, { course: course.id, question: q.id, answer: said }));
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
  actions.append(
    button("🎙 Сдать устно", async () => {
      try {
        await api?.invoke("learn_oral", { course: course.id, topic: scope === "final" ? null : scope });
        showNote(root, "Ноа задаёт вопросы вслух — отвечай голосом, без клавиш. «Не знаю» — скажет ответ, «хватит» — итог.");
      } catch (err) {
        showNote(root, String(err));
      }
    }, true),
  );
  root.append(actions);
}

/** Строка-подсказка под действиями страницы. */
function showNote(root, text) {
  let note = root.querySelector(".page-note");
  if (!note) {
    note = el("p", "note page-note");
    root.append(note);
  }
  note.textContent = text;
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

/* ── Запоминание ────────────────────────────────────────────────────────── */

const LEVELS = { new: "новое", learning: "учится", young: "держится", mature: "уверенно" };

/** Полоса усвоения: уверенно, держится, учится, новое — долями. */
function masteryBar(m) {
  const bar = el("div", "mastery");
  for (const key of ["mature", "young", "learning", "new"]) {
    const part = el("span", `mastery__part mastery__part--${key}`);
    part.style.flexGrow = String(m[key] ?? 0);
    part.title = `${LEVELS[key]}: ${m[key] ?? 0}`;
    bar.append(part);
  }
  return bar;
}

async function startReview(topicId) {
  if (!api) return;
  let queue = [];
  try {
    queue = await api.invoke("learn_review", { course: course.id, topic: topicId });
  } catch (err) {
    page("Повторение не открылось", String(err));
    return;
  }
  review = { queue, topic: topicId, done: 0, remembered: 0, revealed: false, last: "" };
  view = { kind: "review", topic: topicId, step: "review" };
  renderReview();
}

/**
 * Повторение: карточка, попытка вспомнить, ответ, честная оценка себя.
 *
 * Ответ открывается только по просьбе — вспоминание и есть упражнение, а
 * прочитать готовое и согласиться с ним ничего не даёт. Оценка честная, а не
 * «верно/неверно»: «с трудом» и «легко» дают разные интервалы, и от этой
 * честности зависит, придёт ли карточка вовремя.
 */
function renderReview() {
  renderSide();
  const topicTitle = review.topic ? course.topics.find((t) => t.id === review.topic)?.title : null;
  if (!review.queue.length) {
    const root = page(
      review.done ? "Готово" : "Повторять нечего",
      review.done
        ? `Вспомнили ${review.remembered} из ${review.done}. Забытое вернётся сегодня же, остальное — по расписанию.`
        : review.topic
          ? "По этой теме всё держится. Загляните завтра — или прочитайте урок следующей темы."
          : "Всё, что пора, повторено, а новые карточки приходят только из прочитанных тем.",
    );
    refreshOverview().then(() => {
      if (course.mastery?.total) root.append(masteryBar(course.mastery));
      renderSide();
    });
    const actions = el("div", "actions");
    if (review.topic) actions.append(button("Вернуться к теме", () => openTopic(review.topic, "concepts")));
    actions.append(button("К обзору курса", () => renderHome(), Boolean(review.topic)));
    root.append(actions);
    return;
  }

  const card = review.queue[0];
  const root = page(topicTitle ? `Повторение: ${topicTitle}` : "Повторение", `Осталось карточек: ${review.queue.length}`);
  if (review.topic) {
    const back = button("← К теме", () => openTopic(review.topic, "concepts"), true);
    back.classList.add("back");
    root.prepend(back);
  }
  const flash = el("div", "flash");
  const meta = el("div", "flash__meta");
  meta.append(
    el("span", "", card.topicTitle),
    el("span", `level level--${card.fresh ? "new" : card.level}`, card.fresh ? "новая" : LEVELS[card.level]),
  );
  flash.append(meta, el("div", "flash__front", card.front));

  if (!review.revealed) {
    flash.append(el("p", "flash__hint", "Вспомни ответ — вслух или про себя. Потом открой."));
    const show = button("Показать ответ · пробел", () => {
      review.revealed = true;
      renderReview();
    });
    const actions = el("div", "actions flash__actions");
    actions.append(show);
    flash.append(actions);
    root.append(flash);
    if (review.last) root.append(el("p", "muted", review.last));
    return;
  }

  flash.append(el("div", "flash__back", card.back));
  const extras = el("div", "flash__extras");
  if (card.mnemonic) extras.append(el("p", "hook", `🧠 ${card.mnemonic}`));
  if (card.analogy) extras.append(el("p", "hook", `≈ ${card.analogy}`));
  if (card.example) {
    const code = el("pre");
    code.append(el("code", "", card.example));
    extras.append(code);
  }
  if (card.pitfall) extras.append(el("p", "pitfall", `⚠ ${card.pitfall}`));
  if (extras.childNodes.length) flash.append(extras);

  const grades = el("div", "grades");
  for (const [grade, label, key] of [
    ["again", "Не вспомнил", "1"],
    ["hard", "С трудом", "2"],
    ["good", "Вспомнил", "3"],
    ["easy", "Легко", "4"],
  ]) {
    const node = el("button", `grade grade--${grade}`);
    node.append(el("span", "grade__label", label), el("span", "grade__key", key));
    node.addEventListener("click", () => gradeCard(grade));
    grades.append(node);
  }
  flash.append(grades);
  root.append(flash);
}

async function gradeCard(grade) {
  if (!review?.revealed || busy) return;
  const card = review.queue.shift();
  busy = true;
  try {
    const result = await api.invoke("learn_grade", { course: course.id, card: card.key, grade });
    review.done += 1;
    if (grade !== "again") review.remembered += 1;
    // Забытое возвращается в этот же сеанс — в конец очереди.
    if (result.again) review.queue.push({ ...card, fresh: false, level: "learning" });
    review.last = `«${card.front}» — ${result.again ? "вернётся в этом сеансе" : result.next}.`;
  } catch (err) {
    review.queue.unshift(card);
    review.last = String(err);
  } finally {
    busy = false;
  }
  review.revealed = false;
  renderReview();
}

document.addEventListener("keydown", (event) => {
  if (view.kind !== "review" || !review || !ui.focusLayer.hidden || event.target.closest?.("textarea, input")) return;
  if (event.code === "Space" && !review.revealed && review.queue.length) {
    event.preventDefault();
    review.revealed = true;
    renderReview();
    return;
  }
  const grade = { 1: "again", 2: "hard", 3: "good", 4: "easy" }[event.key];
  if (grade && review.revealed) {
    event.preventDefault();
    gradeCard(grade);
  }
});

/** Понятия темы: определение, зацепки, частая ошибка, связи. */
async function renderConcepts(root, topic) {
  let list = [];
  try {
    list = await api.invoke("learn_concepts", { course: course.id, topic: topic.id });
  } catch (err) {
    root.append(el("p", "bad", String(err)));
    return;
  }
  const actions = el("div", "actions");
  actions.append(button("Повторить понятия темы", () => startReview(topic.id)));
  root.append(actions);
  const grid = el("div", "concepts");
  for (const c of list) {
    const node = el("article", "concept");
    node.id = `concept-${c.id}`;
    const head = el("div", "concept__head");
    head.append(el("h3", "concept__term", c.term), el("span", `level level--${c.level}`, LEVELS[c.level]));
    node.append(head, el("p", "concept__def", c.definition));
    if (c.mnemonic) node.append(el("p", "hook", `🧠 ${c.mnemonic}`));
    if (c.analogy) node.append(el("p", "hook", `≈ ${c.analogy}`));
    if (c.example) {
      const code = el("pre");
      code.append(el("code", "", c.example));
      node.append(code);
    }
    if (c.pitfall) node.append(el("p", "pitfall", `⚠ ${c.pitfall}`));
    if (c.links?.length) {
      const links = el("div", "concept__links");
      for (const link of c.links) {
        const chip = el("button", `chip ${link.cross ? "chip--cross" : ""}`, link.term);
        if (link.cross) chip.title = course.topics.find((t) => t.id === link.topic)?.title ?? "";
        chip.addEventListener("click", async () => {
          if (link.topic !== topic.id) await openTopic(link.topic, "concepts");
          else renderTopic();
          setTimeout(() => {
            const target = document.getElementById(`concept-${link.concept}`);
            target?.scrollIntoView({ block: "center" });
            target?.classList.add("concept--flash");
          }, 80);
        });
        links.append(chip);
      }
      node.append(links);
    }
    grid.append(node);
  }
  root.append(grid);
}

function renderSheet(root) {
  const sheet = el("div", "lesson sheet");
  sheet.innerHTML = markdown(topicView.cheatsheet);
  root.append(sheet);
}

/* ── Карта ─────────────────────────────────────────────────────────────── */

const SVG = "http://www.w3.org/2000/svg";

function svg(tag, attrs = {}) {
  const node = document.createElementNS(SVG, tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, String(value));
  return node;
}

/**
 * Подпись в две строки, если длинная: узел карты не должен разрастаться.
 * `anchor` — start, middle или end: подписи по бокам карты уходят наружу от
 * узла и не наезжают на соседей.
 */
function label(parent, text, x, y, cls, anchor = "middle") {
  const words = text.split(" ");
  const lines = [];
  let line = "";
  for (const word of words) {
    if ((line + " " + word).trim().length > 16 && line) {
      lines.push(line);
      line = word;
    } else {
      line = `${line} ${word}`.trim();
    }
  }
  lines.push(line);
  if (lines.length > 3) {
    lines.length = 3;
    lines[2] = `${lines[2].replace(/[,:;]$/, "")}…`;
  }
  const node = svg("text", { x, y: y - (lines.length - 1) * 7, class: cls, "text-anchor": anchor });
  lines.forEach((part, at) => {
    const span = svg("tspan", { x, dy: at ? 14 : 0 });
    span.textContent = part;
    node.append(span);
  });
  parent.append(node);
}

/**
 * Карта темы: тема в центре, её понятия по кругу, снаружи — понятия других
 * тем, с которыми есть связь.
 *
 * Карта нужна не для красоты. Понятие, которое видно среди соседей, — это
 * уже не отдельный факт, а место в системе, и вспоминается оно по любой из
 * связей. Цвет узла — насколько понятие держится в памяти.
 */
async function renderTopicMap(root, topic) {
  let data;
  try {
    data = await api.invoke("learn_map", { course: course.id });
  } catch (err) {
    root.append(el("p", "bad", String(err)));
    return;
  }
  const own = data.topics.find((t) => t.id === topic.id);
  if (!own?.concepts.length) {
    root.append(el("p", "muted", "У темы нет понятий."));
    return;
  }
  const key = (t, c) => `${t}/${c}`;
  const mine = new Set(own.concepts.map((c) => key(topic.id, c.id)));
  const outer = new Map();
  for (const edge of data.edges) {
    const [a, b] = [edge.from, edge.to];
    if (mine.has(a) && !mine.has(b)) outer.set(b, a);
    if (mine.has(b) && !mine.has(a)) outer.set(a, b);
  }
  const find = (id) => {
    const [t, c] = id.split("/");
    const tp = data.topics.find((x) => x.id === t);
    return { topic: tp, concept: tp?.concepts.find((x) => x.id === c) };
  };

  // Кольца растут с числом узлов: на узел по дуге нужно место под подпись.
  const inner = Math.max(170, own.concepts.length * 19);
  const ring = Math.max(inner + 150, outer.size * 15);
  const width = 2 * ring + 260;
  const height = 2 * ring * 0.78 + 120;
  const cx = width / 2;
  const cy = height / 2;
  const place = new Map();
  own.concepts.forEach((c, at) => {
    const angle = (at / own.concepts.length) * Math.PI * 2 - Math.PI / 2;
    place.set(key(topic.id, c.id), { x: cx + Math.cos(angle) * inner, y: cy + Math.sin(angle) * inner * 0.8, angle });
  });
  // Понятие другой темы ставится напротив своего соседа из этой: линии идут
  // наружу, а не через всю карту. Порядок по углу соседа, шаг — ровный.
  const around = [...outer.entries()]
    .map(([id, partner]) => ({ id, want: place.get(partner).angle }))
    .sort((a, b) => a.want - b.want);
  around.forEach(({ id, want }, at, all) => {
    const even = (at / all.length) * Math.PI * 2 - Math.PI / 2;
    const angle = all.length < 8 ? want : (want + even) / 2;
    place.set(id, { x: cx + Math.cos(angle) * ring, y: cy + Math.sin(angle) * ring * 0.78, angle });
  });

  const map = svg("svg", { viewBox: `0 0 ${width} ${height}`, class: "map", role: "img" });
  map.style.setProperty("--map-min", `${Math.round(width * 0.6)}px`);
  for (const edge of data.edges) {
    const a = place.get(edge.from);
    const b = place.get(edge.to);
    if (!a || !b) continue;
    map.append(svg("line", { x1: a.x, y1: a.y, x2: b.x, y2: b.y, class: `map__edge ${edge.cross ? "map__edge--cross" : ""}` }));
  }
  for (const c of own.concepts) {
    const at = place.get(key(topic.id, c.id));
    map.append(svg("line", { x1: cx, y1: cy, x2: at.x, y2: at.y, class: "map__spoke" }));
  }
  map.append(svg("circle", { cx, cy, r: 46, class: "map__hub" }));
  label(map, topic.title, cx, cy + 4, "map__hub-label");

  for (const [id, pos] of place) {
    const { topic: t, concept: c } = find(id);
    if (!c) continue;
    const foreign = t.id !== topic.id;
    const group = svg("g", { class: `map__node map__node--${c.level} ${foreign ? "map__node--foreign" : ""}`, tabindex: 0 });
    group.append(svg("circle", { cx: pos.x, cy: pos.y, r: foreign ? 7 : 10 }));
    placeLabel(group, c.term, pos, foreign ? 7 : 10);
    const title = svg("title");
    title.textContent = foreign ? `${c.term} — из темы «${t.title}»` : `${c.term} — ${LEVELS[c.level]}`;
    group.append(title);
    group.addEventListener("click", async () => {
      await openTopic(t.id, "concepts");
      setTimeout(() => document.getElementById(`concept-${c.id}`)?.scrollIntoView({ block: "center" }), 80);
    });
    map.append(group);
  }
  const holder = el("div", "map-holder");
  holder.append(map);
  root.append(holder);
  const legend = el("div", "mastery__legend");
  legend.append(
    el("span", "legend legend--mature", "уверенно"),
    el("span", "legend legend--young", "держится"),
    el("span", "legend legend--learning", "учится"),
    el("span", "legend legend--new", "новое"),
    el("span", "legend legend--cross", "пунктир — связь с другой темой"),
  );
  root.append(legend);
}

/** Подпись узла снаружи от центра: справа — влево выровнена, слева — вправо. */
function placeLabel(group, text, pos, r) {
  const cos = Math.cos(pos.angle);
  const sin = Math.sin(pos.angle);
  if (Math.abs(cos) < 0.35) {
    label(group, text, pos.x, pos.y + (sin > 0 ? r + 16 : -r - 10), "map__label");
  } else {
    label(group, text, pos.x + (cos > 0 ? r + 6 : -r - 6), pos.y + 4, "map__label", cos > 0 ? "start" : "end");
  }
}

/** Карта курса: темы по кругу, толщина связи — сколько понятий их связывает. */
async function drawCourseMap(holder) {
  let data;
  try {
    data = await api.invoke("learn_map", { course: course.id });
  } catch {
    return;
  }
  const topics = data.topics.filter((t) => t.concepts.length);
  // Много тем — два кольца вперемешку: по одному кругу подписи наезжают.
  const rings = topics.length > 12 ? 2 : 1;
  const rx = rings === 2 ? 400 : 290;
  const ry = rings === 2 ? 290 : 180;
  const width = 2 * rx + 300;
  const height = 2 * ry + 140;
  const cx = width / 2;
  const cy = height / 2;
  const place = new Map();
  topics.forEach((t, at) => {
    const angle = (at / topics.length) * Math.PI * 2 - Math.PI / 2;
    const k = rings === 2 && at % 2 ? 0.62 : 1;
    place.set(t.id, { x: cx + Math.cos(angle) * rx * k, y: cy + Math.sin(angle) * ry * k, angle });
  });
  const weights = new Map();
  for (const edge of data.edges) {
    if (!edge.cross) continue;
    const pair = [edge.from.split("/")[0], edge.to.split("/")[0]].sort().join("|");
    weights.set(pair, (weights.get(pair) ?? 0) + 1);
  }
  const map = svg("svg", { viewBox: `0 0 ${width} ${height}`, class: "map", role: "img" });
  map.style.setProperty("--map-min", `${Math.round(width * 0.6)}px`);
  for (const [pair, weight] of weights) {
    const [a, b] = pair.split("|").map((id) => place.get(id));
    if (!a || !b) continue;
    map.append(
      svg("line", {
        x1: a.x,
        y1: a.y,
        x2: b.x,
        y2: b.y,
        class: "map__edge",
        "stroke-width": Math.min(1 + weight, 6),
        style: `stroke-opacity: ${Math.min(0.2 + weight * 0.15, 0.8)}`,
      }),
    );
  }
  for (const t of topics) {
    const pos = place.get(t.id);
    const mature = t.concepts.filter((c) => c.level === "mature").length;
    const share = mature / t.concepts.length;
    const level = share >= 0.8 ? "mature" : share >= 0.4 ? "young" : t.concepts.some((c) => c.level !== "new") ? "learning" : "new";
    const group = svg("g", { class: `map__node map__node--${level}`, tabindex: 0 });
    const r = 8 + Math.min(t.concepts.length, 16) * 0.75;
    group.append(svg("circle", { cx: pos.x, cy: pos.y, r }));
    placeLabel(group, t.title, pos, r);
    const title = svg("title");
    title.textContent = `${t.title}: уверенно ${mature} из ${t.concepts.length}`;
    group.append(title);
    group.addEventListener("click", () => openTopic(t.id, "map"));
    map.append(group);
  }
  holder.replaceChildren(map);
}

/* ── Фокус-сессия ──────────────────────────────────────────────────────── */

/*
 * Отрезок сосредоточенной работы и перерыв после него — «помидор», но с
 * двумя вещами, без которых он просто таймер:
 *
 * - цель в начале: «разобраться с Engine API» держит внимание лучше, чем
 *   «позаниматься»; расплывчатая цель — первый повод отвлечься;
 * - выгрузка в конце: записать по памяти, что понял, не подглядывая. Это то
 *   же вспоминание, что и в карточках, и оно закрепляет прочитанное сильнее
 *   перечитывания.
 *
 * Мысль, пришедшую посреди отрезка («надо ответить в чат»), не гонят, а
 * записывают «на потом» — одной строкой, и она перестаёт звать.
 *
 * Время — по часам, а не по счёту тиков: скрытое окно тикает реже, а отрезок
 * всё равно кончится вовремя. Состояние лежит в localStorage, чтобы таймер
 * пережил закрытие окна.
 */

const MODES = [
  { work: 15, rest: 3, name: "Разогрев", hint: "когда трудно начать" },
  { work: 25, rest: 5, name: "Классика", hint: "для большинства тем" },
  { work: 50, rest: 10, name: "Глубокая работа", hint: "когда тема затянула" },
];
const FOCUS_KEY = "sufler.learning.focus";
/** Каждый четвёртый перерыв — длинный. */
const LONG_EVERY = 4;

let focus = loadFocus();
let focusMode = 1;

function loadFocus() {
  try {
    return JSON.parse(localStorage.getItem(FOCUS_KEY) ?? "null");
  } catch {
    return null;
  }
}

function saveFocus() {
  try {
    if (focus) localStorage.setItem(FOCUS_KEY, JSON.stringify(focus));
    else localStorage.removeItem(FOCUS_KEY);
  } catch {
    /* таймер просто не переживёт закрытие окна */
  }
}

function clock(ms) {
  const total = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

function focusLeft() {
  if (!focus) return 0;
  return focus.pausedLeft ?? focus.endsAt - Date.now();
}

/** Тихий двойной звон: слышно, но не пугает. */
function chime() {
  try {
    const ctx = new AudioContext();
    [0, 0.28].forEach((delay, at) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.frequency.value = at ? 880 : 660;
      gain.gain.setValueAtTime(0.0001, ctx.currentTime + delay);
      gain.gain.exponentialRampToValueAtTime(0.18, ctx.currentTime + delay + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + delay + 0.9);
      osc.connect(gain).connect(ctx.destination);
      osc.start(ctx.currentTime + delay);
      osc.stop(ctx.currentTime + delay + 1);
    });
    setTimeout(() => ctx.close(), 1600);
  } catch {
    /* без звука — голос Ноа и окно всё равно скажут */
  }
}

function bell(text) {
  chime();
  api?.invoke("learn_focus_bell", { text }).catch(() => {});
}

function layer(...children) {
  const card = el("div", "focus-card");
  card.append(...children);
  ui.focusLayer.replaceChildren(card);
  ui.focusLayer.hidden = false;
  card.querySelector("textarea, input:not([type=checkbox])")?.focus();
  return card;
}

function closeLayer() {
  ui.focusLayer.hidden = true;
  ui.focusLayer.replaceChildren();
}

/** Подготовка: длина отрезка, цель, минута на то, чтобы убрать отвлечения. */
function openFocusSetup() {
  const modes = el("div", "focus-modes");
  const pick = (at) => {
    focusMode = at;
    modes.querySelectorAll(".focus-mode").forEach((node, index) => node.setAttribute("aria-pressed", String(index === at)));
  };
  MODES.forEach((mode, at) => {
    const node = el("button", "focus-mode");
    node.append(
      el("span", "focus-mode__time", `${mode.work} / ${mode.rest}`),
      el("span", "focus-mode__name", `${mode.name} — ${mode.hint}`),
    );
    node.addEventListener("click", () => pick(at));
    modes.append(node);
  });
  pick(focusMode);

  const goal = el("input", "focus-goal");
  goal.placeholder = "Например: понять Engine API и пройти карточки темы";
  goal.value = focus?.goal ?? "";
  const checks = el("div");
  for (const text of [
    "Телефон — в другую комнату или экраном вниз, без звука",
    "Мессенджеры и лишние вкладки закрыты; в Windows — «Не беспокоить» (Win+N)",
    "Вода рядом — чтобы не вставать посреди отрезка",
    "Первые две минуты — просто открыть тему: дальше пойдёт само",
  ]) {
    const row = el("label", "focus-check");
    const box = el("input");
    box.type = "checkbox";
    row.append(box, el("span", "", text));
    checks.append(row);
  }
  const actions = el("div", "actions");
  actions.append(button("Начать", () => startFocus(goal.value.trim())), button("Отмена", closeLayer, true));
  goal.addEventListener("keydown", (event) => {
    if (event.key === "Enter") startFocus(goal.value.trim());
  });
  layer(
    el("h2", "", "Фокус-сессия"),
    el("p", "muted", "Отрезок работы без отвлечений, потом перерыв. В конце — записать по памяти, что понял."),
    modes,
    el("p", "", "Цель на отрезок — одна, конкретная:"),
    goal,
    el("p", "muted", "Минута подготовки:"),
    checks,
    actions,
  );
}

function startFocus(goal) {
  const mode = MODES[focusMode];
  focus = {
    course: course?.id,
    phase: "work",
    work: mode.work,
    rest: mode.rest,
    goal,
    round: (focus?.round ?? 0) + 1,
    parked: focus?.parked ?? [],
    endsAt: Date.now() + mode.work * 60_000,
    pausedLeft: null,
  };
  saveFocus();
  closeLayer();
  renderChip();
  if (view.kind === "home") renderHome();
}

function renderChip() {
  ui.focusChip.hidden = !focus;
  if (!focus) return;
  ui.focusChip.dataset.phase = focus.phase;
  ui.focusChip.dataset.paused = String(focus.pausedLeft != null && focus.phase !== "recall" && focus.phase !== "ready");
  const face = { work: "🎯", break: "☕", recall: "✎", ready: "▶" }[focus.phase] ?? "🎯";
  ui.focusChip.textContent =
    focus.phase === "recall" ? `${face} выгрузка` : focus.phase === "ready" ? `${face} дальше?` : `${face} ${clock(focusLeft())}`;
}

/** Меню таймера: сколько осталось, цель, мысль на потом, пауза, стоп. */
function openFocusMenu() {
  if (!focus) return;
  const paused = focus.pausedLeft != null;
  const note = el("input", "focus-goal");
  note.placeholder = "Мысль на потом — запишите и вернитесь к теме";
  const parked = el("ul", "focus-parked");
  const drawParked = () => parked.replaceChildren(...focus.parked.map((text) => el("li", "", text)));
  drawParked();
  const park = () => {
    const text = note.value.trim();
    if (!text) return;
    focus.parked.push(text);
    saveFocus();
    note.value = "";
    drawParked();
  };
  note.addEventListener("keydown", (event) => {
    if (event.key === "Enter") park();
  });
  const actions = el("div", "actions");
  actions.append(
    button("Вернуться к занятию", closeLayer),
    button(
      paused ? "Продолжить" : "Пауза",
      () => {
        if (focus.pausedLeft != null) {
          focus.endsAt = Date.now() + focus.pausedLeft;
          focus.pausedLeft = null;
        } else {
          focus.pausedLeft = focus.endsAt - Date.now();
        }
        saveFocus();
        renderChip();
        closeLayer();
      },
      true,
    ),
    button("Закончить отрезок", () => finishWork(true), true),
    button("Выйти из фокуса", stopFocus, true),
  );
  layer(
    el("h2", "", `Фокус · осталось ${clock(focusLeft())}`),
    el("p", "", focus.goal ? `Цель: ${focus.goal}` : "Цель не задана."),
    note,
    parked,
    actions,
  );
}

/** Отрезок кончился: выгрузка по памяти и перерыв. */
function finishWork(early = false) {
  if (focus.phase === "work") {
    focus.minutes = early ? Math.max(0, Math.round((focus.work * 60_000 - focusLeft()) / 60_000)) : focus.work;
    focus.phase = "recall";
    focus.pausedLeft = 0;
    saveFocus();
    renderChip();
    if (!early) bell(`${focus.minutes} минут фокуса позади. Запиши по памяти, что понял, — и перерыв.`);
  }
  const minutes = focus.minutes ?? focus.work;
  const rest = focus.round % LONG_EVERY === 0 ? focus.rest * 3 : focus.rest;
  const recall = el("textarea", "answer");
  recall.placeholder = "Своими словами, не подглядывая: главное, термины, что осталось непонятным";
  const actions = el("div", "actions");
  const save = async (breakToo) => {
    try {
      const stats = await api.invoke("learn_focus_done", {
        course: focus.course ?? course.id,
        session: { minutes, goal: focus.goal ?? "", recall: recall.value, parked: focus.parked ?? [] },
      });
      if (course) course.focus = stats;
    } catch (err) {
      console.warn("фокус не записан", err);
    }
    focus.parked = [];
    if (breakToo) {
      focus.phase = "break";
      focus.endsAt = Date.now() + rest * 60_000;
      focus.pausedLeft = null;
      saveFocus();
      renderChip();
      showBreak();
    } else {
      stopFocus();
    }
  };
  actions.append(
    button(`Записать и на перерыв ${rest} мин`, () => save(true)),
    button("Записать и закончить", () => save(false), true),
  );
  const parts = [
    el("h2", "", "Выгрузка"),
    el("p", "muted", "Минута на то, чтобы вспомнить прочитанное без подсказок, закрепляет его сильнее, чем ещё полчаса чтения."),
  ];
  if (focus.goal) parts.push(el("p", "", `Цель была: ${focus.goal}. Получилось?`));
  parts.push(recall);
  if (focus.parked?.length) {
    parts.push(el("p", "muted", "Отложено на потом — теперь можно этим заняться:"));
    const list = el("ul", "focus-parked");
    list.append(...focus.parked.map((text) => el("li", "", text)));
    parts.push(list);
  }
  parts.push(actions);
  layer(...parts);
}

/** Перерыв: часы и что делать, чтобы он отдыхом и был. */
function showBreak() {
  const face = el("div", "focus-clock", clock(focusLeft()));
  face.dataset.focusClock = "";
  const tips = el("ul", "list");
  for (const text of [
    "Встаньте и пройдитесь — мозгу нужен кровоток, а не лента новостей.",
    "Глаза: 20 секунд смотрите вдаль, метров на шесть и дальше.",
    "Вода, окно, пара глубоких вдохов.",
    "Не телефон: новое в перерыве вытесняет только что выученное.",
  ]) {
    tips.append(el("li", "", text));
  }
  const actions = el("div", "actions");
  actions.append(
    button("Перерыв в фоне", closeLayer, true),
    button("Закончить перерыв", finishBreak, true),
    button("Выйти из фокуса", stopFocus, true),
  );
  layer(el("h2", "", "Перерыв"), face, tips, actions);
}

/** Перерыв кончился: следующий отрезок или хватит. */
function finishBreak() {
  if (focus.phase === "break") {
    focus.phase = "ready";
    focus.pausedLeft = 0;
    saveFocus();
    renderChip();
    bell("Перерыв окончен. Продолжим?");
  }
  const goal = el("input", "focus-goal");
  goal.value = focus.goal ?? "";
  goal.placeholder = "Цель следующего отрезка";
  const actions = el("div", "actions");
  actions.append(
    button("Следующий отрезок", () => startFocus(goal.value.trim())),
    button("На сегодня хватит", stopFocus, true),
  );
  layer(
    el("h2", "", "Перерыв окончен"),
    el("p", "", `Отрезков сегодня: ${course?.focus?.sessionsToday ?? focus.round}. Цель следующего — та же или новая?`),
    goal,
    actions,
  );
}

function stopFocus() {
  focus = null;
  saveFocus();
  closeLayer();
  renderChip();
  refreshOverview().then(() => {
    if (view.kind === "home") renderHome();
  });
}

function tickFocus() {
  if (!focus || focus.pausedLeft != null) {
    renderChip();
    return;
  }
  const left = focusLeft();
  if (left > 0) {
    renderChip();
    const face = ui.focusLayer.querySelector("[data-focus-clock]");
    if (face) face.textContent = clock(left);
    return;
  }
  if (focus.phase === "work") finishWork();
  else if (focus.phase === "break") finishBreak();
}

ui.focusChip.addEventListener("click", () => {
  if (!focus) return;
  if (focus.phase === "recall") finishWork(true);
  else if (focus.phase === "ready") finishBreak();
  else if (focus.phase === "break") showBreak();
  else openFocusMenu();
});
setInterval(tickFocus, 1000);

/* ── Данные ────────────────────────────────────────────────────────────── */


/** Курс, выбранный в прошлый раз. Ссылка с ?course= важнее. */
function lastCourse() {
  if (new URLSearchParams(location.search).get("course")) return null;
  try {
    return localStorage.getItem("noa.lastCourse");
  } catch {
    return null;
  }
}

async function refreshOverview() {
  if (!api) return;
  try {
    courses = (await api.invoke("learn_overview")) ?? [];
    course = courses.find((c) => c.id === course?.id) ?? courses.find((c) => c.id === lastCourse()) ?? courses[0] ?? null;
  } catch {
    /* окно покажет прежнее */
  }
}


/* ── Окно ──────────────────────────────────────────────────────────────── */

const win = appWindow();
// Свернуть — окно уходит на панель задач и возвращается оттуда или из трея.
ui.minimize?.addEventListener("click", () => win?.minimize());
ui.head.addEventListener("pointerdown", (event) => {
  if (event.button !== 0 || event.target.closest("button")) return;
  event.preventDefault();
  win?.startDragging();
});

// Esc в поле ответа — выйти из поля.
document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  // Окно Esc не закрывает: Esc — это «замолчи» для голоса Ноа, и нажатый
  // под её речь, он уносил бы вместе с речью и окно, в котором человек
  // работает. Закрывает Esc только окно объяснения выделенного слова.
  if (document.activeElement?.matches("textarea, input")) document.activeElement.blur();
});

ui.close.addEventListener("click", () => {
  api?.invoke("close_learning").catch(() => {});
});

// Тема окна обучения — «Неон», всегда: по ней свёрстаны главная и обзор.
// Программа ставит тему из настроек при создании окна и при догрузке —
// здесь она возвращается.
{
  const root = document.documentElement;
  const neon = () => {
    if (root.dataset.theme !== "neon") root.dataset.theme = "neon";
    if (root.dataset.scale !== "window-l") root.dataset.scale = "window-l";
  };
  neon();
  new MutationObserver(neon).observe(root, { attributes: true, attributeFilter: ["data-theme", "data-scale"] });
}
// Окно открыли голосом на другой теме — показать её.
document.addEventListener("visibilitychange", async () => {
  if (document.visibilityState !== "visible" || busy || view.kind === "final" || exam) return;
  await refreshOverview();
  renderSide();
});

// Голосом: «собери путеводитель про…», «открой путеводители».
api?.listen("guides:open", (event) => {
  if (busy) return;
  api.invoke("guide_wanted").catch(() => null);
  if (event.payload) openGuide(event.payload);
  else renderLobby();
});

/** Голос заказал ролик — урок открывается на разделе, где виден ход сборки. */
async function openVideoPlace(wanted) {
  const other = courses.find((c) => c.id === wanted.course);
  if (!other) return false;
  chooseCourse(other);
  // Ролик по теме виден над первым разделом, по разделу — под ним.
  await openTopic(wanted.topic, "lesson", wanted.section ?? 0);
  return true;
}

// Голосом: «сделай видео по этому разделу», «ролик про сервисы».
api?.listen("learn:video", async (event) => {
  if (busy || !event.payload) return;
  api.invoke("learn_video_wanted").catch(() => null);
  await refreshOverview();
  openVideoPlace(event.payload);
});

refreshOverview().then(async () => {
  const video = await api?.invoke("learn_video_wanted").catch(() => null);
  if (video && (await openVideoPlace(video))) return;
  const wanted = await api?.invoke("guide_wanted").catch(() => null);
  if (wanted != null) return wanted ? openGuide(wanted) : renderLobby();
  // Окно открывается на главной обучения: там «Продолжить» одной кнопкой.
  renderLobby();
});
