// Путеводитель на телефоне: та же страница, что в окне обучения на
// компьютере (guides.js), — обложка, оглавление, разделы с картинками, виды
// карточками. Пишется раздел за разделом; экран дорисовывается сам.
// Плюс полка на главном экране и лист «Собрать путеводитель».

import { el, icon, button, label, loading, header, nav, register, sheet, call, toast, api, inline, plural } from "../core.js";
import { guidePage, lazyImage, tint } from "../../guides.js";
import { pressMenu } from "../gestures.js";

/** Кнопка в стиле приложения: тихая — второстепенная. */
const btn = (text, onClick, quiet = false) => button(text, onClick, quiet ? "btn btn--secondary" : "btn btn--primary");

register("guide", (screen, { id }) => {
  const title = el("span");
  screen.append(header({ title: "Путеводитель" }));
  const content = el("div", "content content--guide");
  content.append(loading());
  screen.append(content);

  let alive = true;
  let guide = null;
  const draw = (top = false) => {
    const scroll = content.scrollTop;
    const page = guidePage(guide, {
      api,
      el: (tag, className = "", text = "") => el(tag, className, text),
      button: btn,
      inline,
      // Вслух на телефоне читает экран урока; здесь — только глазами.
      attachReader: () => document.createComment("reader"),
      onBack: () => nav.back(),
      onDeleted: () => {
        toast("Путеводитель удалён");
        nav.back();
      },
      onChanged: () => reload(),
    });
    content.replaceChildren(page);
    content.scrollTop = top ? 0 : scroll;
  };
  const reload = async () => {
    try {
      guide = await call("guide_get", { id });
    } catch (err) {
      if (!guide) content.replaceChildren(el("p", "muted", String(err)));
      return;
    }
    if (alive) draw(!title.dataset.drawn);
    title.dataset.drawn = "1";
  };
  reload();
  let unlisten = null;
  api?.listen("guides:changed", (event) => {
    if (event.payload === id) reload();
  }).then((stop) => {
    unlisten = stop;
    if (!alive) stop();
  });
  return {
    cleanup: () => {
      alive = false;
      unlisten?.();
    },
  };
});

/**
 * Полка «Путеводители» для главного экрана. Нет команд путеводителей (сайт) —
 * null, полки нет.
 */
export async function guideShelf() {
  let cards;
  try {
    cards = await call("guides_list");
  } catch {
    return null;
  }
  if (!Array.isArray(cards)) return null;
  const list = el("div", "list");
  list.append(label(`Путеводители${cards.length ? ` · ${cards.length}` : ""}`));
  const row = el("div", "guide-row");
  for (const card of cards) row.append(shelfCard(card));
  const add = el("button", "guide-tile guide-tile--add");
  add.append(icon("plus", 24, 2), el("span", "guide-tile__title", "Новый путеводитель"));
  add.addEventListener("click", () => buildGuide());
  row.append(add);
  list.append(row);
  if (!cards.length) {
    list.append(el("span", "small muted", "Деревья Москвы, умный дом, как построить дом — разделы, виды, цифры и картинки, без задач."));
  }
  return list;
}

function shelfCard(card) {
  const node = el("button", "guide-tile");
  node.style.setProperty("--hue", tint(card.title));
  const cover = el("span", "guide-tile__cover");
  if (card.cover) cover.append(lazyImage(api, card.id, card.cover, "guide-tile__img"));
  cover.append(el("span", "guide-tile__emoji", card.emoji || "📖"));
  node.append(cover, el("span", "guide-tile__title", card.title));
  const meta =
    card.status === "building"
      ? `пишется · ${card.written} из ${card.sections || "…"}`
      : plural(card.sections, "раздел", "раздела", "разделов");
  node.append(el("span", `guide-tile__meta${card.status === "building" ? " is-live" : ""}`, meta));
  node.addEventListener("click", () => nav.push("guide", { id: card.id }));
  pressMenu(node, () => [
    { text: "Открыть", icon: "chevron-right", run: () => nav.push("guide", { id: card.id }) },
    {
      text: "Удалить путеводитель",
      icon: "trash",
      danger: true,
      run: async () => {
        await call("guide_delete", { id: card.id }).catch((err) => toast(String(err)));
        nav.refresh();
      },
    },
  ]);
  return node;
}

const IDEAS = ["Деревья Москвы", "Как построить дом самому", "Всё про умный дом", "Грибы средней полосы"];

/** «Собрать путеводитель»: тема словами — дальше он пишется сам. */
export function buildGuide() {
  sheet("Новый путеводитель", (content, close) => {
    content.append(el("span", "muted", "Напишите тему — Ноа соберёт путеводитель с разделами, видами, цифрами, советами и картинками из Википедии. Без задач — просто интересно почитать."));
    const goal = el("input", "field");
    goal.type = "text";
    goal.placeholder = "Например: деревья Москвы";
    const ideas = el("div", "chips");
    for (const idea of IDEAS) {
      const chip = el("button", "chip", idea);
      chip.addEventListener("click", () => {
        goal.value = idea;
        goal.focus();
      });
      ideas.append(chip);
    }
    const error = el("span", "error");
    const go = button("Собрать путеводитель", async () => {
      const text = goal.value.trim();
      if (text.length < 3) {
        error.textContent = "Напишите тему хотя бы парой слов.";
        return;
      }
      go.disabled = true;
      error.textContent = "";
      try {
        const id = await call("guide_build", { goal: text });
        close();
        nav.push("guide", { id });
      } catch (err) {
        error.textContent = String(err);
        go.disabled = false;
      }
    }, "btn btn--primary btn--big");
    content.append(goal, ideas, error, go);
    goal.focus();
  });
}
