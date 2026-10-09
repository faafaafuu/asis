// 04 Практика — Ноа смотрит терминал. Сверху кольцо и шаг, под ним карточка
// шага, терминал свёрнут в последние строки («Развернуть» — на весь экран),
// лента Ноа и поле вопроса с микрофоном и «Готово» (Ноа проверяет шаг).
// Терминал на телефоне — сервер по SSH; сессия живёт, пока жива программа,
// и переживает уход с экрана.

import { el, icon, label, loading, nav, register, iconButton, steps, call, api, voice, ring, prefs, toast, inline, sheet } from "../core.js";
import { store, topicCard } from "../store.js";
import { keyBar, danger, typedLine, confirmDanger } from "../termkeys.js";

/* ── Терминал: один на всё приложение ──────────────────────────────────── */

let term = null;
let fit = null;
let layer = null;
let started = false;
let connected = false;
let practice = null;
/** Экран практики, если он сейчас на виду: получает события программы. */
let view = null;
/** Полоса клавиш терминала (termkeys.js). */
let keys = null;
/** Экран xterm: живёт то в окошке на экране практики, то на весь экран. */
let screenEl = null;
/** Окошко терминала на открытом экране практики. */
let inlineHost = null;
/** Команда проверки текущего шага — для кнопки на полосе клавиш. */
let stepCheck = "";
/**
 * Куда подключались в этот запуск. iOS усыпляет приложение в фоне, и SSH
 * рвётся за секунды; по возвращении практика подключается снова сама.
 * Пароль — только в памяти, до закрытия приложения.
 */
let lastTarget = null;
let reconnecting = null;

/** Переподключиться к прошлому серверу. true — получилось. */
async function reconnect() {
  if (connected) return true;
  if (!lastTarget) return false;
  reconnecting ??= (async () => {
    try {
      await settle();
      await api.invoke("practice_connect", { ...lastTarget, cols: term.cols, rows: term.rows });
      connected = true;
      term.write("\r\n\x1b[2m[снова на связи]\x1b[0m\r\n");
      return true;
    } catch {
      return false;
    } finally {
      reconnecting = null;
    }
  })();
  return reconnecting;
}

document.addEventListener("visibilitychange", async () => {
  if (document.visibilityState !== "visible" || connected || !lastTarget) return;
  if (await reconnect()) view?.update();
});

/** Терминал (xterm) грузится, только когда открыли практику: это самый
 *  тяжёлый файл приложения, а нужен он одному экрану. */
let xterm = null;

async function makeTerminal() {
  if (term) return;
  xterm ??= Promise.all([import("../../../vendor/xterm/xterm.js"), import("../../../vendor/xterm/addon-fit.js")]);
  const [{ Terminal }, { FitAddon }] = await xterm;
  if (term) return;
  term = new Terminal({
    fontFamily: '"JetBrains Mono", ui-monospace, monospace',
    fontSize: 12,
    lineHeight: 1.15,
    cursorBlink: true,
    scrollback: 5000,
    theme: { background: "#020407", foreground: "#b9e3e8", cursor: "#00f0ff", selectionBackground: "rgba(0,240,255,0.28)" },
  });
  fit = new FitAddon();
  term.loadAddon(fit);

  layer = el("div", "fullterm");
  const head = el("div", "fullterm__head");
  const down = el("button", "btn btn--plain");
  down.append(icon("chevron-right", 18, 2.25), "Свернуть");
  down.firstChild.style.transform = "rotate(90deg)";
  down.addEventListener("click", () => fold());
  head.append(down, el("span", "grow"), el("span", "small dim", "Опасные команды Ноа переспросит"));
  const screen = el("div", "fullterm__screen");
  screenEl = screen;
  keys = keyBar({ term, write, step: () => stepCheck });
  layer.append(head, screen, keys.el);
  document.body.append(layer);
  term.open(screen);

  term.onData((data) => {
    data = keys.withCtrl(data);
    // Enter — сначала проверить строку: опасное (rm -rf /, mkfs, остановка
    // SSH…) выполняется только после «Выполнить».
    if (data === "\r") {
      const line = typedLine(term);
      const why = danger(line);
      if (why) return confirmDanger(line.trim(), why, () => write("\r"));
    }
    write(data);
  });
  let keysTimer = 0;
  term.onWriteParsed(() => {
    sendScreen();
    view?.tail();
    // Открыли nano или vim — на полосе их кнопки.
    clearTimeout(keysTimer);
    keysTimer = setTimeout(() => keys.refresh(), 300);
  });
  term.buffer.onBufferChange(sendScreen);

  // Клавиатура телефона съедает пол-экрана — терминал ужимается над ней.
  const fitHeight = () => {
    const vv = window.visualViewport;
    if (vv) {
      layer.style.height = `${vv.height}px`;
      layer.style.top = `${vv.offsetTop}px`;
    }
    refit();
  };
  window.visualViewport?.addEventListener("resize", fitHeight);
  window.visualViewport?.addEventListener("scroll", fitHeight);
  new ResizeObserver(() => refit()).observe(screen);

  api?.listen("practice:out", (event) => term.write(event.payload));
  api?.listen("practice:exit", async (event) => {
    connected = false;
    // Обрыв — сначала тихо подключиться снова; не вышло — форма с причиной.
    if (document.visibilityState === "visible" && (await reconnect())) {
      view?.update();
      return;
    }
    if (document.visibilityState !== "visible" && lastTarget) return;
    fold();
    view?.disconnected(typeof event.payload === "string" ? event.payload : "Соединение закрыто.");
  });
  api?.listen("practice:state", (event) => {
    const { practice: fresh, say, spoken } = event.payload ?? {};
    if (!fresh) return;
    practice = fresh;
    view?.update();
    if (say && !spoken && voice.aloud() && view) voice.speak(say);
  });
}

function write(data) {
  if (connected) api.invoke("practice_term_write", { data }).catch(() => {});
}

let refitTimer = 0;
function refit() {
  clearTimeout(refitTimer);
  refitTimer = setTimeout(() => {
    try {
      fit.fit();
    } catch {
      /* терминал ещё не разложен */
    }
    if (connected) api.invoke("practice_term_resize", { cols: term.cols, rows: term.rows }).catch(() => {});
  }, 60);
}

// Экран, как он нарисован, — Ноа: по нему она видит, что человек пишет в
// редакторе. Строка с курсором помечена.
let screenTimer = 0;
function sendScreen() {
  clearTimeout(screenTimer);
  screenTimer = setTimeout(() => {
    const buffer = term.buffer.active;
    const lines = [];
    for (let row = 0; row < term.rows; row++) {
      let line = buffer.getLine(buffer.viewportY + row)?.translateToString(true) ?? "";
      if (row === buffer.cursorY && buffer.viewportY === buffer.baseY) line += "  ← курсор";
      lines.push(line);
    }
    while (lines.length && !lines[lines.length - 1].trim()) lines.pop();
    api.invoke("practice_screen", { text: lines.join("\n"), alt: buffer.type === "alternate" }).catch(() => {});
  }, 250);
}

/** Последние строки терминала — до строки с курсором. */
function lastLines(count = 5) {
  const buffer = term.buffer.active;
  const end = buffer.baseY + buffer.cursorY;
  const lines = [];
  for (let at = end; at >= 0 && lines.length < count; at--) {
    const line = buffer.getLine(at)?.translateToString(true) ?? "";
    if (!lines.length && !line.trim() && at !== end) continue;
    lines.unshift(line);
  }
  while (lines.length > 1 && !lines[0].trim()) lines.shift();
  return lines;
}

/** На весь экран — по кнопке «Развернуть». */
function unfold() {
  layer.append(screenEl, keys.el);
  layer.classList.add("is-open");
  refit();
  setTimeout(() => term.focus(), 80);
}

/** Обратно в окошко на экране практики — печатать можно и там. */
function fold() {
  layer?.classList.remove("is-open");
  if (inlineHost?.isConnected && screenEl) {
    inlineHost.append(screenEl, keys.el);
    refit();
  } else term?.blur();
}

/** Ждёт, пока терминал разложится: замер раньше даёт пару колонок. */
async function settle() {
  await document.fonts?.ready;
  for (let tries = 0; tries < 20; tries++) {
    await new Promise((resolve) => {
      requestAnimationFrame(resolve);
      setTimeout(resolve, 50);
    });
    try {
      fit.fit();
    } catch {
      /* ещё нет размеров */
    }
    if (term.cols >= 30) break;
  }
}

/** Терминал: уже подключён — вернуть экран; нет — false, нужна форма. */
async function startShell() {
  if (connected) return true;
  await settle();
  try {
    const replay = await api.invoke("practice_term_start", { cols: term.cols, rows: term.rows });
    connected = true;
    if (!started && replay) term.write(replay);
    started = true;
    return true;
  } catch (err) {
    if (String(err).includes("ssh:connect")) return false;
    throw err;
  }
}

/* ── Экран ─────────────────────────────────────────────────────────────── */

register("practice", (screen, { course: courseId, topic: topicId }) => {
  const ready = makeTerminal();
  const course = store.course(courseId);
  const card = topicCard(course, topicId);

  const orb = ring("look", 40);
  const status = el("span", "pbar__status", "Ноа смотрит терминал");
  const sub = el("span", "pbar__sub", card?.title ?? "Практика");
  const text = el("span", "pbar__text");
  text.append(status, sub);
  const speaker = iconButton("speaker", "Читать вслух", () => {
    prefs.set("aloud", !voice.aloud());
    speaker.setAttribute("aria-pressed", String(voice.aloud()));
    if (!voice.aloud()) voice.stop();
  }, 20);
  speaker.setAttribute("aria-pressed", String(voice.aloud()));
  const bar = el("header", "pbar");
  const more = iconButton("more", "Сервер и безопасность", () => serverMenu(), 20);
  bar.append(iconButton("arrow-left", "Назад", () => nav.back()), orb, text, speaker, more);
  const segs = el("div", "pbar-steps");
  const content = el("div", "content");
  content.style.cssText = "padding:12px 16px;gap:10px";
  content.append(loading("Открываю практику…"));
  const dock = el("div", "dock dock--ask");
  screen.append(bar, segs, content, dock, el("div", "homebar"));

  let waiting = false;
  let listening = false;
  let speaking = false;
  let who = "терминал";
  let error = "";
  let alive = true;

  /* Части экрана, которые живут между перерисовками. */
  const stepBox = el("div");
  stepBox.style.display = "contents";
  const termBox = el("div", "term");
  const tail = el("div", "term__tail");
  const feed = el("div", "list");
  feed.style.gap = "8px";

  const askField = el("input", "ask");
  askField.placeholder = "Спросить Ноа…";
  askField.enterKeyHint = "send";
  askField.addEventListener("input", () => voice.stop());
  askField.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && askField.value.trim()) {
      const question = askField.value.trim();
      askField.value = "";
      askField.blur();
      think(() => call("practice_ask", { text: question }, { slow: true }));
    }
  });
  const mic = el("button", "square");
  mic.title = "Голосом";
  mic.append(icon("mic", 22, 1.9));
  mic.addEventListener("click", async () => {
    if (listening) {
      voice.cancel();
      return;
    }
    voice.stop();
    listening = true;
    paintStatus();
    try {
      const said = await voice.listen();
      listening = false;
      paintStatus();
      if (said) await think(() => call("practice_ask", { text: said }, { slow: true }));
      else toast("Не расслышала — скажите ещё раз");
    } catch (err) {
      listening = false;
      paintStatus();
      toast(String(err));
    }
  });
  const done = el("button", "btn btn--primary");
  done.style.cssText = "height:44px;padding:0 12px;font-size:13px";
  done.title = "Шаг выполнен — Ноа проверит";
  done.append(icon("check", 18, 2.5), "Готово");
  done.addEventListener("click", () => think(() => api.invoke("practice_check")));
  dock.append(askField, mic, done);

  view = {
    update: () => alive && paint(),
    tail: () => {},
    disconnected: (reason) => alive && connectForm(reason),
  };

  (async () => {
    try {
      await ready;
      if (courseId && topicId) practice = await call("practice_switch", { course: courseId, topic: topicId });
      else practice = await call("practice_state");
      const server = await call("practice_server").catch(() => null);
      if (server?.host) who = `${server.user || "root"}@${server.host} · ssh`;
      if (!alive) return;
      if (await startShell()) paint();
      else connectForm();
    } catch (err) {
      content.replaceChildren(el("span", "error", String(err)));
    }
  })();

  // Кольцо — по тому, что происходит: говорит, слушает, думает, смотрит.
  const poll = setInterval(async () => {
    speaking = Boolean(await voice.busy());
    paintStatus();
  }, 500);

  function paintStatus() {
    const mode = listening ? "listen" : speaking ? "speak" : "look";
    orb.dataset.ring = mode;
    status.textContent = listening
      ? "Слушаю — спрашивайте"
      : speaking
        ? "Ноа говорит"
        : waiting
          ? "Ноа думает…"
          : !connected
            ? "Сервер не подключён"
            : practice?.watching === false
            ? "Ноа не смотрит"
            : "Ноа смотрит терминал";
    mic.setAttribute("aria-pressed", String(listening));
    const scenario = practice?.scenario;
    sub.textContent = scenario
      ? `${card?.title ?? scenario.title} · ${practice.done ? "пройдено" : `шаг ${practice.step + 1} из ${scenario.steps.length}`}`
      : card?.title ?? "Практика";
    done.disabled = waiting || !scenario || practice.done || !connected;
  }

  async function think(request) {
    if (waiting) return;
    voice.stop();
    waiting = true;
    paintFeed();
    paintStatus();
    try {
      await request();
    } catch (err) {
      practice?.feed.push({ who: "noa", kind: "error", text: String(err) });
    }
    waiting = false;
    paintFeed();
    paintStatus();
  }

  /* ── Подключение к серверу ─────────────────────────────────────────── */

  /**
   * Сервер и безопасность: смотрит ли Ноа терминал, вход по ключу вместо
   * пароля, сменить или забыть сервер. И что защищено само.
   */
  function serverMenu() {
    sheet("Сервер и безопасность", async (box, close) => {
      const watch = el("button", "setting");
      const watchText = el("span", "setting__text");
      watchText.append(
        el("span", "setting__name", "Ноа смотрит терминал"),
        el("span", "setting__hint", "Выключите — вывод перестанет уходить модели; Ноа ответит только на ваши вопросы"),
      );
      const toggle = el("span", "switch");
      toggle.setAttribute("aria-checked", String(practice?.watching !== false));
      watch.append(watchText, toggle);
      watch.addEventListener("click", () => {
        const on = toggle.getAttribute("aria-checked") !== "true";
        toggle.setAttribute("aria-checked", String(on));
        api.invoke("practice_watch", { on }).catch((err) => toast(String(err)));
      });
      box.append(watch);

      // Вход без пароля: ключ Ноа — в authorized_keys сервера. Команда
      // видна в терминале целиком; выполняется по вашему нажатию.
      const server = await call("practice_server").catch(() => null);
      const publicKey = String(server?.publicKey ?? "").trim();
      if (publicKey) {
        const keyRow = el("button", "setting");
        const keyText = el("span", "setting__text");
        keyText.append(
          el("span", "setting__name", "Вход без пароля — ключ Ноа"),
          el("span", "setting__hint", connected ? "Добавить ключ на этот сервер: дальше пароль не нужен" : "Сначала подключитесь к серверу"),
        );
        keyRow.append(icon("key", 20, 2), keyText);
        keyRow.firstChild.style.color = "var(--c-accent)";
        keyRow.disabled = !connected;
        keyRow.addEventListener("click", () => {
          const quoted = publicKey.replace(/'/g, "");
          const command =
            "mkdir -p ~/.ssh && chmod 700 ~/.ssh && touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && " +
            `(grep -qxF '${quoted}' ~/.ssh/authorized_keys || echo '${quoted}' >> ~/.ssh/authorized_keys) && echo 'Ключ Ноа добавлен — вход без пароля'`;
          write(`${command}\r`);
          close();
          unfold();
          toast("Ключ добавляется — следующий вход без пароля");
        });
        box.append(keyRow);
      }

      const swap = el("button", "setting");
      const swapText = el("span", "setting__text");
      swapText.append(el("span", "setting__name", "Другой сервер"), el("span", "setting__hint", "Отключиться и войти на другой"));
      swap.append(icon("refresh", 20, 2), swapText);
      swap.firstChild.style.color = "var(--c-accent)";
      swap.addEventListener("click", async () => {
        close();
        lastTarget = null;
        connected = false;
        await api.invoke("practice_term_stop").catch(() => {});
        connectForm();
      });
      box.append(swap);

      const forget = el("button", "setting");
      const forgetText = el("span", "setting__text");
      forgetText.append(
        el("span", "setting__name", "Забыть ключ сервера"),
        el("span", "setting__hint", "После переустановки сервера его ключ другой — Ноа иначе не пустит, подозревая подмену"),
      );
      forget.append(icon("trash", 20, 2), forgetText);
      forget.firstChild.style.color = "var(--c-bad)";
      forget.addEventListener("click", async () => {
        await api.invoke("practice_forget_server_key").catch(() => {});
        toast("Ключ сервера забыт — при следующем входе запомнится новый");
        close();
      });
      box.append(forget);

      const note = el("div", "note");
      note.append(
        label("// что защищено"),
        el(
          "span",
          "",
          "Пароль от сервера не сохраняется — только в памяти до закрытия приложения. Ключ сервера запоминается при первом входе: сменился — Ноа предупредит о подмене. Перед отправкой модели пароли, токены и приватные ключи в выводе закрываются. Опасные команды (rm -rf /, mkfs, остановка SSH, ufw enable…) Ноа переспросит перед Enter.",
        ),
      );
      box.append(note);
    });
  }

  async function connectForm(reason = "") {
    dock.hidden = true;
    segs.replaceChildren();
    const form = el("form", "plate connect");
    form.append(label("// сервер практики"), el("span", "continue__title", "Подключиться к серверу"));
    form.append(el("span", "muted", "Команды выполняются на вашем сервере: Ноа видит вывод и ведёт по шагам. Пароль не запоминается."));
    const field = (title, name, attrs = {}) => {
      const box = el("label", "field-label", title);
      const input = el("input", "field");
      input.name = name;
      input.autocomplete = "off";
      input.setAttribute("autocapitalize", "off");
      input.spellcheck = false;
      Object.assign(input, attrs);
      box.append(input);
      return [box, input];
    };
    const [hostBox, host] = field("Адрес", "host", { placeholder: "203.0.113.10", required: true });
    const [userBox, user] = field("Пользователь", "user", { placeholder: "root", required: true });
    const [portBox, port] = field("Порт", "port", { value: "22", inputMode: "numeric" });
    const [passBox, pass] = field("Пароль", "password", { type: "password", placeholder: "не нужен, если сервер знает ключ Ноа" });
    const row = el("div", "pair");
    portBox.style.flex = "0 0 88px";
    row.append(userBox, portBox);
    const key = el("details", "connect__key");
    const summary = el("summary", "", "Вход без пароля — ключ Ноа");
    const keyText = el("textarea", "field");
    keyText.readOnly = true;
    keyText.rows = 3;
    const copy = el("button", "btn btn--secondary", "Скопировать ключ");
    copy.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(keyText.value);
        toast("Ключ скопирован");
      } catch {
        keyText.select();
      }
    });
    key.append(summary, el("span", "small dim", "Один раз добавьте строку на сервере в ~/.ssh/authorized_keys — дальше Ноа входит сама."), keyText, copy);
    const problem = el("span", "error", reason);
    const go = el("button", "btn btn--primary btn--big", "Подключиться");
    go.type = "submit";
    form.append(hostBox, row, passBox, key, problem, go);
    content.replaceChildren(form);
    try {
      const server = await call("practice_server");
      if (server.host) host.value = server.host;
      if (server.user) user.value = server.user;
      if (server.port) port.value = server.port;
      keyText.value = server.publicKey ?? "";
    } catch (err) {
      problem.textContent = String(err);
    }
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      go.disabled = true;
      go.textContent = "Подключаюсь…";
      problem.textContent = "";
      try {
        await settle();
        term.reset();
        const target = { host: host.value.trim(), port: Number(port.value) || 22, user: user.value.trim(), password: pass.value };
        await call("practice_connect", { ...target, cols: term.cols, rows: term.rows });
        lastTarget = target;
        pass.value = "";
        connected = true;
        started = true;
        who = `${user.value.trim()}@${host.value.trim()} · ssh`;
        dock.hidden = false;
        paint();
      } catch (err) {
        problem.textContent = String(err);
        go.disabled = false;
        go.textContent = "Подключиться";
      }
    });
  }

  /* ── Сценарий ──────────────────────────────────────────────────────── */

  function paint() {
    dock.hidden = false;
    const scenario = practice?.scenario;
    if (!scenario) return paintSetup();
    segs.replaceChildren(steps(scenario.steps.length, practice.done ? scenario.steps.length : practice.step, practice.done ? -1 : practice.step, { ok: true }));
    paintStep(scenario);
    paintTerm();
    paintFeed();
    paintStatus();
    if (!content.contains(stepBox)) content.replaceChildren(stepBox, termBox, feed);
  }

  function paintStep(scenario) {
    const step = scenario.steps[practice.step];
    // Команда проверки шага — на плашке клавиш, одним касанием.
    stepCheck = practice.done ? "" : (/`([^`]+)`/.exec(step?.check ?? "")?.[1] ?? "").trim();
    keys?.refresh();
    const box = el("div", "step-card");
    if (practice.done || !step) {
      box.append(label("// сценарий пройден"), el("span", "step-card__title", scenario.title));
      if (scenario.goal) box.append(el("span", "step-card__text", scenario.goal));
      const again = el("button", "btn btn--secondary", "Новый сценарий");
      again.addEventListener("click", () => {
        if (again.dataset.sure) return api.invoke("practice_reset").catch(() => {});
        again.dataset.sure = "1";
        again.textContent = "Точно? Нажмите ещё раз";
      });
      box.append(again);
    } else {
      box.append(label(`// шаг ${practice.step + 1} из ${scenario.steps.length}`), el("span", "step-card__title", step.title));
      if (step.concept) box.append(el("span", "step-card__from", `Из урока: ${step.concept}`));
      if (step.goal) box.append(el("span", "step-card__text", step.goal));
      if (step.check) {
        const check = el("div", "step-card__check");
        check.innerHTML = `<b>проверка $</b> ${inline(step.check)}`;
        box.append(check);
      }
      const row = el("div", "step-card__actions");
      if (practice.step > 0) row.append(small("Назад", () => api.invoke("practice_step", { delta: -1 })));
      row.append(
        small("Подсказка", () => think(() => call("practice_ask", { text: "Подскажи, что делать дальше на этом шаге." }, { slow: true }))),
        small("Картина", () => think(() => call("practice_ask", { text: "Общая картина: что уже построено, где мы сейчас, что впереди?" }, { slow: true }))),
        small("Пропустить", () => api.invoke("practice_step", { delta: 1 })),
      );
      box.append(row);
    }
    stepBox.replaceChildren(box);
  }

  function small(text, onClick) {
    const node = el("button", "btn btn--plain", text);
    node.addEventListener("click", () => {
      Promise.resolve(onClick()).catch((err) => toast(String(err)));
    });
    return node;
  }

  // Живое окошко терминала: печатать можно прямо здесь, на весь экран —
  // по «Развернуть».
  const live = el("div", "term__live");
  live.addEventListener("focusin", () => setTimeout(() => termBox.scrollIntoView({ block: "nearest", behavior: "smooth" }), 350));
  function paintTerm() {
    const head = el("div", "term__head");
    const open = el("button", "btn btn--plain");
    open.append(icon("keyboard", 16, 2.25), "Развернуть");
    open.addEventListener("click", () => unfold());
    head.append(el("span", connected ? "term__dot" : "term__dot is-off"), el("span", "term__who", who), open);
    termBox.replaceChildren(head, live);
    inlineHost = live;
    if (screenEl && !layer.classList.contains("is-open") && screenEl.parentElement !== live) {
      live.append(screenEl, keys.el);
      refit();
    }
  }

  function paintTail() {
    const lines = lastLines(5);
    tail.replaceChildren();
    lines.forEach((line, i) => {
      if (i) tail.append("\n");
      const prompt = /^(\S+@\S+?[:~][^#$]*[#$])(.*)$/.exec(line);
      if (prompt && i === lines.length - 1) {
        const p = el("span", "term__prompt", prompt[1]);
        tail.append(p, prompt[2]);
      } else tail.append(el("span", i === lines.length - 1 ? "" : "term__old", line));
    });
    tail.append(el("span", "term__cursor"));
  }

  function paintFeed() {
    const lines = (practice?.feed ?? []).slice(-6);
    feed.replaceChildren(
      ...lines.map((line) => {
        const kind = line.who === "me" ? "say say--me" : line.kind === "error" ? "say say--bad" : line.kind === "step" ? "say say--good" : "say";
        const node = el("div", kind);
        node.innerHTML = inline(line.text).replace(/\n/g, "<br>");
        return node;
      }),
    );
    if (waiting) feed.append(loading("Ноа думает…"));
    requestAnimationFrame(() => (content.scrollTop = content.scrollHeight));
  }

  function paintSetup() {
    segs.replaceChildren();
    paintStatus();
    const box = el("div", "plate plate--active");
    box.append(label("// практика по теме"), el("span", "continue__title", card?.title ?? "Практика"));
    box.append(
      el(
        "span",
        "continue__lead",
        "Ноа соберёт живую задачу на понятиях темы: шаги в порядке урока, всё как в проде — не от root, ключи, фаервол, конфиги файлами. Работаете вы, Ноа смотрит и подключается, когда нужна.",
      ),
    );
    const wish = el("textarea", "field");
    wish.rows = 2;
    wish.style.minHeight = "64px";
    wish.placeholder = "Пожелание, если есть: «на трёх серверах», «через Docker»…";
    const go = el("button", "btn btn--primary btn--big", "Составить сценарий");
    const problem = el("span", "error", error);
    go.addEventListener("click", async () => {
      go.disabled = true;
      go.replaceChildren("Ноа составляет сценарий… до двух минут");
      error = "";
      try {
        practice = await call("practice_plan", { course: courseId ?? null, topic: topicId ?? null, goal: wish.value.trim() });
        await store.practice(true);
      } catch (err) {
        error = String(err);
      }
      if (alive) {
        content.replaceChildren();
        paint();
      }
    });
    box.append(wish, problem, go);
    content.replaceChildren(box, termBox);
    paintTerm();
  }

  return {
    full: true,
    cleanup: () => {
      alive = false;
      view = null;
      clearInterval(poll);
      // Экран практики ушёл — терминал ждёт следующего открытия в своём слое.
      inlineHost = null;
      layer?.classList.remove("is-open");
      if (screenEl) layer?.append(screenEl, keys.el);
      term?.blur();
      voice.stop();
      store.practice(true).catch(() => {});
    },
  };
});
