// Урок вслух — для тех, кому трудно удерживать внимание на тексте.
//
// Ноа читает раздел абзац за абзацем тем же голосом, что и везде. Текущий
// абзац подсвечен и стоит посередине экрана, остальные приглушены («фокус»):
// глазу не за что зацепиться, кроме того, что звучит. Пауза, назад, дальше —
// кнопками и клавишами (пробел, ←, →). Выделили кусок — рядом кнопка
// «Прочитать»: Ноа прочитает только его.
//
// `api` — тот же, что у окна обучения: в программе речь ведёт Rust
// (voice_speak / voice_busy / voice_stop), в браузере — web-api.js.

const FOCUS_KEY = "noa.readFocus";

/** Какие узлы урока читаются по одному: абзацы, пункты, заголовки, цитаты. */
export function readableBlocks(root) {
  return [...root.querySelectorAll("h2, h3, p, li, blockquote")].filter((node) => {
    if (node.closest("pre")) return false;
    // Пункт со вложенным списком читается своими пунктами, а не целиком.
    if (node.tagName === "LI" && node.querySelector("li")) return false;
    if (node.tagName === "P" && node.closest("li, blockquote")) return false;
    return node.textContent.trim().length > 1;
  });
}

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Подключает чтение к узлу урока. Отдаёт панель управления — её ставят над
 * уроком. `onFinish` — раздел дочитан (окно может предложить следующий).
 */
export function attachReader(lesson, { api, el, button, onFinish }) {
  const blocks = readableBlocks(lesson);
  const state = { at: 0, playing: false, run: 0 };
  let focus = true;
  try {
    focus = localStorage.getItem(FOCUS_KEY) !== "off";
  } catch {
    /* по умолчанию — фокус */
  }

  const bar = el("div", "reader");
  const play = button("▶ Слушать урок", () => (state.playing ? stop() : start()));
  const back = button("⏮", () => jump(-1), true);
  const next = button("⏭", () => jump(1), true);
  back.title = "Предыдущий абзац (←)";
  next.title = "Следующий абзац (→)";
  const focusBtn = button("", () => {
    focus = !focus;
    try {
      localStorage.setItem(FOCUS_KEY, focus ? "on" : "off");
    } catch {
      /* не запомнится */
    }
    paint();
  }, true);
  const where = el("span", "reader__where");
  bar.append(play, back, next, focusBtn, where);

  function paint() {
    play.textContent = state.playing ? "⏸ Пауза" : state.at > 0 ? "▶ Дальше слушать" : "▶ Слушать урок";
    focusBtn.textContent = focus ? "◐ Фокус: вкл" : "◑ Фокус: выкл";
    where.textContent = blocks.length ? `абзац ${Math.min(state.at + 1, blocks.length)} из ${blocks.length}` : "";
    lesson.classList.toggle("reader-focus", focus && (state.playing || state.at > 0));
    blocks.forEach((node, i) => node.classList.toggle("reader-now", i === state.at && (state.playing || state.at > 0)));
  }

  /**
   * Ждём, пока голос договорит. В браузере вызов речи и так ждёт конца, в
   * программе — возвращается раньше; тогда ловим, как речь началась и
   * кончилась. Паузы между кусками — доли секунды, а не секунда с лишним.
   */
  async function waitQuiet(startedAt) {
    let heard = false;
    for (;;) {
      const busy = await api.invoke("voice_busy").catch(() => false);
      if (busy) heard = true;
      else if (heard || Date.now() - startedAt > 1500) return;
      await pause(150);
    }
  }


  async function start() {
    if (!blocks.length) return;
    if (state.at >= blocks.length) state.at = 0;
    state.playing = true;
    const run = ++state.run;
    paint();
    while (state.playing && run === state.run && state.at < blocks.length) {
      const node = blocks[state.at];
      node.scrollIntoView({ block: "center", behavior: "smooth" });
      const startedAt = Date.now();
      try {
        await api.invoke("voice_speak", { text: node.textContent.trim() });
      } catch (err) {
        where.textContent = `Голос не отвечает: ${err}`;
        state.playing = false;
        break;
      }
      await waitQuiet(startedAt);
      if (!state.playing || run !== state.run) return;
      state.at += 1;
      paint();
    }
    if (state.at >= blocks.length && run === state.run) {
      state.playing = false;
      paint();
      onFinish?.();
    }
  }

  function stop() {
    state.playing = false;
    state.run += 1;
    api.invoke("voice_stop").catch(() => {});
    paint();
  }

  function jump(delta) {
    const wasPlaying = state.playing;
    stop();
    state.at = Math.max(0, Math.min(blocks.length - 1, state.at + delta));
    blocks[state.at]?.scrollIntoView({ block: "center", behavior: "smooth" });
    paint();
    if (wasPlaying) start();
  }

  // Щелчок по абзацу — читать с него.
  lesson.addEventListener("dblclick", (event) => {
    const at = blocks.findIndex((node) => node.contains(event.target));
    if (at < 0) return;
    stop();
    state.at = at;
    start();
  });

  // Выделенное — прочитать.
  const selectBtn = button("🔊 Прочитать", () => {
    const text = window.getSelection()?.toString().trim();
    selectBtn.hidden = true;
    if (!text) return;
    stop();
    api.invoke("voice_speak", { text }).catch(() => {});
  });
  selectBtn.classList.add("reader__select");
  selectBtn.hidden = true;
  document.body.append(selectBtn);
  // В браузере выделение уже даёт меню «Копировать · Прочитать · Объяснить».
  const ownMenu = !document.documentElement.classList.contains("is-web");
  lesson.addEventListener("mouseup", () => {
    if (!ownMenu) return;
    setTimeout(() => {
      const selection = window.getSelection();
      const text = selection?.toString().trim() ?? "";
      if (text.length < 3 || !lesson.contains(selection.anchorNode)) {
        selectBtn.hidden = true;
        return;
      }
      const rect = selection.getRangeAt(0).getBoundingClientRect();
      selectBtn.style.left = `${Math.max(8, rect.left + rect.width / 2 - 60)}px`;
      selectBtn.style.top = `${Math.max(8, rect.top - 44)}px`;
      selectBtn.hidden = false;
    });
  });
  document.addEventListener("mousedown", (event) => {
    if (event.target !== selectBtn) selectBtn.hidden = true;
  });

  const keys = (event) => {
    if (!lesson.isConnected) {
      document.removeEventListener("keydown", keys);
      selectBtn.remove();
      stop();
      return;
    }
    if (event.target.closest?.("textarea, input")) return;
    if (event.code === "Space") {
      event.preventDefault();
      state.playing ? stop() : start();
    } else if (event.key === "ArrowRight") jump(1);
    else if (event.key === "ArrowLeft") jump(-1);
  };
  document.addEventListener("keydown", keys);

  paint();
  return bar;
}
