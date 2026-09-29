// Вьюха попапа: разметка, состояния и уточняющий разговор. Никакого
// позиционирования здесь нет — им занимается хост: в вебе `position.js`, в
// приложении — Rust (окно двигается целиком).
//
// Один и тот же класс используется и в окне Tauri, и в браузере (демо,
// словарь Ноа онлайн, объяснение на телефоне), поэтому он не знает ничего про
// источник данных: AI-клиент и действия (озвучить, микрофон, настройки)
// передаются снаружи. Нет действия у хоста — нет и кнопки.
//
// Вид — дизайн-система NOAH, опорный экран «Попап объяснения»: шапка с
// термином и кнопками 28×28, тело до 340 px с прокруткой внутри, внизу — поле
// «Уточнить словами или голосом…».

import { DEFAULT_ERROR_TEXT } from "./ai-client.js";
import { normalizeTerm } from "./term.js";
import { icon, mountIcons } from "./icons.js";
import { renderRich } from "./rich-text.js";

/**
 * Предел ожидания ответа. Больше любого разумного сетевого таймаута (у запросов
 * к Википедии он 8 секунд, у моделей — 12), потому что это не замена им, а последняя
 * страховка: срабатывает, только если ответа не будет уже никогда.
 */
const RESPONSE_TIMEOUT_MS = 20_000;

/** Отдельный текст, а не общая «ошибка сети»: причина здесь другая и подсказка тоже. */
const NO_RESPONSE_TEXT = "Ответ не пришёл. Откройте настройку через значок в трее и нажмите «Проверить».";

/** Повторы при обрыве связи: 2 → 4 → 8 → 16 → 30 с, дальше — по 30. */
const RETRY_STEPS = [2, 4, 8, 16, 30];

/**
 * Вопрос, который прячется за кнопкой «?».
 *
 * Спрашивается тогда, когда попросили, а не вместе с первым ответом: каждое
 * выделение иначе ждало бы втрое больше текста, который редко раскрывают.
 * Первый ответ уходит вместе с вопросом (см. `#elaborate`), чтобы модель
 * видела, что уже прозвучало, и разбирала дальше, а не пересказывала.
 */
const ELABORATE_QUESTION =
  "Не повторяй то, что уже сказано. Разбери по шагам, что здесь на самом деле " +
  "происходит: что за чем следует и почему. Три–пять коротких пунктов, каждый с новой строки.";

/**
 * Стоит ли указатель на полосе прокрутки тела окна.
 *
 * Окно таскается за любое место, и нажатие на ползунок раньше тоже хватало
 * окно — прокрутить длинный ответ мышью было нельзя.
 */
export function onScrollbar(event) {
  const box = event.target?.closest?.(".popup__body");
  if (!box || box.scrollHeight <= box.clientHeight) return false;
  // Полоса бывает и поверх текста, без собственной ширины, — тогда разница
  // ширин нулевая. Поэтому правый край в двенадцать пикселей — ползунок всегда.
  const bar = Math.max(box.offsetWidth - box.clientWidth, 12);
  return event.clientX >= box.getBoundingClientRect().right - bar;
}

/**
 * Текст ошибки → заголовок и пояснение. Ошибка пишется «что случилось. Что
 * сделать.» — первое предложение идёт заголовком, остальное под ним.
 */
export function splitMessage(message) {
  const text = String(message ?? "").trim();
  const match = /^(.+?[.!?])\s+(\S.*)$/s.exec(text);
  if (!match) return { title: text.replace(/\.$/, ""), text: "" };
  return { title: match[1].replace(/\.$/, ""), text: match[2] };
}

const TEMPLATE = `
<div class="popup" data-el="root" tabindex="-1" role="dialog" aria-label="Объяснение выделенного текста">
  <div class="popup__head">
    <span class="popup__kicker" data-el="kicker" hidden></span>
    <span class="popup__term" data-el="term"></span>
    <button class="popup__icon" data-el="stop" type="button" tabindex="-1" title="Остановить" aria-label="Остановить" hidden></button>
    <button class="popup__icon" data-el="speak" type="button" tabindex="-1" title="Прочитать вслух" aria-label="Прочитать вслух" hidden></button>
    <button class="popup__icon" data-el="expand" type="button" tabindex="-1" title="Проще и с примерами" aria-label="Проще и с примерами"></button>
    <button class="popup__icon" data-el="close" type="button" tabindex="-1" title="Закрыть · Esc" aria-label="Закрыть окно"></button>
  </div>

  <div class="popup__title" data-el="title" hidden></div>

  <span class="popup__status" data-el="listening" role="status" hidden>
    <span class="spinner" aria-hidden="true"></span>Слушаю…
  </span>

  <div class="popup__loading" data-el="loading" role="status" aria-busy="true">
    <span class="popup__status"><span class="spinner" aria-hidden="true"></span>Думаю…</span>
    <span class="skeleton skeleton--1" aria-hidden="true"></span>
    <span class="skeleton skeleton--2" aria-hidden="true"></span>
    <span class="skeleton skeleton--3" aria-hidden="true"></span>
  </div>

  <div class="popup__main" data-el="main" hidden>
    <div class="popup__body" data-el="body" role="status" aria-live="polite">
      <div class="popup__answer" data-el="answer"></div>
      <div class="popup__extra" data-el="extra" hidden>
        <div class="popup__section" data-el="simpleSection">
          <span class="popup__label">По шагам</span>
          <div class="popup__simple" data-el="simple"></div>
        </div>
        <div class="popup__section" data-el="examplesSection">
          <span class="popup__label">Где встречается</span>
          <div class="popup__examples" data-el="examples"></div>
        </div>
      </div>
      <div class="popup__messages" data-el="thread"></div>
      <span class="popup__status" data-el="pending" hidden>
        <span class="spinner" aria-hidden="true"></span>Думаю…
      </span>
    </div>
    <span class="popup__fade" data-el="fade" aria-hidden="true" hidden></span>
  </div>

  <div class="popup__reading" data-el="reading" hidden>
    <span class="popup__bars" aria-hidden="true"><i></i><i></i><i></i><i></i></span>
    <span class="popup__reading-text">Читаю вслух</span>
    <button class="popup__textbtn" data-el="readStop" type="button" tabindex="-1">Стоп</button>
  </div>

  <div class="popup__error" data-el="error" role="status" hidden>
    <div class="popup__offline" data-el="offline" hidden>
      <span data-el="offlineText"></span>
      <button class="popup__chip-btn" data-el="retryNow" type="button" tabindex="-1">Сейчас</button>
    </div>
    <div class="popup__error-title" data-el="errorHead">
      <span data-el="errorTitle"></span>
    </div>
    <p class="popup__error-text" data-el="errorText"></p>
    <div class="popup__actions" data-el="errorActions">
      <button class="popup__btn popup__btn--primary" data-el="openSettings" type="button" tabindex="-1" hidden>Открыть настройки</button>
      <button class="popup__btn" data-el="retry" type="button" tabindex="-1">Повторить</button>
    </div>
    <details class="popup__details" data-el="details" hidden>
      <summary>Подробности<span data-el="detailsBrief"></span></summary>
      <pre data-el="detailsRaw"></pre>
    </details>
  </div>

  <div class="popup__ask" data-el="ask" hidden>
    <input class="popup__input" data-el="input" type="text" placeholder="Уточнить словами или голосом…"
           aria-label="Уточнить" autocomplete="off" spellcheck="false" />
    <button class="popup__square" data-el="mic" type="button" tabindex="-1"
            title="Зажмите и спросите голосом" aria-label="Спросить голосом" hidden></button>
    <button class="popup__square popup__square--send" data-el="send" type="button" tabindex="-1"
            title="Отправить" aria-label="Отправить"></button>
  </div>
</div>`;

/** Значки кнопок: у каждой — свой, подпись остаётся в title и aria-label. */
const ICONS = {
  stop: "stop",
  speak: "speaker",
  expand: "help",
  close: "close",
  mic: "mic",
  send: "send",
  readStop: "stop",
  retryNow: "refresh",
  errorHead: "alert",
  offline: "offline",
};

export class PopupView {
  /**
   * @param {{
   *   client: { explain: Function, ask: Function },
   *   errorText?: string,
   *   dialogue?: boolean,
   *   onGeometry?: (size: {width: number, height: number}) => void,
   *   onClose?: () => void,
   * }} opts
   *
   * Необязательные действия хоста — свойства экземпляра:
   *   onSpeak(text)       — прочитать вслух (кнопка «Прочитать вслух»);
   *   onStopSpeaking()    — замолчать;
   *   onMic(down)         — кнопка микрофона зажата / отпущена;
   *   onOpenSettings()    — «Открыть настройки» у ошибки ключа или адреса;
   *   onAnswer(text)      — ответ пришёл на голосовой вопрос: прочитать его.
   */
  constructor(opts) {
    this.client = opts.client;
    this.errorText = opts.errorText ?? DEFAULT_ERROR_TEXT;
    /** Умеет ли источник отвечать на вопросы — см. RuntimeConfig::dialogue в Rust. */
    this.dialogue = opts.dialogue ?? false;
    this.onGeometry = opts.onGeometry ?? (() => {});
    this.onClose = opts.onClose ?? (() => {});
    this.onSpeak = null;
    this.onStopSpeaking = null;
    this.onMic = null;
    this.onOpenSettings = null;

    mountIcons();
    const host = document.createElement("div");
    host.innerHTML = TEMPLATE.trim();
    this.el = host.firstElementChild;

    /** @type {Record<string, HTMLElement>} */
    this.ui = {};
    for (const node of this.el.querySelectorAll("[data-el]")) this.ui[node.dataset.el] = node;
    if (this.el.dataset.el) this.ui.root = this.el;
    for (const [name, glyph] of Object.entries(ICONS)) this.ui[name]?.prepend(icon(glyph));

    this.state = this.#fresh();
    /** Читает ли сейчас голос — ставит хост (он знает, когда речь кончилась). */
    this.speakingNow = false;
    this.retries = 0;
    this.retryTimer = 0;
    this.lastOpen = null;

    /** Все незавершённые запросы отменяются при закрытии. */
    this.explainAbort = null;
    this.askAbort = null;
    this.elaborateAbort = null;

    this.#bind();
  }

  #fresh() {
    return {
      term: "",
      context: "",
      phase: "loading", // loading | success | error | stopped
      data: null,
      expanded: false,
      thread: [],
      pending: false,
      notice: null, // { kind: "news" | "reminder", label, title, text }
      error: null, // { title, text, raw, offline, settings }
    };
  }

  #bind() {
    // Фокус документа не отбираем: mousedown внутри окна гасится, иначе страница
    // потеряет выделение, ради которого попап и открылся.
    // Исключение — явный клик в поле ввода: там фокус нужен.
    this.el.addEventListener("mousedown", (e) => {
      if (e.target === this.ui.input) return;
      // С Ctrl в тексте ответа выделяют — чтобы спросить про выделенное
      // (см. mouseup ниже); ползунок прокрутки должен прокручивать.
      if (e.ctrlKey && this.ui.body.contains(e.target)) return;
      if (onScrollbar(e)) return;
      // «Подробности» ошибки раскрываются своим щелчком.
      if (e.target.closest?.("summary")) return;
      e.preventDefault();
    });

    // Выделил с Ctrl кусок ответа — Ноа продолжает про него.
    this.el.addEventListener("mouseup", (e) => {
      if (!e.ctrlKey) return;
      const selection = window.getSelection?.();
      const picked = selection?.toString().trim();
      if (!picked || !this.ui.body.contains(selection.anchorNode)) return;
      selection.removeAllRanges();
      this.askAbout(picked);
    });

    const click = (name, handler) =>
      this.ui[name].addEventListener("click", (e) => {
        e.preventDefault();
        handler();
      });
    click("expand", () => this.expand());
    click("close", () => this.onClose());
    click("send", () => this.submitAsk());
    click("stop", () => this.stop());
    click("speak", () => (this.speakingNow ? this.onStopSpeaking?.() : this.onSpeak?.(this.spokenText())));
    click("readStop", () => this.onStopSpeaking?.());
    click("retry", () => this.retry());
    click("retryNow", () => this.retry());
    click("openSettings", () => this.onOpenSettings?.());

    // Микрофон — как пробел в окне: зажал — говоришь, отпустил — вопрос ушёл.
    let held = false;
    const release = () => {
      if (!held) return;
      held = false;
      this.ui.mic.classList.remove("is-held");
      this.onMic?.(false);
    };
    this.ui.mic.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      held = true;
      this.ui.mic.classList.add("is-held");
      this.onMic?.(true);
    });
    for (const type of ["pointerup", "pointerleave", "pointercancel"]) this.ui.mic.addEventListener(type, release);

    this.ui.input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        this.submitAsk();
      }
      // Esc внутри поля закрывает попап целиком, а не только сбрасывает ввод.
      if (e.key === "Escape") {
        e.preventDefault();
        this.onClose();
      }
    });

    this.ui.body.addEventListener("scroll", () => this.#paintFade(), { passive: true });
  }

  /**
   * Показывает готовое сообщение вместо ответа модели: напоминание о задаче
   * или весть («Вышла версия…»). Спрашивать про него нечего — оно уже написано.
   *
   * Первое предложение — заголовком, остальное — текстом. `kind` —
   * "reminder" (напоминание) или "news" (весть, по умолчанию).
   */
  announce(text, { kind = "news", label = "" } = {}) {
    this.#abortAll();
    this.#clearRetry();
    const { title, text: rest } = splitMessage(text);
    this.state = {
      ...this.#fresh(),
      phase: "success",
      data: { def: rest, simple: "", examples: [] },
      notice: { kind, label: label || (kind === "reminder" ? "Напоминание" : "Весть"), title, text: rest },
    };
    this.render();
  }

  /**
   * Показывает объяснение термина.
   *
   * `speak: true` — прочитать ответ вслух, как только он придёт. Так окно
   * открывается на вопрос, заданный голосом с чистого места: человек спросил
   * вслух, ответа он ждёт тоже вслух, а не глазами.
   */
  open({ term: raw, context = "", speak = false }, { retrying = false } = {}) {
    this.#abortAll();
    this.#clearRetry();
    if (!retrying) this.retries = 0;
    this.lastOpen = { term: raw, context, speak };
    this.speakOnAnswer = Boolean(speak);
    // Окно открыл голосовой вопрос — его «термин» и есть сам вопрос. Следующие
    // вопросы голосом — новые вопросы, а не уточнения к первому: иначе модель
    // слышала «уточни про „Стрелки“» и отвечала про первое снова и снова.
    this.openedByVoice = Boolean(speak);
    const term = normalizeTerm(raw);
    this.state = {
      ...this.#fresh(),
      term,
      // Исходный текст выделения не теряется: если термин был обрезан, полная
      // версия уходит в AI контекстом.
      context: context || raw,
    };
    this.render();

    this.explainAbort = new AbortController();
    const signal = this.explainAbort.signal;

    // Сторож. Обещание, которое не сбылось и не порвалось, — это вечный кружок
    // загрузки: окно висит, объяснения нет, и человеку неоткуда узнать почему.
    // Такое бывает, когда ответ теряется по дороге — например, обработчик в Rust
    // упал на панике и просто ничего не ответил. Молчание — тоже отказ.
    const watchdog = setTimeout(() => {
      if (this.state.term !== term || this.state.phase !== "loading") return;
      this.explainAbort.abort();
      this.#fail({ message: NO_RESPONSE_TEXT });
    }, RESPONSE_TIMEOUT_MS);

    // Вопрос, заданный голосом с чистого места, — это вопрос, а не термин.
    const request = this.speakOnAnswer
      ? this.client.ask("", "", [], raw, { signal }).then((answer) => ({ def: answer, simple: "", examples: [] }))
      : this.client.explain(term, context, { signal });

    request
      .then((data) => {
        clearTimeout(watchdog);
        if (signal.aborted || this.state.term !== term) return;
        this.retries = 0;
        this.state.data = data;
        this.state.phase = "success";
        this.render();
        if (this.speakOnAnswer) {
          this.speakOnAnswer = false;
          this.onAnswer?.(data.def);
        }
      })
      .catch((err) => {
        clearTimeout(watchdog);
        if (signal.aborted || err?.kind === "abort" || this.state.term !== term) return;
        this.#fail(err);
      });
  }

  /** Ошибка ответа: что случилось, что сделать и, при обрыве связи, повтор сам. */
  #fail(err) {
    const message = err?.userText || (err?.kind === undefined ? err?.message : "") || this.errorText;
    const raw = [err?.status ? `HTTP ${err.status}` : "", err?.message && err.message !== message ? err.message : ""].filter(Boolean).join(" · ");
    const offline =
      err?.kind === "network" ||
      err?.kind === "timeout" ||
      message === this.errorText ||
      message === DEFAULT_ERROR_TEXT ||
      /нет (связи|сети|интернета)|сбой сети|failed to fetch|network/i.test(`${message} ${err?.message ?? ""}`);
    const settings =
      !offline &&
      (err?.kind === "config" || [401, 403].includes(err?.status) || /ключ|адрес|настройк|\b40[13]\b/i.test(message));
    const { title, text } = offline
      ? { title: "", text: "Ответ придёт, как только появится связь. Можно не ждать — «Сейчас»." }
      : splitMessage(message);
    this.state.phase = "error";
    this.state.error = { title, text, raw, offline, settings, message };
    if (offline) this.#scheduleRetry();
    this.render();
  }

  #scheduleRetry() {
    this.#clearRetry();
    const wait = RETRY_STEPS[Math.min(this.retries, RETRY_STEPS.length - 1)];
    this.retries++;
    let left = wait;
    const tick = () => {
      if (this.state.phase !== "error" || !this.state.error?.offline) return;
      this.ui.offlineText.textContent = `Нет связи · повтор через ${left} с`;
      if (left <= 0) return this.retry({ auto: true });
      left--;
      this.retryTimer = setTimeout(tick, 1000);
    };
    // Первый кадр рисует render(); отсчёт — со следующего тика.
    this.state.retryIn = wait;
    this.retryTimer = setTimeout(tick, 0);
  }

  #clearRetry() {
    clearTimeout(this.retryTimer);
    this.retryTimer = 0;
  }

  /** Повторить последний вопрос: кнопкой или сам, по отсчёту. */
  retry({ auto = false } = {}) {
    if (!this.lastOpen) return;
    this.open(this.lastOpen, { retrying: auto });
  }

  /** «Остановить»: ответ ещё не пришёл, а ждать его уже не нужно. */
  stop() {
    if (this.state.phase === "loading") {
      this.explainAbort?.abort();
      this.state.phase = "stopped";
      this.render();
      return;
    }
    if (this.state.pending) {
      this.askAbort?.abort();
      this.elaborateAbort?.abort();
      const last = this.state.thread[this.state.thread.length - 1];
      if (last && !last.a) last.a = "Остановлено.";
      else if (this.state.expanded && this.state.data && !this.state.data.simple) this.state.data.simple = "Остановлено.";
      this.state.pending = false;
      this.render();
    }
  }

  /**
   * Раскрывает окно.
   *
   * `elaborate: false` — раскрыть, но не догружать «по шагам». Так приходит
   * голосовой вопрос: человек уже спросил о своём, и подсовывать ему вместо
   * ответа пересказ определения — навязывать то, чего он не просил.
   */
  expand({ elaborate = true } = {}) {
    if (this.state.expanded) return;
    if (elaborate && !this.#canExpand()) return;
    this.state.expanded = true;
    // У Википедии развёрнутый текст уже на руках — он пришёл вместе с определением.
    // У модели его ещё нет: спрашиваем сейчас, раз человек попросил.
    if (elaborate && !this.state.data?.simple && this.dialogue) this.#elaborate();
    this.render();
  }

  /** Догружает «по шагам» отдельным вопросом к модели. */
  #elaborate() {
    const asked = this.state.term;
    this.state.pending = true;
    this.elaborateAbort = new AbortController();
    const signal = this.elaborateAbort.signal;

    // Тот же сторож, что у объяснения и у вопроса: без него незакрытый pending
    // навсегда оставит крутящийся индикатор и заблокирует поле ввода.
    const watchdog = setTimeout(() => {
      if (!this.state.pending || this.state.term !== asked) return;
      this.elaborateAbort.abort();
      if (this.state.data) this.state.data.simple = NO_RESPONSE_TEXT;
      this.state.pending = false;
      this.render();
    }, RESPONSE_TIMEOUT_MS);

    // Первый ответ — в историю разговора: так модель видит, что уже сказала.
    const said = this.state.data?.def
      ? [{ q: this.openedByVoice ? asked : `Что такое «${asked}»?`, a: this.state.data.def }]
      : [];
    this.client
      .ask(asked, this.state.context, said, ELABORATE_QUESTION, { signal })
      .then((answer) => {
        clearTimeout(watchdog);
        if (signal.aborted || this.state.term !== asked) return;
        if (this.state.data) this.state.data.simple = answer;
        this.state.pending = false;
        this.render();
      })
      .catch((err) => {
        clearTimeout(watchdog);
        if (signal.aborted || err?.kind === "abort" || this.state.term !== asked) return;
        // Определение уже показано и остаётся на месте: неудача касается только
        // раскрытия, ронять из-за неё весь попап незачем.
        if (this.state.data) this.state.data.simple = err?.userText ?? this.errorText;
        this.state.pending = false;
        this.render();
      });
  }

  /**
   * Задать вопрос голосом.
   *
   * Разговор стоит под ответом всегда, а не только в раскрытом окне, поэтому
   * голосовому вопросу есть куда лечь из любого состояния.
   */
  askByVoice(text) {
    const question = String(text ?? "").trim();
    if (!question) return;
    // TODO(human): окно может показывать готовое сообщение — напоминание о
    // задаче. У него нет термина, и вопрос уходит модели как «объясни
    // пустоту»: она отвечает «уточните, какой термин нужно объяснить».
    // Решить, что делать в этом случае.
    this.ui.input.value = question;
    this.submitAsk({ byVoice: true });
  }

  /**
   * Спросить про выделенный кусок ответа.
   *
   * Вопрос ложится в разговор как обычный: видно, о чём спросили, и разговор
   * идёт дальше с тем же контекстом. Окно, открытое голосом, и отвечает голосом.
   */
  askAbout(text) {
    const piece = String(text ?? "").trim().replace(/\s+/g, " ");
    if (!piece || this.state.phase !== "success" || this.state.pending) return;
    const short = piece.length > 160 ? `${piece.slice(0, 160)}…` : piece;
    this.ui.input.value = `Подробнее: «${short}»`;
    this.submitAsk({ speak: this.openedByVoice });
  }

  submitAsk({ byVoice = false, speak = false } = {}) {
    const question = this.ui.input.value.trim();
    if (!question || this.state.pending) return;

    this.state.thread.push({ q: question, a: "" });
    this.state.pending = true;
    this.ui.input.value = "";
    this.render();
    this.#scrollToLatest();

    const index = this.state.thread.length - 1;
    this.askAbort = new AbortController();
    const signal = this.askAbort.signal;
    const asked = this.state.term;

    // Тот же сторож, что и у объяснения: вопрос без ответа оставляет разговор
    // с крутящимся индикатором и запрещает задать следующий.
    const watchdog = setTimeout(() => {
      if (!this.state.pending || this.state.term !== asked) return;
      this.askAbort.abort();
      if (this.state.thread[index]) this.state.thread[index].a = NO_RESPONSE_TEXT;
      this.state.pending = false;
      this.render();
    }, RESPONSE_TIMEOUT_MS);

    const standalone = byVoice && this.openedByVoice;
    this.client
      .ask(
        standalone ? "" : this.state.term,
        standalone ? "" : this.state.context,
        this.state.thread.slice(0, index),
        question,
        { signal },
      )
      .then((answer) => {
        clearTimeout(watchdog);
        if (signal.aborted || this.state.term !== asked) return;
        if (this.state.thread[index]) this.state.thread[index].a = answer;
        this.state.pending = false;
        this.render();
        this.#scrollToLatest();
        // Спросили голосом — отвечаем голосом.
        if (byVoice || speak) this.onAnswer?.(answer);
      })
      .catch((err) => {
        clearTimeout(watchdog);
        if (signal.aborted || err?.kind === "abort" || this.state.term !== asked) return;
        // Ошибку показываем на месте ответа, не сбивая уже полученное объяснение.
        if (this.state.thread[index]) this.state.thread[index].a = err?.userText ?? this.errorText;
        this.state.pending = false;
        this.render();
      });
  }

  /**
   * Что читать вслух.
   *
   * Не всё окно целиком, а то, что появилось последним. Так голос повторяет
   * ход разговора: открыли — определение; нажали «?» — по шагам и примеры;
   * спросили — ответ.
   */
  spokenText() {
    const { phase, data, expanded, thread, error, notice } = this.state;
    if (phase === "error") return [error?.title, error?.text].filter(Boolean).join(". ") || this.errorText;
    if (phase !== "success" || !data) return "";
    if (notice) return [notice.title, notice.text].filter(Boolean).join(". ");

    const answered = thread.filter((m) => m.a);
    if (answered.length) return answered[answered.length - 1].a;

    if (expanded) {
      const parts = [];
      if (data.simple) parts.push(data.simple);
      if (data.examples?.length) parts.push("Где встречается. " + data.examples.join(". "));
      if (parts.length) return parts.join(" ");
    }

    return data.def || "";
  }

  /**
   * Идёт ли запись голоса. Строка нужна не для красоты: человек держит клавишу
   * и должен видеть, что его слышат, — иначе непонятно, говорить уже или ещё нет.
   */
  set listening(on) {
    this.ui.listening.hidden = !on;
    this.#reportGeometry();
  }

  /** Читает ли голос сейчас: полоса «Читаю вслух» и кнопка в шапке. */
  set speaking(on) {
    this.speakingNow = Boolean(on);
    this.render();
  }

  get speaking() {
    return this.speakingNow;
  }

  /**
   * Прокручивает тело окна к свежему ответу.
   *
   * Высота тела ограничена, и в длинном разговоре новый ответ появлялся ниже
   * видимой части: человек спрашивал и смотрел на прежний текст, не понимая,
   * ответили ему или нет.
   */
  #scrollToLatest() {
    const body = this.ui.body;
    if (body) body.scrollTop = body.scrollHeight;
    this.#paintFade();
  }

  close() {
    this.#abortAll();
    this.#clearRetry();
  }

  #abortAll() {
    this.explainAbort?.abort();
    this.askAbort?.abort();
    this.elaborateAbort?.abort();
    this.explainAbort = null;
    this.askAbort = null;
    this.elaborateAbort = null;
  }

  #canExpand() {
    if (this.state.expanded || this.state.notice) return false;
    // Текст под «?» либо уже есть, либо его есть у кого спросить. Показывать
    // кнопку, за которой пусто, — обманывать: человек нажмёт и ничего не получит.
    return Boolean(this.state.data?.simple) || (this.dialogue && this.state.phase === "success");
  }

  /** Текст ответа — с кодом, таблицами и списками, если они в нём есть. */
  #rich(node, text) {
    node.replaceChildren(renderRich(text ?? "", { icon }));
  }

  render() {
    const s = this.state;
    const busy = s.phase === "loading" || s.pending;

    this.el.dataset.state = s.phase;
    this.el.dataset.expanded = String(s.expanded);
    this.el.dataset.notice = s.notice ? s.notice.kind : "";

    // Шапка. У вести вместо термина — подпись («Весть», «Напоминание»), а
    // заголовок — строкой ниже, крупно.
    this.ui.kicker.hidden = !s.notice;
    if (s.notice) {
      this.ui.kicker.replaceChildren(icon(s.notice.kind === "reminder" ? "bell" : "refresh", 14), document.createTextNode(s.notice.label));
    }
    this.ui.term.hidden = Boolean(s.notice);
    this.ui.term.textContent = s.term;
    this.ui.term.title = s.term;
    this.ui.title.hidden = !s.notice?.title;
    this.ui.title.textContent = s.notice?.title ?? "";

    this.ui.stop.hidden = !busy;
    this.ui.speak.hidden = !this.onSpeak || busy || !(s.phase === "success" || s.phase === "error");
    this.ui.speak.classList.toggle("is-on", this.speakingNow);
    this.ui.speak.title = this.speakingNow ? "Замолчать" : "Прочитать вслух";
    this.ui.expand.hidden = busy || !this.#canExpand();

    this.ui.loading.hidden = s.phase !== "loading";
    this.el.setAttribute("aria-busy", String(busy));
    this.ui.main.hidden = s.phase !== "success";
    this.ui.error.hidden = s.phase !== "error" && s.phase !== "stopped";
    this.ui.reading.hidden = !this.speakingNow || !this.onStopSpeaking;

    if (s.phase === "error" || s.phase === "stopped") this.#renderError();

    if (s.phase === "success" && s.data) {
      this.#rich(this.ui.answer, s.data.def);
      this.ui.answer.hidden = !s.data.def;
      this.ui.extra.hidden = !s.expanded;
      if (s.expanded) {
        this.#rich(this.ui.simple, s.data.simple ?? "");
        // Раскрыться можно и без «по шагам» — так приходит голосовой вопрос.
        // Заголовок над пустотой выглядел бы недогрузившимся текстом.
        this.ui.simpleSection.hidden = !s.data.simple;
        this.ui.examplesSection.hidden = !s.data.examples?.length;
        this.ui.examples.replaceChildren(
          ...(s.data.examples ?? []).map((text) => {
            const row = document.createElement("span");
            row.className = "popup__example";
            const bullet = document.createElement("span");
            bullet.className = "popup__bullet";
            bullet.textContent = "·";
            bullet.setAttribute("aria-hidden", "true");
            row.append(bullet, document.createTextNode(text));
            return row;
          }),
        );
      }
      this.ui.thread.replaceChildren(
        ...s.thread.map((m) => {
          const wrap = document.createElement("div");
          wrap.className = "popup__message";
          const q = document.createElement("span");
          q.className = "popup__question";
          q.textContent = m.q;
          const a = document.createElement("div");
          a.className = "popup__reply";
          this.#rich(a, m.a);
          wrap.append(q, a);
          return wrap;
        }),
      );
      this.ui.pending.hidden = !s.pending;
    }

    // Поле «Уточнить» — внизу всегда, когда есть у кого спросить и о чём.
    // Там, где спросить не у кого (Википедия) или не о чем (весть), поля нет:
    // вопрос в пустоту хуже, чем его отсутствие.
    const canAsk = this.dialogue && Boolean(s.term) && s.phase === "success" && !s.notice;
    this.ui.ask.hidden = !canAsk;
    this.ui.mic.hidden = !this.onMic;
    this.ui.input.disabled = s.pending;

    this.#reportGeometry();
    this.#paintFade();
    this.onChange?.();
  }

  #renderError() {
    const s = this.state;
    const stopped = s.phase === "stopped";
    const error = stopped ? { title: "Остановлено", text: "", raw: "", offline: false, settings: false } : (s.error ?? {});
    this.ui.offline.hidden = !error.offline;
    if (error.offline && this.state.retryIn !== undefined && !this.ui.offlineText.textContent.startsWith("Нет связи")) {
      this.ui.offlineText.textContent = `Нет связи · повтор через ${this.state.retryIn} с`;
    }
    this.ui.errorHead.hidden = error.offline || !error.title;
    this.ui.errorHead.classList.toggle("is-quiet", stopped);
    this.ui.errorTitle.textContent = error.title ?? "";
    this.ui.errorText.hidden = !error.text;
    this.ui.errorText.textContent = error.text ?? "";
    this.ui.openSettings.hidden = !(error.settings && this.onOpenSettings);
    this.ui.retry.hidden = error.offline;
    this.ui.retry.lastChild.textContent = stopped ? "Спросить снова" : "Повторить";
    this.ui.details.hidden = !error.raw;
    this.ui.detailsBrief.textContent = error.raw ? ` · ${error.raw.slice(0, 40)}` : "";
    this.ui.detailsRaw.textContent = [error.message, error.raw].filter(Boolean).join("\n");
  }

  /** Затухание внизу — когда ниже есть ещё текст. */
  #paintFade() {
    const body = this.ui.body;
    if (!body || this.ui.main.hidden) return;
    const more = body.scrollHeight - body.scrollTop - body.clientHeight > 4;
    this.ui.fade.hidden = !more;
  }

  #reportGeometry() {
    // Синхронно: обращение к offsetWidth само заставляет браузер посчитать лейаут,
    // поэтому размер уже верный. Ждать кадр здесь нельзя — до первого замера окно
    // скрыто (чтобы не прыгало), а мобильный Safari придерживает
    // requestAnimationFrame во время прокрутки.
    this.onGeometry({ width: this.el.offsetWidth, height: this.el.offsetHeight });

    // Повторный замер после отрисовки: к этому моменту подгружаются шрифты и
    // переносы строк могут поменять высоту.
    requestAnimationFrame(() => {
      this.onGeometry({ width: this.el.offsetWidth, height: this.el.offsetHeight });
      this.#paintFade();
    });
  }

  /** Фокус в поле «Уточнить» — только по явному действию пользователя. */
  focusAsk() {
    if (!this.ui.ask.hidden) this.ui.input.focus();
  }

  /** Пусто ли поле вопроса: тогда пробел в окне значит «прочитай», а не слово. */
  get inputEmpty() {
    return this.ui.input.value === "";
  }
}
