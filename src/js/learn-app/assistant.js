// Ассистент — Ноа, как на компьютере: кольцо поверх приложения. Коснулись —
// всплывает панель, кольцо слушает (зелёное), думает (смотрит по кругу),
// говорит (пурпурные зубцы). Разговор идёт сам: ответила — снова слушает,
// пока человек не замолчит или не попрощается. Можно и написать.
// Путь вопроса тот же, что у голоса на компьютере (phone_talk, потоком): распоряжения,
// обсуждение урока, модель с памятью разговора.

import { el, icon, iconButton, ring, voice, call, markdown, api } from "./core.js";

let fab = null;
let panel = null;
let session = 0;

const STATUS = {
  listen: "Слушаю",
  look: "Думаю…",
  speak: "Говорю",
  idle: "Коснитесь кольца и говорите",
};

/** Кнопка-кольцо: на экранах с вкладками — над панелью вкладок. */
export function mountAssistant() {
  if (fab) return;
  fab = el("button", "orb-fab");
  fab.setAttribute("aria-label", "Ноа — спросить голосом");
  fab.append(ring("look", 50));
  fab.addEventListener("click", () => (panel ? stop() : open()));
  document.body.append(fab);
}

/** Показать или спрятать кольцо: на полноэкранных режимах у них свой микрофон. */
export function showAssistant(on) {
  if (fab) fab.hidden = !on || Boolean(panel);
}

export function open(first = "") {
  if (panel) return;
  session += 1;
  const id = session;
  voice.stop();
  panel = el("div", "assistant");
  const head = el("div", "assistant__head");
  const orb = ring("listen", 64);
  const state = el("span", "assistant__state", STATUS.listen);
  head.append(orb, state, el("span", "grow"), iconButton("close", "Закрыть", () => stop(), 20));
  const log = el("div", "assistant__log");
  const typed = el("input", "field");
  typed.placeholder = "Или напишите — Enter";
  typed.enterKeyHint = "send";
  const again = el("button", "square");
  again.title = "Сказать ещё";
  again.append(icon("mic", 22, 1.9));
  const row = el("div", "pair");
  row.style.flex = "none";
  typed.style.flex = "1";
  row.append(typed, again);
  panel.append(head, log, row);
  document.body.append(panel);
  if (fab) fab.hidden = true;

  const alive = () => panel && session === id;
  const mode = (name) => {
    orb.dataset.ring = name === "idle" ? "look" : name;
    state.textContent = STATUS[name];
    panel?.setAttribute("data-mode", name);
  };
  const say = (who, text) => {
    const line = el("div", who === "me" ? "say say--me" : "say");
    if (who === "me") line.textContent = text;
    else line.append(markdown(text, "md md--small"));
    log.append(line);
    log.scrollTop = log.scrollHeight;
  };

  /** Один обмен: вопрос — ответ вслух. true — можно слушать дальше.
   *  Ответ идёт потоком (phone_talk): текст растёт по мере того, как модель
   *  пишет, а фразы звучат по готовности — Ноа заговаривает через пару секунд. */
  const exchange = async (text) => {
    say("me", text);
    mode("look");
    let live = null;
    const unlisten = await api?.listen("phone:answer", ({ payload }) => {
      if (!alive() || !payload?.text) return;
      if (!live) {
        live = el("div", "say");
        log.append(live);
        mode("speak");
      }
      live.replaceChildren(markdown(payload.text, "md md--small"));
      log.scrollTop = log.scrollHeight;
    });
    let answer, spoken = false;
    try {
      const got = await call("phone_talk", { text, speak: true }, { slow: true }).catch(async (err) => {
        // Старая программа без потока — прежний путь.
        if (/phone_talk|not found|unknown command/i.test(String(err))) return { text: await call("phone_ask", { text }, { slow: true }), spoken: false };
        throw err;
      });
      answer = got?.text ?? "";
      spoken = Boolean(got?.spoken);
    } catch (err) {
      unlisten?.();
      if (!alive()) return false;
      say("noa", String(err));
      mode("idle");
      return false;
    }
    unlisten?.();
    if (!alive()) return false;
    if (!answer) {
      mode("idle");
      return false;
    }
    if (live) live.replaceChildren(markdown(answer, "md md--small"));
    else say("noa", answer);
    if (!spoken) {
      mode("speak");
      await voice.speak(answer);
    }
    if (!alive()) return false;
    // Попрощались — разговор окончен.
    return !/^до связи/i.test(answer.trim());
  };

  /** Разговор без рук: слушать, отвечать, снова слушать — до тишины. */
  const converse = async () => {
    for (let quiet = 0; alive() && quiet < 2; ) {
      mode("listen");
      let heard = "";
      try {
        // Разговор: секунды тишины хватает, чтобы понять, что договорили.
        heard = await voice.listen({ pause: 1.2 });
      } catch (err) {
        if (!alive()) return;
        say("noa", String(err));
        mode("idle");
        return;
      }
      if (!alive()) return;
      if (!heard) {
        quiet += 1;
        continue;
      }
      quiet = 0;
      if (!(await exchange(heard))) return;
    }
    if (alive()) mode("idle");
  };

  typed.addEventListener("keydown", async (event) => {
    if (event.key !== "Enter" || !typed.value.trim()) return;
    const text = typed.value.trim();
    typed.value = "";
    typed.blur();
    voice.cancel();
    if (await exchange(text)) mode("idle");
  });
  typed.addEventListener("focus", () => {
    voice.cancel();
    voice.stop();
    mode("idle");
  });
  again.addEventListener("click", () => {
    voice.stop();
    voice.cancel();
    converse();
  });

  if (first) exchange(first).then((more) => more && converse());
  else converse();
}

export function stop() {
  session += 1;
  voice.cancel();
  voice.stop();
  panel?.remove();
  panel = null;
  if (fab) fab.hidden = false;
}
