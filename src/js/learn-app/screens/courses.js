// 01 Курсы — главный экран: приветствие и серия, «Продолжить» с текущим
// шагом, плитки «Повторить» и «Фокус», мои курсы (и собирающийся —
// пунктиром), закреплённая «Собрать курс».

import { el, icon, button, label, progress, steps, loading, header, nav, register, prefs, plural, sheet, call, toast, isWeb } from "../core.js";
import { store, currentTopic, topicCard, greeting, isDone } from "../store.js";
import { open as openAssistant } from "../assistant.js";
import { lastInterview } from "./interview.js";

register("courses", (screen) => {
  screen.append(header({ home: true, onMic: () => openAssistant() }));
  const content = el("div", "content");
  content.append(loading());
  const sticky = el("div", "sticky");
  const build = button("", () => buildCourse(), "btn btn--outline");
  build.append(icon("plus", 20, 2), "Собрать курс");
  sticky.append(build);
  screen.append(content, sticky);

  let alive = true;
  let waitTimer = 0;
  (async () => {
    // Курсы, практика и вход — с устройства, без сети: экран рисуется сразу.
    // Сборки на сайте — сетевой запрос, их дорисовываем, когда придут.
    const [courses, , signed] = await Promise.all([
      store.courses(true),
      store.practice(true),
      call("account_status").catch(() => true),
    ]);
    if (!alive) return;
    content.replaceChildren();
    const course = store.currentCourse();
    nav.setBadge(course?.mastery?.due ?? 0);

    // Приветствие и серия дней.
    const hello = el("div", "hello");
    const name = prefs.get("name", "");
    hello.append(el("span", "hello__text", `${greeting()}${name ? `, ${name}` : ""}`));
    const streak = course?.focus?.streak ?? 0;
    if (streak > 0) {
      const chip = el("span", "chip chip--streak");
      chip.append(icon("flame", 16, 2.25), `${plural(streak, "день", "дня", "дней")} подряд`);
      hello.append(chip);
    }
    content.append(hello);

    if (!signed) content.append(signInCard());

    if (!courses.length && signed) {
      // Вошли, а курсов на устройстве ещё нет — они в пути с сайта.
      // Ждём их, а не пишем «курсов нет»: курс появится сам.
      const wait = el("div", "plate");
      wait.append(label("// курсы аккаунта"), loading("Подтягиваю курсы с сайта…"));
      content.append(wait);
      let tries = 0;
      const poll = async () => {
        tries += 1;
        const list = await store.courses(true);
        if (!alive) return;
        if (list.length) return nav.refresh();
        if (tries < 40) waitTimer = setTimeout(poll, 3000);
        else wait.replaceChildren(label("// курсов пока нет"), el("span", "muted", "В аккаунте курсов нет. Соберите первый кнопкой внизу — Ноа напишет его тема за темой."));
      };
      waitTimer = setTimeout(poll, 3000);
    } else if (!courses.length) {
      const empty = el("div", "plate");
      empty.append(
        label("// курсов пока нет"),
        el("span", "muted", "Соберите первый курс кнопкой внизу — Ноа напишет его тема за темой через ваш мост. Или войдите: подтянутся курсы аккаунта."),
      );
      content.append(empty);
    } else {
      content.append(continueCard(course));
      content.append(tiles(course));
      content.append(interviewCard(course));
    }

    // Мои курсы — с собирающимся пунктиром.
    const list = el("div", "list");
    if (courses.length) {
      list.append(label(`// мои курсы · ${courses.length}`));
      for (const item of courses) list.append(courseCard(item));
      content.append(list);
    }
    if (!signed) return;
    const builds = await store.builds(true);
    if (!alive) return;
    const running = builds.filter((b) => b.status === "running" && !courses.some((c) => c.id === b.courseId));
    if (!running.length) return;
    if (!list.isConnected) {
      list.append(label(`// мои курсы · ${courses.length}`));
      content.append(list);
    }
    for (const build of running) {
      const card = el("div", "plate plate--building");
      const top = el("div", "course-card__top");
      top.append(el("span", "course-card__title", build.title || "Новый курс"), el("span", "building", "собираю"));
      card.append(top, el("span", "small muted", build.total ? `Готово ${build.done} из ${build.total} тем` : "Составляю план курса"));
      list.append(card);
    }
  })();
  return {
    cleanup: () => {
      alive = false;
      clearTimeout(waitTimer);
    },
  };
});

function continueCard(course) {
  const topic = currentTopic(course);
  const own = topic ? store.practiceOf(course.id, topic.id) : null;
  const card = el("button", "plate plate--active");
  card.append(label("// продолжить"));
  const text = el("div", "concept");
  const mode = own && !own.done ? "практика" : topic?.read ? "проверка" : "урок";
  text.append(el("span", "continue__title", `${topic?.title ?? course.title} · ${mode}`));
  if (topic?.summary) text.append(el("span", "continue__lead", topic.summary));
  card.append(text);
  if (own && !own.done) card.append(steps(own.total, own.step, -1));
  else {
    const done = course.topics.filter(isDone).length;
    card.append(steps(Math.min(course.topics.length, 12), Math.round((done / Math.max(course.topics.length, 1)) * Math.min(course.topics.length, 12))));
  }
  const foot = el("div", "continue__foot");
  const at = course.topics.findIndex((t) => t.id === topic?.id) + 1;
  const where = own && !own.done ? `Шаг ${own.step + 1} из ${own.total}` : `Тема ${at} из ${course.topics.length}`;
  foot.append(el("span", "continue__meta", `${where} · ${course.title}`));
  const go = el("span", "btn btn--primary");
  go.append(icon("play", 16, 2.5), "Дальше");
  foot.append(go);
  card.append(foot);
  // Тап — сразу в текущий шаг.
  card.addEventListener("click", () => {
    if (!topic) return nav.push("course", { course: course.id });
    store.rememberCourse(course.id);
    if (own && !own.done) nav.push("practice", { course: course.id, topic: topic.id });
    else if (!topic.read) nav.push("lesson", { course: course.id, topic: topic.id });
    else nav.push("topic", { course: course.id, topic: topic.id });
  });
  return card;
}

function tiles(course) {
  const row = el("div", "tiles");
  const due = course?.mastery?.due ?? 0;
  const review = el("button", "plate");
  const head = el("span", "tile__head");
  head.append(icon("cards", 20, 2), label("Повторить"));
  review.append(head, el("span", "tile__value", due), el("span", "small muted", `карточек · ≈ ${Math.max(1, Math.round(due * 0.4))} мин`));
  review.addEventListener("click", () => nav.tab("review", true));
  const focus = el("button", "plate");
  const head2 = el("span", "tile__head");
  head2.append(icon("timer", 20, 2), label("Фокус"));
  // В настройке — номер длины отрезка (15 / 25 / 50), не минуты.
  const minutes = [15, 25, 50][prefs.get("focusLength", 1)] ?? 25;
  const value = el("span", "tile__value", minutes);
  value.append(el("small", "", " мин"));
  const today = course?.focus?.sessionsToday ?? 0;
  focus.append(head2, value, el("span", "small muted", `сегодня ${plural(today, "сессия", "сессии", "сессий")} · ${course?.focus?.today ?? 0} мин`));
  focus.addEventListener("click", () => nav.tab("focus", true));
  row.append(review, focus);
  return row;
}

/** Собеседование по курсу — мок-интервью, с прошлым баллом. */
function interviewCard(course) {
  const card = el("button", "plate interview-card");
  const head = el("span", "tile__head");
  head.append(icon("user", 20, 2), label("Собеседование"));
  const last = lastInterview(course.id);
  card.append(
    head,
    el("span", "interview-card__title", last ? `Прошлый раз — ${last.score}%` : "Мок-интервью по курсу"),
    el("span", "small muted", "Ноа задаёт вопросы вслух, уточняет и разбирает, чего не хватило"),
  );
  card.addEventListener("click", () => nav.push("interview", { course: course.id }));
  return card;
}

function courseCard(course) {
  const card = el("button", "plate");
  const top = el("div", "course-card__top");
  top.append(el("span", "course-card__title", course.title), el("span", "course-card__pct", `${course.percent}%`));
  const done = course.topics.filter(isDone).length;
  card.append(
    top,
    progress(course.percent),
    el("span", "small dim", `${done} из ${course.topics.length} тем · финальный экзамен ${course.finalUnlocked ? "открыт" : "закрыт"}`),
  );
  card.addEventListener("click", () => {
    store.rememberCourse(course.id);
    nav.push("course", { course: course.id });
  });
  return card;
}

/** Вход в аккаунт через Telegram — без ключа вручную. */
export function signInCard() {
  const card = el("div", "plate plate--active");
  card.append(label("// вход"), el("span", "continue__title", "Войдите в аккаунт"));
  card.append(el("span", "continue__lead", "Подтянутся курсы, прогресс и мост — то же, что на компьютере."));
  const status = el("span", "small muted");
  const go = button("", async () => {
    go.disabled = true;
    status.textContent = isWeb ? "Открываю вход на сайте…" : "Открываю Telegram — нажмите там Start и вернитесь сюда.";
    try {
      const name = await call("account_login");
      if (name) prefs.set("name", name);
      status.textContent = "Вход выполнен. Подтягиваю курсы…";
      // Курсы аккаунта приходят кусками — большой курс по мобильной сети
      // идёт минуту-другую.
      for (let i = 0; i < 60; i++) {
        await new Promise((resolve) => setTimeout(resolve, 3000));
        const list = await store.courses(true);
        if (list.length) break;
      }
      nav.refresh();
    } catch (err) {
      status.textContent = String(err);
      go.disabled = false;
    }
  }, "btn btn--primary");
  // В браузере — вход сайта (Telegram или Google), на телефоне — бот Telegram.
  go.append(icon("user", 16, 2.25), isWeb ? "Войти" : "Войти через Telegram");
  card.append(go, status);
  return card;
}

/** «Собрать курс»: цель и модель — сервер соберёт через мост. */
export function buildCourse() {
  sheet("Собрать курс", (content, close) => {
    content.append(el("span", "muted", "Напишите, о чём курс и зачем. Ноа составит план и напишет курс тема за темой: урок, понятия, карточки, задачи, мини-экзамен. Курс появится сам — по первой теме можно учиться сразу."));
    const goal = el("textarea", "field");
    goal.placeholder = "Например: Kubernetes с нуля до продакшена — чтобы пройти собеседование на DevOps";
    const quality = el("div", "seg");
    let level = "sonnet";
    for (const [value, text] of [
      ["sonnet", "Sonnet — точнее"],
      ["haiku", "Haiku — бережёт лимит"],
    ]) {
      const option = el("button", "", text);
      option.setAttribute("aria-pressed", String(value === level));
      option.addEventListener("click", () => {
        level = value;
        for (const other of quality.children) other.setAttribute("aria-pressed", String(other === option));
      });
      quality.append(option);
    }
    const error = el("span", "error");
    const go = button("Собрать курс", async () => {
      const text = goal.value.trim();
      if (text.length < 10) {
        error.textContent = "Опишите курс подробнее: о чём он и для чего — хотя бы одним предложением.";
        return;
      }
      go.disabled = true;
      error.textContent = "";
      try {
        await call("learn_build", { goal: text, quality: level }, { slow: true });
        close();
        toast("Курс собирается — первая тема появится через пару минут");
        nav.refresh();
      } catch (err) {
        error.textContent = String(err);
        go.disabled = false;
      }
    }, "btn btn--primary btn--big");
    content.append(goal, quality, error, go);
    goal.focus();
  });
}

export { topicCard };
