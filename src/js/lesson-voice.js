// «Объясни голосом»: Ноа рассказывает тему, раздел или разбор своими словами,
// как репетитор вслух, — а человек в любой момент перебивает и спрашивает.
//
// Объяснение пишет модель по материалу темы (learn_ask — тот же разговор, что
// у «Обсудить»: модель помнит, что уже сказала). Звучит оно по фразе: так
// перебить можно сразу, а после ответа на вопрос рассказ продолжается с той
// фразы, на которой остановились. Спросить — кнопкой «✋ Спросить» или
// пробелом: Ноа замолкает и слушает; договорили — нажать ещё раз. Можно и
// написать.
//
// `api` — тот же, что у окна обучения (программа или браузер).

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Текст модели → фразы для речи: без разметки, по предложениям. */
export function phrases(text) {
  const clean = String(text ?? "")
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/[*_#`>|]/g, "")
    .replace(/^\s*[-•]\s+/gm, "")
    .replace(/\s+/g, " ")
    .trim();
  if (!clean) return [];
  const parts = clean.match(/[^.!?…]+(?:[.!?…]+["»)]*|$)/g) ?? [clean];
  return parts.map((part) => part.trim()).filter((part) => part.length > 1);
}

const ASK = {
  topic:
    "Объясни эту тему вслух, как живой репетитор объясняет ученику, которому трудно читать: " +
    "разговорно, короткими фразами, с одним примером из обычной жизни, от простого к сложному. " +
    "Восемь–двенадцать предложений. Без списков, заголовков, разметки и кода — это будет звучать голосом. " +
    "Не здоровайся и не предлагай помощь в конце.",
  section:
    "Объясни этот раздел урока вслух, как живой репетитор: разговорно, короткими фразами, " +
    "с примером из жизни. Шесть–десять предложений. Без списков, разметки и кода — это будет звучать голосом. " +
    "Не здоровайся и не предлагай помощь в конце.",
  deep:
    "Перескажи этот подробный разбор вслух, разговорно и короткими фразами, как репетитор: " +
    "по шагам, что за чем и почему. Восемь–двенадцать предложений, без списков и разметки. " +
    "Не здоровайся. Разбор:\n\n",
};

let current = null;

/**
 * Кнопка «🎧 …». По нажатию под `anchor` открывается панель объяснения.
 * deps: { api, el, button }; kind: topic | section | deep; extra() — текст
 * разбора для kind = deep.
 */
export function voiceButton(label, target, kind, anchor, deps, extra = () => "") {
  const node = deps.button(label, () => open(target, kind, anchor, deps, extra()), true);
  node.classList.add("vx__open");
  return node;
}

function open(target, kind, anchor, deps, extra) {
  current?.close();
  // Разбор ещё не открыт — объясняем сам раздел.
  if (kind === "deep" && !String(extra ?? "").trim()) kind = "section";
  const { api, el, button } = deps;
  const panel = el("section", "vx");
  const head = el("div", "vx__head");
  head.append(el("span", "vx__title", "🎧 Ноа объясняет"), el("span", "vx__hint", "Пробел или ✋ — перебить и спросить"));
  const close = button("×", () => finish(), true);
  close.classList.add("vx__close");
  close.title = "Закрыть";
  head.append(close);

  const script = el("div", "vx__script");
  const status = el("p", "vx__status", "Ноа готовит объяснение…");
  const ask = button("✋ Спросить", () => onAsk());
  ask.classList.add("vx__ask");
  const toggle = button("⏸ Пауза", () => (state.playing ? stop() : play()), true);
  const input = el("input", "answer vx__input");
  input.type = "text";
  input.placeholder = "или напишите вопрос и Enter";
  const controls = el("div", "actions vx__controls");
  controls.append(ask, toggle, input);
  panel.append(head, status, script, controls);
  anchor.after(panel);
  panel.scrollIntoView({ block: "nearest", behavior: "smooth" });

  const state = { lines: [], at: 0, playing: false, run: 0, listening: false, busy: false };

  const addLines = (texts, who) => {
    const start = state.lines.length;
    for (const text of texts) {
      const node = el("span", `vx__line vx__line--${who}`, `${text} `);
      script.append(node);
      state.lines.push({ text, node });
    }
    return start;
  };

  async function waitQuiet() {
    const since = Date.now();
    for (;;) {
      await pause(250);
      const busy = await api.invoke("voice_busy").catch(() => false);
      if (!busy && Date.now() - since > 900) return;
    }
  }

  async function play() {
    if (state.at >= state.lines.length) return;
    state.playing = true;
    const run = ++state.run;
    paint();
    while (state.playing && run === state.run && state.at < state.lines.length) {
      const line = state.lines[state.at];
      paint();
      line.node.scrollIntoView({ block: "nearest", behavior: "smooth" });
      try {
        await api.invoke("voice_speak", { text: line.text });
      } catch (err) {
        status.textContent = `Голос не отвечает: ${err}. Текст — ниже.`;
        state.playing = false;
        break;
      }
      await waitQuiet();
      if (!state.playing || run !== state.run) return;
      state.at += 1;
    }
    if (run === state.run) {
      state.playing = false;
      paint();
    }
  }

  function stop() {
    state.playing = false;
    state.run += 1;
    api.invoke("voice_stop").catch(() => {});
    paint();
  }

  function paint() {
    state.lines.forEach((line, i) => {
      line.node.classList.toggle("vx__line--now", i === state.at && state.playing);
      line.node.classList.toggle("vx__line--said", i < state.at);
    });
    toggle.textContent = state.playing ? "⏸ Пауза" : state.at < state.lines.length ? "▶ Продолжить" : "↻ Сначала";
    ask.textContent = state.listening ? "■ Готово — спросить" : "✋ Спросить";
    ask.disabled = state.busy;
    if (!state.busy && !state.listening) {
      status.textContent = state.playing
        ? "Ноа рассказывает. Непонятно — перебейте."
        : state.at >= state.lines.length && state.lines.length
          ? "Рассказ окончен. Остались вопросы — ✋ или напишите."
          : "Пауза.";
    }
  }

  toggle.addEventListener("click", () => {
    if (!state.playing && state.at >= state.lines.length && state.lines.length) {
      state.at = 0;
      play();
    }
  });

  async function onAsk() {
    if (state.busy) return;
    if (!state.listening) {
      stop();
      try {
        await api.invoke("learn_dictate_start");
        state.listening = true;
        status.textContent = "Слушаю — говорите. Договорили — нажмите «Готово» или пробел.";
      } catch {
        status.textContent = "Микрофон недоступен — напишите вопрос в поле.";
        input.focus();
      }
      paint();
      return;
    }
    state.listening = false;
    state.busy = true;
    status.textContent = "Распознаю…";
    paint();
    let text = "";
    try {
      text = String((await api.invoke("learn_dictate_stop")) ?? "").trim();
    } catch {
      text = "";
    }
    state.busy = false;
    if (!text) {
      status.textContent = "Не расслышала — повторите или напишите.";
      paint();
      return;
    }
    question(text);
  }

  async function question(text) {
    stop();
    state.busy = true;
    // Фраза, которую перебили, — та, что звучала.
    const said = (state.lines[state.at] ?? state.lines[state.at - 1])?.text ?? "";
    // Недосказанное снимается и вернётся после ответа.
    const rest = state.lines.splice(state.at);
    for (const line of rest) line.node.remove();
    const restore = () => {
      for (const line of rest) {
        script.append(line.node);
        state.lines.push(line);
      }
    };
    script.append(el("span", "vx__line vx__line--me", `Вы: ${text} `));
    status.textContent = "Ноа думает…";
    paint();
    let reply = "";
    try {
      reply = await api.invoke("learn_ask", {
        target,
        text:
          `Я перебил тебя${said ? ` на фразе «${said}»` : ""} и спрашиваю: «${text}». ` +
          "Ответь вслух, разговорно, 2–4 коротких предложения, без списков и разметки. " +
          "Если рассказ ещё не окончен — закончи ответ словами «Продолжаю».",
      });
    } catch (err) {
      reply = "";
      status.textContent = `Ответа нет: ${err}`;
    }
    state.busy = false;
    if (!reply) {
      restore();
      return paint();
    }
    // Ответ встаёт сразу за вопросом и звучит первым, потом — недосказанное.
    addLines(phrases(reply), "answer");
    restore();
    play();
  }

  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && input.value.trim()) {
      event.preventDefault();
      const text = input.value.trim();
      input.value = "";
      question(text);
    }
  });

  const keys = (event) => {
    if (!panel.isConnected) return document.removeEventListener("keydown", keys, true);
    if (event.code !== "Space" || event.target.closest?.("textarea, input")) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    onAsk();
  };
  document.addEventListener("keydown", keys, true);

  function finish() {
    stop();
    if (state.listening) api.invoke("learn_dictate_stop").catch(() => {});
    document.removeEventListener("keydown", keys, true);
    panel.remove();
    if (current?.panel === panel) current = null;
  }
  current = { panel, close: finish };

  (async () => {
    try {
      const text = await api.invoke("learn_ask", { target, text: ASK[kind] + (kind === "deep" ? String(extra).slice(0, 4000) : "") });
      const lines = phrases(text);
      if (!lines.length) throw new Error("пустой ответ");
      addLines(lines, "noa");
      play();
    } catch (err) {
      status.textContent = `Объяснение не пришло: ${err}. Проверьте модель в настройках.`;
    }
  })();
}
