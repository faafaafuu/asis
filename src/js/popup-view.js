// Вьюха попапа: разметка, состояния Loading/Success/Error, раскрытие и follow-up-тред
// (SPEC §6, §7, §8). Никакого позиционирования здесь нет — им занимается хост:
// в вебе `position.js`, в приложении — Rust (окно двигается целиком).
//
// Один и тот же класс используется и в окне Tauri, и в демо-документе в браузере,
// поэтому он не знает ничего про источник данных: AI-клиент передаётся снаружи.

import { DEFAULT_ERROR_TEXT } from "./ai-client.js";
import { normalizeTerm } from "./term.js";

/**
 * Предел ожидания ответа. Больше любого разумного сетевого таймаута (у запросов
 * к Википедии он 8 секунд, у моделей — 12), потому что это не замена им, а последняя
 * страховка: срабатывает, только если ответа не будет уже никогда.
 */
const RESPONSE_TIMEOUT_MS = 20_000;

/** Отдельный текст, а не общая «ошибка сети»: причина здесь другая и подсказка тоже. */
const NO_RESPONSE_TEXT = "Ответ не пришёл. Откройте настройку через значок в трее и нажмите «Проверить».";

const TEMPLATE = `
<div class="popup" data-el="root" tabindex="-1" role="dialog" aria-label="Объяснение выделенного текста">
  <div class="popup__head">
    <span class="popup__term" data-el="term"></span>
    <button class="popup__expand" data-el="expand" type="button" tabindex="-1"
            title="Проще и с примерами" aria-label="Показать проще и с примерами">?</button>
    <span class="popup__mark" data-el="mark" aria-hidden="true" hidden></span>
  </div>

  <div class="popup__loading" data-el="loading" role="status">
    <div class="popup__loading-row"><span class="spinner" aria-hidden="true"></span>Анализирую…</div>
    <div class="skeleton skeleton--1" aria-hidden="true"></div>
    <div class="skeleton skeleton--2" aria-hidden="true"></div>
    <div class="skeleton skeleton--3" aria-hidden="true"></div>
  </div>

  <div class="popup__body" data-el="body" role="status" aria-live="polite" hidden>
    <div data-el="answer"></div>
    <div class="popup__extra" data-el="extra" hidden>
      <div class="popup__section">
        <span class="popup__label">Простыми словами</span>
        <span class="popup__simple" data-el="simple"></span>
      </div>
      <div class="popup__section" data-el="examplesSection">
        <span class="popup__label">Примеры</span>
        <div class="popup__examples" data-el="examples"></div>
      </div>
      <div class="popup__thread">
        <div class="popup__messages" data-el="thread"></div>
        <span class="popup__pending" data-el="pending" hidden>
          <span class="spinner spinner--sm" aria-hidden="true"></span>Думаю…
        </span>
        <div class="popup__ask">
          <input class="popup__input" data-el="input" type="text" placeholder="Спросить ещё…"
                 aria-label="Спросить ещё" autocomplete="off" spellcheck="false" />
          <button class="popup__send" data-el="send" type="button" tabindex="-1"
                  title="Отправить" aria-label="Отправить вопрос">↵</button>
        </div>
      </div>
    </div>
  </div>

  <div class="popup__error" data-el="error" role="status" hidden>
    <span class="popup__error-dot" aria-hidden="true"></span><span data-el="errorText"></span>
  </div>
</div>`;

export class PopupView {
  /**
   * @param {{
   *   client: { explain: Function, ask: Function },
   *   errorText?: string,
   *   onGeometry?: (size: {width: number, height: number}) => void,
   *   onClose?: () => void,
   * }} opts
   */
  constructor(opts) {
    this.client = opts.client;
    this.errorText = opts.errorText ?? DEFAULT_ERROR_TEXT;
    this.onGeometry = opts.onGeometry ?? (() => {});
    this.onClose = opts.onClose ?? (() => {});

    const host = document.createElement("div");
    host.innerHTML = TEMPLATE.trim();
    this.el = host.firstElementChild;

    /** @type {Record<string, HTMLElement>} */
    this.ui = {};
    for (const node of this.el.querySelectorAll("[data-el]")) this.ui[node.dataset.el] = node;
    if (this.el.dataset.el) this.ui.root = this.el;

    this.state = {
      term: "",
      context: "",
      phase: "loading", // loading | success | error
      data: null,
      expanded: false,
      thread: [],
      pending: false,
    };

    /** Все незавершённые запросы отменяются при закрытии (SPEC §8). */
    this.explainAbort = null;
    this.askAbort = null;

    this.#bind();
  }

  #bind() {
    // Фокус документа не отбираем: mousedown внутри окна гасится, иначе страница
    // потеряет выделение, ради которого попап и открылся (SPEC §8).
    // Исключение — явный клик в поле ввода: там фокус нужен.
    this.el.addEventListener("mousedown", (e) => {
      if (e.target === this.ui.input) return;
      e.preventDefault();
    });

    this.ui.expand.addEventListener("click", (e) => {
      e.preventDefault();
      this.expand();
    });

    this.ui.send.addEventListener("click", (e) => {
      e.preventDefault();
      this.submitAsk();
    });

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
  }

  /** Открытие с новым термином: полный сброс состояния, мгновенный Loading. */
  open({ term: raw, context = "" }) {
    this.#abortAll();
    const term = normalizeTerm(raw);
    this.state = {
      term,
      // Исходный текст выделения не теряется: если термин был обрезан, полная
      // версия уходит в AI контекстом.
      context: context || raw,
      phase: "loading",
      data: null,
      expanded: false,
      thread: [],
      pending: false,
    };
    this.render();

    this.explainAbort = new AbortController();
    const signal = this.explainAbort.signal;

    // Сторож. Обещание, которое не сбылось и не порвалось, — это вечный кружок
    // загрузки: окно висит, объяснения нет, и человеку неоткуда узнать почему.
    // Такое бывает не от медленной сети (у сетевых запросов свой таймаут), а когда
    // ответ теряется по дороге — например, обработчик в Rust упал на панике и просто
    // ничего не ответил. Молчание — тоже отказ, и показать его обязаны мы.
    const watchdog = setTimeout(() => {
      if (this.state.term !== term || this.state.phase !== "loading") return;
      this.explainAbort.abort();
      this.state.phase = "error";
      this.state.errorMessage = NO_RESPONSE_TEXT;
      this.render();
    }, RESPONSE_TIMEOUT_MS);

    this.client
      .explain(term, context, { signal })
      .then((data) => {
        clearTimeout(watchdog);
        if (signal.aborted || this.state.term !== term) return;
        this.state.data = data;
        this.state.phase = "success";
        this.render();
      })
      .catch((err) => {
        clearTimeout(watchdog);
        if (signal.aborted || err?.kind === "abort" || this.state.term !== term) return;
        this.state.phase = "error";
        this.state.errorMessage = err?.userText ?? this.errorText;
        this.render();
      });
  }

  expand() {
    if (this.state.expanded || !this.#canExpand()) return;
    this.state.expanded = true;
    this.render();
  }

  submitAsk() {
    const question = this.ui.input.value.trim();
    if (!question || this.state.pending) return;

    this.state.thread.push({ q: question, a: "" });
    this.state.pending = true;
    this.ui.input.value = "";
    this.render();

    const index = this.state.thread.length - 1;
    this.askAbort = new AbortController();
    const signal = this.askAbort.signal;
    const asked = this.state.term;

    // Тот же сторож, что и у объяснения: вопрос без ответа оставляет тред
    // с крутящимся индикатором и запрещает задать следующий (pending не снимется).
    const watchdog = setTimeout(() => {
      if (!this.state.pending || this.state.term !== asked) return;
      this.askAbort.abort();
      if (this.state.thread[index]) this.state.thread[index].a = NO_RESPONSE_TEXT;
      this.state.pending = false;
      this.render();
    }, RESPONSE_TIMEOUT_MS);

    this.client
      .ask(this.state.term, this.state.context, this.state.thread.slice(0, index), question, { signal })
      .then((answer) => {
        clearTimeout(watchdog);
        if (signal.aborted || this.state.term !== asked) return;
        if (this.state.thread[index]) this.state.thread[index].a = answer;
        this.state.pending = false;
        this.render();
      })
      .catch((err) => {
        clearTimeout(watchdog);
        if (signal.aborted || err?.kind === "abort" || this.state.term !== asked) return;
        // Ошибку в треде показываем на месте вопроса, не сбивая уже полученное объяснение.
        if (this.state.thread[index]) this.state.thread[index].a = this.errorText;
        this.state.pending = false;
        this.render();
      });
  }

  close() {
    this.#abortAll();
  }

  #abortAll() {
    this.explainAbort?.abort();
    this.askAbort?.abort();
    this.explainAbort = null;
    this.askAbort = null;
  }

  #canExpand() {
    return Boolean(this.state.data?.simple) && !this.state.expanded;
  }

  render() {
    const s = this.state;

    this.ui.term.textContent = s.term;
    this.ui.term.title = s.term;

    this.el.dataset.state = s.phase;
    this.el.dataset.expanded = String(s.expanded);

    this.ui.expand.hidden = !this.#canExpand();
    this.ui.mark.hidden = !s.expanded;

    this.ui.loading.hidden = s.phase !== "loading";
    this.ui.body.hidden = s.phase !== "success";
    this.ui.error.hidden = s.phase !== "error";

    if (s.phase === "error") {
      this.ui.errorText.textContent = s.errorMessage ?? this.errorText;
    }

    if (s.phase === "success" && s.data) {
      this.ui.answer.textContent = s.data.def;
      this.ui.extra.hidden = !s.expanded;
      if (s.expanded) {
        this.ui.simple.textContent = s.data.simple;
        this.ui.examplesSection.hidden = !s.data.examples?.length;
        this.#renderList(this.ui.examples, s.data.examples ?? [], (text) => {
          const row = document.createElement("span");
          row.className = "popup__example";
          const bullet = document.createElement("span");
          bullet.className = "popup__bullet";
          bullet.textContent = "·";
          bullet.setAttribute("aria-hidden", "true");
          row.append(bullet, document.createTextNode(text));
          return row;
        });
        this.#renderList(this.ui.thread, s.thread, (m) => {
          const wrap = document.createElement("div");
          wrap.className = "popup__message";
          const q = document.createElement("span");
          q.className = "popup__question";
          q.textContent = m.q;
          const a = document.createElement("span");
          a.className = "popup__answer-line";
          a.textContent = m.a;
          wrap.append(q, a);
          return wrap;
        });
        this.ui.pending.hidden = !s.pending;
      }
    }

    this.#reportGeometry();
  }

  /** Полная перерисовка списка: он короткий, дифф не окупается. */
  #renderList(container, items, makeNode) {
    container.replaceChildren(...items.map(makeNode));
  }

  #reportGeometry() {
    // Синхронно: обращение к offsetWidth само заставляет браузер посчитать лейаут,
    // поэтому размер уже верный. Ждать кадр здесь нельзя — до первого замера окно
    // скрыто (чтобы не прыгало), а мобильный Safari придерживает
    // requestAnimationFrame во время прокрутки, и попап оставался невидимым
    // по секунде и дольше.
    this.onGeometry({ width: this.el.offsetWidth, height: this.el.offsetHeight });

    // Повторный замер после отрисовки: к этому моменту подгружаются шрифты и
    // переносы строк могут поменять высоту.
    requestAnimationFrame(() => {
      this.onGeometry({ width: this.el.offsetWidth, height: this.el.offsetHeight });
    });
  }

  /** Фокус в поле «доспросить» — только по явному действию пользователя. */
  focusAsk() {
    if (this.state.expanded) this.ui.input.focus();
  }
}
