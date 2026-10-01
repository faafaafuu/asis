// Урок с Ноа — урок как разговор с репетитором, от первого раздела до
// последнего.
//
// Ноа держит в уме весь урок (план разделов и текст текущего) и ведёт по нему:
// рассказывает раздел своими словами — что это, зачем на практике, как об этом
// спросят на собеседовании, — и просит пересказать, как вы поняли. Ваш
// пересказ она разбирает: что верно, что упустили или перепутали. Поняли —
// идёте дальше; нет — объясняет иначе и спрашивает снова. Можно перебивать
// вопросами — она ответит и вернётся к уроку. Отвечать — голосом или текстом.
//
// Модель зовётся через learn_ask (обсуждение раздела, разговорный стиль):
// в программе — Rust, в браузере — web-api.js. Ход урока — здесь.

/** Метка в ответе модели: раздел понят, можно дальше. */
export const NEXT = "[ДАЛЬШЕ]";

const TELL = (n, total, title) =>
  `Ведём урок разговором. Сейчас раздел ${n} из ${total}${title ? ` — «${title}»` : ""}. ` +
  "Расскажи его своими словами, как репетитор вживую: 4–7 коротких фраз, главное и зачем это на практике, " +
  "пример из жизни, и как об этом спрашивают на собеседовании. Команды и код не зачитывай — скажи словами, что они делают. " +
  "В конце попроси меня пересказать своими словами, как я понял.";

const CHECK = (said) =>
  `Мой ответ: «${said}».\n` +
  "Если это пересказ — оцени по существу: что верно, что упустил или перепутал, коротко, 2–4 фразы. " +
  `Понял главное — скажи, что идём дальше, и закончи ответ меткой ${NEXT}. ` +
  "Не понял — объясни упущенное иначе, проще, и снова попроси пересказать. " +
  "Если это вопрос — ответь на него и попроси пересказать раздел.";

/** Заголовок раздела «## …» — для плана и подписи. */
export function sectionTitle(part) {
  return /^##\s+(.+)$/m.exec(String(part ?? ""))?.[1]?.trim() ?? "";
}

/** Ответ модели → текст без метки и признак «можно дальше». */
export function splitNext(reply) {
  const text = String(reply ?? "");
  const next = text.includes(NEXT);
  return { next, text: text.replaceAll(NEXT, "").trim() };
}

/**
 * Рисует урок с Ноа в `root`.
 * deps: { api, course, topic, parts, el, button, dictateButton, markdown, onDone, start }
 * parts — разделы урока (sections()), start — с какого начать.
 */
export function renderLessonAgent(root, deps) {
  const { api, course, topic, parts, el, button, dictateButton, markdown } = deps;
  const total = parts.length;
  const state = { at: Math.min(deps.start ?? 0, total - 1), busy: false, voice: readVoice(), ready: false };

  const box = el("section", "agent");
  const head = el("div", "agent__head");
  const where = el("span", "agent__where");
  const voiceBtn = button("", () => {
    state.voice = !state.voice;
    saveVoice(state.voice);
    if (!state.voice) hush();
    paint();
  }, true);
  head.append(where, voiceBtn);

  const log = el("div", "agent__log");
  const area = el("textarea", "answer agent__input");
  area.rows = 2;
  area.placeholder = "Перескажите своими словами или спросите — Enter, отправить";
  const send = button("Отправить", () => reply());
  const dictate = dictateButton(area);
  dictate.addEventListener("click", hush, true);
  const next = button("Следующий раздел →", () => go(state.at + 1), true);
  const actions = el("div", "actions agent__actions");
  actions.append(send, dictate, next);
  box.append(head, log, area, actions);
  root.append(box);

  area.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      reply();
    }
  });

  function paint() {
    const title = sectionTitle(parts[state.at]);
    where.textContent = `Раздел ${state.at + 1} из ${total}${title ? ` · ${title}` : ""}`;
    voiceBtn.textContent = state.voice ? "🔊 Ноа говорит вслух" : "🔇 Только текстом";
    send.disabled = state.busy;
    next.disabled = state.busy;
    next.textContent = state.at + 1 >= total ? "Закончить урок ✓" : "Следующий раздел →";
  }

  function hush() {
    api.invoke("voice_stop").catch(() => {});
  }

  function line(who, text) {
    const node = el("div", `agent__line agent__line--${who}`);
    if (who === "noa") node.innerHTML = markdown(text);
    else node.textContent = text;
    log.append(node);
    node.scrollIntoView({ block: "nearest", behavior: "smooth" });
    return node;
  }

  const target = () => ({ course: course.id, topic: topic.id, section: state.at });

  async function ask(text) {
    state.busy = true;
    paint();
    const wait = line("wait", "Ноа думает…");
    try {
      const answer = await api.invoke("learn_ask", { target: target(), text, voice: true });
      wait.remove();
      return answer;
    } catch (err) {
      wait.remove();
      line("error", `Ответа нет: ${err}`);
      return "";
    } finally {
      state.busy = false;
      paint();
    }
  }

  /** Ждём, пока Ноа договорит (не дольше минуты). */
  async function quiet() {
    const started = Date.now();
    let heard = false;
    while (Date.now() - started < 60_000) {
      const busy = await api.invoke("voice_busy").catch(() => false);
      if (busy) heard = true;
      else if (heard || Date.now() - started > 1500) return;
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
  }

  function say(text) {
    if (state.voice && text) api.invoke("voice_speak", { text }).catch(() => {});
  }

  /** Начать раздел: Ноа рассказывает и просит пересказать. */
  async function go(at) {
    hush();
    if (at >= total) return finish();
    state.at = at;
    api.invoke("learn_place", { course: course.id, topic: topic.id, step: "talk", section: at }).catch(() => {});
    paint();
    line("mark", `— Раздел ${at + 1}${sectionTitle(parts[at]) ? `: ${sectionTitle(parts[at])}` : ""} —`);
    const answer = await ask(TELL(at + 1, total, sectionTitle(parts[at])));
    if (!answer) return;
    line("noa", answer);
    say(answer);
    area.focus();
  }

  /** Ответ человека: пересказ или вопрос. */
  async function reply() {
    const said = area.value.trim();
    if (!said || state.busy) return;
    hush();
    area.value = "";
    line("me", said);
    if (/^(дальше|далее|следующ|пропусти|го дальше)/i.test(said)) return go(state.at + 1);
    const answer = await ask(CHECK(said));
    if (!answer) return;
    const { next: understood, text } = splitNext(answer);
    line("noa", text);
    say(text);
    if (understood) {
      // Раздел понят — дослушали оценку, и Ноа сама ведёт к следующему.
      const at = state.at;
      await quiet();
      if (state.at === at && !state.busy) go(at + 1);
    }
  }

  async function finish() {
    await api.invoke("learn_read", { course: course.id, topic: topic.id }).catch(() => {});
    line("mark", "— Урок пройден —");
    const done = "Урок пройден. Дальше — «Проверить себя»: задачи и мини-экзамен закрепят тему.";
    line("noa", done);
    say(done);
    next.hidden = true;
    deps.onDone?.();
  }

  paint();
  go(state.at);
}

const VOICE_KEY = "noa.agentVoice";

function readVoice() {
  try {
    return localStorage.getItem(VOICE_KEY) !== "off";
  } catch {
    return true;
  }
}

function saveVoice(on) {
  try {
    localStorage.setItem(VOICE_KEY, on ? "on" : "off");
  } catch {
    /* не запомнится */
  }
}
