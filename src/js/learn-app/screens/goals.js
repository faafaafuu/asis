// Цели — год, месяц, неделя, день по курсам. Сверху срок (День · Неделя ·
// Месяц · Год) и листание по нему, ниже — цели срока: галочка (у учебных
// ставится сама), что за цель и насколько сделана, к какой большой цели
// относится. «+ Цель» — поставить руками, «План к цели» — Ноа разложит
// большую цель на месяцы, недели и дни. На сегодня Ноа предлагает шаги
// по текущему курсу.

import { el, icon, label, header, nav, register, call, toast, sheet, progress, isWeb, prefs } from "../core.js";
import { store, topicCard, isDone } from "../store.js";
import { swipeRow, pressMenu, collapse, confirmAction } from "../gestures.js";
import { coachJson } from "../coach.js";
import { goals, PERIODS, KINDS, periodStart, shiftStart, periodLabel, progressOf, settle, suggestDay } from "../goals.js";
import { signInCard } from "./courses.js";

/** Открыть то, о чём цель: урок, проверку, карточки, фокус… */
export function openGoal(goal) {
  const params = { course: goal.course, topic: goal.topic };
  store.rememberCourse(goal.course);
  switch (KINDS[goal.kind]?.go) {
    case "lesson":
      return nav.push("lesson", params);
    case "check":
      return nav.push("check", { course: goal.course, scope: goal.topic });
    case "drill":
      return nav.push("drill", params);
    case "practice":
      if (isWeb) return toast("Практика в терминале — в приложении на телефоне и в NOAH на компьютере.");
      return nav.push("practice", params);
    case "interview":
      return nav.push("interview", goal.topic ? params : { course: goal.course });
    case "review":
      return nav.tab("review", true);
    case "focus":
      return nav.tab("focus", true);
    default:
      return nav.push("course", { course: goal.course });
  }
}

/** Строка цели: галочка, заголовок, прогресс, к чему относится; смахнуть — удалить. */
function goalRow(goal, { compact = false } = {}) {
  const p = progressOf(goal);
  const done = goal.done || p.done;
  const row = el("div", `goal${done ? " is-done" : ""}${compact ? " goal--compact" : ""}`);
  const check = el("button", "goal__check");
  check.setAttribute("aria-label", done ? "Сделано" : "Отметить сделанным");
  check.setAttribute("aria-pressed", String(done));
  if (done) check.append(icon("check", 16, 2.5));
  check.addEventListener("click", (event) => {
    event.stopPropagation();
    // Учебная цель отмечается сама — но и вручную можно: сделал на компьютере.
    goals.save({ ...goal, done: !done, doneAt: done ? 0 : Date.now() });
  });
  const body = el("button", "goal__body");
  body.append(el("span", "goal__title", goal.title));
  const course = store.course(goal.course);
  if (!compact) {
    const sub = [course?.title, p.text].filter(Boolean).join(" · ");
    if (sub) body.append(el("span", "goal__sub", sub));
    if (p.total > 1 && !done) body.append(progress((p.value / p.total) * 100, "m-bar"));
    const parent = goal.parent ? goals.byId(goal.parent) : null;
    if (parent) body.append(el("span", "goal__parent", `↑ ${parent.title}`));
    if (goal.by === "ai" || goal.by === "noa") body.append(el("span", "goal__by", goal.by === "ai" ? "поставила нейросеть" : "предложила Ноа"));
  }
  body.addEventListener("click", () => openGoal(goal));
  row.append(check, body);
  if (compact) return row;
  const wrap = swipeRow(row, [{ text: "Удалить", icon: "trash", danger: true, run: () => drop() }]);
  const drop = async () => {
    goals.remove(goal.id);
    await collapse(wrap);
  };
  pressMenu(row, () => [
    { text: "Открыть", icon: "chevron-right", run: () => openGoal(goal) },
    { text: done ? "Вернуть в работу" : "Сделано", icon: "check", run: () => goals.save({ ...goal, done: !done, doneAt: done ? 0 : Date.now() }) },
    { text: `Перенести ${PERIODS.find((x) => x.id === goal.period)?.next ?? ""}`, icon: "calendar", run: () => goals.save({ ...goal, start: shiftStart(goal.period, goal.start, 1) }) },
    { text: "Удалить", icon: "trash", danger: true, run: () => drop() },
  ]);
  return wrap;
}

register("goals", (screen) => {
  const meta = el("span", "");
  screen.append(header({ big: "Цели", meta }));
  const content = el("div", "content");
  const sticky = el("div", "sticky goal-actions");
  const add = el("button", "btn btn--secondary");
  add.append(icon("plus", 18, 2.25), "Цель");
  add.addEventListener("click", () => addGoal(period, start));
  const plan = el("button", "btn btn--primary");
  plan.append(icon("target", 18, 2.25), "План к цели");
  plan.addEventListener("click", () => planGoals());
  sticky.append(add, plan);
  screen.append(content, sticky);

  let period = prefs.get("goals.period", "day");
  let start = periodStart(period);
  let alive = true;
  let signed = true;

  const paint = () => {
    if (!alive) return;
    content.replaceChildren();
    if (!signed) {
      content.append(signInCard(), el("span", "muted", "Цели живут в аккаунте: их видно на телефоне и в браузере, а нейросеть по MCP может ставить их сама."));
      sticky.hidden = true;
      return;
    }
    sticky.hidden = false;
    const seg = el("div", "seg");
    for (const p of PERIODS) {
      const btn = el("button", "", p.name);
      btn.setAttribute("aria-pressed", String(p.id === period));
      btn.addEventListener("click", () => {
        period = p.id;
        start = periodStart(period);
        prefs.set("goals.period", period);
        paint();
      });
      seg.append(btn);
    }
    const when = el("div", "period-nav");
    const prev = el("button", "icon-btn");
    prev.setAttribute("aria-label", "Раньше");
    prev.append(icon("arrow-left", 20, 2));
    prev.addEventListener("click", () => {
      start = shiftStart(period, start, -1);
      paint();
    });
    const next = el("button", "icon-btn");
    next.setAttribute("aria-label", "Дальше");
    next.append(icon("chevron-right", 20, 2));
    next.addEventListener("click", () => {
      start = shiftStart(period, start, 1);
      paint();
    });
    const now = el("button", "period-nav__label", periodLabel(period, start));
    now.addEventListener("click", () => {
      start = periodStart(period);
      paint();
    });
    when.append(prev, now, next);
    content.append(seg, when);

    const list = goals.of(period, start).sort((a, b) => Number(a.done) - Number(b.done) || a.created - b.created);
    const made = list.filter((g) => g.done || progressOf(g).done).length;
    meta.textContent = list.length ? `${made} из ${list.length}` : "";
    const box = el("div", "list goal-list");
    for (const goal of list) box.append(goalRow(goal));
    if (!list.length) box.append(el("span", "muted", period === "day" ? "На этот день целей нет." : "На этот срок целей нет — поставьте одну или попросите Ноа составить план."));
    content.append(box);

    // На сегодня — шаги по текущему курсу, одним касанием.
    if (period === "day" && start === periodStart("day")) {
      const ideas = suggestDay();
      if (ideas.length) {
        const offer = el("div", "list");
        offer.append(label("// ноа предлагает на сегодня"));
        for (const idea of ideas) {
          const row = el("button", "setting");
          const text = el("span", "setting__text");
          text.append(el("span", "setting__name", idea.title), el("span", "setting__hint", store.course(idea.course)?.title ?? ""));
          row.append(icon("plus", 20, 2), text);
          row.firstChild.style.color = "var(--c-accent)";
          row.addEventListener("click", () => {
            goals.save(idea);
            toast("Цель на сегодня поставлена");
          });
          offer.append(row);
        }
        if (ideas.length > 1) {
          const all = el("button", "btn btn--plain", "Поставить всё");
          all.style.cssText = "height:40px;padding:0;align-self:flex-start";
          all.addEventListener("click", () => ideas.forEach((idea) => goals.save(idea)));
          offer.append(all);
        }
        content.append(offer);
      }
    }
  };

  const off = goals.onChange(() => {
    settle();
    paint();
  });
  (async () => {
    const [courses, isIn] = await Promise.all([store.courses(), call("account_status").catch(() => true), store.practice()]);
    if (!alive) return;
    signed = Boolean(isIn);
    settle();
    paint();
    if (signed && courses.length) await goals.pull();
  })();
  return {
    cleanup: () => {
      alive = false;
      off();
    },
  };
});

/* ── Поставить цель руками ───────────────────────────────────────────── */

function addGoal(period, start) {
  sheet("Новая цель", (content, close) => {
    const courses = store.list();
    const list = courses.length ? courses : [store.currentCourse()].filter(Boolean);
    if (!list.length) {
      content.append(el("span", "muted", "Цели ставятся по курсам — сначала соберите или подтяните курс."));
      return;
    }
    let draft = { period, start, course: store.currentCourse()?.id ?? list[0].id, kind: "lesson", topic: "", target: null, title: "", parent: "" };
    const form = el("div", "list");
    content.append(form);
    const paint = () => {
      form.replaceChildren();
      const seg = el("div", "seg");
      for (const p of PERIODS) {
        const btn = el("button", "", p.name);
        btn.setAttribute("aria-pressed", String(p.id === draft.period));
        btn.addEventListener("click", () => {
          draft = { ...draft, period: p.id, start: periodStart(p.id), parent: "" };
          paint();
        });
        seg.append(btn);
      }
      form.append(label("// срок"), seg);
      const course = el("select", "field");
      for (const c of list) course.append(new Option(c.title, c.id));
      course.value = draft.course;
      course.addEventListener("change", () => {
        draft = { ...draft, course: course.value, topic: "", parent: "" };
        paint();
      });
      form.append(label("// курс"), course);
      const kinds = el("div", "chips");
      for (const [id, kind] of Object.entries(KINDS)) {
        if (id === "practice" && isWeb) continue;
        const chip = el("button", "chip", kind.name);
        chip.setAttribute("aria-pressed", String(id === draft.kind));
        chip.addEventListener("click", () => {
          draft = { ...draft, kind: id, target: KINDS[id].target ? draft.target ?? defaultTarget(id) : null };
          paint();
        });
        kinds.append(chip);
      }
      form.append(label("// что сделать"), kinds);
      const chosen = store.course(draft.course);
      if (KINDS[draft.kind].topic || draft.kind === "interview") {
        const topic = el("select", "field");
        if (!KINDS[draft.kind].topic) topic.append(new Option("Весь курс", ""));
        for (const t of chosen?.topics ?? []) topic.append(new Option(`${isDone(t) ? "✓ " : ""}${t.title}`, t.id));
        if (KINDS[draft.kind].topic && !draft.topic) draft.topic = (chosen?.topics.find((t) => !isDone(t)) ?? chosen?.topics[0])?.id ?? "";
        topic.value = draft.topic;
        topic.addEventListener("change", () => (draft.topic = topic.value));
        form.append(label("// тема"), topic);
      }
      if (KINDS[draft.kind].target) {
        const count = el("input", "field");
        count.type = "number";
        count.min = "1";
        count.value = String(draft.target ?? defaultTarget(draft.kind));
        count.addEventListener("input", () => (draft.target = Number(count.value)));
        draft.target = Number(count.value);
        form.append(label(`// сколько ${KINDS[draft.kind].target}`), count);
      }
      const title = el("input", "field");
      title.placeholder = KINDS[draft.kind].title ? "Что именно сделать" : "Название — по желанию";
      title.value = draft.title;
      title.addEventListener("input", () => (draft.title = title.value));
      form.append(label("// название"), title);
      const parents = goals.parents(draft.period, draft.course);
      if (parents.length) {
        const parent = el("select", "field");
        parent.append(new Option("Сама по себе", ""));
        for (const g of parents) parent.append(new Option(`↑ ${g.title}`, g.id));
        parent.value = draft.parent;
        parent.addEventListener("change", () => (draft.parent = parent.value));
        form.append(label("// часть большой цели"), parent);
      }
      const error = el("span", "error");
      const go = el("button", "btn btn--primary btn--big", "Поставить цель");
      go.addEventListener("click", () => {
        if (KINDS[draft.kind].title && !draft.title.trim()) return (error.textContent = "Напишите, что именно сделать.");
        const goal = { ...draft, title: draft.title.trim() || autoTitle(draft), by: "user" };
        goals.save(goal);
        close();
        toast("Цель поставлена");
      });
      form.append(error, go);
    };
    paint();
  });
}

const defaultTarget = (kind) => ({ cards: 20, focus: 25, topics: 2 })[kind] ?? 1;

function autoTitle(goal) {
  const course = store.course(goal.course);
  const topic = goal.topic ? topicCard(course, goal.topic)?.title : "";
  return (
    {
      lesson: `Урок: ${topic}`,
      check: `Проверка: ${topic}`,
      drill: `Наизусть: ${topic}`,
      practice: `Практика: ${topic}`,
      interview: topic ? `Собеседование: ${topic}` : `Собеседование по курсу`,
      cards: `Повторить ${goal.target} карточек`,
      focus: `Фокус ${goal.target} мин`,
      topics: `Пройти ${goal.target} тем`,
      course: `Пройти курс «${course?.title ?? ""}»`,
    }[goal.kind] ?? goal.title
  );
}

/* ── План к цели — Ноа раскладывает большую цель на сроки ───────────── */

function planGoals() {
  sheet("План к цели", (content, close) => {
    const course = el("select", "field");
    const all = store.list();
    for (const c of all) course.append(new Option(c.title, c.id));
    course.value = store.currentCourse()?.id ?? all[0]?.id ?? "";
    const want = el("textarea", "field");
    want.placeholder = "Например: пройти курс и собеседование на DevOps к концу декабря, по часу в будни";
    const until = el("input", "field");
    until.type = "date";
    const later = new Date();
    later.setMonth(later.getMonth() + 2);
    until.value = later.toISOString().slice(0, 10);
    const error = el("span", "error");
    const go = el("button", "btn btn--primary btn--big", "Составить план");
    const preview = el("div", "list");
    content.append(
      el("span", "muted", "Ноа разложит цель от большого к малому: на весь срок, месяцы, недели и ближайшие дни — по темам курса и вашему темпу."),
      label("// курс"),
      course,
      label("// чего хотите"),
      want,
      label("// к какому сроку"),
      until,
      error,
      go,
      preview,
    );
    go.addEventListener("click", async () => {
      const chosen = store.course(course.value);
      if (!chosen) return (error.textContent = "Выберите курс.");
      go.disabled = true;
      error.textContent = "";
      go.textContent = "Ноа составляет план…";
      try {
        const plan = await askPlan(chosen, want.value.trim() || "Пройти курс целиком", until.value);
        showPlan(plan, chosen, preview, close);
      } catch (err) {
        error.textContent = String(err).replace(/^Error:\s*/, "");
      }
      go.disabled = false;
      go.textContent = "Составить план заново";
    });
  });
}

async function askPlan(course, want, until) {
  const today = periodStart("day");
  const topics = course.topics.map((t, i) => `${i + 1}. ${t.id} — ${t.title}${isDone(t) ? " (пройдена)" : t.read ? " (урок прочитан)" : ""}`).join("\n");
  const rules =
    "Ты — Ноа, наставник. Разложи цель человека на цели обучения по курсу — от большого к малому, чтобы он дошёл к сроку. " +
    `Сегодня ${today}. Срок — ${until}. Курс «${course.title}», темы по порядку (id — название):\n${topics}\n\n` +
    "Правила: одна главная цель на весь срок (period year, если срок дальше двух месяцев, иначе month); затем месячные; " +
    "недельные — до срока, но не больше восьми недель вперёд; задачи дня — только на ближайшие три дня. Темп реальный: " +
    "1–2 новые темы в неделю, в неделе — урок, проверка и карточки; перед концом — собеседование. Виды (kind) только: " +
    Object.entries(KINDS).map(([id, k]) => `${id} — ${k.name}`).join("; ") +
    ". Для lesson, check, drill, practice — topic из списка; для cards, focus, topics — target (число). Не больше 30 целей. " +
    'Ответь только JSON: {"goals":[{"ref":"g1","period":"year|month|week|day","start":"ГГГГ-ММ-ДД","kind":"…","topic":"id или пусто","target":число или null,"title":"коротко","parent":"ref большей цели или пусто"}]}';
  const data = await coachJson(rules, want, { long: true });
  const list = Array.isArray(data) ? data : data.goals ?? [];
  const topicIds = new Set(course.topics.map((t) => t.id));
  return list
    .filter((g) => KINDS[g?.kind] && PERIODS.some((p) => p.id === g.period))
    .filter((g) => !KINDS[g.kind].topic || topicIds.has(g.topic))
    .filter((g) => !(isWeb && g.kind === "practice"))
    .map((g) => ({
      ref: String(g.ref ?? ""),
      parentRef: String(g.parent ?? ""),
      period: g.period,
      start: periodStart(g.period, /^\d{4}-\d{2}-\d{2}$/.test(g.start ?? "") ? new Date(`${g.start}T12:00:00`) : new Date()),
      kind: g.kind,
      topic: topicIds.has(g.topic) ? g.topic : "",
      target: KINDS[g.kind].target ? Math.max(1, Math.round(Number(g.target) || defaultTarget(g.kind))) : null,
      title: String(g.title ?? "").slice(0, 140),
      course: course.id,
    }))
    .slice(0, 30);
}

function showPlan(plan, course, box, close) {
  box.replaceChildren(label(`// план · ${plan.length} целей`));
  if (!plan.length) return box.append(el("span", "muted", "План не сложился — попробуйте описать цель иначе."));
  const order = { year: 0, month: 1, week: 2, day: 3 };
  const names = Object.fromEntries(PERIODS.map((p) => [p.id, p.name]));
  const picked = new Set(plan.map((_, i) => i));
  plan.forEach((goal, i) => {
    const row = el("button", "setting");
    row.style.paddingLeft = `${12 + order[goal.period] * 12}px`;
    const text = el("span", "setting__text");
    text.append(el("span", "setting__name", goal.title || autoTitle(goal)), el("span", "setting__hint", `${names[goal.period]} · ${periodLabel(goal.period, goal.start)}`));
    const toggle = el("span", "switch");
    toggle.setAttribute("aria-checked", "true");
    row.append(text, toggle);
    row.addEventListener("click", () => {
      if (picked.has(i)) picked.delete(i);
      else picked.add(i);
      toggle.setAttribute("aria-checked", String(picked.has(i)));
    });
    box.append(row);
  });
  const take = el("button", "btn btn--primary btn--big", "Поставить выбранные цели");
  take.addEventListener("click", () => {
    // От большого к малому: у меньшей цели parent — id уже поставленной большей.
    const ids = new Map();
    const chosen = plan.filter((_, i) => picked.has(i)).sort((a, b) => order[a.period] - order[b.period]);
    for (const goal of chosen) {
      const saved = goals.save({ ...goal, title: goal.title || autoTitle(goal), parent: ids.get(goal.parentRef) ?? "", by: "noa", ref: undefined, parentRef: undefined });
      if (goal.ref) ids.set(goal.ref, saved.id);
    }
    close();
    toast(`Целей поставлено: ${chosen.length}`);
    nav.refresh();
  });
  box.append(take);
}

/* ── «Сегодня» на главной ─────────────────────────────────────────────── */

/** Карточка дня на главной: цели на сегодня и сколько сделано. */
export function todayCard() {
  settle();
  const list = goals.of("day", periodStart("day"));
  const card = el("div", "plate today-card");
  const made = list.filter((g) => g.done || progressOf(g).done).length;
  const head = el("div", "today-card__head");
  const all = el("button", "btn btn--plain", list.length ? "Все цели" : "Поставить");
  all.addEventListener("click", () => nav.tab("goals"));
  head.append(label(list.length ? `// сегодня · ${made} из ${list.length}` : "// цели на сегодня"), all);
  card.append(head);
  if (!list.length) {
    const ideas = suggestDay();
    card.append(el("span", "small muted", ideas.length ? `Ноа предлагает: ${ideas.map((i) => i.title.replace(/^./, (c) => c.toLowerCase())).join(", ")}.` : "Поставьте цель — шаг по курсу на сегодня."));
    if (ideas.length) {
      const take = el("button", "btn btn--secondary", "Поставить на сегодня");
      take.style.cssText = "height:40px;align-self:flex-start";
      take.addEventListener("click", () => {
        ideas.forEach((idea) => goals.save(idea));
        card.replaceWith(todayCard());
      });
      card.append(take);
    }
    return card;
  }
  card.append(progress((made / list.length) * 100, "m-bar"));
  for (const goal of list.sort((a, b) => Number(a.done) - Number(b.done)).slice(0, 4)) card.append(goalRow(goal, { compact: true }));
  // Отметили цель или пришли новые (MCP, другое устройство) — карточка перерисовывается.
  const off = goals.onChange(() => {
    off();
    requestAnimationFrame(() => card.isConnected && card.replaceWith(todayCard()));
  });
  return card;
}
