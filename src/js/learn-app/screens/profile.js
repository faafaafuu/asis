// Профиль — аккаунт, итоги занятий, настройки голоса, сборки курсов и версия.

import { el, icon, label, header, nav, register, call, prefs, toast, plural, loading } from "../core.js";
import { store } from "../store.js";
import { signInCard, buildCourse } from "./courses.js";

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
      card.append(el("span", "small muted", "noahlab.ru · курсы, прогресс и мост — общие с компьютером"));
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
    settings.append(label("// голос"));
    settings.append(
      setting("Читать вслух", "Ноа озвучивает разборы, ответы и замечания практики. Код не зачитывает.", prefs.get("aloud", true), (on) => {
        prefs.set("aloud", on);
        if (!on) call("voice_stop").catch(() => {});
      }),
    );
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
            await call("learn_build_stop", { id: build.id }, { slow: true });
            toast("Сборка остановлена");
            nav.refresh();
          } catch (err) {
            toast(String(err));
            stop.disabled = false;
          }
        });
        row.append(stop);
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

    const about = el("div", "note");
    about.append(
      label("// как это работает"),
      el(
        "span",
        "",
        "Уроки, разборы, проверка ответов и практика идут через ваш мост на noahlab.ru — ту же модель, что на компьютере. Если ответы не приходят, мост не запущен или его вход истёк.",
      ),
    );
    content.append(about);
    if (version) {
      const foot = el("span", "small dim", `NOAH Учёба · ${version}`);
      foot.style.textAlign = "center";
      content.append(foot);
    }
  })();
  return { cleanup: () => (alive = false) };
});
