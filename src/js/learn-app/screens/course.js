// 02 Курс — заголовок и прогресс, сегменты «Темы / Карта / Шпаргалка»,
// темы строками (✓ пройдена, ▸ текущая, номер — впереди), финальный экзамен.

import { el, icon, label, progress, loading, header, nav, register, call, iconButton, plural, markdown, toast, prefs, isWeb } from "../core.js";
import { store, topicPercent, isDone, currentTopic } from "../store.js";
import { pressMenu, confirmAction } from "../gestures.js";
import { tiles } from "./courses.js";

/**
 * Удалить курс — после подтверждения снизу: курс собирается долго, а
 * промахнуться легко. Курс уходит из аккаунта и со всех устройств; файл
 * остаётся в courses/.removed, прогресс — в аккаунте. true — удалён.
 */
export async function removeCourse(course) {
  const sure = await confirmAction({
    title: `Удалить курс «${course.title}»?`,
    text: "Уберётся с телефона, из программы и с сайта. Прогресс сохранится — курс можно собрать заново.",
    ok: "Удалить курс",
  });
  if (!sure) return false;
  try {
    await call("learn_course_remove", { course: course.id }, { slow: true });
  } catch (err) {
    toast(String(err));
    return false;
  }
  store.forget();
  if (prefs.get("course") === course.id) prefs.set("course", null);
  toast("Курс удалён");
  return true;
}

/** Чем заняться по теме — меню по удержанию её строки или карточки. */
export function topicActions(course, topic) {
  const params = { course: course.id, topic: topic.id };
  return [
    { text: "Урок", icon: "learn", run: () => nav.push("lesson", params) },
    { text: "Наизусть", icon: "cards", run: () => nav.push("drill", params) },
    { text: "Проверка", icon: "target", run: () => nav.push("check", { course: course.id, scope: topic.id }) },
    { text: "Собеседование по теме", icon: "user", run: () => nav.push("interview", params) },
    ...(isWeb ? [] : [{ text: "Практика в терминале", icon: "keyboard", run: () => nav.push("practice", params) }]),
  ];
}

register("course", (screen, { course: courseId }) => {
  const course = store.course(courseId);
  if (!course) {
    nav.back();
    return {};
  }
  const at = currentTopic(course);
  screen.append(header({ title: "Обзор курса", right: iconButton("more", "Ещё", () => nav.push("topics-more", { course: courseId })) }));
  const content = el("div", "content");
  screen.append(content);

  // Заголовок 24 и прогресс: процент · полоса · сдано из.
  const head = el("div", "m-course-head");
  const done = course.topics.filter(isDone).length;
  const line = el("div", "m-course-head__progress");
  line.append(el("span", "m-course-head__pct", `${course.percent}%`), progress(course.percent, "m-bar"), el("span", "m-progress__pct", `${done} / ${course.topics.length}`));
  head.append(el("span", "m-course-head__title", course.title), line);
  content.append(head);

  // «Дальше»: тема, «Начать» на всю ширину, ниже урок с Ноа и практика.
  if (at && !isDone(at)) {
    const next = el("div", "m-active m-next");
    const n = course.topics.indexOf(at) + 1;
    const section = at.step === "lesson" || at.step === "talk" ? (at.section ?? 0) : 0;
    const text = el("span", "m-next__text");
    text.append(label(`// дальше${at.read || section ? ` · раздел ${section + 1}` : ""}`), el("span", "m-next__title", `${n}. ${at.title}`));
    const params = { course: course.id, topic: at.id };
    const start = el("button", "m-btn m-btn--primary");
    start.append(icon("play", 16, 2.5), at.read || section ? "Дальше" : "Начать");
    start.addEventListener("click", () => nav.push(at.read ? "topic" : "lesson", params));
    const pair = el("div", "m-next__pair");
    const talk = el("button", "m-btn m-btn--soft");
    talk.append(icon("mic", 16, 2), "Урок с Ноа");
    talk.addEventListener("click", () => nav.push("lesson", params));
    pair.append(talk);
    if (!isWeb) {
      const practice = el("button", "m-btn m-btn--soft");
      practice.append(icon("window", 16, 2), "Практика");
      practice.addEventListener("click", () => nav.push("practice", params));
      pair.append(practice);
    }
    next.append(text, start, pair);
    content.append(next);
  }

  // Путь курса: 12 клеток в ряд, заливка — сколько темы пройдено.
  const path = el("div", "m-block");
  path.append(label("Путь курса"));
  const cells = el("div", "m-path");
  course.topics.forEach((topic, i) => {
    const cell = el("button", `m-cell${topic.id === at?.id && !isDone(topic) ? " is-now" : ""}`);
    const pct = isDone(topic) ? 100 : topicPercent(topic, course.topicPass);
    cell.setAttribute("aria-label", `${i + 1}. ${topic.title} · ${pct}%`);
    const fill = el("span");
    fill.style.height = `${pct}%`;
    cell.append(fill);
    cell.addEventListener("click", () => nav.push("topic", { course: course.id, topic: topic.id }));
    cells.append(cell);
  });
  path.append(cells);
  content.append(path);

  content.append(tiles(course));

  // Слабые места: больше всего ошибок — сверху.
  const weak = course.topics.filter((t) => t.mistakes > 0).sort((a, b) => b.mistakes - a.mistakes);
  if (weak.length) {
    const box = el("div", "m-block");
    const top = el("div", "m-head");
    top.append(label("Слабые места"), el("span", "m-head__more", `всего · ${weak.length}`));
    box.append(top);
    const rows = el("div", "m-rows");
    for (const topic of weak.slice(0, 4)) {
      const row = el("button", "m-row m-row--weak");
      row.append(el("span", "m-row__title", topic.title), el("span", "m-row__err", `ошибок ${topic.mistakes}`), icon("chevron-right", 20, 2));
      row.addEventListener("click", () => nav.push("topic", { course: course.id, topic: topic.id }));
      rows.append(row);
    }
    box.append(rows);
    content.append(box);
  }

  content.append(label("Все темы"));
  const seg = el("div", "seg");
  const body = el("div", "list");
  const views = { topics: "Темы", map: "Карта", sheet: "Шпаргалка" };
  let view = "topics";
  for (const [key, text] of Object.entries(views)) {
    const item = el("button", "", text);
    item.setAttribute("aria-pressed", String(key === view));
    item.addEventListener("click", () => {
      view = key;
      for (const other of seg.children) other.setAttribute("aria-pressed", String(other === item));
      paint();
    });
    seg.append(item);
  }
  content.append(seg, body);

  function paint() {
    body.replaceChildren();
    if (view === "topics") return paintTopics();
    if (view === "map") return paintMap();
    return paintSheet();
  }

  function paintTopics() {
    const rows = el("div", "rows");
    course.topics.forEach((topic, i) => {
      const done = isDone(topic);
      const now = topic.id === at?.id && !done;
      const row = el("button", `row${done ? " is-done" : ""}${now ? " is-now" : ""}`);
      const pct = topicPercent(topic, course.topicPass);
      const mark = el("span", "row__mark", done ? "✓" : now ? "▸" : String(i + 1));
      const text = el("span", "row__body");
      const bar = el("span", "row__bar");
      const fill = el("span");
      fill.style.width = `${pct}%`;
      bar.append(fill);
      text.append(el("span", "row__title", `${i + 1}. ${topic.title}`), bar);
      row.append(mark, text, el("span", "row__pct", topic.status === "new" ? "—" : `${pct}%`));
      row.addEventListener("click", () => nav.push("topic", { course: course.id, topic: topic.id }));
      pressMenu(row, () => [{ text: "Открыть тему", icon: "chevron-right", run: () => nav.push("topic", { course: course.id, topic: topic.id }) }, ...topicActions(course, topic)]);
      rows.append(row);
    });
    body.append(rows);
    const final = el("button", course.finalUnlocked ? "final is-open" : "final");
    final.append(icon(course.finalUnlocked ? "target" : "lock", 20, 2), el("span", "grow", "Финальный экзамен"));
    final.append(
      el(
        "span",
        "small dim",
        course.finalBest != null ? `лучший ${course.finalBest}%` : course.finalUnlocked ? `порог ${course.finalPass}%` : `после ${plural(course.topics.length, "темы", "тем", "тем")}`,
      ),
    );
    final.addEventListener("click", () => {
      if (course.finalUnlocked) nav.push("check", { course: course.id, scope: "final" });
    });
    body.append(final);
    const interview = el("button", "final is-open");
    interview.append(icon("user", 20, 2), el("span", "grow", "Собеседование по курсу"), el("span", "small dim", "мок-интервью"));
    interview.addEventListener("click", () => nav.push("interview", { course: course.id }));
    body.append(interview);
  }

  async function paintMap() {
    body.append(loading());
    try {
      const map = await call("learn_map", { course: course.id });
      body.replaceChildren();
      body.append(el("span", "small dim", "Понятия по темам. Цвет — насколько держится: зелёный — уверенно, циан — держится, жёлтый — учится, тусклый — ещё не видели."));
      for (const topic of map.topics) {
        const block = el("button", "plate map-topic");
        block.append(label(`// ${topic.title}`));
        const chips = el("div", "chips");
        for (const concept of topic.concepts) {
          const chip = el("span", "chip");
          const dot = el("span", `concept-dot is-${concept.level}`);
          chip.append(dot, concept.term);
          chips.append(chip);
        }
        block.append(chips);
        block.addEventListener("click", () => nav.push("topic", { course: course.id, topic: topic.id }));
        body.append(block);
      }
    } catch (err) {
      body.replaceChildren(el("span", "error", String(err)));
    }
  }

  function paintSheet() {
    body.append(el("span", "small dim", "Конспект каждой темы — на одну страницу, для быстрого повторения."));
    const rows = el("div", "rows");
    course.topics.forEach((topic, i) => {
      const row = el("button", "row");
      row.append(el("span", "row__mark", String(i + 1)), el("span", "row__title", topic.title), icon("chevron-right", 18, 2));
      row.addEventListener("click", () => nav.push("sheet", { course: course.id, topic: topic.id }));
      rows.append(row);
    });
    body.append(rows);
  }

  paint();
  return {};
});

// «Ещё» на экране курса: описание курса и удаление.
register("topics-more", (screen, { course: courseId }) => {
  const course = store.course(courseId);
  screen.append(header({ title: course?.title ?? "Курс" }));
  const content = el("div", "content");
  content.append(label("// о курсе"));
  content.append(markdown(course?.description || "Описания у курса нет.", "md md--small"));

  content.append(label("// удалить"));
  const remove = el("button", "setting setting--danger");
  const text = el("span", "setting__text");
  const name = el("span", "setting__name", "Удалить курс");
  const hint = el("span", "setting__hint", "Уберётся с телефона, из программы и с сайта. Прогресс сохранится — курс можно собрать заново.");
  text.append(name, hint);
  remove.append(icon("trash", 20, 2), text);
  remove.addEventListener("click", async () => {
    if (!course) return;
    remove.disabled = true;
    if (await removeCourse(course)) nav.home();
    else remove.disabled = false;
  });
  content.append(remove);
  screen.append(content);
  return {};
});
