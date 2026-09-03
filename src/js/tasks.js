// Окно задач: список, добавление, отметка о выполнении.
//
// Список хранится в Rust, здесь только показ. После каждого изменения окно
// перечитывает его целиком: задач десятки, а не тысячи, и точечное обновление
// разметки стоило бы дороже, чем перерисовка, — зато список нельзя рассинхронить
// с тем, что на диске.

import { tauri, appWindow, applyTheme } from "./bridge.js";

const api = tauri();
const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

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
    ui.list.append(renderGroup("Сделано", "done", sorted(done).reverse()));
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

function renderGroup(name, modifier, tasks) {
  const group = document.createElement("section");
  group.className = `group group--${modifier}`;

  const title = document.createElement("span");
  title.className = "group__name";
  title.textContent = name;
  group.append(title);

  for (const task of tasks) group.append(renderTask(task));
  return group;
}

function renderTask(task) {
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

  const title = document.createElement("span");
  title.className = "task__title";
  title.textContent = task.title;
  text.append(title);

  if (task.due) {
    const due = document.createElement("span");
    due.className = "task__due";
    due.textContent = dueLabel(task.due);
    text.append(due);
  }

  // Шаги показываются под названием: у них свои отметки, но своего срока нет —
  // это части одного дела, а не соседние с ним.
  if (task.steps?.length) {
    const steps = document.createElement("div");
    steps.className = "task__steps";
    task.steps.forEach((step, at) => {
      const row = document.createElement("label");
      row.className = "step" + (step.done ? " step--done" : "");

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
      row.append(box, name);
      steps.append(row);
    });
    text.append(steps);
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

  // Кнопка помощи: просит модель разложить дело на шаги. Показывается только
  // там, где шагов ещё нет, — второй раз разбивать уже разбитое незачем.
  const plan = document.createElement("button");
  plan.className = "task__plan";
  plan.type = "button";
  plan.textContent = "⋯";
  plan.title = "Разбить на шаги";
  plan.setAttribute("aria-label", `Разбить на шаги: ${task.title}`);
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

  const drop = document.createElement("button");
  drop.className = "task__drop";
  drop.type = "button";
  drop.textContent = "×";
  drop.title = "Удалить";
  drop.setAttribute("aria-label", `Удалить: ${task.title}`);
  drop.addEventListener("click", async () => {
    await api?.invoke("task_remove", { id: task.id });
    refresh();
  });

  row.append(mark, text);
  if (!task.done && !task.steps?.length) row.append(plan);
  row.append(drop);
  return row;
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

// Рамки у окна нет — двигают его за заголовок.
const win = appWindow();
ui.head.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  event.preventDefault();
  win?.startDragging();
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") api?.invoke("close_tasks").catch(() => {});
});

api?.invoke("runtime_config").then((config) => applyTheme(config?.theme));
// Список меняют и голосом, и напоминаниями — окно должно это показывать само.
api?.listen("tasks:changed", refresh);

refresh();
