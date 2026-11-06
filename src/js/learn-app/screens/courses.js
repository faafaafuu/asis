// 01 Курсы — главный экран: приветствие и серия, «Продолжить» с текущим
// шагом, плитки «Повторить» и «Фокус», мои курсы (и собирающийся —
// пунктиром), закреплённая «Собрать курс».

import { el, icon, button, label, progress, steps, loading, header, nav, register, prefs, plural, sheet, call, toast, isWeb } from "../core.js";
import { featuresBox } from "../../learn-features.js";
import { store, currentTopic, topicCard, greeting, isDone } from "../store.js";
import { open as openAssistant } from "../assistant.js";
import { removeCourse, topicActions } from "./course.js";
import { swipeRow, pressMenu, collapse, pullToRefresh } from "../gestures.js";
import { guideShelf } from "./guide.js";
import { confirmAction } from "../gestures.js";
import { todayCard } from "./goals.js";
import { goals } from "../goals.js";

register("courses", (screen) => {
  screen.append(header({ home: true, onMic: () => openAssistant() }));
  const content = el("div", "content");
  content.append(loading());
  screen.append(content);
  // Потянуть вниз — сверить курсы с аккаунтом. Большой курс по мобильной
  // сети качается долго: кольцо крутится до 6 с, курс дорисуется сам.
  pullToRefresh(content, async () => {
    const sync = call("learn_sync").catch(() => false);
    sync.then((changed) => {
      if (!changed) return;
      store.forget();
      if (nav.current === "courses" && nav.depth === 1) nav.refresh();
    });
    await Promise.race([sync, new Promise((done) => setTimeout(done, 6000))]);
    store.forget();
    nav.refresh();
  });

  let alive = true;
  let waitTimer = 0;
  let buildTimer = 0;
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
      // Цели на сегодня — сразу под «Продолжить»: шаг дня тянет за собой курс.
      if (signed) {
        content.append(todayCard());
        goals.pull();
      }
      content.append(tiles(course));
    }

    // Мои курсы — строками 64, «+» справа собирает новый; собирающийся —
    // пунктиром.
    const list = el("div", "list m-courses");
    const head = el("div", "m-head");
    const add = el("button", "m-add");
    add.title = "Собрать курс";
    add.setAttribute("aria-label", "Собрать курс");
    add.append(icon("plus", 16, 2.25));
    add.addEventListener("click", () => buildCourse());
    head.append(label(`Мои курсы · ${courses.length}`), add);
    list.append(head);
    const rows = el("div", "m-rows");
    for (const item of courses) rows.append(courseCard(item));
    if (courses.length) list.append(rows);
    content.append(list);
    // Путеводители — ниже курсов; в браузере их нет, полки тоже.
    const guides = el("div");
    content.append(guides);
    guideShelf().then((shelf) => alive && shelf && guides.replaceWith(shelf));
    // Что использовать в учёбе: выключенное пропадает из темы и урока.
    const tune = el("button", "btn btn--plain tune-link");
    tune.append(icon("settings", 16, 2), "Что использовать в учёбе");
    tune.addEventListener("click", () =>
      sheet("Что использовать в учёбе", (box) => {
        box.append(el("span", "muted", "Выключенное не показывается в темах и уроках. Включить можно здесь же."));
        box.append(featuresBox({ el }));
      }),
    );
    content.append(tune);
    if (!signed) return;
    // Сборки — над списком курсов: идущая с «Остановить», упавшая или
    // остановленная — с причиной, «Продолжить» и «Убрать». Раньше упавшая
    // сборка просто исчезала, и было не понять, что случилось.
    const shelf = el("div", "list build-shelf");
    list.before(shelf);
    const paint = (builds) => {
      const shown = builds.filter((b) => b.status !== "done").slice(0, 2);
      shelf.replaceChildren(...shown.map((b) => buildCard(b, refreshBuilds)));
      shelf.hidden = !shown.length;
      return shown;
    };
    let seen = "";
    async function refreshBuilds() {
      clearTimeout(buildTimer);
      const builds = await store.builds(true);
      if (!alive) return;
      // Тема дописалась или сборка кончилась — курсы перечитываются: новый
      // курс появляется в списке сам.
      const mark = builds.map((b) => `${b.id}:${b.status}:${b.done}`).join("|");
      if (seen && mark !== seen) {
        store.forget();
        await store.courses(true);
        if (alive && nav.current === "courses" && nav.depth === 1) return nav.refresh();
      }
      seen = mark;
      const shown = paint(builds);
      if (shown.some((b) => b.status === "running")) buildTimer = setTimeout(refreshBuilds, 8000);
    }
    refreshBuilds();
  })();
  return {
    cleanup: () => {
      alive = false;
      clearTimeout(waitTimer);
      clearTimeout(buildTimer);
    },
  };
});

/**
 * «Продолжить»: подпись с темой, заголовок в две строки (48), полоса и
 * три кнопки 48 — раздел (основная), урок с Ноа, практика.
 */
function continueCard(course) {
  const topic = currentTopic(course);
  const own = topic ? store.practiceOf(course.id, topic.id) : null;
  const card = el("div", "m-active m-continue");
  const at = course.topics.findIndex((t) => t.id === topic?.id) + 1;
  card.append(label(`// продолжить · тема ${at} из ${course.topics.length}`));
  card.append(el("span", "m-continue__title", topic?.title ?? course.title));
  const line = el("div", "m-progress");
  line.append(progress(course.percent, "m-bar"), el("span", "m-progress__pct", `${course.percent}%`));
  card.append(line);
  const params = topic ? { course: course.id, topic: topic.id } : null;
  const go = (screen) => {
    if (!topic) return nav.push("course", { course: course.id });
    store.rememberCourse(course.id);
    nav.push(screen, params);
  };
  const section = topic?.step === "lesson" || topic?.step === "talk" ? (topic.section ?? 0) : 0;
  const actions = el("div", "m-continue__actions");
  const main = el("button", "m-btn m-btn--primary");
  main.append(icon("play", 16, 2.5), own && !own.done ? `Шаг ${own.step + 1}` : topic?.read || section ? `Раздел ${section + 1}` : "Начать");
  main.addEventListener("click", () => {
    if (own && !own.done) go("practice");
    else if (!topic?.read) go("lesson");
    else go("topic");
  });
  const talk = el("button", "m-btn m-btn--icon");
  talk.title = "Урок с Ноа";
  talk.setAttribute("aria-label", "Урок с Ноа");
  talk.append(icon("mic", 20, 2));
  talk.addEventListener("click", () => go("lesson"));
  actions.append(main, talk);
  if (!isWeb) {
    const practice = el("button", "m-btn m-btn--icon");
    practice.title = "Практика";
    practice.setAttribute("aria-label", "Практика");
    practice.append(icon("window", 20, 2));
    practice.addEventListener("click", () => go("practice"));
    actions.append(practice);
  } else {
    actions.classList.add("is-two");
  }
  card.append(actions);
  if (topic)
    pressMenu(card, () => [
      { text: "Открыть тему", icon: "chevron-right", run: () => nav.push("topic", { course: course.id, topic: topic.id }) },
      ...topicActions(course, topic),
      { text: "Весь курс", icon: "learn", run: () => nav.push("course", { course: course.id }) },
    ]);
  return card;
}

/** Две плитки 112: подпись и стрелка · число · пояснение. */
export function tiles(course) {
  const row = el("div", "m-tiles");
  const due = (course?.mastery?.due ?? 0) + (course?.mastery?.newLeft ?? 0);
  row.append(
    tileOf("Повторение", String(due), "карточек", due ? `≈ ${Math.max(1, Math.round(due * 0.25))} мин` : "на сегодня всё", () => nav.tab("review", true)),
    tileOf("Фокус", String(course?.focus?.today ?? 0), "мин сегодня", `${course?.focus?.week ?? 0} мин за неделю`, () => nav.tab("focus", true)),
  );
  return row;
}

function tileOf(name, value, unit, note, onClick) {
  const tile = el("button", "m-tile");
  const head = el("span", "m-tile__head");
  head.append(el("span", "m-tile__label", name), icon("chevron-right", 16, 2.25));
  const number = el("span", "m-tile__value");
  number.append(el("span", "m-tile__num", value), el("span", "m-tile__unit", unit));
  tile.append(head, number, el("span", "m-tile__note", note));
  tile.addEventListener("click", onClick);
  return tile;
}

const TILES = ["#2b5bc4", "#b5342e", "#5f8c4c", "#8a4fc4", "#c4842b", "#2b8fa8"];
const LATIN = { А: "A", Б: "B", В: "V", Г: "G", Д: "D", Е: "E", Ж: "Z", З: "Z", И: "I", Й: "I", К: "K", Л: "L", М: "M", Н: "N", О: "O", П: "P", Р: "R", С: "S", Т: "T", У: "U", Ф: "F", Х: "H", Ц: "C", Ч: "C", Ш: "S", Щ: "S", Э: "E", Ю: "U", Я: "Y" };

/** Плитка-монограмма курса: первые буквы двух значимых слов, цвет — из названия. */
export function monogram(title, size = "") {
  const words = title.split(/[\s:—,/-]+/).filter((w) => (w.length > 2 || /^\p{Lu}{2}$/u.test(w)) && /^[\p{L}\d]/u.test(w));
  const letters = words.slice(0, 2).map((w) => w[0].toUpperCase());
  if (/^[A-Z]/.test(letters[0] ?? "")) letters[1] = LATIN[letters[1]] ?? letters[1];
  let hash = 0;
  for (const ch of title) hash = (hash * 31 + ch.codePointAt(0)) >>> 0;
  const tile = el("span", `m-mono${size}`, (letters.join("") || title.slice(0, 2)).toUpperCase());
  tile.style.background = TILES[hash % TILES.length];
  return tile;
}

/** Строка курса 64: монограмма · название и % над полосой · стрелка. */
function courseCard(course) {
  const card = el("button", "m-row m-row--course");
  const body = el("span", "m-row__body");
  const line = el("span", "m-row__line");
  line.append(el("span", "m-row__title", course.title), el("span", `m-row__pct${course.percent ? " is-on" : ""}`, `${course.percent}%`));
  body.append(line, progress(course.percent, "m-bar m-bar--thin"));
  card.append(monogram(course.title), body, icon("chevron-right", 20, 2));
  const open = () => {
    store.rememberCourse(course.id);
    nav.push("course", { course: course.id });
  };
  card.addEventListener("click", open);
  // Смахнуть влево — «Удалить»; удержать — меню курса.
  const row = swipeRow(card, [{ text: "Удалить", icon: "trash", danger: true, run: () => drop() }]);
  const drop = async () => {
    if (!(await removeCourse(course))) return row.close();
    await collapse(row);
    nav.refresh();
  };
  pressMenu(card, () => [
    { text: "Открыть курс", icon: "chevron-right", run: open },
    { text: "Собеседование", icon: "user", run: () => nav.push("interview", { course: course.id }) },
    ...(course.finalUnlocked ? [{ text: "Финальный экзамен", icon: "target", run: () => nav.push("check", { course: course.id, scope: "final" }) }] : []),
    { text: "Удалить курс", icon: "trash", danger: true, run: () => drop() },
  ]);
  return row;
}

/** Как входят: подсказка под кнопкой, пока ждём возвращения. */
const WAIT_HINT = {
  telegram: "Открыла Telegram — нажмите там Start и вернитесь сюда.",
};

/**
 * Способы входа — те же, что на сайте и в Ноа: Google, Яндекс, VK, GitHub,
 * X, Telegram. Выбрал — открывается браузер или бот, приложение ждёт ключ.
 */
function chooseSignIn() {
  return new Promise((resolve, reject) => {
    let settled = false;
    sheet("Вход в аккаунт", async (content, close) => {
      content.append(el("span", "muted", "Один аккаунт на компьютере, телефоне и в браузере: курсы, прогресс и цели — везде."));
      const list = el("div", "signin");
      const status = el("span", "small muted");
      list.append(loading());
      content.append(list, status);
      let offer;
      try {
        offer = await call("account_login_options", {}, { retries: 1 });
      } catch (err) {
        list.replaceChildren();
        status.textContent = String(err);
        return;
      }
      list.replaceChildren();
      for (const option of offer.options ?? []) {
        const pick = button("", async () => {
          for (const other of list.querySelectorAll("button")) other.disabled = true;
          status.textContent = WAIT_HINT[option.id] ?? `Открыла вход через ${option.title} в браузере — войдите и вернитесь сюда.`;
          try {
            await call("account_login_open", { link: option.link }, { retries: 0 });
            const name = await call("account_login_wait", { code: offer.code }, { retries: 0, slow: true });
            settled = true;
            close();
            resolve(name);
          } catch (err) {
            status.textContent = String(err);
            for (const other of list.querySelectorAll("button")) other.disabled = false;
          }
        }, "btn btn--secondary signin__option");
        pick.append(el("span", `signin__mark signin__mark--${option.id}`, option.title.slice(0, 1)), el("span", "", option.title));
        list.append(pick);
      }
    }, () => {
      if (!settled) reject(new Error("closed"));
    });
  });
}

/** Вход в аккаунт — способами сайта, без ключа вручную. */
export function signInCard() {
  const card = el("div", "plate plate--active");
  card.append(label("// вход"), el("span", "continue__title", "Войдите в аккаунт"));
  card.append(el("span", "continue__lead", "Google, Яндекс, Telegram и другие — как на сайте. Подтянутся курсы, прогресс и цели со всех устройств."));
  const status = el("span", "small muted");
  const go = button("", async () => {
    go.disabled = true;
    try {
      let name;
      if (isWeb) {
        status.textContent = "Открываю вход на сайте…";
        name = await call("account_login");
      } else {
        name = await chooseSignIn().catch((err) => {
          if (err?.message === "closed") return null;
          throw err;
        });
        if (name === null) {
          go.disabled = false;
          return;
        }
      }
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
  go.append(icon("user", 16, 2.25), "Войти");
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
        await call("learn_build", { goal: text, quality: level }, { slow: true, retries: 0 });
        close();
        toast("Курс собирается — первая тема появится через несколько минут");
        store.forget();
        nav.home();
      } catch (err) {
        // Уже идёт сборка — её карточка на главной, с «Остановить».
        if (/уже собирается/i.test(String(err))) {
          close();
          toast(String(err).replace(/^Error:\s*/, ""), 5000);
          store.forget();
          nav.home();
          return;
        }
        error.textContent = String(err).replace(/^Error:\s*/, "");
        go.disabled = false;
      }
    }, "btn btn--primary btn--big");
    content.append(goal, quality, error, go);
    goal.focus();
  });
}

/**
 * Карточка сборки курса: что сейчас делается, сколько тем готово, а если
 * сборка встала — почему, словами, и что можно сделать.
 */
function buildCard(build, changed) {
  const running = build.status === "running";
  const card = el("div", running ? "plate plate--building build-card" : "plate build-card is-stopped");
  const top = el("div", "course-card__top");
  const state = { running: "собираю", failed: "не собрался", stopped: "остановлен" }[build.status] ?? build.status;
  top.append(el("span", "course-card__title", build.title || build.goal || "Новый курс"), el("span", running ? "building" : "small dim", state));
  card.append(top);
  if (build.total) card.append(progress((build.done / build.total) * 100, "m-bar"));
  const what = running ? build.message || (build.total ? `Готово ${build.done} из ${build.total} тем` : "Составляю план курса") : build.message;
  if (what) card.append(el("span", running ? "small muted" : "small build-card__why", what));
  if (build.total && !running) card.append(el("span", "small dim", `Готово ${build.done} из ${build.total} тем — по ним уже можно учиться`));
  const tools = el("div", "build-tools");
  const act = (text, run, kind = "btn btn--secondary") => {
    const btn = el("button", kind, text);
    btn.style.cssText = "height:40px;padding:0 14px";
    btn.addEventListener("click", async () => {
      for (const one of tools.children) one.disabled = true;
      try {
        await run();
        await changed();
      } catch (err) {
        const text = String(err?.message ?? err).replace(/^Error:\s*/, "");
        if (text) toast(text);
        for (const one of tools.children) one.disabled = false;
      }
    });
    return btn;
  };
  if (running) {
    tools.append(
      act("Остановить", async () => {
        const sure = await confirmAction({ title: "Остановить сборку?", text: "Готовые темы останутся — по ним можно учиться. Продолжить можно потом.", ok: "Остановить" });
        if (!sure) throw new Error("");
        await call("learn_build_stop", { id: build.id }, { slow: true });
        toast("Сборка остановлена");
      }),
    );
  } else {
    tools.append(
      act("Продолжить", async () => {
        await call("learn_build_resume", { id: build.id }, { slow: true, retries: 0 });
        toast("Сборка продолжается — с последней готовой темы");
      }, "btn btn--primary"),
      act("Убрать", () => call("learn_build_remove", { id: build.id }, { slow: true }), "btn btn--plain"),
    );
  }
  card.append(tools);
  return card;
}

export { topicCard };
