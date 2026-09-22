// Окно задач: список, добавление, отметка, перенос, шаги, правка.
//
// Список хранится в Rust, здесь только показ. После каждого изменения окно
// перечитывает его целиком: задач десятки, а не тысячи, и точечное обновление
// разметки стоило бы дороже, чем перерисовка, — зато список нельзя рассинхронить
// с тем, что на диске.
//
// Всё, что можно сделать с задачей голосом, можно сделать и здесь руками:
// перенести, разбить на шаги, убрать лишний шаг, переименовать. Окно, в
// котором видно, но ничего не поправить, заставляло звать Ноа ради щелчка.

import { tauri, appWindow, applyTheme } from "./bridge.js";

const api = tauri();
const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

/** Задача, у которой сейчас открыт выбор нового срока. */
let postponing = null;
/** Задача, у которой сейчас открыто поле нового шага. */
let addingStep = null;

/* ── Группы ────────────────────────────────────────────────────────────── */

/** Порядок групп сверху вниз и правила, по которым задача в них попадает. */
const GROUPS = [
  { name: "Просрочено", modifier: "overdue", fits: (task) => task.overdue },
  { name: "Сегодня", modifier: "today", fits: (task) => isToday(task.due) },
  { name: "Дальше", modifier: "later", fits: (task) => Boolean(task.due) },
  { name: "Когда-нибудь", modifier: "someday", fits: () => true },
];

function isToday(due) {
  if (!due) return false;
  const when = new Date(due);
  const now = new Date();
  return (
    when.getFullYear() === now.getFullYear() &&
    when.getMonth() === now.getMonth() &&
    when.getDate() === now.getDate()
  );
}

/**
 * Срок словами.
 *
 * Полная дата у задачи на сегодня — лишняя работа для глаза: человек и так
 * знает, какое сегодня число. Поэтому у ближних сроков остаётся только то,
 * что их различает.
 */
function dueLabel(due) {
  if (!due) return "";

  const when = new Date(due);
  const time = when.toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" });
  const days = Math.round((startOfDay(when) - startOfDay(new Date())) / 86_400_000);

  if (days === 0) return `сегодня в ${time}`;
  if (days === 1) return `завтра в ${time}`;
  if (days === -1) return `вчера в ${time}`;
  if (days > 1 && days < 7) {
    const weekday = when.toLocaleDateString("ru-RU", { weekday: "long" });
    return `${weekday}, ${time}`;
  }

  const date = when.toLocaleDateString("ru-RU", { day: "numeric", month: "long" });
  return `${date}, ${time}`;
}

function startOfDay(date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

/* ── Перенос ───────────────────────────────────────────────────────────── */

/**
 * Готовые сроки для переноса.
 *
 * Переносят почти всегда на одно из четырёх: «через час», «вечером», «завтра
 * утром», «на следующей неделе». Выбирать это в календаре — пять щелчков
 * вместо одного. Для остального есть поле с датой.
 */
function presets() {
  const now = new Date();
  const at = (days, hours, minutes = 0) => {
    const when = new Date(now);
    when.setDate(when.getDate() + days);
    when.setHours(hours, minutes, 0, 0);
    return when;
  };

  const evening = at(0, 19);
  const eveningLabel = evening > now ? "Вечером" : "Завтра вечером";
  if (evening <= now) evening.setDate(evening.getDate() + 1);

  return [
    ["Через час", new Date(now.getTime() + 60 * 60 * 1000)],
    [eveningLabel, evening],
    ["Завтра утром", at(1, 10)],
    ["Через неделю", at(7, 10)],
  ];
}

/** Новый срок: первому сроку — правка, остальным — перенос, он считается. */
async function reschedule(task, when) {
  const due = when.toISOString();
  if (task.due) {
    await api?.invoke("task_postpone", { id: task.id, due });
  } else {
    await api?.invoke("task_edit", { id: task.id, due });
  }
  postponing = null;
  refresh();
}

function renderPostpone(task) {
  const bar = document.createElement("div");
  bar.className = "later";

  for (const [label, when] of presets()) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "later__option";
    button.textContent = label;
    button.title = when.toLocaleString("ru-RU", {
      weekday: "short",
      day: "numeric",
      month: "short",
      hour: "2-digit",
      minute: "2-digit",
    });
    button.addEventListener("click", () => reschedule(task, when));
    bar.append(button);
  }

  const pick = document.createElement("input");
  pick.type = "datetime-local";
  pick.className = "later__pick";
  pick.setAttribute("aria-label", "Свой срок");
  pick.addEventListener("change", () => {
    const when = new Date(pick.value);
    if (!Number.isNaN(when.getTime())) reschedule(task, when);
  });
  bar.append(pick);

  if (task.due) {
    const clear = document.createElement("button");
    clear.type = "button";
    clear.className = "later__option later__option--quiet";
    clear.textContent = "Без срока";
    clear.addEventListener("click", async () => {
      await api?.invoke("task_edit", { id: task.id, due: "" });
      postponing = null;
      refresh();
    });
    bar.append(clear);
  }

  return bar;
}

/* ── Показ ─────────────────────────────────────────────────────────────── */

async function refresh() {
  const tasks = (await api?.invoke("task_list")) ?? [];

  const undone = tasks.filter((task) => !task.done);
  const done = tasks.filter((task) => task.done);

  // Сделанное уходит вниз одной группой: оно нужно как след работы, а не как
  // список дел, и разбивать его по срокам незачем.
  const buckets = GROUPS.map((group) => ({ ...group, tasks: [] }));
  for (const task of sorted(undone)) {
    const bucket = buckets.find((group) => group.fits(task));
    bucket.tasks.push(task);
  }

  ui.list.replaceChildren();
  for (const bucket of buckets) {
    if (bucket.tasks.length) ui.list.append(renderGroup(bucket.name, bucket.modifier, bucket.tasks));
  }
  if (done.length) {
    ui.list.append(renderGroup("Сделано", "done", sorted(done).reverse(), clearDoneButton()));
  }

  const pending = undone.length;
  ui.count.textContent = pending ? `${pending} ${plural(pending)}` : "всё сделано";
  ui.empty.hidden = tasks.length > 0;
}

/** Ближайший срок выше; задачи без срока — в конце, новые сверху. */
function sorted(tasks) {
  return [...tasks].sort((a, b) => {
    if (a.due && b.due) return new Date(a.due) - new Date(b.due);
    if (a.due) return -1;
    if (b.due) return 1;
    return 0;
  });
}

function plural(count) {
  const tail = count % 100;
  if (tail >= 11 && tail <= 14) return "дел";
  switch (count % 10) {
    case 1:
      return "дело";
    case 2:
    case 3:
    case 4:
      return "дела";
    default:
      return "дел";
  }
}

function renderGroup(name, modifier, tasks, extra) {
  const group = document.createElement("section");
  group.className = `group group--${modifier}`;

  const head = document.createElement("div");
  head.className = "group__head";
  const title = document.createElement("span");
  title.className = "group__name";
  title.textContent = name;
  head.append(title);
  if (extra) head.append(extra);
  group.append(head);

  for (const task of tasks) group.append(renderTask(task));
  return group;
}

/** «Очистить» у сделанного: убирает всё сделанное разом. */
function clearDoneButton() {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "group__clear";
  button.textContent = "Очистить";
  button.title = "Убрать все сделанные";
  armTwice(button, "Точно?", async () => {
    await api?.invoke("task_clear_done");
    refresh();
  });
  return button;
}

/**
 * Необратимое — со второго нажатия.
 *
 * Крестик удаления стоит рядом с остальными кнопками, и промахнуться легко.
 * Первое нажатие только спрашивает «точно?», второе — удаляет; если второго
 * не было за три секунды, кнопка возвращается как была.
 */
function armTwice(button, question, action) {
  const original = button.textContent;
  let armed = null;
  button.addEventListener("click", async (event) => {
    event.stopPropagation();
    if (armed) {
      clearTimeout(armed);
      armed = null;
      await action();
      return;
    }
    button.textContent = question;
    button.classList.add("is-armed");
    armed = setTimeout(() => {
      armed = null;
      button.textContent = original;
      button.classList.remove("is-armed");
    }, 3000);
  });
}

/** Значок «перенести»: циферблат со стрелками, цветом текста. */
const CLOCK =
  '<svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" ' +
  'stroke-width="1.4" stroke-linecap="round"><circle cx="8" cy="8" r="6"/>' +
  '<path d="M8 4.8V8l2.2 1.6"/></svg>';

function iconButton(className, text, title, onClick) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `task__act ${className}`;
  button.textContent = text;
  button.title = title;
  button.setAttribute("aria-label", title);
  if (onClick) button.addEventListener("click", onClick);
  return button;
}

function renderTask(task) {
  const wrap = document.createElement("div");
  wrap.className = "task-wrap";

  const row = document.createElement("div");
  row.className = "task";
  if (task.done) row.classList.add("task--done");
  if (task.overdue) row.classList.add("task--overdue");

  const mark = document.createElement("button");
  mark.className = "task__mark";
  mark.type = "button";
  mark.title = task.done ? "Вернуть в работу" : "Сделано";
  mark.setAttribute("aria-label", mark.title);
  mark.addEventListener("click", async () => {
    await api?.invoke("task_done", { id: task.id, done: !task.done });
    refresh();
  });

  const text = document.createElement("span");
  text.className = "task__text";
  text.append(renderTitle(task));

  if (task.due) {
    // Срок — тоже кнопка: щёлкнул по «завтра в 10:00» — открылся перенос.
    const due = document.createElement("button");
    due.type = "button";
    due.className = "task__due";
    due.textContent = dueLabel(task.due);
    due.title = "Перенести";
    due.addEventListener("click", () => togglePostpone(task));
    text.append(due);
  }

  // Шаги показываются под названием: у них свои отметки, но своего срока нет —
  // это части одного дела, а не соседние с ним.
  if (task.steps?.length || addingStep === task.id) {
    text.append(renderSteps(task));
  }

  if (task.advice) {
    const advice = document.createElement("span");
    advice.className = "task__advice";
    advice.textContent = task.advice;
    text.append(advice);
  }

  if (task.postponed >= 3) {
    const warn = document.createElement("span");
    warn.className = "task__warn";
    warn.textContent = `Переносили ${task.postponed} раза`;
    text.append(warn);
  }

  const actions = document.createElement("div");
  actions.className = "task__actions";

  if (!task.done) {
    // Часы — рисунком, а не эмодзи: эмодзи в Windows цветной, и красный
    // будильник был бы единственным пятном цвета в окне, где цвет один.
    const later = iconButton("task__act--later", "", "Перенести", () => togglePostpone(task));
    later.innerHTML = CLOCK;
    actions.append(
      later,
      iconButton("task__act--step", "＋", "Добавить шаг", () => {
        addingStep = addingStep === task.id ? null : task.id;
        refresh();
      }),
    );

    // Разбить на шаги просит модель. Только там, где шагов ещё нет, — второй
    // раз разбивать уже разбитое незачем.
    if (!task.steps?.length) {
      const plan = iconButton("task__act--plan", "⋯", "Разбить на шаги с помощью Ноа");
      plan.addEventListener("click", async () => {
        plan.disabled = true;
        plan.textContent = "…";
        try {
          await api?.invoke("task_plan", { id: task.id });
        } catch (err) {
          console.error("не вышло разбить на шаги", err);
        }
        refresh();
      });
      actions.append(plan);
    }
  }

  const drop = iconButton("task__act--drop", "×", `Удалить: ${task.title}`);
  armTwice(drop, "Удалить?", async () => {
    await api?.invoke("task_remove", { id: task.id });
    refresh();
  });
  actions.append(drop);

  row.append(mark, text, actions);
  wrap.append(row);
  if (postponing === task.id) wrap.append(renderPostpone(task));
  return wrap;
}

function togglePostpone(task) {
  postponing = postponing === task.id ? null : task.id;
  refresh();
}

/**
 * Название, которое правится двойным щелчком.
 *
 * Enter сохраняет, Esc возвращает как было. Опечатка в названии, сделанная
 * распознаванием, иначе оставалась бы навсегда — удалить и завести заново.
 */
function renderTitle(task) {
  const title = document.createElement("span");
  title.className = "task__title";
  title.textContent = task.title;
  title.title = "Дважды щёлкните, чтобы переименовать";
  title.addEventListener("dblclick", () => {
    const input = document.createElement("input");
    input.type = "text";
    input.className = "task__title-input";
    input.value = task.title;
    let finished = false;
    const finish = async (save) => {
      if (finished) return;
      finished = true;
      const value = input.value.trim();
      if (save && value && value !== task.title) {
        await api?.invoke("task_edit", { id: task.id, title: value });
      }
      refresh();
    };
    input.addEventListener("keydown", (event) => {
      if (event.key === "Enter") finish(true);
      if (event.key === "Escape") {
        event.stopPropagation();
        finish(false);
      }
    });
    input.addEventListener("blur", () => finish(true));
    title.replaceWith(input);
    input.focus();
    input.select();
  });
  return title;
}

function renderSteps(task) {
  const steps = document.createElement("div");
  steps.className = "task__steps";

  (task.steps ?? []).forEach((step, at) => {
    const row = document.createElement("div");
    row.className = "step" + (step.done ? " step--done" : "");

    const label = document.createElement("label");
    label.className = "step__label";

    const box = document.createElement("input");
    box.type = "checkbox";
    box.className = "step__box";
    box.checked = step.done;
    box.addEventListener("change", async () => {
      await api?.invoke("task_step", { id: task.id, at, done: box.checked });
      refresh();
    });

    const name = document.createElement("span");
    name.textContent = step.title;
    label.append(box, name);

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "step__drop";
    remove.textContent = "×";
    remove.title = "Убрать шаг";
    remove.setAttribute("aria-label", `Убрать шаг: ${step.title}`);
    remove.addEventListener("click", async () => {
      await api?.invoke("task_step_remove", { id: task.id, at });
      refresh();
    });

    row.append(label, remove);
    steps.append(row);
  });

  if (addingStep === task.id) {
    const input = document.createElement("input");
    input.type = "text";
    input.className = "step__new";
    input.placeholder = "Новый шаг… Enter — добавить";
    input.addEventListener("keydown", async (event) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        addingStep = null;
        refresh();
        return;
      }
      if (event.key !== "Enter") return;
      const title = input.value.trim();
      if (!title) return;
      await api?.invoke("task_step_add", { id: task.id, title });
      // Поле остаётся открытым: шаги обычно добавляют несколько подряд.
      refresh();
    });
    steps.append(input);
    requestAnimationFrame(() => input.focus());
  }

  return steps;
}

/* ── Добавление ────────────────────────────────────────────────────────── */

ui.form.addEventListener("submit", async (event) => {
  event.preventDefault();

  const title = ui.title.value.trim();
  if (!title) return;

  try {
    await api?.invoke("task_add", { title, due: isoFromField(ui.due.value) });
    ui.title.value = "";
    ui.due.value = "";
    refresh();
  } catch (err) {
    console.error("задача не добавилась", err);
  }
});

/**
 * Значение поля даты — в вид, понятный Rust.
 *
 * `datetime-local` отдаёт время без пояса, а срок без пояса — это срок, который
 * при следующем запуске окажется другим. Часовой пояс берём здешний: человек
 * назначал дело себе, а не абстрактному наблюдателю.
 */
function isoFromField(value) {
  if (!value) return null;
  const when = new Date(value);
  return Number.isNaN(when.getTime()) ? null : when.toISOString();
}

/* ── Окно ──────────────────────────────────────────────────────────────── */

function closeWindow() {
  api?.invoke("close_tasks").catch(() => {});
}

// Рамки у окна нет — двигают его за заголовок. Кнопка закрытия живёт там же,
// и нажатие на неё окно не таскает.
const win = appWindow();
// Свернуть — окно уходит на панель задач и возвращается оттуда или из трея.
ui.minimize?.addEventListener("click", () => win?.minimize());
ui.head.addEventListener("pointerdown", (event) => {
  if (event.button !== 0 || event.target.closest("button")) return;
  event.preventDefault();
  win?.startDragging();
});

ui.close.addEventListener("click", closeWindow);

document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  // Сначала Esc закрывает открытый перенос или поле шага, и только потом —
  // само окно: иначе, передумав переносить, человек терял бы всё окно.
  if (postponing || addingStep) {
    postponing = null;
    addingStep = null;
    refresh();
    return;
  }
  closeWindow();
});

api?.invoke("runtime_config").then((config) => applyTheme(config?.theme));
// Список меняют и голосом, и напоминаниями — окно должно это показывать само.
api?.listen("tasks:changed", refresh);

refresh();

/* ── Настройки: вечерний разбор и календарь ───────────────────────────────
   Раньше они жили в общем окне программы, среди микрофона и модели. Но это
   настройки задач, и человек ищет их там, где задачи. */

function showCalendarStatus(connected) {
  ui.calendarStatus.textContent = connected ? "Подключено" : "Не подключено";
  ui.calendarForget.hidden = !connected;
}

async function loadTune() {
  if (!api || !ui.tune) return;
  try {
    const review = await api.invoke("review_settings");
    if (review) {
      ui.reviewEnabled.checked = Boolean(review.enabled);
      ui.reviewAt.value = `${String(review.hour).padStart(2, "0")}:${String(review.minute).padStart(2, "0")}`;
    }
    const calendar = await api.invoke("calendar_settings");
    if (calendar) {
      ui.calendarEnabled.checked = Boolean(calendar.enabled);
      ui.calendarClientId.value = calendar.clientId ?? "";
      ui.calendarSecret.value = calendar.clientSecret ?? "";
      showCalendarStatus(calendar.connected);
    }
  } catch (err) {
    ui.calendarStatus.textContent = String(err);
  }
}

async function saveReview() {
  const [hour, minute] = (ui.reviewAt.value || "20:30").split(":").map(Number);
  await api?.invoke("save_review_settings", {
    settings: {
      enabled: ui.reviewEnabled.checked,
      hour: Number.isFinite(hour) ? hour : 20,
      minute: Number.isFinite(minute) ? minute : 30,
    },
  });
}

async function saveCalendar() {
  await api?.invoke("save_calendar_settings", {
    settings: {
      clientId: ui.calendarClientId.value,
      clientSecret: ui.calendarSecret.value,
      calendarId: "primary",
      enabled: ui.calendarEnabled.checked,
      connected: false,
    },
  });
}

ui.reviewEnabled?.addEventListener("change", saveReview);
ui.reviewAt?.addEventListener("change", saveReview);
ui.calendarEnabled?.addEventListener("change", saveCalendar);
ui.calendarClientId?.addEventListener("change", saveCalendar);
ui.calendarSecret?.addEventListener("change", saveCalendar);

ui.calendarConnect?.addEventListener("click", async () => {
  // Ключ и секрет могли только что вписать и не увести фокус с поля.
  await saveCalendar();
  ui.calendarStatus.textContent = "Открываю браузер…";
  try {
    await api?.invoke("calendar_connect");
    showCalendarStatus(true);
  } catch (err) {
    ui.calendarStatus.textContent = String(err);
  }
});

ui.calendarForget?.addEventListener("click", async () => {
  await api?.invoke("calendar_forget");
  showCalendarStatus(false);
});

loadTune();
