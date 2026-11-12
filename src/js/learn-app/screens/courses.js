// 01 Курсы — главный экран: приветствие и серия, «Продолжить» с текущим
// шагом, плитки «Повторить» и «Фокус», мои курсы (и собирающийся —
// пунктиром), закреплённая «Собрать курс».

import { el, icon, button, label, progress, steps, loading, header, nav, register, prefs, plural, sheet, call, toast, isWeb } from "../core.js";
import { featuresBox, uses, suggestChoice, FEATURES } from "../../learn-features.js";
import { store, currentTopic, topicCard, greeting, isDone } from "../store.js";
import { open as openAssistant } from "../assistant.js";
import { removeCourse, topicActions } from "./course.js";
import { swipeRow, pressMenu, collapse, pullToRefresh } from "../gestures.js";
import { guideShelf } from "./guide.js";
import { confirmAction } from "../gestures.js";
import { todayCard } from "./goals.js";
import { goals } from "../goals.js";
import { buildLocal, buildAction, onBuild } from "../builder.js";
import { modelRow, openModels, bridgeLevel } from "../models.js";
import { setCourseChoice } from "../../learn-features.js";

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
  let stopWatch = null;
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

    if (!courses.length && signed && isWeb) {
      // В браузере курсы аккаунта приходят сразу — ждать нечего: курсов нет.
      const name = await call("account_name").catch(() => "");
      const empty = el("div", "plate");
      empty.append(
        label("// курсов пока нет"),
        el("span", "muted", `${name ? `Вы вошли как ${name}. ` : ""}В этом аккаунте курсов нет — соберите первый кнопкой «+». Если курсы были в другом аккаунте (например, входили через Telegram, а сейчас через Google), войдите тем способом: Профиль → Выйти.`),
      );
      content.append(empty);
    } else if (!courses.length && signed) {
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
    // Сборки — над списком курсов: идущая с «Остановить», упавшая или
    // остановленная — с причиной, «Продолжить» и «Убрать». Раньше упавшая
    // сборка просто исчезала, и было не понять, что случилось.
    const shelf = el("div", "list build-shelf");
    list.before(shelf);
    const paint = (builds) => {
      const wanted = prefs.get("serverChoice");
      const named = wanted && builds.find((b) => !b.local && b.goal === wanted.goal && b.courseId);
      if (named) {
        setCourseChoice(named.courseId, wanted.choice);
        prefs.set("serverChoice", null);
      }
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
    stopWatch = onBuild(() => {
      if (alive) store.builds().then((builds) => alive && paint(builds));
    });
  })();
  return {
    cleanup: () => {
      alive = false;
      clearTimeout(waitTimer);
      clearTimeout(buildTimer);
      stopWatch?.();
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
  if (!isWeb && uses("practice", course)) {
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
      // Почта и пароль — те же аккаунты, что на сайте.
      const mail = button("", () => {
        mail.hidden = true;
        form.hidden = false;
        email.focus();
      }, "btn btn--secondary signin__option");
      mail.append(el("span", "signin__mark signin__mark--mail", "@"), el("span", "", "Почта и пароль"));
      list.append(mail);
      const form = el("div", "signin__form");
      form.hidden = true;
      const email = el("input", "field");
      email.type = "email";
      email.autocomplete = "email";
      email.placeholder = "Почта";
      const password = el("input", "field");
      password.type = "password";
      password.autocomplete = "current-password";
      password.placeholder = "Пароль — от 10 знаков";
      const send = (create) => async () => {
        if (!email.value.trim() || !password.value) {
          status.textContent = "Впишите почту и пароль.";
          return;
        }
        for (const one of form.querySelectorAll("button")) one.disabled = true;
        status.textContent = create ? "Создаю аккаунт…" : "Вхожу…";
        try {
          const name = await call("account_login_password", { email: email.value.trim(), password: password.value, create }, { retries: 0 });
          settled = true;
          close();
          resolve(name);
        } catch (err) {
          status.textContent = String(err?.message ?? err).replace(/^Error:\s*/, "");
          for (const one of form.querySelectorAll("button")) one.disabled = false;
        }
      };
      const row = el("div", "signin__row");
      row.append(button("Войти", send(false), "btn btn--primary"), button("Создать аккаунт", send(true), "btn btn--plain"));
      form.append(email, password, row);
      list.append(form);
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
      const name = await chooseSignIn().catch((err) => {
        if (err?.message === "closed") return null;
        throw err;
      });
      if (name === null) {
        go.disabled = false;
        return;
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

/**
 * «Собрать курс»: цель словами — и курс собирается прямо здесь, на модели
 * из Профиль → Модель, как в Ноа.
 */
export function buildCourse() {
  sheet("Собрать курс", (content, close) => {
    content.append(el("span", "muted", "Напишите, о чём курс и зачем. Ноа составит план и напишет курс тема за темой: урок, понятия, карточки, задачи, мини-экзамен. По первой теме можно учиться сразу, пока остальные дописываются."));
    const goal = el("textarea", "field");
    goal.placeholder = "Например: матанализ к сессии — пределы, производные, Тейлор, интегралы; нужны доказательства для устного";
    // На какой модели собирать — здесь же, с «Сменить»: без модели курс не собрать.
    const model = el("div", "build-model");
    const noModel = el("span", "error");
    let onServer = false;
    let level = "sonnet";
    let engine = "claude-code-bridge";
    const checkModel = async () => {
      const ai = await call("ai_settings").catch(() => null);
      const ready = Boolean(ai?.endpoint);
      noModel.textContent = ready ? "" : "Модель не выбрана — нажмите на строку выше и выберите OpenRouter, Ollama, мост или свою.";
      go.disabled = !ready;
      // Мост NOAH — курс собирает сервер: приложение можно закрыть.
      onServer = /\/api\/noa\/bridge/.test(ai?.endpoint ?? "");
      level = bridgeLevel(ai?.model);
      engine = String(ai?.model ?? "").split(":")[0] || "claude-code-bridge";
      note.textContent = onServer
        ? "Курс собирает сервер — приложение можно закрыть, сборка не прервётся. Первая тема появится через несколько минут."
        : "Курс собирается в приложении на этой модели — не закрывайте его, пока идёт сборка. Прервётся — «Продолжить» доберёт с готовой темы.";
      return ready;
    };
    modelRow().then((row) => {
      // Строка открывает выбор модели; после выбора — проверяем ещё раз.
      row.addEventListener("click", () => setTimeout(checkModel, 400));
      model.replaceChildren(row);
    });
    // Подробно или быстро: на медленной модели «быстро» — в 2–3 раза скорее.
    let depth = "full";
    const speed = el("div", "seg");
    for (const [value, text] of [
      ["full", "Подробно"],
      ["brief", "Быстро — короче уроки"],
    ]) {
      const option = el("button", "", text);
      option.type = "button";
      option.setAttribute("aria-pressed", String(value === depth));
      option.addEventListener("click", () => {
        depth = value;
        for (const other of speed.children) other.setAttribute("aria-pressed", String(other === option));
      });
      speed.append(option);
    }
    const note = el("span", "small muted", "Не закрывайте приложение, пока идёт сборка: прервётся — «Продолжить» доберёт с готовой темы. Учиться можно с первой темы.");
    // Что нужно в курсе: лишнее не показывается и не тратит силы модели.
    // Пока человек не трогал переключатели, они следуют за темой: для
    // матанализа терминал выключен, для DevOps — включён.
    let choice = suggestChoice("");
    let touched = false;
    // Всё на виду: чипы с галочкой — нажал, и часть включилась или выключилась.
    const parts = el("div", "build-parts");
    const summary = el("span", "small dim", "");
    const list = el("div", "build-parts__grid");
    const chips = {};
    for (const feature of FEATURES) {
      const chip = el("button", "build-part");
      chip.type = "button";
      chip.title = feature.about;
      chip.append(el("span", "build-part__tick"), el("span", "", feature.name));
      chip.addEventListener("click", () => {
        touched = true;
        choice[feature.id] = choice[feature.id] === false;
        paint();
      });
      chips[feature.id] = chip;
      list.append(chip);
    }
    const paint = () => {
      for (const [id, chip] of Object.entries(chips)) chip.setAttribute("aria-pressed", String(choice[id] !== false));
      const off = FEATURES.filter((f) => choice[f.id] === false).map((f) => f.name.toLowerCase());
      summary.textContent = off.length ? `Не будет: ${off.join(", ")}` : "Всё включено";
    };
    goal.addEventListener("input", () => {
      if (touched) return;
      choice = suggestChoice(goal.value);
      paint();
    });
    paint();
    parts.append(label("// что будет в курсе"), list, summary);
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
        if (!(await checkModel())) {
          go.disabled = false;
          return;
        }
        if (onServer) {
          await call("learn_build", { goal: text, quality: level, depth, model: engine }, { slow: true, retries: 0 });
          // Что в курсе — когда сервер назовёт курс (карточка сборки это увидит).
          prefs.set("serverChoice", { goal: text, choice: { ...choice } });
        } else {
          buildLocal(text, { ...choice }, depth);
        }
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
    content.append(model, noModel, goal, speed, parts, note, error, go);
    checkModel();
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
  const badge = el("span", running ? "building build-card__state" : "small dim", state);
  // Идёт — крутится кольцо: видно, что сборка жива.
  if (running) badge.prepend(el("span", "build-spin"));
  top.append(el("span", "course-card__title", build.title || build.goal || "Новый курс"), badge);
  card.append(top);
  // Полоса с процентами: план — первые 5 %, дальше каждая тема — урок,
  // понятия, вопросы; пока плана нет — бегущая полоса.
  const percent = buildPercent(build);
  if (running && percent === null) card.append(el("div", "m-bar build-bar--wait"));
  else if (percent !== null) {
    const bar = el("div", "build-bar");
    bar.append(progress(percent, "m-bar"), el("span", "build-bar__pct", `${Math.round(percent)}%`));
    card.append(bar);
  }
  const what = running ? build.message || (build.total ? `Готово ${build.done} из ${build.total} тем` : "Составляю план курса") : build.message;
  if (running && build.total) card.append(el("span", "small build-card__step", `Тема ${Math.min(build.done + 1, build.total)} из ${build.total}`));
  if (what) card.append(el("span", running ? "small muted" : "small build-card__why", what));
  // Сборка на устройстве: сколько идёт, на какой модели, сколько модель думает
  // над этим шагом. Долго молчит — так и пишем, и даём сменить модель.
  if (running && !build.local) {
    const fresh = build.updated ? Date.parse(`${String(build.updated).replace(" ", "T")}Z`) : NaN;
    const ago = Number.isFinite(fresh) ? ` · последний шаг ${elapsed(Date.now() - fresh)} назад` : "";
    card.append(el("span", "small dim build-card__time", `собирает сервер — приложение можно закрыть${ago}`));
  }
  if (running && build.local && build.startedAt) {
    const now = Date.now();
    const parts = [`идёт ${elapsed(now - build.startedAt)}`];
    if (build.model) parts.push(build.model);
    if (build.waitingSince) parts.push(`модель думает ${elapsed(now - build.waitingSince)}`);
    card.append(el("span", "small dim build-card__time", parts.join(" · ")));
    if (build.waitingSince && now - build.waitingSince > 180_000) {
      const slow = el("span", "small build-card__why", "Модель отвечает дольше 3 минут — так бывает на бесплатных и локальных моделях. Можно подождать, остановить или сменить модель: готовые темы останутся, «Продолжить» доберёт с того же места.");
      card.append(slow);
    }
  }
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
  if (running && build.local) tools.append(act("Сменить модель", async () => openModels(), "btn btn--plain"));
  if (running) {
    tools.append(
      act("Остановить", async () => {
        const sure = await confirmAction({ title: "Остановить сборку?", text: "Готовые темы останутся — по ним можно учиться. Продолжить можно потом.", ok: "Остановить" });
        if (!sure) throw new Error("");
        await buildAction(build, "stop");
        toast("Сборка остановлена");
      }),
    );
  } else {
    tools.append(
      act("Продолжить", async () => {
        await buildAction(build, "resume");
        toast("Сборка продолжается — с последней готовой темы");
      }, "btn btn--primary"),
      act("Убрать", () => buildAction(build, "remove"), "btn btn--plain"),
    );
  }
  card.append(tools);
  return card;
}

export { topicCard };

/** 1 мин 20 с, 45 с — сколько идёт сборка или думает модель. */
function elapsed(ms) {
  const sec = Math.max(0, Math.round(ms / 1000));
  if (sec < 60) return `${sec} с`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min} мин${sec % 60 ? ` ${sec % 60} с` : ""}`;
  return `${Math.floor(min / 60)} ч ${min % 60} мин`;
}

/**
 * Сколько собрано, в процентах: план — 5 %, остальное — темы поровну, а
 * внутри темы урок, понятия и вопросы — по трети. Плана ещё нет — null.
 */
function buildPercent(build) {
  if (!build.total) return build.status === "running" ? null : 0;
  if (build.status === "done") return 100;
  const per = 95 / build.total;
  const message = String(build.message ?? "");
  const inside = /задачи и экзамен/i.test(message) ? 2 / 3 : /понятия/i.test(message) ? 1 / 3 : 0;
  const finishing = /финальн/i.test(message);
  return Math.min(99, 5 + per * (build.done + (finishing ? 0 : inside)));
}
