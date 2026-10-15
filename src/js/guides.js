// Путеводители — второй вид материала в обучении. Курс учит (урок, задачи,
// экзамен), путеводитель рассказывает: «Деревья Москвы», «Как построить дом
// самому», «Всё про умный дом». Обложка, оглавление, разделы с картинками,
// виды карточками, цифры, советы и «а вы знали». Листают его как журнал.
//
// Пишет путеводитель Rust (guides.rs) раздел за разделом; окно слышит
// `guides:changed` и дорисовывает готовое. Картинки лежат рядом с текстом и
// приходят строкой data: — окно само не ходит ни в сеть, ни на диск.

/** Картинки — по одной на файл, один раз за жизнь окна. */
const pictures = new Map();

export function pictureOf(api, id, file) {
  const key = `${id}/${file}`;
  if (!pictures.has(key)) {
    pictures.set(
      key,
      api.invoke("guide_picture", { id, file }).catch(() => {
        pictures.delete(key);
        return null;
      }),
    );
  }
  return pictures.get(key);
}

/** Картинки грузятся, когда до них долистали, а не все разом. */
const watcher =
  "IntersectionObserver" in window
    ? new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            if (!entry.isIntersecting) continue;
            watcher.unobserve(entry.target);
            entry.target.__load?.();
          }
        },
        { rootMargin: "400px" },
      )
    : null;

/** `<img>` для картинки путеводителя — с ленивой загрузкой. */
export function lazyImage(api, id, picture, className) {
  const img = document.createElement("img");
  img.className = className;
  img.alt = picture.caption || "";
  img.decoding = "async";
  // Показывается, только когда загрузилась: битая картинка не закрывает обложку.
  img.addEventListener("load", () => img.classList.add("is-loaded"));
  img.addEventListener("error", () => img.remove());
  img.__load = async () => {
    const src = await pictureOf(api, id, picture.file);
    if (src) img.src = src;
    else img.remove();
  };
  if (watcher) watcher.observe(img);
  else img.__load();
  return img;
}

/** Цвет обложки без картинки — из названия, чтобы каждый был узнаваем. */
export function tint(text) {
  let hash = 0;
  for (const ch of text) hash = (hash * 31 + ch.codePointAt(0)) >>> 0;
  return hash % 360;
}

/** Обложка путеводителя в лобби. */
export function guideCard(card, { api, el, onOpen }) {
  const node = el("button", "shelf__card shelf__card--guide");
  node.style.setProperty("--hue", tint(card.title));
  const cover = el("span", "shelf__cover");
  if (card.cover) cover.append(lazyImage(api, card.id, card.cover, "shelf__img"));
  cover.append(el("span", "shelf__emoji", card.emoji || "📖"));
  const body = el("span", "shelf__body");
  body.append(el("span", "shelf__title", card.title));
  if (card.subtitle) body.append(el("span", "shelf__about", card.subtitle));
  const meta =
    card.status === "building"
      ? `пишется: ${card.written} из ${card.sections || "…"}`
      : card.status === "failed"
        ? "не сложился — откройте, чтобы повторить"
        : card.status === "stopped"
          ? `готово ${card.written} из ${card.sections} разделов`
          : `${card.sections} ${plural(card.sections, "раздел", "раздела", "разделов")}`;
  body.append(el("span", `shelf__meta${card.status === "building" ? " is-live" : ""}`, meta));
  node.append(cover, body);
  node.addEventListener("click", () => onOpen(card.id));
  return node;
}

const IDEAS = ["Деревья Москвы", "Как построить дом самому", "Всё про умный дом", "Грибы средней полосы", "Кофе: от зерна до чашки"];

/** «Новый путеводитель»: тема словами и идеи, с чего начать. */
export function guideBuilder({ api, el, button, onStarted }) {
  const box = el("section", "maker");
  box.append(el("h3", "maker__title", "＋ Новый путеводитель"));
  box.append(
    el("p", "maker__lead", "Напишите тему — Ноа соберёт путеводитель с разделами, видами, цифрами, советами и картинками из Википедии. Без задач и экзаменов — просто интересно почитать."),
  );
  const row = el("div", "maker__row");
  const input = el("input", "maker__input");
  input.type = "text";
  input.placeholder = "О чём путеводитель — например, «деревья Москвы»";
  const status = el("p", "maker__status");
  const go = button("Собрать", async () => {
    const goal = input.value.trim();
    if (goal.length < 3) {
      status.textContent = "Напишите тему хотя бы парой слов.";
      input.focus();
      return;
    }
    go.disabled = true;
    status.textContent = "Начинаю: сначала план, потом раздел за разделом…";
    try {
      const id = await api.invoke("guide_build", { goal });
      input.value = "";
      status.textContent = "";
      onStarted(id);
    } catch (err) {
      status.textContent = String(err);
    }
    go.disabled = false;
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") go.click();
  });
  row.append(input, go);
  const ideas = el("div", "maker__ideas");
  for (const idea of IDEAS) {
    const chip = el("button", "chip-link", idea);
    chip.addEventListener("click", () => {
      input.value = idea;
      input.focus();
    });
    ideas.append(chip);
  }
  box.append(row, ideas, status);
  return box;
}

function plural(n, one, few, many) {
  const tens = n % 100;
  const last = n % 10;
  if (tens >= 11 && tens <= 14) return many;
  if (last === 1) return one;
  if (last >= 2 && last <= 4) return few;
  return many;
}

/**
 * Страница путеводителя. Отдаёт статью — окно ставит её в main и при
 * `guides:changed` перерисовывает, сохранив место прокрутки.
 */
export function guidePage(guide, { api, el, button, inline, attachReader, onBack, onDeleted, onChanged }) {
  const page = el("article", "guide");
  page.style.setProperty("--hue", tint(guide.title));

  // Обложка: картинка во всю ширину, поверх — название.
  const cover = el("header", `guide__cover${guide.cover ? "" : " guide__cover--plain"}`);
  if (guide.cover) cover.append(lazyImage(api, guide.id, guide.cover, "guide__cover-img"));
  const over = el("div", "guide__over");
  const back = el("button", "guide__back", "← Обучение");
  back.addEventListener("click", onBack);
  over.append(back, el("span", "guide__emoji", guide.emoji || "📖"), el("h2", "guide__title", guide.title));
  if (guide.subtitle) over.append(el("p", "guide__subtitle", guide.subtitle));
  cover.append(over);
  page.append(cover);

  const written = guide.sections.filter((s) => s.text.length || s.items.length);
  const body = el("div", "guide__body");
  if (guide.intro) {
    const intro = el("p", "guide__intro");
    intro.innerHTML = inline(guide.intro);
    body.append(intro);
  }

  if (guide.status === "building") {
    body.append(el("p", "guide__note is-live", guide.sections.length ? `Пишу путеводитель: готово ${written.length} из ${guide.sections.length} разделов. Читать можно уже сейчас — новые разделы появятся сами.` : "Составляю план путеводителя…"));
  } else if (guide.status === "failed") {
    body.append(el("p", "guide__note is-bad", guide.message || "Путеводитель не сложился."));
  } else if (guide.status === "stopped" && written.length < guide.sections.length) {
    body.append(el("p", "guide__note", `${guide.message || "Сборка прервалась."} Готово ${written.length} из ${guide.sections.length} разделов.`));
  }

  // Оглавление — значки и названия, щелчок ведёт к разделу.
  if (guide.sections.length) {
    const toc = el("nav", "guide__toc");
    guide.sections.forEach((section) => {
      const ready = section.text.length || section.items.length;
      const link = el("button", `guide__toc-item${ready ? "" : " is-pending"}`);
      link.append(el("span", "", section.emoji || "•"), el("span", "", section.title));
      link.addEventListener("click", () => page.querySelector(`#guide-${section.id}`)?.scrollIntoView({ behavior: "smooth", block: "start" }));
      toc.append(link);
    });
    body.append(toc);
  }

  // Читать вслух — те же абзацы, что глазами.
  const text = el("div", "guide__text");
  if (written.length) {
    const reader = attachReader(text, { api, el, button, label: "путеводитель", keys: false });
    body.append(reader);
  }

  for (const section of guide.sections) text.append(sectionNode(guide, section, { api, el, inline }));
  body.append(text);
  body.append(footer(guide, { api, el, button, onDeleted, onChanged }));
  page.append(body);
  return page;
}

function sectionNode(guide, section, { api, el, inline }) {
  const node = el("section", "gsec");
  node.id = `guide-${section.id}`;
  const head = el("div", "gsec__head");
  head.append(el("span", "gsec__emoji", section.emoji || "•"), el("h2", "gsec__title", section.title));
  node.append(head);

  if (!section.text.length && !section.items.length) {
    node.classList.add("is-pending");
    node.append(el("p", "gsec__about", section.about || ""));
    const skeleton = el("div", "gsec__skeleton");
    for (let i = 0; i < 3; i++) skeleton.append(el("span"));
    node.append(skeleton);
    return node;
  }

  if (section.lead) node.append(el("p", "gsec__lead", section.lead));
  if (section.picture) node.append(figure(guide, section.picture, "gsec__figure", { api, el }));
  for (const paragraph of section.text) {
    const p = el("p", "gsec__p");
    p.innerHTML = inline(paragraph);
    node.append(p);
  }

  if (section.facts.length) {
    const facts = el("dl", "facts");
    for (const fact of section.facts) {
      const cell = el("div", "facts__cell");
      cell.append(el("dt", "facts__label", fact.label), el("dd", "facts__value", fact.value));
      facts.append(cell);
    }
    node.append(facts);
  }

  if (section.items.length) {
    const grid = el("div", "kinds");
    for (const item of section.items) {
      const card = el("div", "kind");
      if (item.picture) card.append(lazyImage(api, guide.id, item.picture, "kind__img"));
      else card.append(el("span", "kind__placeholder", section.emoji || "•"));
      const info = el("div", "kind__info");
      info.append(el("h3", "kind__name", item.name));
      if (item.tags.length) {
        const tags = el("div", "kind__tags");
        for (const tag of item.tags) tags.append(el("span", "kind__tag", tag));
        info.append(tags);
      }
      if (item.text) {
        const p = el("p", "kind__text");
        p.innerHTML = inline(item.text);
        info.append(p);
      }
      card.append(info);
      grid.append(card);
    }
    node.append(grid);
  }

  if (section.wow) {
    const wow = el("aside", "wow");
    wow.append(el("span", "wow__label", "💡 А вы знали?"));
    const p = el("p", "wow__text");
    p.innerHTML = inline(section.wow);
    wow.append(p);
    node.append(wow);
  }

  if (section.tips.length) {
    const tips = el("div", "tips");
    tips.append(el("span", "tips__label", "Советы"));
    const list = el("ul", "tips__list");
    for (const tip of section.tips) {
      const li = el("li");
      li.innerHTML = inline(tip);
      list.append(li);
    }
    tips.append(list);
    node.append(tips);
  }
  return node;
}

function figure(guide, picture, className, { api, el }) {
  const box = el("figure", className);
  box.append(lazyImage(api, guide.id, picture, `${className}-img`));
  if (picture.caption) {
    const caption = el("figcaption", "", "");
    caption.append(document.createTextNode(`${picture.caption} · `));
    const source = el("button", "figure__source", "Википедия");
    source.title = "Открыть статью, откуда картинка";
    source.addEventListener("click", () => api.invoke("guide_source", { url: picture.source }).catch(() => {}));
    if (picture.source) caption.append(source);
    else caption.append(document.createTextNode("Википедия"));
    box.append(caption);
  }
  return box;
}

/** Низ путеводителя: дописать, добавить раздел, остановить, удалить. */
function footer(guide, { api, el, button, onDeleted, onChanged }) {
  const box = el("footer", "guide__foot");
  const status = el("p", "maker__status");
  const act = (fn) => async () => {
    status.textContent = "";
    try {
      await fn();
      onChanged();
    } catch (err) {
      status.textContent = String(err);
    }
  };

  if (guide.status === "building") {
    box.append(button("■ Остановить сборку", act(() => api.invoke("guide_stop")), true));
  } else {
    const pending = guide.sections.some((s) => !s.text.length && !s.items.length);
    if (pending || guide.status === "failed") {
      box.append(button(guide.sections.length ? "↻ Дописать путеводитель" : "↻ Собрать ещё раз", act(() => api.invoke("guide_resume", { id: guide.id, wish: null }))));
    }
    const more = el("div", "maker__row");
    const wish = el("input", "maker__input");
    wish.type = "text";
    wish.placeholder = "Ещё раздел — о чём? Например, «вредители и болезни»";
    const add = button("＋ Добавить раздел", act(async () => {
      if (wish.value.trim().length < 3) throw new Error("Напишите, о чём раздел.");
      await api.invoke("guide_resume", { id: guide.id, wish: wish.value.trim() });
    }), true);
    wish.addEventListener("keydown", (event) => {
      if (event.key === "Enter") add.click();
    });
    more.append(wish, add);
    box.append(more);
  }

  const remove = button("Удалить путеводитель", async () => {
    if (!remove.dataset.sure) {
      remove.dataset.sure = "1";
      remove.textContent = "Точно удалить? Нажмите ещё раз";
      setTimeout(() => {
        delete remove.dataset.sure;
        remove.textContent = "Удалить путеводитель";
      }, 4000);
      return;
    }
    try {
      await api.invoke("guide_delete", { id: guide.id });
      onDeleted();
    } catch (err) {
      status.textContent = String(err);
    }
  }, true);
  remove.classList.add("guide__remove");
  box.append(remove, status);
  return box;
}
