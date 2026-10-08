// 06 Повторение — карточки по расписанию: стопка карточек, вопрос, ответ по
// касанию, «Зацепка», четыре оценки. Вкладка «Повторение» — всё, что пора
// повторить по курсу; режим «Карточки» в теме — карточки темы.

import { el, label, loading, header, nav, register, iconButton, voice, call } from "../core.js";
import { store, topicCard } from "../store.js";

const GRADES = [
  { id: "again", name: "Не помню", hint: "10 мин", cls: "grade--again" },
  { id: "hard", name: "Трудно", hint: "1 д", cls: "grade--hard" },
  { id: "good", name: "Помню", hint: "3 д", cls: "grade--good" },
  { id: "easy", name: "Легко", hint: "7 д", cls: "grade--easy" },
];

function reviewScreen(screen, { course: courseId, topic: topicId = null, tab = false }) {
  const meta = el("span", "");
  let content;
  if (tab) {
    screen.append(header({ big: "Повторение", meta }));
  } else {
    const card = topicCard(store.course(courseId), topicId);
    const bar = header({ close: true, title: `${card?.title ?? ""} · карточки`, right: meta });
    meta.className = "bar__meta";
    bar.lastChild.style.marginRight = "8px";
    screen.append(bar);
  }
  const progressBar = el("div", "bar3");
  progressBar.style.flex = "none";
  const fill = el("span");
  fill.style.width = "0%";
  progressBar.append(fill);
  content = el("div", "content");
  content.style.padding = "20px 16px";
  content.append(loading());
  screen.append(progressBar, content);
  // Во весь экран (карточки темы) — отступ под полоску «домой».
  if (!tab) screen.append(el("div", "homebar"));

  let queue = [];
  let total = 0;
  let shown = false;
  let alive = true;

  (async () => {
    const courses = await store.courses();
    const course = courseId ? store.course(courseId) : store.currentCourse();
    if (!course) {
      content.replaceChildren(el("div", "empty", courses.length ? "Выберите курс." : "Курсов пока нет — соберите первый на вкладке «Курсы»."));
      return;
    }
    try {
      queue = await call("learn_review", { course: course.id, topic: topicId });
    } catch (err) {
      content.replaceChildren(el("span", "error", String(err)));
      return;
    }
    if (!alive) return;
    total = queue.length;
    paint(course);
  })();

  function paint(course) {
    meta.replaceChildren();
    fill.style.width = `${total ? ((total - queue.length) / total) * 100 : 100}%`;
    if (!queue.length) {
      meta.append("всё повторено");
      nav.setBadge(0);
      content.replaceChildren();
      const done = el("div", "plate plate--active");
      done.append(label("// на сегодня всё"));
      done.append(el("span", "continue__title", total ? "Карточки повторены" : "Повторять пока нечего"));
      done.append(
        el(
          "span",
          "continue__lead",
          total
            ? "Ноа вернёт их, когда придёт время — через часы, дни или недели."
            : "Карточки появляются, когда вы проходите уроки: понятия темы встают в повторение.",
        ),
      );
      content.append(done);
      return;
    }
    meta.append("осталось ");
    const left = el("span", "", queue.length);
    left.style.color = "var(--c-accent)";
    meta.append(left, ` из ${total}`);
    shown = false;
    const item = queue[0];
    content.replaceChildren();
    const deck = el("div", "deck");
    deck.append(el("div", "deck__under"), el("div", "deck__under"));
    const card = el("button", "card");
    const head = el("div", "card__head");
    head.append(label(`// ${item.topicTitle} · ${item.fresh ? "новая" : "понятие"}`), el("span", "grow"));
    const read = iconButton("speaker", "Прочитать вслух", (event) => {
      event.stopPropagation();
      voice.speak(shown ? `${item.front}. ${item.back}` : item.front);
    }, 20);
    read.style.width = read.style.height = "36px";
    head.append(read);
    card.append(head, el("span", "card__q", item.front));
    const tap = el("span", "card__tap", "Коснитесь, чтобы увидеть ответ");
    card.append(tap);
    deck.append(card);
    const grades = el("div", "grades");
    grades.hidden = true;
    for (const grade of GRADES) {
      const btn = el("button", `grade ${grade.cls}`);
      btn.append(grade.name, el("small", "", grade.hint));
      btn.addEventListener("click", () => rate(course, item, grade.id));
      grades.append(btn);
    }
    card.addEventListener("click", () => {
      if (shown) return;
      shown = true;
      tap.remove();
      card.append(el("span", "card__rule"), el("span", "card__a", item.back));
      const hook = item.mnemonic || item.analogy;
      if (hook) {
        const line = el("span", "card__hook");
        line.append(el("b", "", item.mnemonic ? "Зацепка: " : "Похоже на: "), hook);
        card.append(line);
      }
      grades.hidden = false;
    });
    content.append(deck, grades);
  }

  async function rate(course, item, grade) {
    try {
      const result = await call("learn_grade", { course: course.id, card: item.key, grade });
      queue.shift();
      // «Не помню» — карточка вернётся в этот же сеанс, в конец очереди.
      if (result.again) queue.push(item);
      nav.setBadge(Math.max(0, (course.mastery?.due ?? 0) - (total - queue.length)));
    } catch (err) {
      content.append(el("span", "error", String(err)));
      return;
    }
    paint(course);
  }

  return {
    full: !tab,
    cleanup: () => {
      alive = false;
      voice.stop();
      store.courses(true);
    },
  };
}

register("review", (screen, params) => reviewScreen(screen, { ...params, tab: true }));
register("review-topic", (screen, params) => reviewScreen(screen, params));
