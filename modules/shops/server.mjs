// Магазины: поиск товаров и список покупок.
//
// Раньше это делала отдельная служба FoodPilot: NestJS, PostgreSQL, Redis и
// Docker — ради поиска, который целиком состоит из двух запросов в интернет и
// разбора ответа. Пока служба не запущена, Ноа на просьбу «закажи молока»
// отвечала «поиск по магазинам не отвечает», а запустить её значило поставить
// Docker. Здесь то же самое живёт модулем: ни базы снаружи, ни контейнеров.

import readline from "node:readline";
import * as base from "./base.mjs";
import { CLOSED_STORES, STORE_NAMES, searchEverywhere } from "./stores.mjs";

const send = (message) => process.stdout.write(`${JSON.stringify(message)}\n`);
const reply = (id, result) => send({ jsonrpc: "2.0", id, result });
const ok = (id, text) => reply(id, { content: [{ type: "text", text: cut(text) }] });
const bad = (id, text) => reply(id, { content: [{ type: "text", text: cut(text) }], isError: true });

/** Ноа читает ответ вслух, поэтому он короткий. */
const cut = (text) => (text.length > 3900 ? `${text.slice(0, 3900)}…` : text);

/**
 * Настройки поиска: чем открывать закрытые магазины и куда не ходить.
 *
 * Ключи приходят от Ноа переменными окружения — она хранит их зашифрованными и
 * не показывает никому, — а выключенные магазины лежат в базе модуля.
 */
const how = () => ({
  key: process.env.PARSE_BOT_KEY ?? "",
  lentaScraper: process.env.PARSE_BOT_LENTA ?? "",
  skip: base.disabled(),
});

const money = (rubles) => (rubles === null || rubles === undefined ? "цена не указана" : `${Math.round(rubles)} ₽`);
const nameOf = (store) => STORE_NAMES[store] ?? store;

const TOOLS = [
  {
    name: "search",
    description:
      "Ищет товар в магазинах (ВкусВилл, Магнит, Метро, с ключом — Пятёрочка) и показывает цены. Зовите, когда просят найти продукт, узнать цену или где дешевле.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Что искать, например «молоко 2.5»" },
        store: { type: "string", description: "Искать только в одном магазине: vkusvill, magnit, metro, pyaterochka" },
      },
      required: ["query"],
    },
  },
  {
    name: "cart_add",
    description: "Кладёт товар в список покупок: ищет его и берёт самый дешёвый подходящий. Зовите на «добавь в список», «закажи».",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Что добавить, например «молоко»" },
        store: { type: "string", description: "Магазин, если он назван" },
        amount: { type: "number", description: "Сколько штук, по умолчанию одна" },
      },
      required: ["query"],
    },
  },
  {
    name: "cart_show",
    description: "Показывает список покупок и его стоимость. Зовите на «что в списке», «сколько выходит».",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "cart_drop",
    description: "Убирает строку из списка покупок по её номеру.",
    inputSchema: {
      type: "object",
      properties: { number: { type: "number", description: "Номер строки, как в списке" } },
      required: ["number"],
    },
  },
  {
    name: "cart_clear",
    description: "Очищает список покупок целиком.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "stores",
    description: "Показывает, в каких магазинах идёт поиск и какие выключены. Зовите на «где ты ищешь», «какие магазины».",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "store_on",
    description: "Включает магазин в поиск: vkusvill, magnit, metro, pyaterochka, lenta. Зовите на «ищи и в Метро».",
    inputSchema: {
      type: "object",
      properties: { store: { type: "string", description: "Название магазина или его код" } },
      required: ["store"],
    },
  },
  {
    name: "store_off",
    description: "Убирает магазин из поиска. Зовите на «не ищи в Магните».",
    inputSchema: {
      type: "object",
      properties: { store: { type: "string", description: "Название магазина или его код" } },
      required: ["store"],
    },
  },
];

/** Код магазина по тому, как его назвали: «в метро», «Пятёрочка», «lenta». */
function codeOf(said) {
  const asked = String(said ?? "").trim().toLowerCase().replace(/ё/g, "е");
  if (!asked) return "";
  for (const [code, name] of Object.entries(STORE_NAMES)) {
    const plain = name.toLowerCase().replace(/ё/g, "е");
    if (asked === code || asked.includes(plain) || plain.includes(asked)) return code;
  }
  return "";
}

/* ── Инструменты ──────────────────────────────────────────────────────────── */

async function search({ query, store }) {
  const asked = String(query ?? "").trim();
  if (!asked) return "Не сказано, что искать.";

  const shelves = await searchEverywhere(asked, { ...how(), only: store });
  const lines = [];

  for (const shelf of shelves.sort(byCheapest)) {
    if (!shelf.reachable) {
      lines.push(`${nameOf(shelf.store)}: не ответил (${shelf.note}).`);
      continue;
    }
    const found = shelf.products.filter((item) => item.available);
    if (!found.length) {
      lines.push(`${nameOf(shelf.store)}: ничего не нашлось.`);
      continue;
    }
    const best = found.slice(0, 3).map((item) => `${item.name} — ${money(item.price)}`);
    lines.push(`${nameOf(shelf.store)}: ${best.join("; ")}.`);
  }

  return `«${asked}»\n${lines.join("\n")}`;
}

/** Сначала магазины, где нашлось и дешевле: человек слушает первую строку. */
function byCheapest(one, other) {
  const price = (shelf) => {
    const prices = shelf.products.filter((item) => item.available && item.price).map((item) => item.price);
    return prices.length ? Math.min(...prices) : Number.POSITIVE_INFINITY;
  };
  return price(one) - price(other);
}

async function cartAdd({ query, store, amount }) {
  const asked = String(query ?? "").trim();
  if (!asked) return "Не сказано, что добавить.";
  const count = Number.isFinite(Number(amount)) && Number(amount) > 0 ? Math.round(Number(amount)) : 1;

  // Список собирается в одном магазине: доставка одна, и половина списка в
  // другом магазине человеку не нужна. Первый добавленный товар и выбирает
  // магазин, дальше ищем только в нём.
  const chosen = store || base.store();
  const shelves = await searchEverywhere(asked, { ...how(), only: chosen });

  const offers = shelves
    .filter((shelf) => shelf.reachable)
    .flatMap((shelf) => shelf.products)
    .filter((item) => item.available && item.price);

  if (!offers.length) {
    const silent = shelves.filter((shelf) => !shelf.reachable).map((shelf) => nameOf(shelf.store));
    return silent.length
      ? `«${asked}» не нашлось; не ответили: ${silent.join(", ")}.`
      : `«${asked}» в магазинах не нашлось.`;
  }

  const best = offers.sort((one, other) => one.price - other.price)[0];
  if (!base.store()) base.chooseStore(best.store);
  const lines = base.add(best, count);

  return `В список: ${best.name} — ${money(best.price)} (${nameOf(best.store)}). Всего строк ${lines.length}, на ${money(base.total(lines))}.`;
}

function cartShow() {
  const lines = base.cart();
  if (!lines.length) return "Список покупок пуст.";
  const shown = lines
    .map((line, at) => `${at + 1}. ${line.name} — ${money(line.price)}${line.amount > 1 ? ` × ${line.amount}` : ""}`)
    .join("\n");
  return `${nameOf(base.store())}, на ${money(base.total(lines))}:\n${shown}`;
}

function cartDrop({ number }) {
  const gone = base.drop(number);
  return `Убрал: ${gone.name}. Осталось строк ${base.cart().length}.`;
}

function cartClear() {
  base.clear();
  return "Список покупок очищен.";
}

function stores() {
  const off = new Set(base.disabled());
  const key = Boolean(how().key.trim());
  const lines = Object.entries(STORE_NAMES).map(([code, name]) => {
    if (off.has(code)) return `${name} — выключен`;
    if (CLOSED_STORES.includes(code) && !key) return `${name} — нужен ключ parse.bot`;
    if (code === "lenta" && !how().lentaScraper.trim()) return `${name} — нужен номер сборщика parse.bot`;
    return `${name} — ищем`;
  });
  return ["Магазины:", ...lines].join("\n");
}

function storeOn({ store }) {
  const code = codeOf(store);
  if (!code) return `Магазин «${store}» модулю неизвестен.`;
  base.enable(code, true);
  return `${nameOf(code)} — снова в поиске.`;
}

function storeOff({ store }) {
  const code = codeOf(store);
  if (!code) return `Магазин «${store}» модулю неизвестен.`;
  base.enable(code, false);
  return `${nameOf(code)} больше не спрашиваю.`;
}

const RUN = {
  search,
  stores,
  store_on: storeOn,
  store_off: storeOff,
  cart_add: cartAdd,
  cart_show: cartShow,
  cart_drop: cartDrop,
  cart_clear: cartClear,
};

/* ── Протокол ─────────────────────────────────────────────────────────────── */

readline.createInterface({ input: process.stdin }).on("line", async (line) => {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    return;
  }
  // Уведомление: ответа не ждут.
  if (message.id === undefined) return;

  if (message.method === "initialize") {
    reply(message.id, {
      protocolVersion: "2025-06-18",
      capabilities: { tools: {} },
      serverInfo: { name: "shops", version: "1.0.0" },
    });
    return;
  }
  if (message.method === "tools/list") {
    reply(message.id, { tools: TOOLS });
    return;
  }
  if (message.method !== "tools/call") {
    send({ jsonrpc: "2.0", id: message.id, error: { code: -32601, message: "нет такого метода" } });
    return;
  }

  const name = message.params?.name;
  const run = RUN[name];
  if (!run) {
    bad(message.id, `Нет инструмента «${name}».`);
    return;
  }

  try {
    ok(message.id, await run(message.params?.arguments ?? {}));
  } catch (error) {
    // Ни одна ошибка не роняет сервер: модуль живёт всё время работы Ноа.
    bad(message.id, `Не вышло: ${error?.message ?? error}`);
  }
});
