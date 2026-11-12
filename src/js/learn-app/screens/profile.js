// Профиль — аккаунт, итоги занятий, настройки голоса, сборки курсов и версия.

import { el, icon, label, header, nav, register, call, prefs, toast, plural, loading, isWeb, sheet } from "../core.js";
import { store } from "../store.js";
import { signInCard, buildCourse } from "./courses.js";
import { buildAction } from "../builder.js";
import { modelRow } from "../models.js";
import { confirmAction } from "../gestures.js";

function setting(name, hint, on, onToggle) {
  const row = el("button", "setting");
  const text = el("span", "setting__text");
  text.append(el("span", "setting__name", name), el("span", "setting__hint", hint));
  const toggle = el("span", "switch");
  toggle.setAttribute("aria-checked", String(on));
  row.setAttribute("role", "switch");
  row.setAttribute("aria-checked", String(on));
  row.append(text, toggle);
  row.addEventListener("click", () => {
    const next = toggle.getAttribute("aria-checked") !== "true";
    toggle.setAttribute("aria-checked", String(next));
    row.setAttribute("aria-checked", String(next));
    onToggle(next);
  });
  return row;
}

function link(name, hint, iconName, onClick) {
  const row = el("button", "setting");
  const text = el("span", "setting__text");
  text.append(el("span", "setting__name", name));
  if (hint) text.append(el("span", "setting__hint", hint));
  row.append(icon(iconName, 20, 2), text, icon("chevron-right", 18, 2));
  row.firstChild.style.color = "var(--c-accent)";
  row.addEventListener("click", onClick);
  return row;
}

register("profile", (screen) => {
  screen.append(header({ big: "Профиль" }));
  const content = el("div", "content");
  content.append(loading());
  screen.append(content);
  let alive = true;

  (async () => {
    const [courses, signed, builds, version] = await Promise.all([
      store.courses(true),
      call("account_status").catch(() => false),
      store.builds(true),
      call("app_version").catch(() => ""),
    ]);
    if (!alive) return;
    content.replaceChildren();

    if (signed) {
      const card = el("div", "plate");
      const name = prefs.get("name", "");
      card.append(label("// аккаунт"), el("span", "continue__title", name || "Вход выполнен"));
      card.append(el("span", "small muted", "Курсы, прогресс и цели — общие на телефоне, компьютере и в браузере"));
      // Выйти — чтобы войти другим способом (другим аккаунтом).
      const out = el("button", "btn btn--plain", "Выйти");
      out.style.cssText = "height:36px;padding:0;justify-content:flex-start";
      out.addEventListener("click", async () => {
        const sure = await confirmAction({ title: "Выйти из аккаунта?", text: "Курсы и прогресс останутся в аккаунте — войдёте снова, и они вернутся.", ok: "Выйти" });
        if (!sure) return;
        out.disabled = true;
        try {
          await call("account_logout", {}, { retries: 0 });
          prefs.set("name", "");
          store.forget();
          nav.home();
        } catch (err) {
          toast(String(err));
          out.disabled = false;
        }
      });
      card.append(out);
      content.append(card);
    } else content.append(signInCard());

    // Итоги: фокус и курсы.
    const course = store.currentCourse();
    const f = course?.focus ?? { today: 0, week: 0, streak: 0 };
    const tiles = el("div", "tiles");
    const tile = (title, value, small, iconName) => {
      const box = el("div", "plate");
      const head = el("span", "tile__head");
      head.append(icon(iconName, 20, 2), label(title));
      const big = el("span", "tile__value", value);
      if (small) big.append(el("small", "", small));
      box.append(head, big);
      return box;
    };
    tiles.append(tile("Фокус", String(f.week), " мин за неделю", "timer"), tile("Серия", String(f.streak), ` ${plural(f.streak, "день", "дня", "дней").split(" ")[1]}`, "flame"));
    content.append(tiles);
    const mature = courses.reduce((sum, c) => sum + (c.mastery?.mature ?? 0), 0);
    const total = courses.reduce((sum, c) => sum + (c.mastery?.total ?? 0), 0);
    content.append(
      el(
        "span",
        "small dim",
        `${plural(courses.length, "курс", "курса", "курсов")} · ${total ? `${mature} из ${total} понятий держатся` : "понятий пока нет"} · сегодня ${f.today} мин фокуса`,
      ),
    );

    // Настройки.
    const settings = el("div", "list");
    // Вид: Неон или Классика — тема NOAH, светлая.
    settings.append(label("// вид"));
    const look = el("div", "seg");
    for (const [id, name] of [
      ["neon", "Неон"],
      ["classic", "Классика"],
    ]) {
      const btn = el("button", "", name);
      btn.setAttribute("aria-pressed", String(prefs.get("look", "neon") === id));
      btn.addEventListener("click", () => {
        prefs.set("look", id);
        for (const other of look.children) other.setAttribute("aria-pressed", String(other === btn));
        dispatchEvent(new Event("learn:look"));
      });
      look.append(btn);
    }
    settings.append(look);
    settings.append(label("// голос"));
    settings.append(
      setting("Читать вслух", "Ноа озвучивает разборы, ответы и замечания практики. Код не зачитывает.", prefs.get("aloud", true), (on) => {
        prefs.set("aloud", on);
        if (!on) call("voice_stop").catch(() => {});
      }),
    );
    // Голос: Светлана с сервера — как у Ноа на компьютере; Ирина в
    // телефоне — сразу и без сети.
    const current = await call("phone_voice", {}).catch(() => null);
    if (current) {
      settings.append(
        setting(
          "Голос как на компьютере",
          "Светлана — живой нейроголос, как у Ноа на ПК; каждая фраза ждёт сервер. Выключено — Ирина прямо в телефоне: сразу и без сети.",
          current === "server",
          (on) => call("phone_voice", { voice: on ? "server" : "phone" }).catch((err) => toast(String(err))),
        ),
      );
    }
    // Модель — тем же окном везде: в приложении и в браузере, как у Ноа.
    settings.append(label("// модель"), await modelRow());
    content.append(settings);

    // Курсы: собрать, идущие сборки.
    const make = el("div", "list");
    make.append(label("// курсы"));
    make.append(link("Собрать курс", "По цели — Ноа напишет курс тема за темой через мост", "plus", () => buildCourse()));
    for (const build of builds.slice(0, 3)) {
      const row = el("div", build.status === "running" ? "plate plate--building" : "plate");
      const top = el("div", "course-card__top");
      top.append(
        el("span", "course-card__title", build.title || build.goal || "Курс"),
        el("span", build.status === "running" ? "building" : "small dim", { running: "собираю", done: "готов", failed: "ошибка", stopped: "остановлен" }[build.status] ?? build.status),
      );
      row.append(top);
      if (build.message) row.append(el("span", "small muted", build.message));
      if (build.status === "running") {
        const stop = el("button", "btn btn--plain", "Остановить — готовые темы останутся");
        stop.style.cssText = "height:32px;padding:0;justify-content:flex-start;font-size:12.5px";
        stop.addEventListener("click", async () => {
          if (!stop.dataset.sure) {
            stop.dataset.sure = "1";
            stop.textContent = "Точно остановить? Нажмите ещё раз";
            return;
          }
          stop.disabled = true;
          try {
            await buildAction(build, "stop");
            toast("Сборка остановлена");
            nav.refresh();
          } catch (err) {
            toast(String(err));
            stop.disabled = false;
          }
        });
        row.append(stop);
      }
      if (build.status === "failed" || build.status === "stopped") {
        const tools = el("div", "build-tools");
        const resume = el("button", "btn btn--plain", "Продолжить");
        const remove = el("button", "btn btn--plain", "Убрать");
        resume.addEventListener("click", async () => {
          resume.disabled = remove.disabled = true;
          try {
            await buildAction(build, "resume");
            toast("Сборка продолжается — с последней готовой темы");
            await store.builds(true);
            nav.refresh();
          } catch (err) {
            toast(String(err));
            resume.disabled = remove.disabled = false;
          }
        });
        remove.addEventListener("click", async () => {
          resume.disabled = remove.disabled = true;
          try {
            await buildAction(build, "remove");
            await store.builds(true);
            row.remove();
          } catch (err) {
            toast(String(err));
            resume.disabled = remove.disabled = false;
          }
        });
        tools.append(resume, remove);
        row.append(tools);
      }
      make.append(row);
    }
    make.append(
      link("Обновить курсы", "Подтянуть свежие курсы и прогресс с сайта", "learn", async () => {
        store.forget();
        await store.courses(true);
        toast("Курсы обновлены");
        nav.refresh();
      }),
    );
    content.append(make);

    content.append(label("// справка"), link("Как пользоваться Qrack", "Суфлёр, разговор по теме, чтение вслух, модели, сборка курса", "help", () => openHelp()));
    // Удалить аккаунт — из самого приложения, со всем обучением.
    if (signed) {
      const erase = el("button", "setting setting--danger");
      const text = el("span", "setting__text");
      text.append(el("span", "setting__name", "Удалить аккаунт"), el("span", "setting__hint", "Курсы, прогресс и цели пропадут на всех устройствах"));
      erase.append(icon("trash", 20, 2), text);
      erase.addEventListener("click", async () => {
        const sure = await confirmAction({
          title: "Удалить аккаунт навсегда?",
          text: "Курсы, прогресс, цели и ключи доступа сотрутся с сервера и со всех устройств. Это не отменить.",
          ok: "Удалить навсегда",
        });
        if (!sure) return;
        erase.disabled = true;
        try {
          await call("account_delete", {}, { slow: true, retries: 0 });
          for (const key of ["goals", "goals.pending", "name", "course", "interviews", "interviewSession", "drilled"]) prefs.set(key, null);
          store.forget();
          toast("Аккаунт удалён");
          nav.refresh();
        } catch (err) {
          erase.disabled = false;
          toast(String(err).replace(/^Error:\s*/, ""));
        }
      });
      content.append(erase);
    }
    if (version) {
      const foot = el("span", "small dim", `Qrack · ${version}`);
      foot.style.textAlign = "center";
      content.append(foot);
    }
  })();
  return { cleanup: () => (alive = false) };
});

/** Справка: что умеет Qrack и где это найти — в одном месте, а не на каждом экране. */
const HELP = [
  ["Непонятное слово — выделите", "Выделите слово, формулу или фразу в уроке, карточке или ответе (на телефоне — задержите палец) — появится «Объяснить». Ноа объяснит в смысле абзаца, а дальше можно уточнять текстом или голосом, сколько нужно."],
  ["Разобрать подробно и Обсудить", "Под каждым разделом урока: «Разобрать подробно» — раздел по шагам с примером, «Обсудить» — разговор по разделу голосом или текстом."],
  ["Чтение вслух и пауза", "Значок динамика в уроке читает раздел. Нажмите ещё раз — пауза; снова — продолжит с того же места, даже если вы выходили из урока."],
  ["Модель", "Профиль → Модель. Мост NOAH — без ключа: Claude Sonnet (подробнее) или Haiku (быстрее), ChatGPT, Gemini, Qwen. Или свой ключ: OpenRouter, Gemini, Groq, Claude; на компьютере — Ollama."],
  ["Сборка курса", "«+» в «Моих курсах»: опишите цель, выберите «Подробно», «Быстро» или «Суть» и что будет в курсе. С мостом курс собирает сервер — приложение можно закрыть; с другой моделью держите его открытым."],
  ["Цели", "Год → месяц → неделя → сегодня. Задачи дня отмечаются сами: урок прочитан, экзамен сдан, карточки повторены."],
];

function openHelp() {
  sheet("Как пользоваться Qrack", (box) => {
    for (const [title, text] of HELP) {
      const item = el("div", "help-item");
      item.append(el("span", "help-item__title", title), el("span", "help-item__text", text));
      box.append(item);
    }
  });
}
