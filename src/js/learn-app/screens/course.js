// 02 Курс — заголовок и прогресс, сегменты «Темы / Карта / Шпаргалка»,
// темы строками (✓ пройдена, ▸ текущая, номер — впереди), финальный экзамен.

import { el, icon, label, progress, loading, header, nav, register, call, iconButton, plural, markdown } from "../core.js";
import { store, topicPercent, isDone, currentTopic } from "../store.js";

register("course", (screen, { course: courseId }) => {
  const course = store.course(courseId);
  if (!course) {
    nav.back();
    return {};
  }
  const at = currentTopic(course);
  screen.append(header({ title: "Курс", right: iconButton("more", "Ещё", () => nav.push("topics-more", { course: courseId })) }));
  const content = el("div", "content");
  screen.append(content);

  const head = el("div", "course-head");
  const line = el("div", "course-head__progress");
  line.append(progress(course.percent, "bar4"), el("span", "", `${course.percent}%`));
  const mature = course.mastery ? `${course.mastery.mature} из ${course.mastery.total} понятий держатся` : "";
  head.append(el("span", "display course-head__title", course.title), line, el("span", "small dim", [plural(course.topics.length, "тема", "темы", "тем"), mature].filter(Boolean).join(" · ")));
  content.append(head);

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

// «Ещё» на экране курса: описание курса.
register("topics-more", (screen, { course: courseId }) => {
  const course = store.course(courseId);
  screen.append(header({ title: course?.title ?? "Курс" }));
  const content = el("div", "content");
  content.append(label("// о курсе"));
  content.append(markdown(course?.description || "Описания у курса нет.", "md md--small"));
  screen.append(content);
  return {};
});
