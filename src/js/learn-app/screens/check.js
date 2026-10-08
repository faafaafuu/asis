// 07 Проверить себя — мини-экзамен темы (или финальный экзамен курса):
// вопрос, варианты A–D или ответ своими словами; после ответа — выбранный
// неверный розовым, верный зелёным, объяснение Ноа. В конце — итог и балл
// в прогресс (learn_submit).

import { el, label, loading, nav, register, iconButton, call, markdown, voice, ring, speakable, icon, button } from "../core.js";
import { store, topicCard } from "../store.js";

register("check", (screen, { course: courseId, scope }) => {
  const course = store.course(courseId);
  const topic = scope === "final" ? null : topicCard(course, scope);
  const counter = el("span", "");
  counter.style.cssText = "font-size:13px;color:var(--c-accent)";
  const bar = el("header", "bar");
  bar.style.paddingRight = "16px";
  bar.append(iconButton("close", "Закрыть", () => nav.back()), el("span", "bar__title", `${topic?.title ?? "Финальный экзамен"} · проверка`), counter);
  const content = el("div", "content");
  content.style.cssText = "padding:20px 16px;gap:12px";
  content.append(loading());
  const dock = el("div", "dock");
  screen.append(bar, content, dock, el("div", "homebar"));

  let exam = null;
  let at = 0;
  const answers = {};
  let alive = true;

  (async () => {
    try {
      exam = await call("learn_exam", { course: courseId, scope });
    } catch (err) {
      content.replaceChildren(el("span", "error", String(err)));
      return;
    }
    if (!alive) return;
    if (!exam.questions.length) {
      content.replaceChildren(el("div", "empty", "Вопросов для проверки у этой темы нет."));
      return;
    }
    paint();
  })();

  function paint() {
    voice.stop();
    const q = exam.questions[at];
    counter.textContent = `${at + 1} / ${exam.questions.length}`;
    content.replaceChildren(el("span", "question", q.q));
    const explain = el("div", "explain");
    explain.hidden = true;
    const next = el("button", "btn btn--primary");
    next.style.cssText = "flex:1.4;height:52px;font-size:15px";
    next.append(at + 1 < exam.questions.length ? "Следующий" : "Завершить", icon("chevron-right", 18, 2.25));
    next.disabled = true;
    next.addEventListener("click", () => {
      if (at + 1 < exam.questions.length) {
        at += 1;
        paint();
      } else submit();
    });
    const toCards = el("button", "btn btn--secondary", "В карточки");
    toCards.style.cssText = "flex:1;height:52px;font-size:13.5px";
    toCards.addEventListener("click", () => {
      if (topic) nav.push("review-topic", { course: courseId, topic: topic.id });
      else nav.tab("review", true);
    });
    const skip = el("button", "btn btn--plain", "Пропустить");
    skip.style.cssText = "flex:1;height:52px";
    skip.addEventListener("click", () => {
      if (at + 1 < exam.questions.length) {
        at += 1;
        paint();
      } else submit();
    });
    dock.replaceChildren(skip, next);

    const verdictShown = (verdict) => {
      answers[q.id] = q.kind === "choice" ? verdict.chosen : answers[q.id];
      explain.hidden = false;
      const who = el("span", "explain__who");
      who.append(ring("speak", 28), "Ноа объясняет");
      const text = [verdict.feedback, verdict.right ? "" : verdict.reference].filter(Boolean).join("\n\n");
      explain.replaceChildren(who, markdown(text || (verdict.right ? "Верно." : "Не совсем."), "md md--small"));
      if (voice.aloud()) voice.speak(speakable(verdict.feedback || verdict.reference));
      next.disabled = false;
      dock.replaceChildren(toCards, next);
    };

    if (q.kind === "choice" && q.options.length) {
      const options = el("div", "options");
      const letters = "ABCDEFGH";
      const items = q.options.map((option, i) => {
        const item = el("button", "option");
        item.append(el("span", "option__mark", letters[i]), el("span", "", option));
        options.append(item);
        return item;
      });
      items.forEach((item, i) => {
        item.addEventListener("click", async () => {
          if (options.dataset.done) return;
          options.dataset.done = "1";
          answers[q.id] = i;
          let verdict;
          try {
            verdict = await call("learn_check", { course: courseId, question: q.id, answer: i }, { slow: true });
          } catch (err) {
            delete options.dataset.done;
            content.append(el("span", "error", String(err)));
            return;
          }
          const right = verdict.answer;
          items.forEach((other, j) => {
            if (j === right) {
              other.classList.add("is-right");
              other.firstChild.textContent = "✓";
            } else if (j === i) {
              other.classList.add("is-wrong");
              other.firstChild.textContent = "✕";
              other.append(el("span", "option__note", "ваш ответ"));
            }
          });
          verdictShown(verdict);
        });
      });
      content.append(options, explain);
    } else {
      const box = el("textarea", "answer-box");
      box.placeholder = "Ответьте своими словами — как на собеседовании";
      const mic = button("", async () => {
        mic.disabled = true;
        try {
          const said = await voice.listen();
          if (said) box.value = `${box.value} ${said}`.trim();
        } catch (err) {
          content.append(el("span", "error", String(err)));
        }
        mic.disabled = false;
      }, "square");
      mic.append(icon("mic", 22, 1.9));
      mic.title = "Ответить голосом";
      const go = el("button", "btn btn--primary", "Проверить");
      go.style.flex = "1";
      go.addEventListener("click", async () => {
        const text = box.value.trim();
        if (!text) return;
        go.disabled = true;
        go.textContent = "Ноа проверяет…";
        answers[q.id] = text;
        try {
          const verdict = await call("learn_check", { course: courseId, question: q.id, answer: text }, { slow: true });
          box.readOnly = true;
          row.remove();
          const score = el("span", verdict.right ? "ok" : "error", verdict.score != null ? `Балл: ${verdict.score} из 100` : "Модель не проверила — оцените себя сами");
          explain.before(score);
          verdictShown(verdict);
          // Модель не ответила: сверить с эталоном и честно сказать, знал ли.
          if (verdict.score == null) {
            const own = el("div", "pair");
            for (const [text, knew] of [
              ["Знал", true],
              ["Не знал", false],
            ]) {
              const pick = el("button", "btn btn--secondary", text);
              pick.addEventListener("click", async () => {
                await call("learn_self_grade", { course: courseId, question: q.id, knew }).catch(() => {});
                own.replaceChildren(el("span", knew ? "ok" : "error", knew ? "Засчитано" : "Вопрос вернётся в повторение"));
              });
              own.append(pick);
            }
            explain.after(own);
          }
        } catch (err) {
          go.disabled = false;
          go.textContent = "Проверить";
          content.append(el("span", "error", String(err)));
        }
      });
      const row = el("div", "pair");
      row.style.flex = "none";
      row.append(mic, go);
      content.append(box, row, explain);
    }
    content.scrollTop = 0;
  }

  async function submit() {
    voice.stop();
    counter.textContent = "итог";
    content.replaceChildren(loading("Считаю итог…"));
    dock.replaceChildren();
    try {
      const result = await call("learn_submit", { course: courseId, scope, answers }, { slow: true });
      await store.courses(true);
      content.replaceChildren();
      const head = el("div", "plate plate--active");
      head.append(label(result.passed ? "// сдано" : "// пока не сдано"), el("span", "score", `${result.score}%`));
      head.append(el("span", "muted", result.passed ? `Порог ${result.pass}% пройден.` : `Порог — ${result.pass}%. Ошибки вернулись в повторение — пройдите карточки и попробуйте ещё раз.`));
      if (result.unchecked) head.append(el("span", "small dim", `Без проверки модели: ${result.unchecked} — они посчитаны нулём.`));
      content.append(head);
      for (const item of result.items.filter((v) => !v.right)) {
        const box = el("div", "plate");
        box.append(el("span", "concept__term", item.q));
        if (item.reference) box.append(markdown(item.reference, "md md--small"));
        content.append(box);
      }
      const back = el("button", "btn btn--primary btn--big", "Готово");
      back.addEventListener("click", () => nav.back());
      dock.replaceChildren(back);
    } catch (err) {
      content.replaceChildren(el("span", "error", String(err)));
      const back = el("button", "btn btn--secondary", "Закрыть");
      back.addEventListener("click", () => nav.back());
      dock.replaceChildren(back);
    }
  }

  return {
    full: true,
    cleanup: () => {
      alive = false;
      voice.stop();
    },
  };
});
