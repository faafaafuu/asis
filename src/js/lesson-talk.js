// Урок разговором — для тех, у кого чтение не держится в голове.
//
// Вместо длинного текста — одна мысль за раз и сразу работа с ней:
//   1. Ноа спрашивает раньше, чем объясняет: «как думаете, что такое X?».
//      Попытка угадать — даже неверная — цепляет объяснение к памяти.
//   2. Объяснение — одна фраза определения, образ и зацепка. Вслух.
//   3. «Теперь своими словами». Вспомнить и сказать — это и есть запоминание;
//      ответ оценивает модель по смыслу, итог идёт в повторение по расписанию.
// Порции по четыре понятия — минут на пять-семь; после порции — короткий итог
// и выбор: дальше или перерыв. Отвечать можно голосом.
//
// Модуль не знает, где он работает: `api` — тот же, что у окна обучения
// (программа — Rust, браузер — web-api.js).

const CHUNK = 4;
const VOICE_KEY = "noa.talkVoice";

/** Оценка ответа по первому слову ответа модели. */
export function gradeOf(reply) {
  const head = String(reply ?? "").trim().toLowerCase();
  if (/^верно|^правильно|^да[,.! ]/.test(head)) return "good";
  if (/^почти|^частично|^близко/.test(head)) return "hard";
  if (/^неверно|^неправильно|^нет[,.! ]/.test(head)) return "again";
  return null;
}

/**
 * Какие понятия учить: сначала новые и слабые, выученные — в конце.
 * `level` — new | learning | young | mature (как у понятий темы).
 */
export function talkOrder(concepts) {
  const rank = { new: 0, learning: 1, young: 2, mature: 3 };
  return [...concepts].sort((a, b) => (rank[a.level] ?? 0) - (rank[b.level] ?? 0));
}

const readVoice = () => {
  try {
    return localStorage.getItem(VOICE_KEY) !== "off";
  } catch {
    return true;
  }
};

/**
 * Рисует урок-разговор в `root`.
 * deps: { api, course, topic, el, button, dictateButton, onDone }
 */
export async function renderTalkLesson(root, deps) {
  const { api, course, topic, el, button, dictateButton } = deps;
  let concepts = [];
  try {
    concepts = await api.invoke("learn_concepts", { course: course.id, topic: topic.id });
  } catch (err) {
    root.append(el("p", "bad", String(err)));
    return;
  }
  if (!concepts.length) {
    root.append(el("p", "note", "В этой теме нет понятий — откройте «Урок»."));
    return;
  }

  const queue = talkOrder(concepts);
  const state = { at: 0, chunk: [], voice: readVoice(), stage: "guess", guess: "", mine: "" };
  const box = el("section", "tl");
  root.append(box);

  const say = (text) => {
    if (!state.voice || !text) return;
    api.invoke("voice_speak", { text }).catch(() => {});
  };
  const hush = () => api.invoke("voice_stop").catch(() => {});

  const voiceToggle = () => {
    const node = button(state.voice ? "🔊 Ноа говорит" : "🔇 Без голоса", () => {
      state.voice = !state.voice;
      try {
        localStorage.setItem(VOICE_KEY, state.voice ? "on" : "off");
      } catch {
        /* не запомнится — не беда */
      }
      if (!state.voice) hush();
      draw();
    }, true);
    node.classList.add("tl__voice");
    return node;
  };

  const progress = () => {
    const inChunk = (state.at % CHUNK) + 1;
    const bar = el("div", "tl__progress");
    for (let i = 0; i < Math.min(CHUNK, queue.length - Math.floor(state.at / CHUNK) * CHUNK); i++) {
      const dot = el("span", "tl__dot");
      if (i < inChunk - 1) dot.classList.add("tl__dot--done");
      if (i === inChunk - 1) dot.classList.add("tl__dot--now");
      bar.append(dot);
    }
    bar.append(el("span", "tl__count", `понятие ${state.at + 1} из ${queue.length}`));
    return bar;
  };

  /** Поле ответа: текст или голос, Enter — отправить. */
  const answerArea = (placeholder, onSend) => {
    const area = el("textarea", "answer tl__input");
    area.rows = 2;
    area.placeholder = placeholder;
    area.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        onSend(area.value.trim());
      }
    });
    setTimeout(() => area.focus(), 0);
    return area;
  };

  const head = () => {
    const bar = el("div", "tl__head");
    bar.append(progress(), voiceToggle());
    return bar;
  };

  function draw() {
    box.replaceChildren();
    if (state.stage === "summary") return drawSummary();
    if (state.stage === "done") return drawDone();
    const c = queue[state.at];
    box.append(head());
    if (state.stage === "guess") drawGuess(c);
    else if (state.stage === "explain") drawExplain(c);
    else if (state.stage === "retell") drawRetell(c);
  }

  // 1. Вопрос раньше объяснения.
  function drawGuess(c) {
    const question = `Как думаете: что такое «${c.term}»?`;
    box.append(el("p", "tl__ask", question), el("p", "tl__sub", "Любая догадка лучше молчания — даже неверная помогает запомнить. Можно голосом."));
    const area = answerArea("Ваша догадка…", (text) => go(text));
    const go = (text) => {
      state.guess = text;
      state.stage = "explain";
      draw();
    };
    const actions = el("div", "actions");
    actions.append(button("Ответить", () => go(area.value.trim())), button("Не знаю — объясни", () => go(""), true), dictateButton(area));
    box.append(area, actions);
    say(question);
  }

  // 2. Объяснение — коротко, с образом и зацепкой.
  function drawExplain(c) {
    if (state.guess) box.append(el("p", "tl__mine", `Вы сказали: «${state.guess}»`));
    box.append(el("p", "tl__term", c.term), el("p", "tl__def", c.definition));
    if (c.analogy) box.append(el("p", "tl__hook", `≈ ${c.analogy}`));
    if (c.mnemonic) box.append(el("p", "tl__hook", `🧠 ${c.mnemonic}`));
    if (c.example) {
      const code = el("pre", "tl__example");
      code.append(el("code", "", c.example));
      box.append(code);
    }
    if (c.pitfall) box.append(el("p", "tl__pitfall", `⚠ ${c.pitfall}`));
    const next = button("Понятно — дальше →", () => {
      hush();
      state.stage = "retell";
      draw();
    });
    const actions = el("div", "actions");
    actions.append(next, button("Повторить вслух", () => say(spoken(c)), true));
    box.append(actions);
    setTimeout(() => next.focus(), 0);
    say(spoken(c));
  }

  const spoken = (c) => [c.definition, c.analogy ? `Похоже на: ${c.analogy}` : ""].filter(Boolean).join(" ");

  // 3. Своими словами — проверка по смыслу и оценка в повторение.
  function drawRetell(c) {
    const question = `Теперь своими словами: что такое «${c.term}»?`;
    box.append(el("p", "tl__ask", question), el("p", "tl__sub", "Не заглядывая. Пары слов хватит."));
    const area = answerArea("Своими словами…", (text) => check(text));
    const verdict = el("div", "tl__verdict");
    const actions = el("div", "actions");
    const checkBtn = button("Проверить", () => check(area.value.trim()));
    actions.append(checkBtn, button("Не помню — показать", () => settle("again", "Ничего страшного — вернётся в повторении скоро."), true), dictateButton(area));
    box.append(area, actions, verdict);
    say(question);

    async function check(text) {
      if (!text) return area.focus();
      checkBtn.disabled = true;
      verdict.replaceChildren(el("p", "note", "Ноа проверяет…"));
      let reply = "";
      try {
        reply = await api.invoke("learn_ask", {
          target: { course: course.id, topic: topic.id, question: `Что такое «${c.term}»?`, answer: text },
          text:
            `Проверь ответ ученика по смыслу. Определение: «${c.definition}». Ответ ученика: «${text}». ` +
            "Начни ровно с одного слова: «Верно», «Почти» или «Неверно». Дальше одна короткая фраза: " +
            "чего не хватило или что перепутано. Без вступлений, без похвалы, по-русски.",
        });
      } catch {
        reply = "";
      }
      const grade = gradeOf(reply);
      if (!grade) return selfGrade(text);
      settle(grade, reply.trim());
    }

    /** Модель не ответила — оценить самому, сверившись с определением. */
    function selfGrade(text) {
      state.mine = text;
      verdict.replaceChildren(el("p", "tl__def", c.definition), el("p", "note", "Сравните со своим ответом и оцените честно:"));
      const row = el("div", "actions");
      row.append(
        button("Помню", () => settle("good", "")),
        button("Почти", () => settle("hard", ""), true),
        button("Не помню", () => settle("again", ""), true),
      );
      verdict.append(row);
    }

    async function settle(grade, text) {
      await api.invoke("learn_grade", { course: course.id, card: `${topic.id}/c-${c.id}`, grade }).catch(() => {});
      state.chunk.push({ term: c.term, grade });
      verdict.replaceChildren();
      if (grade !== "good") verdict.append(el("p", "tl__def", c.definition));
      const said = text || { good: "Засчитано.", hard: "Почти — вернётся в повторении пораньше.", again: "Вернётся в повторении скоро." }[grade];
      verdict.append(el("p", `tl__feedback tl__feedback--${grade}`, said));
      actions.replaceChildren();
      const last = state.at + 1 >= queue.length;
      const endOfChunk = (state.at + 1) % CHUNK === 0;
      const next = button(last ? "Итог темы →" : endOfChunk ? "Итог порции →" : "Следующее понятие →", () => {
        hush();
        state.at += 1;
        state.guess = "";
        state.stage = last ? "done" : endOfChunk ? "summary" : "guess";
        draw();
      });
      actions.append(next);
      setTimeout(() => next.focus(), 0);
      if (text) say(text);
    }
  }

  // Итог порции: что было, и выбор — дальше или перерыв.
  function drawSummary() {
    const good = state.chunk.filter((x) => x.grade === "good").length;
    box.append(el("p", "tl__ask", `Порция позади: ${good} из ${state.chunk.length} своими словами сразу.`));
    box.append(el("p", "tl__sub", "Вспомните вслух хоть одно из этого — и перерыв или дальше:"));
    const list = el("ul", "tl__list");
    for (const x of state.chunk) list.append(el("li", x.grade === "good" ? "" : "tl__weak", x.term));
    box.append(list);
    const actions = el("div", "actions");
    const more = button("Ещё порция →", () => {
      state.chunk = [];
      state.stage = "guess";
      draw();
    });
    actions.append(more, button("Перерыв — продолжу потом", () => deps.onDone?.(), true));
    box.append(actions);
    setTimeout(() => more.focus(), 0);
    say(`Порция позади. ${good} из ${state.chunk.length} сразу. Вспомните вслух хоть одно — и дальше или перерыв.`);
  }

  function drawDone() {
    api.invoke("learn_read", { course: course.id, topic: topic.id }).catch(() => {});
    box.append(
      el("p", "tl__ask", "Тема пройдена разговором."),
      el("p", "tl__sub", "Слабые понятия вернутся в повторении — Ноа спросит их, когда начнут забываться. Задачи и мини-экзамен — во вкладках выше."),
    );
    const actions = el("div", "actions");
    actions.append(button("Готово", () => deps.onDone?.()));
    box.append(actions);
    say("Тема пройдена. Слабые понятия вернутся в повторении.");
  }

  draw();
}
