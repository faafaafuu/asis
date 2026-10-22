// Аудиодрама к уроку: материал — живая сцена на три голоса. Одна и та же
// панель — в окне обучения на компьютере, в приложении на телефоне и в Ноа
// онлайн.
//
// Серию пишет и озвучивает сервер (минуты три). Пока пишется — можно
// заниматься дальше: панель следит за ходом сама. Готовая серия играет с
// подсветкой: кто говорит и что; щелчок по реплике — туда. После серии —
// вопросы, привязанные к сценам: ошибся — «вернуться к сцене» проигрывает
// тот самый момент истории. Через день — три вопроса на повтор.

const SETTINGS_KEY = "noa.drama.settings";
const LISTENED_KEY = "noa.drama.listened";
const LATER_MS = 20 * 3600_000;
const NAMES = { lena: "Лена", palych: "Палыч", narrator: "Рассказчик" };

function load(key, fallback) {
  try {
    return { ...fallback, ...JSON.parse(localStorage.getItem(key) ?? "{}") };
  } catch {
    return { ...fallback };
  }
}

function store(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* не запомнится — не беда */
  }
}

/** Серия строкой base64 (программа) или адресом (браузер) — в src для <audio>. */
function source(value) {
  if (/^(https?:|\/|blob:)/.test(value)) return value;
  const bytes = Uint8Array.from(atob(value), (c) => c.charCodeAt(0));
  return URL.createObjectURL(new Blob([bytes], { type: "audio/mpeg" }));
}

function clock(ms) {
  const s = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/**
 * Панель аудиодрамы для темы. `button(text, onClick)` — кнопка в стиле окна.
 */
export function dramaBox({ api, el, button, course, topic }) {
  const box = el("div", "drama");
  const row = el("div", "drama__row");
  // Одна серия на урок — про этот урок. Сериал складывается сам: серия
  // продолжает сюжет серии прошлого урока (сервер, drama.mjs).
  const goLesson = button("🎧 Аудиодрама", () => pick("lesson"));
  const gear = button("⚙", () => (tune.hidden = !tune.hidden));
  gear.title = "Уровень, имя джуна, длина серии";
  gear.classList.add("drama__gear");
  const status = el("span", "drama__status");
  row.append(goLesson, gear, status);

  // Настройки: уровень, имя джуна, длина серии.
  const prefs = load(SETTINGS_KEY, { level: "junior", junior: "Костя", minutes: 5 });
  const tune = el("div", "drama__tune");
  tune.hidden = true;
  const level = el("select", "drama__select");
  for (const [value, text] of [
    ["junior", "Джун — с нуля"],
    ["middle", "Мидл — тонкости и отказы"],
  ]) {
    const option = el("option", "", text);
    option.value = value;
    option.selected = prefs.level === value;
    level.append(option);
  }
  const junior = el("input", "drama__input");
  junior.value = prefs.junior;
  junior.placeholder = "Имя джуна";
  junior.maxLength = 30;
  const minutes = el("select", "drama__select");
  for (const value of [3, 5, 8]) {
    const option = el("option", "", `${value} мин`);
    option.value = String(value);
    option.selected = Number(prefs.minutes) === value;
    minutes.append(option);
  }
  const remember = () => store(SETTINGS_KEY, { level: level.value, junior: junior.value.trim() || "Костя", minutes: Number(minutes.value) });
  for (const field of [level, junior, minutes]) field.addEventListener("change", remember);
  const tuneLabel = (text, field) => {
    const wrap = el("label", "drama__field");
    wrap.append(el("span", "", text), field);
    return wrap;
  };
  tune.append(tuneLabel("Уровень", level), tuneLabel("Джун", junior), tuneLabel("Серия", minutes));
  tune.append(el("p", "drama__hint", "Персонажи: джун паникует и спрашивает, Лена объясняет, Палыч ворчит и даёт заповеди. Серия каждого урока продолжает историю прошлого. Настройки — для новых серий."));

  const player = el("div", "drama__player");
  player.hidden = true;
  box.append(row, tune, player);

  const jobs = { lesson: null, section: null };
  const timers = { lesson: 0, section: 0 };
  let shown = null;

  const say = (text, bad = false) => {
    status.textContent = text;
    status.classList.toggle("is-bad", bad);
  };

  const label = (scope) => {
    const job = jobs[scope];
    if (scope !== "lesson") return;
    const node = goLesson;
    const base = "Аудиодрама";
    if (!job) node.textContent = "🎧 Аудиодрама";
    else if (job.status === "running" && !job.ready) node.textContent = `⏳ ${base}…`;
    else if (job.status === "failed") node.textContent = `↻ ${base} — ещё раз`;
    else node.textContent = `▶ ${base}${job.total > 1 ? ` · ${job.ready} из ${job.total}` : ""}`;
  };

  function watch(scope, job) {
    clearTimeout(timers[scope]);
    jobs[scope] = job;
    label(scope);
    if (!box.isConnected) return;
    if (job.status === "failed" && shown?.scope === scope) say(job.message || "Аудиодрама не собралась.", true);
    // Дописалась — ход сборки больше не показываем: на телефоне «Пишу
    // сценарий» оставалось висеть рядом с готовой кнопкой «▶».
    if (job.status === "done" && /Пишу|Правлю|Жду|Озвучиваю|Свожу|Заказываю|сценари/.test(status.textContent)) {
      say(job.title ? `«${job.title}» готова — нажмите ▶` : "Готово — нажмите ▶");
    }
    if (job.status !== "running") return;
    if (shown?.scope === scope || !shown) {
      const started = Date.parse(`${job.created?.replace(" ", "T")}Z`) || Date.now();
      const part = job.total > 1 ? ` · ${job.message || `серия ${job.ready + 1} из ${job.total}`}` : "";
      say(`${job.stageText || "Пишу"}${part} · ${clock(Date.now() - started)} · можно заниматься дальше`);
    }
    timers[scope] = setTimeout(async () => {
      try {
        const fresh = await api.invoke("learn_drama_status", { id: job.id });
        // Дописалась новая серия — открыть её вкладку, играющую не трогать.
        if (shown?.scope === scope && fresh.ready > job.ready) {
          for (const tab of player.querySelectorAll(".drama__tab")) {
            if (Number(tab.textContent) <= fresh.ready) {
              tab.disabled = false;
              tab.title = `Серия ${tab.textContent}`;
              tab.onclick = () => openPlayer(scope, fresh, Number(tab.textContent));
            }
          }
        }
        watch(scope, fresh);
      } catch {
        watch(scope, job);
      }
    }, 4000);
  }

  async function order(scope, again = false) {
    remember();
    const settings = load(SETTINGS_KEY, {});
    say(scope === "lesson" ? "Заказываю аудиодраму…" : "Заказываю сериал…");
    try {
      watch(scope, await api.invoke("learn_drama_make", { course, topic, scope, settings, again }));
    } catch (err) {
      say(String(err), true);
    }
  }

  function pick(scope) {
    const job = jobs[scope];
    if (!job || job.status === "failed") return order(scope, Boolean(job));
    if (job.ready > 0) return shown?.scope === scope && !player.hidden ? (player.hidden = true) : openPlayer(scope, job, 1);
    say(job.stageText ? `${job.stageText}…` : "Пишется — подождите пару минут.");
  }

  /* ── Плеер ──────────────────────────────────────────────────────────── */

  async function openPlayer(scope, job, episodeNo = 1) {
    say("Загружаю…");
    let script;
    try {
      script = await api.invoke("learn_drama_script", { id: job.id });
    } catch (err) {
      return say(String(err), true);
    }
    const episodes = (script.episodes ?? []).filter((ep) => ep.ready);
    if (!episodes.length) return say("Серия ещё озвучивается.");
    const episode = episodes.find((ep) => ep.n === episodeNo) ?? episodes[0];
    shown = { scope, episode: episode.n, id: job.id };
    const names = { ...NAMES, junior: script.cast?.find((c) => c.id === "junior")?.name || "Костя" };

    let file;
    try {
      file = await api.invoke("learn_drama_file", { id: job.id, episode: episode.n });
    } catch (err) {
      return say(String(err), true);
    }
    say(script.title ? `«${script.title}»` : "");

    const head = el("div", "drama__head");
    head.append(el("span", "drama__title", `${script.title}${episodes.length > 1 || job.total > 1 ? ` · серия ${episode.n}` : ""}: ${episode.title}`));
    if (job.total > 1) {
      const tabs = el("div", "drama__tabs");
      for (let n = 1; n <= job.total; n++) {
        const ready = episodes.some((ep) => ep.n === n);
        const tab = button(String(n), () => ready && openPlayer(scope, job, n));
        tab.classList.add("drama__tab");
        tab.disabled = !ready;
        if (n === episode.n) tab.setAttribute("aria-current", "true");
        tab.title = ready ? `Серия ${n}` : "Ещё пишется";
        tabs.append(tab);
      }
      head.append(tabs);
    }

    const audio = el("audio", "drama__audio");
    audio.controls = true;
    audio.preload = "auto";
    audio.src = source(file);

    // Сейчас: ремарка сцены и реплика крупно.
    const now = el("div", "drama__now");
    const setting = el("p", "drama__setting");
    const who = el("span", "drama__who");
    const said = el("p", "drama__said");
    // Команда и что показал терминал — как в терминале, под репликой.
    const shell = el("div", "drama__shell");
    now.append(setting, who, said, shell);

    // Вся серия строками: щелчок — туда.
    const lines = new Map();
    const scenes = new Map();
    for (const scene of episode.scenes ?? []) {
      scenes.set(scene.id, scene);
      for (const line of scene.lines ?? []) lines.set(`${scene.id}/${line.id}`, line);
    }
    const timeline = episode.timeline ?? [];
    const list = el("ol", "drama__lines");
    const rows = timeline.map((mark) => {
      const line = lines.get(`${mark.scene}/${mark.line}`);
      const text = line?.display || (mark.line === "setting" ? scenes.get(mark.scene)?.setting : mark.line === "recap" ? episode.recap : mark.line === "hook" ? episode.hook : "");
      const item = el("li", `drama__line is-${mark.speaker}`);
      const body = el("span", "drama__body");
      body.append(el("span", "drama__text", text ?? ""), ...terminal(line));
      item.append(el("span", "drama__name", names[mark.speaker] ?? mark.speaker), body);
      item.addEventListener("click", () => {
        audio.currentTime = mark.start;
        audio.play().catch(() => {});
      });
      list.append(item);
      return item;
    });

    let stopAt = null;
    let current = -1;
    audio.addEventListener("timeupdate", () => {
      const t = audio.currentTime;
      if (stopAt != null && t >= stopAt) {
        audio.pause();
        stopAt = null;
      }
      const at = timeline.findIndex((mark, i) => t >= mark.start && t < (timeline[i + 1]?.start ?? Infinity));
      if (at === current || at < 0) return;
      rows[current]?.classList.remove("is-now");
      current = at;
      const mark = timeline[at];
      rows[at]?.classList.add("is-now");
      rows[at]?.scrollIntoView({ block: "nearest" });
      const scene = scenes.get(mark.scene);
      // Ремарку читает рассказчик — тогда она и есть реплика, второй раз не нужна.
      setting.textContent = mark.speaker === "narrator" ? "" : (scene?.setting ?? "");
      who.textContent = names[mark.speaker] ?? "";
      who.className = `drama__who is-${mark.speaker}`;
      said.textContent = rows[at]?.querySelector(".drama__text")?.textContent ?? "";
      shell.replaceChildren(...terminal(lines.get(`${mark.scene}/${mark.line}`)));
    });

    const quizBox = el("div", "drama__quiz");
    quizBox.hidden = true;
    const showQuiz = () => {
      quizBox.hidden = false;
      quizBox.replaceChildren(quiz(episode.quiz ?? [], (ref) => replay(ref)));
      quizBox.scrollIntoView({ block: "nearest", behavior: "smooth" });
    };
    // Вернуться к сцене: проиграть ту реплику и остановиться.
    const replay = (ref) => {
      const mark = timeline.find((m) => m.scene === ref?.scene && m.line === ref?.line);
      if (!mark) return;
      audio.currentTime = Math.max(0, mark.start - 0.2);
      stopAt = mark.end + 0.3;
      audio.play().catch(() => {});
    };
    audio.addEventListener("ended", () => {
      const listened = load(LISTENED_KEY, {});
      listened[`${job.id}/${episode.n}`] ||= Date.now();
      store(LISTENED_KEY, listened);
      showQuiz();
    });

    const tools = el("div", "drama__tools");
    tools.append(
      button("❓ Вопросы", showQuiz),
      button("↻ Пересобрать", () => {
        audio.pause();
        player.hidden = true;
        order(scope, true);
      }),
      button("Свернуть", () => {
        audio.pause();
        player.hidden = true;
      }),
    );

    player.replaceChildren(head, audio, now, list, tools, quizBox, later(episode, job.id));
    player.hidden = false;
    audio.play().catch(() => {});
  }

  /** «$ команда» и вывод терминала у реплики, где их вводят и смотрят. */
  function terminal(line) {
    const out = [];
    if (String(line?.command ?? "").trim()) out.push(el("code", "drama__cmd", `$ ${line.command.trim()}`));
    if (String(line?.output ?? "").trim()) out.push(el("pre", "drama__out", line.output.trim()));
    return out;
  }

  /** Вопросы по одному: верно — объяснение; нет — объяснение и «к сцене». */
  function quiz(questions, replay) {
    const wrap = el("div", "drama__questions");
    let at = 0;
    let right = 0;
    const kinds = { recall: "вспомнить", scenario: "что делать", compare: "чем отличается" };
    const draw = () => {
      if (at >= questions.length) {
        wrap.replaceChildren(el("p", "drama__done", `Готово: ${right} из ${questions.length}. Ошибки не считаются — их разбирали по сценам.`));
        return;
      }
      const q = questions[at];
      const card = el("div", "drama__q");
      card.append(el("span", "drama__kind", `${at + 1} из ${questions.length} · ${kinds[q.type] ?? q.type}`), el("p", "drama__ask", q.q));
      const result = el("div", "drama__result");
      const next = button(at + 1 < questions.length ? "Дальше →" : "Итог", () => {
        at += 1;
        draw();
      });
      next.hidden = true;
      const answer = (ok) => {
        if (ok) right += 1;
        result.replaceChildren(el("p", ok ? "is-good" : "is-bad", ok ? `Верно. ${q.explain ?? ""}` : `Ответ: ${q.answer}. ${q.explain ?? ""}`));
        if (!ok && q.ref) result.append(button("↺ Вернуться к сцене", () => replay(q.ref)));
        next.hidden = false;
      };
      // Модель любит ставить верный ответ первым — порядок свой.
      const options = Array.isArray(q.options) && q.options.length ? shuffled(q.options) : null;
      if (options) {
        const choices = el("div", "drama__options");
        for (const option of options) {
          const choice = button(option, () => {
            for (const other of choices.children) other.disabled = true;
            choice.classList.add(option === q.answer ? "is-good" : "is-bad");
            answer(option === q.answer);
          });
          choices.append(choice);
        }
        card.append(choices);
      } else {
        card.append(button("Показать ответ", () => answer(true)));
      }
      card.append(result, next);
      wrap.replaceChildren(card);
    };
    draw();
    return wrap;
  }

  /** Варианты в случайном порядке. */
  function shuffled(list) {
    const out = [...list];
    for (let i = out.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [out[i], out[j]] = [out[j], out[i]];
    }
    return out;
  }

  /** Повтор через день: три вопроса без подсказок из сцены. */
  function later(episode, id) {
    const box = el("div", "drama__later");
    const listened = load(LISTENED_KEY, {})[`${id}/${episode.n}`];
    const delayed = episode.delayed_quiz ?? [];
    if (!delayed.length) return box;
    if (!listened || Date.now() - listened < LATER_MS) {
      if (listened) box.append(el("p", "drama__hint", "Завтра здесь будут три вопроса на повтор — без подсказок."));
      return box;
    }
    box.append(el("span", "drama__kind", "Повтор через день"));
    box.append(quiz(delayed, () => {}));
    return box;
  }

  // Уже заказывали — показать, что есть.
  (async () => {
    try {
      const { dramas } = await api.invoke("learn_dramas", { course, topic });
      for (const job of dramas ?? []) if (job.scope !== "section") watch("lesson", job);
    } catch {
      /* без связи — кнопки всё равно есть */
    }
  })();

  return box;
}
