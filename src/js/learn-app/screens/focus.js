// 08 Фокус — отрезок работы без отвлечений: 15 / 25 / 50 минут, кольцо с
// оставшимся временем, цель, «Записка на потом», пауза. В конце — выгрузка
// по памяти (что понял) и перерыв. Таймер живёт в настройках страницы и
// идёт, даже если уйти на другую вкладку или свернуть приложение.

import { el, icon, label, header, nav, register, call, api, prefs, toast, plural } from "../core.js";
import { store } from "../store.js";

const LENGTHS = [
  { work: 15, rest: 3 },
  { work: 25, rest: 5 },
  { work: 50, rest: 10 },
];
/** Каждый четвёртый перерыв — длинный. */
const LONG_EVERY = 4;

const load = () => prefs.get("focus", null);
const save = (state) => prefs.set("focus", state);

const left = (f) => (f ? (f.pausedLeft ?? f.endsAt - Date.now()) : 0);

function clock(ms) {
  const total = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

function bell(text) {
  api?.invoke("plugin:sufler|chime").catch(() => {});
  api?.invoke("learn_focus_bell", { text }).catch(() => {});
}

/** Конец отрезка или перерыва — даже когда экран фокуса не открыт. */
function tick() {
  const f = load();
  if (!f || f.pausedLeft != null || left(f) > 0) return;
  if (f.phase === "work") {
    f.phase = "recall";
    f.minutes = f.work;
    f.pausedLeft = 0;
    save(f);
    bell(`${f.work} минут фокуса позади. Запиши по памяти, что понял, — и перерыв.`);
    if (nav.current !== "focus") toast("Отрезок фокуса закончился — вкладка «Фокус»", 5000);
    onChange?.();
  } else if (f.phase === "break") {
    f.phase = "ready";
    f.pausedLeft = 0;
    save(f);
    bell("Перерыв закончился. Продолжим?");
    if (nav.current !== "focus") toast("Перерыв закончился", 5000);
    onChange?.();
  }
}
setInterval(tick, 1000);
document.addEventListener("visibilitychange", tick);

let onChange = null;

register("focus", (screen) => {
  const course = store.currentCourse();
  const stats = course?.focus ?? { today: 0, week: 0, streak: 0 };
  const streak = el("span", "chip chip--streak");
  streak.style.cssText = "border:0;background:none;padding:0";
  streak.append(icon("flame", 16, 2.25), plural(stats.streak, "день", "дня", "дней"));
  screen.append(header({ big: "Фокус", meta: streak }));
  streak.hidden = !stats.streak;
  const content = el("div", "content");
  content.style.cssText = "padding:20px 16px;gap:16px";
  const actions = el("div", "dock");
  actions.style.cssText = "border:0;padding:0 16px 12px";
  screen.append(content, actions);

  let length = prefs.get("focusLength", 1);
  let timer = 0;

  function paint() {
    clearInterval(timer);
    const f = load();
    content.replaceChildren();
    actions.replaceChildren();
    if (!f || f.phase === "idle") return paintIdle();
    if (f.phase === "recall") return paintRecall(f);
    if (f.phase === "ready") return paintReady(f);
    paintRunning(f);
  }

  function seg(disabled) {
    const box = el("div", "seg");
    box.style.gridTemplateColumns = "repeat(3, minmax(0, 1fr))";
    LENGTHS.forEach((mode, i) => {
      const item = el("button", "", i === length ? `${mode.work} мин` : String(mode.work));
      item.setAttribute("aria-pressed", String(i === length));
      item.disabled = disabled;
      item.addEventListener("click", () => {
        length = i;
        prefs.set("focusLength", i);
        paint();
      });
      box.append(item);
    });
    return box;
  }

  function dial(ms, total, caption, color = "var(--c-accent)") {
    const ring = el("div", "ring");
    const part = total ? Math.max(0, Math.min(1, ms / total)) : 1;
    ring.style.background = `conic-gradient(${color} 0 ${part * 100}%, rgba(0,240,255,0.1) ${part * 100}% 100%)`;
    const inner = el("div", "ring__inner");
    const time = el("span", "ring__time", clock(ms));
    inner.append(time, el("span", "small dim", caption));
    ring.append(inner);
    return { ring, time };
  }

  function paintIdle() {
    const mode = LENGTHS[length];
    const { ring } = dial(mode.work * 60_000, mode.work * 60_000, "готово к старту");
    const goal = el("label", "goal");
    const input = el("input");
    input.placeholder = "Цель на отрезок — одна, конкретная";
    input.value = prefs.get("focusGoal", "");
    input.enterKeyHint = "go";
    goal.append(icon("target", 20, 2), input);
    const today = el("span", "small dim", `Сегодня ${stats.today} мин фокуса · за неделю ${stats.week} мин`);
    today.style.textAlign = "center";
    content.append(seg(false), ring, goal, today);
    const go = el("button", "btn btn--primary btn--big");
    go.style.flex = "1";
    go.append(icon("play", 20, 2.25), "Начать");
    const begin = () => {
      const goalText = input.value.trim();
      prefs.set("focusGoal", goalText);
      const prev = load();
      save({
        phase: "work",
        work: mode.work,
        rest: mode.rest,
        goal: goalText,
        parked: prev?.parked ?? [],
        round: (prev?.round ?? 0) + 1,
        course: course?.id ?? null,
        endsAt: Date.now() + mode.work * 60_000,
        pausedLeft: null,
      });
      paint();
    };
    go.addEventListener("click", begin);
    input.addEventListener("keydown", (event) => {
      if (event.key === "Enter") begin();
    });
    actions.append(go);
  }

  function paintRunning(f) {
    const work = f.phase === "work";
    const total = (work ? f.work : f.restNow ?? f.rest) * 60_000;
    const paused = f.pausedLeft != null;
    const { ring, time } = dial(left(f), total, paused ? "пауза" : work ? `из ${clock(total)}` : "перерыв", work ? "var(--c-accent)" : "var(--c-label)");
    if (work) {
      length = Math.max(0, LENGTHS.findIndex((m) => m.work === f.work));
      content.append(seg(true), ring);
      const goal = el("div", "goal");
      goal.append(icon("target", 20, 2), el("span", "", f.goal ? `Цель: ${f.goal}` : "Цель не задана"));
      content.append(goal);
      // Мысль, которая отвлекает, — записать и вернуться к теме.
      const later = el("label", "later");
      const note = el("input");
      note.placeholder = "Записка на потом — не отвлекайтесь";
      note.enterKeyHint = "done";
      later.append(icon("note", 20, 2), note);
      const parked = el("div", "list");
      parked.style.gap = "6px";
      const drawParked = () =>
        parked.replaceChildren(...(load()?.parked ?? []).map((text) => el("span", "small dim", `· ${text}`)));
      note.addEventListener("keydown", (event) => {
        if (event.key !== "Enter" || !note.value.trim()) return;
        const now = load();
        now.parked = [...(now.parked ?? []), note.value.trim()];
        save(now);
        note.value = "";
        note.blur();
        drawParked();
        toast("Записано — вернётесь после отрезка");
      });
      drawParked();
      content.append(later, parked);
    } else {
      content.append(ring);
      const tips = el("div", "note");
      tips.append(
        label("// перерыв"),
        el("span", "", "Встаньте, пройдитесь, посмотрите вдаль. Не в телефон: мозг в перерыве укладывает прочитанное, лента ему мешает."),
      );
      content.append(tips);
    }
    const pause = el("button", "btn btn--outline");
    pause.style.cssText = "flex:1;height:52px";
    pause.append(icon(paused ? "play" : "pause", 18, 2.25), paused ? "Продолжить" : "Пауза");
    pause.addEventListener("click", () => {
      const now = load();
      if (now.pausedLeft != null) {
        now.endsAt = Date.now() + now.pausedLeft;
        now.pausedLeft = null;
      } else now.pausedLeft = now.endsAt - Date.now();
      save(now);
      paint();
    });
    const stop = el("button", "btn btn--plain", work ? "Завершить" : "Закончить перерыв");
    stop.style.cssText = "flex:1;height:52px";
    stop.addEventListener("click", () => {
      const now = load();
      if (work) {
        now.minutes = Math.max(0, Math.round((now.work * 60_000 - left(now)) / 60_000));
        now.phase = "recall";
        now.pausedLeft = 0;
        save(now);
      } else {
        now.phase = "idle";
        save(now);
      }
      paint();
    });
    actions.append(pause, stop);
    timer = setInterval(() => {
      const now = load();
      if (!now || now.phase !== f.phase) return paint();
      const ms = left(now);
      time.textContent = clock(ms);
      const part = Math.max(0, Math.min(1, ms / total)) * 100;
      ring.style.background = `conic-gradient(${work ? "var(--c-accent)" : "var(--c-label)"} 0 ${part}%, rgba(0,240,255,0.1) ${part}% 100%)`;
    }, 1000);
  }

  /** Выгрузка: вспомнить без подсказок — закрепляет сильнее ещё получаса чтения. */
  function paintRecall(f) {
    const minutes = f.minutes ?? f.work;
    const rest = f.round % LONG_EVERY === 0 ? f.rest * 3 : f.rest;
    const box = el("div", "plate plate--active");
    box.append(label(`// ${plural(minutes, "минута", "минуты", "минут")} фокуса`), el("span", "continue__title", "Выгрузка"));
    box.append(el("span", "continue__lead", "Своими словами, не подглядывая: главное, термины, что осталось непонятным. Минута вспоминания закрепляет сильнее ещё получаса чтения."));
    if (f.goal) box.append(el("span", "small", `Цель была: ${f.goal}. Получилось?`));
    const recall = el("textarea", "field");
    recall.placeholder = "Что понял и запомнил…";
    box.append(recall);
    content.append(box);
    if (f.parked?.length) {
      const parked = el("div", "note");
      parked.append(label("// отложено на потом"));
      for (const text of f.parked) parked.append(el("span", "", `· ${text}`));
      content.append(parked);
    }
    const record = async (breakToo) => {
      const now = load();
      const courseId = now.course ?? course?.id;
      if (courseId) {
        try {
          const fresh = await call("learn_focus_done", {
            course: courseId,
            session: { minutes, goal: now.goal ?? "", recall: recall.value.trim(), parked: now.parked ?? [] },
          });
          if (course) course.focus = fresh;
          Object.assign(stats, fresh);
          streak.lastChild.textContent = plural(fresh.streak, "день", "дня", "дней");
          streak.hidden = !fresh.streak;
        } catch (err) {
          toast(`Не записалось: ${err}`);
        }
      }
      now.parked = [];
      if (breakToo) {
        now.phase = "break";
        now.restNow = rest;
        now.endsAt = Date.now() + rest * 60_000;
        now.pausedLeft = null;
      } else now.phase = "idle";
      save(now);
      paint();
    };
    const rest_ = el("button", "btn btn--primary");
    rest_.style.cssText = "flex:1.3;height:52px";
    rest_.append(`Перерыв ${rest} мин`);
    rest_.addEventListener("click", () => record(true));
    const end = el("button", "btn btn--secondary", "Закончить");
    end.style.cssText = "flex:1;height:52px";
    end.addEventListener("click", () => record(false));
    actions.append(end, rest_);
  }

  function paintReady(f) {
    const box = el("div", "plate plate--active");
    box.append(label("// перерыв закончился"), el("span", "continue__title", "Ещё отрезок?"));
    box.append(el("span", "continue__lead", f.goal ? `Цель была: ${f.goal}` : "Новый отрезок — новая цель."));
    content.append(box);
    const again = el("button", "btn btn--primary btn--big");
    again.style.flex = "1";
    again.append(icon("play", 20, 2.25), "Ещё отрезок");
    again.addEventListener("click", () => {
      const now = load();
      now.phase = "idle";
      save(now);
      paint();
    });
    actions.append(again);
  }

  onChange = () => paint();
  paint();
  // Итоги — свежие из программы: занятия могли записаться на компьютере.
  store.courses(true).then(() => {
    const fresh = store.currentCourse()?.focus;
    if (!fresh || !onChange) return;
    Object.assign(stats, fresh);
    streak.lastChild.textContent = plural(stats.streak, "день", "дня", "дней");
    streak.hidden = !stats.streak;
    if ((load()?.phase ?? "idle") === "idle") paint();
  });
  return {
    cleanup: () => {
      clearInterval(timer);
      onChange = null;
    },
  };
});
