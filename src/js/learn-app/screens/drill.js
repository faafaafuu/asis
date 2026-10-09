// «Наизусть» — понятия темы до автоматизма. Порция в шесть понятий, каждое
// проходит три шага: запомнить (прочитать с Ноа), вспомнить пропуски в
// определении, рассказать по памяти. Ошибся — шаг назад; вспомнил целиком
// дважды подряд — выучено. Между подходами к одному понятию — другие:
// интервал, а не зубрёжка подряд. Итог уходит в интервальное повторение.

import { el, icon, label, loading, nav, register, iconButton, call, voice, toast, plural, steps, prefs } from "../core.js";
import { store, topicCard } from "../store.js";
import { checkRecall } from "../coach.js";

/** Сколько понятий за подход: больше в кратковременную память не влезает. */
const PORTION = 6;
/** Сколько раз подряд вспомнить целиком, чтобы считать выученным. */
const SOLID = 2;

const STOP = new Set("который которая которое которые этого этому через между потому только тоже также чтобы когда если может можно нужно всего этим этой этих".split(" "));

/** Определение с пропусками: прячем каждое третье «весомое» слово. */
function blanks(text) {
  const parts = String(text).split(/(\s+)/);
  const heavy = parts
    .map((word, at) => ({ word, at, core: word.replace(/[^\p{L}\p{N}-]/gu, "") }))
    .filter((item) => item.core.length >= 5 && !STOP.has(item.core.toLowerCase()));
  const hide = new Set(heavy.filter((_, i) => i % 3 === 1 || heavy.length <= 3).map((item) => item.at));
  if (!hide.size && heavy.length) hide.add(heavy[0].at);
  // Прячется само слово; знаки вокруг остаются на месте.
  return parts.map((word, at) => {
    if (!hide.has(at)) return { word, hidden: false };
    const [, before = "", core = word, after = ""] = /^([^\p{L}\p{N}]*)([\p{L}\p{N}-]+)(.*)$/u.exec(word) ?? [];
    return { word: core, before, after, hidden: true };
  });
}

register("drill", (screen, { course: courseId, topic: topicId }) => {
  const course = store.course(courseId);
  const card = topicCard(course, topicId);
  const meta = el("span", "bar__meta");
  const bar = el("header", "bar");
  bar.append(iconButton("close", "Закрыть", () => nav.back()), el("span", "bar__title", `${card?.title ?? ""} · наизусть`), meta);
  meta.style.marginRight = "8px";
  const progressRow = el("div", "pbar-steps");
  const content = el("div", "content");
  content.style.cssText = "padding:16px;gap:14px";
  content.append(loading("Собираю порцию понятий…"));
  const dock = el("div", "dock");
  screen.append(bar, progressRow, content, dock, el("div", "homebar"));

  let queue = [];
  let items = [];
  let alive = true;
  /** Понятия, уже пройденные в этом подходе: следующая порция — из остальных. */
  const seen = new Set();

  async function loadPortion() {
    content.replaceChildren(loading("Собираю порцию понятий…"));
    dock.replaceChildren();
    let concepts;
    try {
      concepts = await call("learn_concepts", { course: courseId, topic: topicId });
    } catch (err) {
      content.replaceChildren(el("span", "error", String(err)));
      return;
    }
    if (!alive) return;
    // Сначала то, что держится хуже: новое и учимое, потом остальное.
    const rank = { new: 0, learning: 1, young: 2, mature: 3 };
    const sorted = [...concepts].sort((a, b) => (rank[a.level] ?? 0) - (rank[b.level] ?? 0));
    const chunk = sorted.filter((c) => !seen.has(c.id)).slice(0, PORTION);
    if (!chunk.length) {
      content.replaceChildren(el("div", "empty", seen.size ? "Все понятия темы пройдены — загляните в «Повторение» завтра." : "В теме нет понятий для заучивания."));
      return;
    }
    for (const c of chunk) seen.add(c.id);
    items = chunk.map((c) => ({ c, stage: 0, streak: 0, misses: 0, done: false }));
    queue = [...items];
    next();
  }

  function paintProgress() {
    const solid = items.filter((item) => item.done).length;
    meta.textContent = `${solid} из ${items.length}`;
    progressRow.replaceChildren(steps(items.length, solid, -1, { ok: true, thin: true }));
  }

  /** Следующее понятие: первое невыученное в очереди. */
  function next() {
    voice.stop();
    paintProgress();
    const item = queue.find((one) => !one.done);
    if (!item) return finish();
    queue = [...queue.filter((one) => one !== item), item];
    if (item.stage === 0) learn(item);
    else if (item.stage === 1) gaps(item);
    else recall(item);
  }

  function head(item, step, title) {
    const box = el("div", "drill__head");
    box.append(label(`// шаг ${step} из 3 · ${title}`), el("span", "drill__term", item.c.term));
    return box;
  }

  /** Шаг 1: прочитать — с зацепкой, Ноа читает вслух. */
  function learn(item) {
    const box = el("div", "plate plate--active drill");
    box.append(head(item, 1, "запомни"), el("span", "drill__def", item.c.definition));
    const hook = item.c.mnemonic || item.c.analogy;
    if (hook) {
      const line = el("span", "card__hook");
      line.append(el("b", "", item.c.mnemonic ? "Зацепка: " : "Похоже на: "), hook);
      box.append(line);
    }
    if (item.c.example) {
      const line = el("span", "concept__extra");
      line.append(el("b", "", "Пример: "), item.c.example);
      box.append(line);
    }
    content.replaceChildren(box, el("span", "small dim", "Прочитайте вслух вместе с Ноа и представьте пример — так понятие ложится в память, а не скользит мимо."));
    if (voice.aloud()) voice.speak(`${item.c.term}. ${item.c.definition}`);
    const say = el("button", "square");
    say.title = "Прочитать ещё раз";
    say.append(icon("speaker", 22, 1.9));
    say.addEventListener("click", () => voice.speak(`${item.c.term}. ${item.c.definition}`));
    const ok = el("button", "btn btn--primary btn--big", "Запомнил");
    ok.style.flex = "1";
    ok.addEventListener("click", () => {
      item.stage = 1;
      next();
    });
    dock.replaceChildren(say, ok);
  }

  /** Шаг 2: вспомнить пропуски — коснуться, чтобы проверить себя. */
  function gaps(item) {
    const box = el("div", "plate plate--active drill");
    const text = el("span", "drill__def");
    for (const part of blanks(item.c.definition)) {
      if (!part.hidden) {
        text.append(part.word);
        continue;
      }
      const gap = el("button", "drill__gap", "·".repeat(Math.min(10, Math.max(4, part.word.length))));
      gap.addEventListener("click", () => {
        gap.textContent = part.word;
        gap.classList.add("is-open");
      });
      text.append(part.before, gap, part.after);
    }
    box.append(head(item, 2, "вспомни пропуски"), text);
    content.replaceChildren(box, el("span", "small dim", "Сначала проговорите пропущенное про себя, потом коснитесь пропуска и сверьтесь."));
    const miss = el("button", "btn btn--secondary", "Не вспомнил");
    miss.style.cssText = "flex:1;height:52px";
    miss.addEventListener("click", () => {
      item.stage = 0;
      item.misses += 1;
      next();
    });
    const ok = el("button", "btn btn--primary", "Вспомнил всё");
    ok.style.cssText = "flex:1.3;height:52px";
    ok.addEventListener("click", () => {
      item.stage = 2;
      next();
    });
    dock.replaceChildren(miss, ok);
  }

  /** Шаг 3: рассказать по памяти — голосом (Ноа сверит) или про себя. */
  function recall(item) {
    const box = el("div", "plate plate--active drill");
    box.append(head(item, 3, item.streak ? "ещё раз, по памяти" : "по памяти"), el("span", "muted", "Расскажите определение целиком — вслух или про себя."));
    const heard = el("div", "say say--me");
    heard.hidden = true;
    const verdict = el("div", "explain");
    verdict.hidden = true;
    content.replaceChildren(box, heard, verdict);

    const grade = (result) => {
      if (result === "solid") {
        item.streak += 1;
        if (item.streak >= SOLID) {
          item.done = true;
          toast(`«${item.c.term}» — наизусть`);
        }
      } else if (result === "almost") {
        item.streak = 0;
        item.misses += 1;
      } else {
        item.streak = 0;
        item.misses += 1;
        item.stage = 1;
      }
      next();
    };
    const grades = () => {
      const row = el("div", "drill__grades");
      for (const [result, text, cls] of [
        ["miss", "Не вспомнил", "grade--again"],
        ["almost", "Почти", "grade--hard"],
        ["solid", "Наизусть", "grade--good"],
      ]) {
        const btn = el("button", `grade ${cls}`, text);
        btn.addEventListener("click", () => grade(result));
        row.append(btn);
      }
      dock.replaceChildren(row);
    };
    const reveal = () => {
      const answer = el("div", "plate drill");
      answer.append(label("// эталон"), el("span", "drill__def", item.c.definition));
      content.append(answer);
      grades();
    };

    const mic = el("button", "btn btn--secondary");
    mic.style.cssText = "flex:1;height:52px";
    mic.append(icon("mic", 18, 2.25), "Сказать");
    mic.addEventListener("click", async () => {
      voice.stop();
      mic.disabled = true;
      mic.lastChild.textContent = "Слушаю…";
      let said = "";
      try {
        said = await voice.listen({ pause: 2.2 });
      } catch (err) {
        toast(String(err));
      }
      mic.disabled = false;
      mic.lastChild.textContent = "Сказать";
      if (!said || !alive) return;
      heard.hidden = false;
      heard.textContent = said;
      verdict.hidden = false;
      verdict.replaceChildren(loading("Ноа сверяет с эталоном…"));
      dock.replaceChildren();
      try {
        const result = await checkRecall(item.c.term, item.c.definition, said);
        if (!alive) return;
        const score = Math.max(0, Math.min(100, Number(result.score) || 0));
        const who = el("span", "explain__who", `Совпало на ${score}%`);
        verdict.replaceChildren(who);
        if (result.say) verdict.append(el("span", "", result.say));
        const missing = (result.missing ?? []).filter(Boolean);
        if (missing.length) verdict.append(el("span", "small", `Не хватило: ${missing.join("; ")}`));
        if (result.say && voice.aloud()) voice.speak(result.say);
        reveal();
      } catch (err) {
        verdict.replaceChildren(el("span", "error", String(err)));
        reveal();
      }
    });
    const show = el("button", "btn btn--primary", "Показать ответ");
    show.style.cssText = "flex:1.3;height:52px";
    show.addEventListener("click", () => {
      dock.replaceChildren();
      reveal();
    });
    dock.replaceChildren(mic, show);
  }

  /** Итог порции — в интервальное повторение: без ошибок — «легко». */
  async function finish() {
    voice.stop();
    paintProgress();
    content.replaceChildren(loading("Записываю в повторение…"));
    dock.replaceChildren();
    for (const item of items) {
      const level = item.misses === 0 ? "easy" : item.misses <= 2 ? "good" : "hard";
      await call("learn_grade", { course: courseId, card: `${topicId}/c-${item.c.id}`, grade: level }).catch(() => {});
    }
    await store.courses(true).catch(() => {});
    // Тема пройдена «наизусть» — дальше её место в теме — проверка.
    const drilled = new Set(prefs.get("drilled", []));
    drilled.add(`${courseId}/${topicId}`);
    prefs.set("drilled", [...drilled]);
    if (!alive) return;
    const box = el("div", "plate plate--active");
    box.append(label("// порция выучена"), el("span", "continue__title", `${plural(items.length, "понятие", "понятия", "понятий")} наизусть`));
    const clean = items.filter((item) => item.misses === 0).length;
    box.append(
      el(
        "span",
        "continue__lead",
        `С первого раза — ${clean} из ${items.length}. Ноа вернёт их в повторение через день, потом реже: так выученное держится неделями, а не до вечера.`,
      ),
    );
    content.replaceChildren(box);
    const more = el("button", "btn btn--secondary", "Ещё порция");
    more.style.cssText = "flex:1;height:52px";
    more.addEventListener("click", () => loadPortion());
    const interview = el("button", "btn btn--primary");
    interview.style.cssText = "flex:1.3;height:52px";
    interview.append(icon("user", 18, 2.25), "Собеседование");
    interview.addEventListener("click", () => nav.replace("interview", { course: courseId, topic: topicId }));
    dock.replaceChildren(more, interview);
  }

  loadPortion();
  return {
    full: true,
    cleanup: () => {
      alive = false;
      voice.stop();
      voice.cancel();
    },
  };
});
