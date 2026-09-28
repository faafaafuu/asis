// Разговор голосом в браузере: экран с фирменным кольцом Ноа.
//
// Одно нажатие — и дальше без рук: Ноа слушает, отвечает вслух и снова
// слушает, пока не скажут «спасибо» или не нажмут «Закончить». Кольцо
// показывает, что сейчас происходит: слушает, думает, говорит. Нажатие на
// кольцо, пока Ноа говорит, перебивает её — как пробел в программе.
//
// Один и тот же экран у разговора на главной, у обсуждения урока и у
// устного зачёта; различается только то, кто отвечает (`reply`).

import { createOrb } from "../orb.js";
import { speak, stopSpeaking, listen, canListen, speaking } from "./voice.js";

/** «Спасибо», «хватит» — закончить разговор. Слово целиком: \b не видит кириллицу. */
const BYE = /(^|[^\p{L}])(спасибо|хватит|стоп|пока|закончим)([^\p{L}]|$)/iu;

/** Сколько раз подряд можно промолчать, прежде чем Ноа закончит сама. */
const QUIET_LIMIT = 3;

const STATE_TEXT = {
  idle: "",
  loading: "Готовлюсь…",
  listening: "Слушаю",
  thinking: "Думаю",
  speaking: "Говорю — нажмите на кольцо, чтобы перебить",
};

function addStyle() {
  const href = new URL("../../styles/talk.css", import.meta.url).href;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = href;
  document.head.append(link);
}

function el(tag, className, text = "") {
  const node = document.createElement(tag);
  node.className = className;
  if (text) node.textContent = text;
  if (tag === "button") node.type = "button";
  return node;
}

let current = null;

/**
 * Начать разговор.
 *
 * @param {{
 *   title?: string,
 *   intro?: string,
 *   reply: (said: string) => Promise<string | {text: string, done?: boolean}>,
 *   onExchange?: (said: string, answer: string) => void,
 *   bye?: boolean,
 *   onQuiet?: () => string | null,
 * }} opts
 * `bye: false` — слова прощания разбирает сам `reply` (устный зачёт: «хватит»
 * там значит «подвести итог»). `onQuiet` — что сказать, если человек замолчал.
 * `greeting` — показать текстом, не говоря: микрофон тогда включается сразу,
 * в том же нажатии. Телефонные браузеры охотнее дают микрофон по нажатию,
 * чем через несколько секунд после него.
 */
export function startTalk({ title = "Разговор", intro = "", greeting = "", reply, onExchange, bye = true, onQuiet }) {
  if (!canListen) throw new Error("Голос в этом браузере не работает — напишите текстом.");
  current?.stop();
  addStyle();

  const overlay = el("div", "talk-overlay");
  overlay.setAttribute("role", "dialog");
  overlay.setAttribute("aria-label", title);
  const card = el("div", "talk-card");
  const head = el("div", "talk-head");
  const close = el("button", "talk-close", "Закончить");
  head.append(el("span", "talk-title", title), close);
  const canvas = el("canvas", "talk-orb");
  canvas.width = 360;
  canvas.height = 180;
  canvas.setAttribute("aria-hidden", "true");
  const state = el("p", "talk-state");
  const heard = el("p", "talk-heard");
  const answer = el("div", "talk-answer");
  const hint = el("p", "talk-hint", "Говорите, как с человеком. «Спасибо» — закончить.");
  card.append(head, canvas, state, heard, answer, hint);
  overlay.append(card);
  document.body.append(overlay);
  document.documentElement.classList.add("talk-open");

  const orb = createOrb(canvas);
  let stopped = false;
  let hearing = null;

  const setMode = (mode) => {
    orb.setMode(mode);
    state.textContent = STATE_TEXT[mode] ?? "";
    state.dataset.mode = mode;
  };

  const stop = () => {
    if (stopped) return;
    stopped = true;
    hearing?.abort();
    stopSpeaking();
    for (const resolve of closeWaits) resolve();
    orb.stop();
    overlay.remove();
    document.documentElement.classList.remove("talk-open");
    document.removeEventListener("keydown", onKey);
    if (current === session) current = null;
  };

  const onKey = (event) => {
    if (event.key === "Escape") stop();
  };
  document.addEventListener("keydown", onKey);
  close.addEventListener("click", stop);
  // Нажатие на кольцо: пока Ноа говорит — перебить; в режиме «по нажатию» —
  // начать слушать.
  let tapped = null;
  canvas.addEventListener("click", () => {
    if (speaking()) stopSpeaking();
    tapped?.();
  });
  /** Браузер не дал микрофон без нажатия — дальше слушаем по нажатию на кольцо. */
  let tapToTalk = false;
  const waitTap = () =>
    new Promise((resolve) => {
      tapped = () => {
        tapped = null;
        resolve();
      };
      closeWaits.push(resolve);
    });
  const closeWaits = [];

  const say = async (text) => {
    if (stopped || !text) return;
    answer.textContent = text;
    setMode("speaking");
    await speak(text);
  };

  const done = (async () => {
    setMode("loading");
    try {
      if (greeting) answer.textContent = greeting;
      if (intro) await say(intro);
      let quiet = 0;
      while (!stopped) {
        if (tapToTalk) {
          orb.setMode("idle");
          state.textContent = "Нажмите на кольцо и говорите";
          state.dataset.mode = "idle";
          await waitTap();
          if (stopped) break;
        }
        setMode("listening");
        heard.textContent = "";
        hearing = new AbortController();
        let said;
        try {
          said = await listen({
            signal: hearing.signal,
            onHeard: (text) => (heard.textContent = text),
          });
        } catch (err) {
          // Первый отказ — возможно, браузер хочет нажатия: пробуем по нажатию.
          if (!tapToTalk && /микрофон/i.test(err?.message ?? "")) {
            tapToTalk = true;
            continue;
          }
          throw err;
        }
        if (stopped) break;
        if (!said) {
          if (tapToTalk) continue;
          if (++quiet >= QUIET_LIMIT) {
            await say(onQuiet?.() ?? "Закончим. Нажмите кнопку, если захотите ещё.");
            break;
          }
          continue;
        }
        quiet = 0;
        heard.textContent = said;
        if (bye && BYE.test(said) && said.split(/\s+/).length <= 4) {
          await say("Хорошо, до связи.");
          break;
        }
        setMode("thinking");
        let result;
        try {
          result = await reply(said);
        } catch (err) {
          result = `Не получилось ответить: ${err?.message ?? err}`;
        }
        const text = typeof result === "string" ? result : result?.text ?? "";
        onExchange?.(said, text);
        await say(text);
        if (typeof result === "object" && result?.done) break;
      }
    } catch (err) {
      answer.textContent = err?.message ?? String(err);
      state.textContent = "";
      orb.setMode("idle");
      // Ошибку показываем, а не закрываем экран сразу: её надо успеть прочитать.
      await new Promise((resolve) => setTimeout(resolve, 6000));
    } finally {
      stop();
    }
  })();

  const session = { stop, done };
  current = session;
  return session;
}

/**
 * Маленький знак Ноа для кнопки «Поговорить»: то же кольцо, в покое.
 * Отдаёт canvas, который можно положить в кнопку.
 */
export function orbIcon() {
  const canvas = document.createElement("canvas");
  canvas.width = 360;
  canvas.height = 180;
  canvas.className = "orb-icon";
  canvas.setAttribute("aria-hidden", "true");
  createOrb(canvas).setMode("listening");
  addStyle();
  return canvas;
}
