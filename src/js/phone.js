// Разговор с Ноа на телефоне — главный экран мобильной версии.
//
// На компьютере Ноа зовут именем или левым Alt с пробелом, отвечает он
// голосом и окном у курсора. У телефона нет ни клавиш, ни окон поверх других
// программ, поэтому всё это собрано на одном экране: кнопка микрофона
// вместо Alt с пробелом, «Стоп» вместо Esc, «Из буфера» вместо «Вот,
// показываю», «Без рук» вместо разговора по имени. Позвать Ноа снаружи можно
// жестом ассистента, ярлыком на иконке и плиткой в шторке — они приводят
// сюда же и сразу включают микрофон.
//
// Думает Ноа в Rust (`phone_ask` — тот же путь, что у голоса на компьютере:
// распоряжения, модули, модель с памятью разговора), говорит и слушает
// система Android через плагин `sufler`.

const PLUGIN = "sufler";
const LOG_KEY = "phone.log";
const HANDS_KEY = "phone.handsFree";
/** Сколько реплик держать на экране и в памяти страницы. */
const KEEP = 60;
/** Ничего не расслышано за это время — запись закрывается. Служба распознавания
 *  иногда принимает шум за начало речи и ждёт конца фразы бесконечно. */
const QUIET_MS = 8000;
/** Самая длинная фраза. */
const LONGEST_MS = 30000;

/**
 * @param {{invoke: Function, listen: Function}} api
 * @param {Record<string, HTMLElement>} ui
 * @param {{explain: (text: string) => void, show: () => void}} host
 */
export function attachPhone(api, ui, host) {
  const plugin = (cmd, args) => api.invoke(`plugin:${PLUGIN}|${cmd}`, args ?? {});

  /** @type {{who: 'me'|'noa'|'note', text: string}[]} */
  let log = restore();
  let listening = false;
  let busy = false;
  let handsFree = readFlag(HANDS_KEY);
  /** Номер хода: «Стоп» делает недействительным всё, что было начато до него. */
  let turn = 0;
  let quietTimer = 0;

  render();
  showHandsFree();

  /* ── Реплики ─────────────────────────────────────────────────────────── */

  function restore() {
    try {
      const saved = JSON.parse(sessionStorage.getItem(LOG_KEY) || "[]");
      return Array.isArray(saved) ? saved : [];
    } catch {
      return [];
    }
  }

  function persist() {
    try {
      sessionStorage.setItem(LOG_KEY, JSON.stringify(log.slice(-KEEP)));
    } catch {
      /* история просто не переживёт переход на страницу модуля */
    }
  }

  function add(who, text) {
    log.push({ who, text });
    log = log.slice(-KEEP);
    persist();
    render();
  }

  function render() {
    ui.phoneLog.replaceChildren(
      ...log.map(({ who, text }) => {
        const line = document.createElement("p");
        line.className = `phone__line phone__line--${who}`;
        line.textContent = who === "noa" ? plain(text) : text;
        return line;
      }),
    );
    ui.phoneEmpty.hidden = log.length > 0;
    ui.phoneLog.lastElementChild?.scrollIntoView({ block: "end" });
  }

  /* ── Вопрос и ответ ──────────────────────────────────────────────────── */

  async function ask(text) {
    text = text.trim();
    if (!text || busy) return;
    const mine = ++turn;
    busy = true;
    add("me", text);
    setState("thinking");
    let answer = "";
    try {
      answer = await api.invoke("phone_ask", { text });
    } catch (err) {
      answer = String(err);
    }
    busy = false;
    if (mine !== turn) return;
    if (!answer) {
      setState("idle");
      return;
    }
    add("noa", answer);
    await say(answer, mine);
    if (mine !== turn) return;
    setState("idle");
    // Без рук: ответил — слушает дальше, пока не попрощались или не замолчали.
    if (handsFree && !isGoodbye(text)) listen();
  }

  /** Читает вслух, если голос включён в настройках. */
  async function say(text, mine = turn) {
    const voice = await api.invoke("voice_settings").catch(() => null);
    if (!voice?.enabled || mine !== turn) return;
    setState("speaking");
    await api.invoke("voice_speak", { text: plain(text) }).catch(() => {});
  }

  /* ── Микрофон ────────────────────────────────────────────────────────── */

  async function listen() {
    if (listening) {
      // Второе касание — «я всё сказал»: услышанное уходит сразу, без паузы.
      plugin("stopListening").catch(() => {});
      return;
    }
    if (busy) return;
    host.show();
    const mine = ++turn;
    listening = true;
    setState("listening");
    ui.phoneHeard.textContent = "";
    ui.phoneHeard.hidden = false;
    let heard = "";
    const finish = () => listening && mine === turn && plugin("stopListening").catch(() => {});
    quietTimer = setTimeout(finish, QUIET_MS);
    const longest = setTimeout(finish, LONGEST_MS);
    try {
      heard = (await plugin("listen", { lang: "ru-RU" }))?.text ?? "";
    } catch (err) {
      add("note", String(err?.message ?? err));
    }
    clearTimeout(quietTimer);
    clearTimeout(longest);
    listening = false;
    ui.phoneHeard.hidden = true;
    if (mine !== turn) return;
    setState("idle");
    if (heard.trim()) {
      ask(heard);
    } else if (handsFree) {
      // Тишина в разговоре без рук — конец разговора, а не повод слушать вечно.
      add("note", "Не слышу — выключаю микрофон.");
    }
  }

  function stop() {
    turn++;
    busy = false;
    listening = false;
    ui.phoneHeard.hidden = true;
    plugin("cancelListening").catch(() => {});
    api.invoke("voice_stop").catch(() => {});
    setState("idle");
  }

  function setState(state) {
    ui.phone.dataset.state = state;
    if (state !== "listening") ui.phone.style.setProperty("--level", "0");
    ui.phoneMic.setAttribute("aria-pressed", String(state === "listening"));
    ui.phoneMicLabel.textContent =
      {
        listening: "Слушаю — коснитесь, когда договорите",
        thinking: "Думаю…",
        speaking: "Говорю — «Стоп», чтобы замолчать",
      }[state] ?? "Коснитесь и говорите";
  }

  function showHandsFree() {
    ui.phoneHands.setAttribute("aria-pressed", String(handsFree));
    ui.phoneHands.textContent = handsFree ? "Без рук: вкл" : "Без рук: выкл";
  }

  /* ── Кнопки ──────────────────────────────────────────────────────────── */

  ui.phoneMic.addEventListener("click", listen);
  ui.phoneStop.addEventListener("click", stop);

  ui.phoneForm.addEventListener("submit", (event) => {
    event.preventDefault();
    const text = ui.phoneInput.value;
    ui.phoneInput.value = "";
    ui.phoneInput.blur();
    ask(text);
  });

  ui.phoneHands.addEventListener("click", () => {
    handsFree = !handsFree;
    writeFlag(HANDS_KEY, handsFree);
    showHandsFree();
  });

  ui.phoneNew.addEventListener("click", () => {
    stop();
    log = [];
    persist();
    render();
    api.invoke("phone_forget").catch(() => {});
  });

  // «Вот, показываю»: то, что человек скопировал, Ноа объясняет тем же окном,
  // что и пункт «Объяснить» в меню выделения.
  ui.phoneClip.addEventListener("click", async () => {
    const text = (await plugin("clipboard").catch(() => null))?.text?.trim() ?? "";
    if (text) host.explain(text);
    else add("note", "В буфере обмена пусто — скопируйте текст и нажмите ещё раз.");
  });

  /* ── Сигналы от Rust и системы ───────────────────────────────────────── */

  // Напоминание, будильник, разбор дня: Rust уже читает их вслух.
  api.listen("noa:message", ({ payload }) => add("noa", String(payload ?? "")));
  // Вечерний разбор задал вопрос — ждёт ответа голосом.
  api.listen("noa:listen", () => listen());
  // Устный зачёт и обсуждение урока: сказать и слушать ответ.
  api.listen("noa:say-then-listen", async ({ payload }) => {
    const text = String(payload ?? "");
    host.show();
    add("noa", text);
    await say(text);
    listen();
  });

  // Живая расшифровка и громкость — пока человек говорит.
  const onSpeech = (payload) => {
    if (payload?.state === "partial" && listening) {
      ui.phoneHeard.textContent = payload.text;
      // Слова пошли — это речь, а не шум: даём договорить.
      clearTimeout(quietTimer);
    }
    if (payload?.state === "level") ui.phone.style.setProperty("--level", String(payload.level ?? 0));
    if (payload?.state === "thinking" && listening) setState("thinking");
  };
  const addListener = globalThis.__TAURI__?.core?.addPluginListener;
  if (typeof addListener === "function") {
    addListener(PLUGIN, "speech", onSpeech).catch(() => {});
    // Жест ассистента, ярлык на иконке, плитка в шторке — Ноа слушает сразу.
    addListener(PLUGIN, "voiceRequest", () => listen()).catch(() => {});
  }
  plugin("pendingVoice")
    .then((pending) => pending?.listen && listen())
    .catch(() => {});

  setState("idle");

  return { ask, listen, stop };
}

/* ── Помощники ──────────────────────────────────────────────────────────── */

/** Разметка модели — звёздочки, решётки, обратные кавычки — ни глазу, ни голосу не нужна. */
export function plain(text) {
  return String(text ?? "")
    .replace(/```[a-z]*\n?/gi, "")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/__([^_]+)__/g, "$1")
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/^\s*[-*]\s+/gm, "• ")
    .trim();
}

/** Попрощались — разговор без рук на этом заканчивается. */
export function isGoodbye(text) {
  return /(^|\s)(пока|спасибо|хватит|стоп|всё|все|отбой|до связи)[\s.!]*$/i.test(String(text).trim());
}

function readFlag(key) {
  try {
    return localStorage.getItem(key) === "1";
  } catch {
    return false;
  }
}

function writeFlag(key, value) {
  try {
    localStorage.setItem(key, value ? "1" : "0");
  } catch {
    /* выбор просто не запомнится */
  }
}
