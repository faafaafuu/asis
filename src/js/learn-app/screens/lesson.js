// 05 Урок с Ноа — урок по разделам: сегменты разделов, «Читать вслух»
// (Ноа читает раздел, код не зачитывает), текст, код с «Копировать»,
// «Частая ошибка», «Разобрать подробно» и «Обсудить» голосом. Внизу —
// «Назад» и «Дальше: следующий раздел». Место в уроке помнится.

import { el, icon, label, loading, nav, register, iconButton, sections, sectionTitle, sectionBody, markdown, speakable, voice, call, sheet, steps, ring, toast } from "../core.js";
import { store, topicCard } from "../store.js";

register("lesson", (screen, { course: courseId, topic: topicId }) => {
  const course = store.course(courseId);
  const card = topicCard(course, topicId);
  let parts = [];
  let at = card?.step === "lesson" || card?.step === "talk" ? card.section ?? 0 : 0;
  let reading = false;

  const bar = el("header", "bar");
  const segs = el("span", "grow");
  segs.style.padding = "0 8px";
  const speaker = iconButton("speaker", "Читать вслух", () => toggleRead(), 20);
  bar.append(iconButton("close", "Закрыть", () => nav.back()), segs, speaker);
  const content = el("div", "content content--read");
  content.append(loading());
  const dock = el("div", "dock");
  const prev = el("button", "square");
  prev.style.width = prev.style.height = "52px";
  prev.append(icon("arrow-left", 22, 1.9));
  prev.title = "Назад";
  const next = el("button", "btn btn--primary btn--big");
  next.style.flex = "1";
  dock.append(prev, next);
  screen.append(bar, content, dock, el("div", "homebar"));

  prev.addEventListener("click", () => {
    if (at > 0) go(at - 1);
    else nav.back();
  });
  next.addEventListener("click", () => {
    if (at < parts.length - 1) go(at + 1);
    else finish();
  });

  let alive = true;
  (async () => {
    try {
      const view = await store.topic(courseId, topicId);
      if (!alive) return;
      parts = sections(view.topic.lesson);
      at = Math.min(at, parts.length - 1);
      paint();
    } catch (err) {
      content.replaceChildren(el("span", "error", String(err)));
    }
  })();

  function paint() {
    segs.replaceChildren(steps(parts.length, at, at, { thin: true }));
    const part = parts[at];
    const title = sectionTitle(part) || (at === 0 ? card.title : `Раздел ${at + 1}`);
    content.replaceChildren(label(`// раздел ${at + 1} из ${parts.length} · ${card.title}`), el("span", "display read-title", title), markdown(sectionBody(part)));
    const pair = el("div", "pair");
    const deep = el("button", "btn btn--secondary", "Разобрать подробно");
    deep.addEventListener("click", () => openDeep());
    const talk = el("button", "btn btn--secondary");
    talk.append(icon("mic", 16, 2.25), "Обсудить");
    talk.addEventListener("click", () => discuss());
    pair.append(deep, talk);
    content.append(pair);
    content.scrollTop = 0;
    const following = parts[at + 1];
    next.replaceChildren(
      el("span", "btn__text", following ? `Дальше: ${sectionTitle(following) || `раздел ${at + 2}`}` : "Урок пройден"),
      icon(following ? "chevron-right" : "check", 18, 2.25),
    );
    call("learn_place", { course: courseId, topic: topicId, step: "lesson", section: at }).catch(() => {});
    if (reading) readAloud();
  }

  function go(index) {
    voice.stop();
    at = index;
    paint();
  }

  function toggleRead() {
    reading = !reading;
    speaker.setAttribute("aria-pressed", String(reading));
    if (reading) readAloud();
    else voice.stop();
  }

  function readAloud() {
    const part = parts[at];
    voice.speak(`${sectionTitle(part)}. ${speakable(sectionBody(part))}`);
  }

  async function finish() {
    voice.stop();
    await call("learn_read", { course: courseId, topic: topicId }).catch(() => {});
    await store.courses(true);
    toast("Урок пройден — закрепите карточками или проверкой");
    // Под уроком — его тема: она перерисуется со свежим прогрессом.
    nav.back();
  }

  /** Подробный разбор раздела: пишется один раз и дальше открывается сразу. */
  function openDeep() {
    sheet("Разобрать подробно", async (box) => {
      box.append(loading("Ноа разбирает раздел…"));
      try {
        const text =
          (await call("learn_deep", { course: courseId, topic: topicId, section: at, cached: true }).catch(() => null)) ??
          (await call("learn_deep", { course: courseId, topic: topicId, section: at }, { slow: true }));
        box.replaceChildren(markdown(text ?? "Разбор не пришёл.", "md md--small"));
      } catch (err) {
        box.replaceChildren(el("span", "error", String(err)));
      }
    });
  }

  /** Обсудить раздел: вопрос голосом или текстом — Ноа отвечает вслух. */
  function discuss() {
    voice.stop();
    sheet("Обсудить раздел", (box) => {
      const target = { course: courseId, topic: topicId, section: at };
      const log = el("div", "list");
      const who = el("div", "explain__who");
      const orb = ring("listen", 28);
      who.append(orb, el("span", "", "Спросите о разделе — голосом или текстом"));
      const typed = el("input", "field");
      typed.placeholder = "Вопрос о разделе — Enter";
      const mic = el("button", "btn btn--primary btn--big");
      mic.append(icon("mic", 18, 2.25), "Сказать");
      const ask = async (text) => {
        log.append(el("div", "say say--me", text));
        const wait = loading("Ноа думает…");
        log.append(wait);
        orb.dataset.ring = "look";
        try {
          const answer = await call("learn_ask", { target, text, voice: true }, { slow: true });
          wait.remove();
          const reply = el("div", "say");
          reply.append(markdown(answer, "md md--small"));
          log.append(reply);
          orb.dataset.ring = "speak";
          if (voice.aloud()) await voice.speak(answer);
        } catch (err) {
          wait.remove();
          log.append(el("div", "say say--bad", String(err)));
        }
        orb.dataset.ring = "listen";
        box.scrollTop = box.scrollHeight;
      };
      typed.addEventListener("keydown", (event) => {
        if (event.key === "Enter" && typed.value.trim()) {
          const text = typed.value.trim();
          typed.value = "";
          ask(text);
        }
      });
      mic.addEventListener("click", async () => {
        voice.stop();
        mic.disabled = true;
        who.lastChild.textContent = "Слушаю — говорите";
        try {
          const said = await voice.listen();
          who.lastChild.textContent = "Спросите о разделе — голосом или текстом";
          if (said) await ask(said);
          else toast("Не расслышала — скажите ещё раз");
        } catch (err) {
          who.lastChild.textContent = String(err);
        }
        mic.disabled = false;
      });
      box.append(who, log, typed, mic);
    });
  }

  return {
    full: true,
    cleanup: () => {
      alive = false;
      voice.stop();
    },
  };
});
