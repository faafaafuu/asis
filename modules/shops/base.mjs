// Встроенная база модуля: один файл рядом с ним.
//
// Прежняя служба заказов держала PostgreSQL и Redis — для списка покупок на
// десяток строк. Ставить ради него две базы значит требовать от человека
// Docker и полчаса настройки перед первой покупкой молока, поэтому здесь
// обычный файл: он переживает перезапуск, читается глазами и чинится руками.
//
// Запись идёт через временный файл с переименованием: переименование внутри
// одной папки атомарно, и выключение света посреди записи оставит прежний
// список целым, а не половину нового.

import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const FILE = join(HERE, "shops.json");
const EMPTY = { cart: [], store: "", updated: "" };

function read() {
  try {
    const saved = JSON.parse(readFileSync(FILE, "utf8"));
    return {
      cart: Array.isArray(saved.cart) ? saved.cart : [],
      store: typeof saved.store === "string" ? saved.store : "",
      updated: typeof saved.updated === "string" ? saved.updated : "",
    };
  } catch {
    // Файла ещё нет или он испорчен. Пустой список — рабочее состояние, а не
    // ошибка: модуль должен отвечать с первой секунды.
    return { ...EMPTY };
  }
}

function write(state) {
  const next = { ...state, updated: new Date().toISOString() };
  mkdirSync(HERE, { recursive: true });
  const temporary = `${FILE}.tmp`;
  writeFileSync(temporary, JSON.stringify(next, null, 1), "utf8");
  renameSync(temporary, FILE);
  return next;
}

/** Что сейчас в списке. */
export function cart() {
  return read().cart;
}

/** В каком магазине собираем. Пусто — ещё не выбрали. */
export function store() {
  return read().store;
}

export function chooseStore(name) {
  const state = read();
  write({ ...state, store: String(name || "") });
}

/**
 * Кладёт товар в список.
 *
 * Тот же товар второй раз не двоится, а прибавляет количество: «добавь молоко»,
 * сказанное дважды, — это два молока в одной строке, а не две строки.
 */
export function add(item, amount = 1) {
  const state = read();
  const same = state.cart.find((line) => line.store === item.store && line.id === item.id);
  if (same) same.amount += amount;
  else state.cart.push({ ...item, amount });
  write(state);
  return state.cart;
}

/** Убирает строку по номеру, считая с единицы. */
export function drop(number) {
  const state = read();
  const index = Number(number) - 1;
  if (!Number.isInteger(index) || index < 0 || index >= state.cart.length) {
    throw new Error(`В списке нет строки ${number}.`);
  }
  const [gone] = state.cart.splice(index, 1);
  write(state);
  return gone;
}

export function clear() {
  write({ ...EMPTY, store: read().store });
}

/** Сколько стоит список. Строки без цены считаются нулём и названы отдельно. */
export function total(lines = cart()) {
  return lines.reduce((sum, line) => sum + (line.price ?? 0) * line.amount, 0);
}
