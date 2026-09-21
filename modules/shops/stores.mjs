// Магазины: как спросить каждый и как разобрать ответ.
//
// Ни одного стороннего пакета: всё, что нужно, — fetch и разбор текста. Раньше
// этот же разбор жил в отдельной службе с базой и очередью; базы он никогда не
// требовал, и теперь живёт здесь.

const MAX_PRODUCTS = 12;
const ATTEMPTS = 2;
/** Сколько ждём одну попытку. */
const ATTEMPT_MS = 4000;
/** Сколько всего готовы потратить на один магазин. */
const BUDGET_MS = 8000;
/**
 * Сколько не тревожим магазин после отказа.
 *
 * Список покупок — это несколько поисков подряд, и лежачий магазин отвечал бы
 * отказом на каждый, каждый раз выбирая весь предел ожидания. За минуту он не
 * починится, зато человек не будет платить за него ожиданием на каждом товаре:
 * первый поиск ждёт восемь секунд, следующие — ни одной.
 */
const REST_MS = 60_000;
const AGENT = "Mozilla/5.0 (compatible; NOAH-shops/1.0)";

/**
 * Читает страницу магазина, повторяя попытку при обрыве связи.
 *
 * Повторы не перестраховка: на домашнем канале связь с магазином рвётся через
 * раз, причём неудачная попытка даже не устанавливается — не «медленно», а
 * «никак». Без повторов поиск проваливался бы чаще, чем работал, и человек
 * винил бы программу, а не сеть.
 *
 * Повторяется только чтение страницы поиска: запрос ничего не меняет, и лишний
 * поход стоит секунды. Осознанный отказ магазина не повторяется — второй раз он
 * скажет то же самое.
 */
async function page(url) {
  const deadline = Date.now() + BUDGET_MS;
  let last = null;

  for (let attempt = 1; attempt <= ATTEMPTS; attempt += 1) {
    try {
      const response = await fetch(String(url), {
        signal: AbortSignal.timeout(Math.max(1000, Math.min(ATTEMPT_MS, deadline - Date.now()))),
        headers: {
          Accept: "text/html,application/xhtml+xml",
          "Accept-Language": "ru-RU,ru;q=0.9,en;q=0.7",
          "User-Agent": AGENT,
        },
      });
      if (!response.ok) throw new Refusal(`магазин ответил ${response.status}`);
      return await response.text();
    } catch (error) {
      if (error instanceof Refusal) throw error;
      last = error;
      const pause = 700 * attempt;
      if (attempt < ATTEMPTS && Date.now() + pause < deadline) {
        await new Promise((done) => setTimeout(done, pause));
      } else break;
    }
  }
  throw new Refusal(`не достучались: ${last?.message ?? last}`);
}

/** Отказ магазина: повторять его незачем. */
export class Refusal extends Error {}

const text = (raw) =>
  entities(raw)
    .replace(/<[^>]*>/g, " ")
    .replace(/\s+/g, " ")
    .trim();

function entities(raw) {
  const named = { amp: "&", hellip: "…", laquo: "«", nbsp: " ", quot: '"', raquo: "»" };
  return String(raw)
    .replace(/&#(\d+);/g, (_, code) => String.fromCharCode(Number(code)))
    .replace(/&([a-z]+);/gi, (whole, name) => named[name] ?? whole);
}

const first = (input, pattern) => input.match(pattern)?.[1] ?? null;

function rubles(raw) {
  const number = Number(String(raw).replace(/\s+/g, "").replace(",", "."));
  return Number.isFinite(number) && number > 0 ? number : null;
}

/* ── ВкусВилл ─────────────────────────────────────────────────────────────── */

const VKUSVILL = "https://vkusvill.ru";

async function vkusvill(query) {
  const url = new URL("/search/", VKUSVILL);
  url.searchParams.set("q", query);
  url.searchParams.set("type", "products");
  const html = await page(url);

  const cards = html
    .split('<div class="ProductCards__item')
    .slice(1)
    .map((part) => `<div class="ProductCards__item${part}`);

  const products = cards
    .map((card) => {
      const id = first(card, /data-id="([^"]+)"/);
      const href = first(card, /class="[^"]*js-product-detail-link[^"]*"[^>]*href="([^"]+)"/);
      const name =
        first(card, /class="js-product-v-tizer__title-text"[^>]*>([\s\S]*?)<\/span>/) ??
        first(card, /class="ProductCard__imageImg"[^>]*alt="([^"]+)"/);
      if (!id || !href || !name) return null;

      const priceText =
        first(card, /class="js-datalayer-catalog-list-price hidden">([\s\S]*?)<\/span>/) ??
        first(card, /class="Price__value"[^>]*>([\s\S]*?)<\/span>/);

      return {
        store: "vkusvill",
        id,
        name: text(name),
        price: priceText ? rubles(text(priceText)) : null,
        url: new URL(entities(href), VKUSVILL).toString(),
        available: !card.includes("_restDisabled"),
      };
    })
    .filter(Boolean)
    .slice(0, MAX_PRODUCTS);

  return { store: "vkusvill", searchUrl: url.toString(), products };
}

/* ── Магнит ───────────────────────────────────────────────────────────────── */

const MAGNIT = "https://magnit.ru";

/**
 * Товары берутся не из разметки, а из данных, которые страница везёт с собой:
 * Nuxt кладёт их в `__NUXT_DATA__` готовым JSON. Это надёжнее разбора HTML —
 * вёрстка меняется от релиза к релизу, поля товара куда реже.
 *
 * Нагрузка — плоский массив, где объекты ссылаются на значения номерами ячеек.
 * Разворачивать её целиком не нужно и рискованно: в ней есть ссылки по кругу.
 */
async function magnit(query) {
  const url = new URL("/search/", MAGNIT);
  url.searchParams.set("term", query);
  const html = await page(url);

  const payload = html.match(/id="__NUXT_DATA__"[^>]*>([\s\S]*?)<\/script>/);
  if (!payload) return { store: "magnit", searchUrl: url.toString(), products: [] };

  let cells;
  try {
    cells = JSON.parse(payload[1]);
  } catch {
    return { store: "magnit", searchUrl: url.toString(), products: [] };
  }

  const at = (index) => (typeof index === "number" && index >= 0 ? cells[index] : undefined);
  const seen = new Set();
  const products = [];

  for (const cell of cells) {
    if (!cell || typeof cell !== "object" || Array.isArray(cell)) continue;

    const title = at(cell.title);
    const link = at(cell.link);
    if (typeof title !== "string" || typeof link !== "string" || !title || !link) continue;
    // В нагрузке лежат не только товары: так же устроены баннеры и категории.
    // Товар отличает адрес карточки.
    if (!link.startsWith("/product/") || seen.has(link)) continue;
    seen.add(link);

    const price = at(cell.price);
    const stock = at(cell.quantity);
    products.push({
      store: "magnit",
      id: String(at(cell.id) ?? link),
      name: title,
      price: typeof price === "string" ? rubles(price) : null,
      url: new URL(link, MAGNIT).toString(),
      available: typeof stock === "number" ? stock > 0 : true,
    });
    if (products.length >= MAX_PRODUCTS) break;
  }

  return { store: "magnit", searchUrl: url.toString(), products };
}

/* ── Метро ────────────────────────────────────────────────────────────────── */

const METRO = "https://online.metro-cc.ru";

/**
 * Метро кладёт состояние страницы в присваивание `window.__NUXT__=…`, и это не
 * JSON, а вызов функции. Прочитать его разбором текста нельзя: у объекта нет
 * цельного вида, он собирается только при выполнении.
 *
 * Выполнение чужого кода здесь осознанное и ограниченное: выражение считается в
 * отдельном контексте без единой глобальной переменной — ни `process`, ни
 * `fetch` внутри не видно, — и с жёстким пределом по времени, чтобы страница не
 * подвесила модуль бесконечным циклом. Так читается только страница поиска.
 */
async function metro(query) {
  const { runInNewContext } = await import("node:vm");
  const url = new URL("/search", METRO);
  url.searchParams.set("q", query);
  const html = await page(url);

  const payload = html.match(/window\.__NUXT__=([\s\S]*?);?<\/script>/);
  if (!payload) return { store: "metro", searchUrl: url.toString(), products: [] };

  let state;
  try {
    state = runInNewContext(`(${payload[1]})`, Object.create(null), { timeout: 1000 });
  } catch {
    return { store: "metro", searchUrl: url.toString(), products: [] };
  }

  // Ключ, под которым лежит ответ поиска, содержит хеш сборки магазина и
  // меняется при каждом их релизе. Поэтому полка ищется по своему виду.
  let shelf = [];
  for (const chunk of Object.values(state?.fetch ?? {})) {
    if (Array.isArray(chunk?.productsData?.products)) {
      shelf = chunk.productsData.products;
      break;
    }
  }

  const products = shelf
    .slice(0, MAX_PRODUCTS)
    .map((raw) => {
      if (typeof raw?.name !== "string" || typeof raw?.url !== "string") return null;
      const stock = Array.isArray(raw.stocks) ? raw.stocks[0] : undefined;
      // Цена доставки, а не зала: у Метро это разные числа, платит человек за первое.
      const price = typeof stock?.prices?.price === "number" ? stock.prices.price : null;
      return {
        store: "metro",
        id: String(raw.id ?? raw.url),
        name: raw.name,
        price,
        url: new URL(raw.url, METRO).toString(),
        available: stock?.eshop_availability === true,
      };
    })
    .filter(Boolean);

  return { store: "metro", searchUrl: url.toString(), products };
}

/* ── Пятёрочка ────────────────────────────────────────────────────────────── */

const PARSE_BOT = "https://api.parse.bot/scraper/aae3e5f6-fa2a-444d-9fb9-c4bbdf7aced1";

/**
 * Сама Пятёрочка программе отвечает 403: и страницы, и API приложения закрыты
 * защитой от ботов, своего открытого API у X5 нет. parse.bot читает каталог сам
 * и отдаёт его по ключу — ключ принадлежит человеку и приходит с запросом.
 * Без ключа Пятёрочку просто не спрашиваем: отсутствие ключа не поломка.
 */
async function pyaterochka(query, key) {
  const url = new URL(`${PARSE_BOT}/get_products`);
  url.searchParams.set("query", query);
  url.searchParams.set("limit", String(MAX_PRODUCTS));

  let response;
  try {
    response = await fetch(url.toString(), {
      headers: { "X-API-Key": key, Accept: "application/json" },
      signal: AbortSignal.timeout(BUDGET_MS),
    });
  } catch (error) {
    throw new Refusal(`parse.bot недоступен: ${error?.message ?? error}`);
  }
  if (response.status === 401 || response.status === 403) throw new Refusal("parse.bot не принял ключ");
  if (response.status === 429 || response.status === 402) throw new Refusal("запросы parse.bot кончились");
  if (!response.ok) throw new Refusal(`parse.bot ответил ${response.status}`);

  const body = await response.json();
  const items = Array.isArray(body) ? body : (body?.results ?? body?.data?.results ?? []);

  const products = items
    .map((item) => {
      const name = typeof item?.name === "string" ? item.name.trim() : "";
      const id = item?.id ?? item?.plu;
      if (!name || (typeof id !== "string" && typeof id !== "number")) return null;
      return {
        store: "pyaterochka",
        id: String(id),
        name,
        price: priceOf(item.price ?? item.prices),
        url: `https://5ka.ru/product/${id}/`,
        available: item.is_available !== false && item.stock !== 0,
      };
    })
    .filter(Boolean)
    .slice(0, MAX_PRODUCTS);

  return { store: "pyaterochka", searchUrl: url.toString(), products };
}

/**
 * Цена в рублях из того, как её прислали: число, строка «89.99» или объект с
 * обычной и акционной ценой — берётся акционная. Целое от десяти тысяч считаем
 * копейками: продуктов по десять тысяч рублей в Пятёрочке нет.
 */
function priceOf(value) {
  if (value && typeof value === "object") {
    return priceOf(value.discount ?? value.promo ?? value.regular ?? value.price ?? null);
  }
  const number = typeof value === "string" ? Number(value.replace(",", ".")) : value;
  if (typeof number !== "number" || !Number.isFinite(number) || number <= 0) return null;
  return Number.isInteger(number) && number >= 10000 ? number / 100 : number;
}

/* ── Все магазины разом ───────────────────────────────────────────────────── */

export const STORE_NAMES = {
  vkusvill: "ВкусВилл",
  magnit: "Магнит",
  metro: "Метро",
  pyaterochka: "Пятёрочка",
};

/**
 * Спрашивает магазины разом и отдаёт их полки порознь.
 *
 * Порознь, а не общим списком: заказ собирается в одном магазине, и решать, в
 * каком, должен тот, кто знает про доставку. Отказ одного магазина не роняет
 * остальных — иначе сравнение пропадало бы из-за магазина, который человеку и
 * не нужен.
 */
const resting = new Map();

export async function searchEverywhere(query, { key = "", only = "" } = {}) {
  const wanted = String(only || "").trim().toLowerCase();
  const asks = [
    ["vkusvill", () => vkusvill(query)],
    ["magnit", () => magnit(query)],
    ["metro", () => metro(query)],
  ];
  if (key.trim()) asks.push(["pyaterochka", () => pyaterochka(query, key.trim())]);

  const chosen = wanted ? asks.filter(([store]) => store === wanted) : asks;
  if (!chosen.length) throw new Error(`Магазин «${only}» модулю неизвестен.`);

  return Promise.all(
    chosen.map(async ([store, ask]) => {
      const rest = resting.get(store);
      if (rest && Date.now() < rest.until) {
        return { store, searchUrl: "", products: [], reachable: false, note: rest.note };
      }
      try {
        const shelf = { ...(await ask()), reachable: true, note: "" };
        resting.delete(store);
        return shelf;
      } catch (error) {
        // Пустая полка и молчащий магазин выглядят одинаково — ни одного
        // товара, — но означают разное: в первом случае просят другое, во
        // втором пробуют позже.
        const note = String(error?.message ?? error);
        resting.set(store, { until: Date.now() + REST_MS, note });
        return { store, searchUrl: "", products: [], reachable: false, note };
      }
    }),
  );
}
