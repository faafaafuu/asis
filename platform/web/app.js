// NOAH — площадка модулей. Одна страница, маршруты в адресе после «#».

import { renderHome, stopHome } from "./home.js?v=43";

import { RELEASES, SITE, REPO, STANDARD_DOC } from "./links.js?v=43";
import { DOCS } from "./i18n.js?v=43";

// Шрифты — после первой отрисовки, чтобы не держать страницу (см. index.html).
{
  const fonts = document.querySelector('link[rel="preload"][as="style"]');
  if (fonts) {
    const sheet = document.createElement("link");
    sheet.rel = "stylesheet";
    sheet.href = fonts.href;
    document.head.append(sheet);
  }
}


import { $, ACCENTS, CATEGORY_ICON, EXAMPLE_MANIFEST, ICONS, NAV_ICON, api, codeBlock, copy, h, highlight, hooks, icon, iconFor, number, pageHead, paletteFor, pickLang, state, t, tile, toast, when } from "./core.js?v=43";

/** Страницы кабинета — отдельным файлом, по требованию. */
const account = () => import("./account.js?v=43");

// Страницам кабинета нужны route и renderChrome — функции объявлены ниже,
// но доступны с начала модуля.
hooks.route = route;
hooks.renderChrome = renderChrome;

function renderChrome(route) {
  const tr = t();
  document.documentElement.lang = state.lang;
  for (const node of document.querySelectorAll("[data-t]")) node.textContent = tr[node.dataset.t];
  $("search").placeholder = tr.searchPh;
  $("download").href = RELEASES;
  for (const button of document.querySelectorAll("[data-lang]")) {
    button.setAttribute("aria-pressed", String(button.dataset.lang === state.lang));
  }
  $("langBtn").textContent = `${state.lang.toUpperCase()} ▾`;
  $("signInIcon").setAttribute("aria-label", tr.signIn);
  $("signInIcon").title = tr.signIn;

  const count = state.stats ? String(state.stats.modules) : "";
  const moduleLink = state.lastModule ? `module/${state.lastModule}` : "library";
  const groups = [
    [tr.navDiscover, [["home", tr.navHome, "", "#F2C14E", ""], ["library", tr.navLibrary, count, "#2B5BC4"], ["module", tr.navModule, "", "#D4564A", moduleLink]]],
    [tr.navBuild, [["connect", tr.navConnect, "MCP", "#F2C14E"], ["studio", tr.navStudio, "", "#D4564A"], ["standard", tr.navStandard, "5", "#7FB069"], ["docs", tr.navDocsFull, "", "#2B5BC4", undefined, tr.navDocs]]],
    [tr.navAccount, [["seller", tr.navSeller, "$0", "#D4564A"]]],
  ];
  $("nav").replaceChildren(
    ...groups.map(([label, items]) =>
      h(
        "div",
        { class: "nav__group" },
        h("span", { class: "nav__label" }, label),
        items.map(([id, text, badge, dot, target, short]) =>
          h(
            "a",
            { class: "nav__item", "data-id": id, href: `#/${target ?? id}`, "aria-current": route === id ? "page" : null, vars: { "--dot": dot } },
            h("span", { class: "nav__dot" }),
            h("span", { class: "navicon" }, icon(NAV_ICON[id])),
            // Полное имя в сайдбаре; в нижнем меню телефона под тем же пунктом —
            // короткое, там для длинных слов нет места.
            h("span", { class: "nav__text" }, text),
            h("span", { class: "nav__text nav__text--short" }, short ?? text),
            h("span", { class: "nav__count" }, badge),
          ),
        ),
      ),
    ),
  );

  const user = state.user;
  $("guest").hidden = Boolean(user);
  $("meBtn").hidden = !user;
  $("sideMe").hidden = !user;
  $("sideGuest").hidden = Boolean(user);
  if (!user) $("menu").hidden = true;
  if (user) {
    for (const el of ["meAvatar", "sideAvatar"]) $(el).textContent = user.name.slice(0, 1);
    for (const el of ["meName", "sideName", "menuName"]) $(el).textContent = user.name;
    $("menuEmail").textContent = user.email;
    const hints = [String(state.myCount ?? ""), "", ""];
    const targets = ["#/seller", "#/seller/keys", "#/account"];
    $("menuItems").replaceChildren(
      ...tr.menu.map((label, at) => h("a", { class: "menu__item", href: targets[at] }, label, h("span", { class: "mono" }, hints[at]))),
    );
  }

  $("footCols").replaceChildren(
    ...tr.footer.map(([label, links]) =>
      h(
        "div",
        { class: "foot__col" },
        h("span", { class: "label" }, label),
        links.map(([text, href]) =>
          h("a", { href, target: href.startsWith("http") ? "_blank" : null, rel: href.startsWith("http") ? "noopener" : null }, text),
        ),
      ),
    ),
  );
}

/* ── Библиотека ──────────────────────────────────────────────────────────── */

function moduleCard(module) {
  const tr = t();
  const { accent } = paletteFor(module.id);
  return h(
    "a",
    { class: "card", href: `#/module/${module.id}`, vars: { "--accent": accent } },
    h(
      "div",
      { class: "card__head" },
      tile(module),
      h("span", { class: "card__name" }, h("span", { class: "card__title" }, module.title), h("span", { class: "card__author" }, module.builtin ? tr.builtIn : module.core ? tr.core : module.author)),
    ),
    h("span", { class: "card__about" }, module.about),
    h(
      "div",
      { class: "card__foot" },
      // «0 установок» у свежего модуля читается как поломка — пишем, что он новый.
      h("span", {}, module.builtin ? tr.builtIn : module.installs ? `${number(module.installs)} ${tr.installsWord}` : tr.fresh),
      h("strong", {}, `${module.builtin ? tr.open : tr.install} →`),
    ),
  );
}

async function loadLibrary() {
  const params = new URLSearchParams({ sort: state.sort, category: state.category, q: state.query });
  const [list, stats] = await Promise.all([api(`/api/modules?${params}`), api("/api/stats")]);
  state.modules = list.modules;
  state.categories = list.categories;
  state.stats = stats;
}

function renderLibrary(page) {
  const tr = t();
  const sortTabs = h(
    "div",
    { class: "tabs" },
    [["popular", tr.sortPopular], ["new", tr.sortNew], ["free", tr.sortFree]].map(([id, label]) =>
      h("button", { type: "button", "aria-pressed": String(state.sort === id), onclick: () => ((state.sort = id), route()) }, label),
    ),
  );
  const s = state.stats ?? { modules: 0, authors: 0, installs: 0, brains: 0 };
  const stats = h(
    "div",
    { class: "strip" },
    [
      [s.modules, tr.statModules, "#E9EEF9"],
      [s.authors, tr.statAuthors, "#FBF2DC"],
      [s.brains, tr.statBrains, "#F8E9E6"],
      [s.installs, tr.statInstalls, "#EAF0E6"],
    ].map(([value, label, bg]) =>
      h("div", { class: "stat", vars: { "--bg": bg } }, h("div", { class: "stat__value" }, number(value)), h("div", { class: "stat__label" }, label)),
    ),
  );
  const chips = h(
    "div",
    { class: "chips" },
    ["all", "free", "work", "home", "finance", "dev", "health", "local"].map((id) =>
      h("button", { type: "button", class: "chip", "aria-pressed": String(state.category === id), onclick: () => ((state.category = id), route()) }, tr.cat[id] ?? id),
    ),
  );
  const grid = state.modules.length
    ? h("div", { class: "grid" }, state.modules.map(moduleCard))
    : h("p", { class: "empty" }, tr.noModules);
  const add = h("a", { class: "addm", href: "#/studio", title: tr.newModule, "aria-label": tr.newModule }, "+");
  page.replaceChildren(pageHead(tr.libKicker, tr.libTitle, h("div", { class: "head__tools" }, sortTabs, add)), stats, chips, grid);
}

/* ── Страница модуля ─────────────────────────────────────────────────────── */

function renderModule(page, module) {
  const tr = t();
  const { accent } = paletteFor(module.id);
  const ask = state.lang === "ru" ? `Поставь модуль NOAH ${module.id}` : `Install the NOAH module ${module.id}`;
  const askButton = h("button", { type: "button", class: "btn", onclick: () => copy(ask, askButton, tr.copyAsk) }, tr.copyAsk);
  const howTo = h(
    "div",
    { class: "plate", hidden: true },
    h("div", { class: "step__head" }, tr.installHow),
    tr.installSteps(module.id, module.title).map(([title, body], at) =>
      h("div", { class: "step__row" }, h("span", { class: "num", vars: { "--accent": "#F2C14E" } }, at + 1), h("div", {}, title, h("p", {}, body))),
    ),
    h("div", { class: "step__body" }, h("a", { class: "btn btn--gold", href: RELEASES }, tr.appTitle)),
  );

  const perms = [];
  if (module.secrets?.length) perms.push(tr.permKeys(module.secrets.map((s) => s.title).join(", ")));
  else perms.push(tr.permNone);
  perms.push(module.builtin ? tr.permBuiltin : tr.permLocal);

  page.replaceChildren(
    h("a", { class: "back", href: "#/library" }, `← ${tr.backLib}`),
    h(
      "div",
      { class: "plate plate--accent", vars: { "--accent": accent } },
      h(
        "div",
        { class: "plate__head" },
        tile(module, "tile--big"),
        h(
          "div",
          { class: "plate__name" },
          h("h1", {}, module.title),
          h("div", { class: "plate__meta" }, module.builtin ? tr.builtIn : `${module.core ? tr.core : module.author} · v${module.version} · MCP`),
        ),
        h(
          "div",
          { class: "plate__buy" },
          h("span", { class: "big-price" }, tr.free),
          module.builtin
            ? h("a", { class: "btn btn--gold", href: RELEASES }, tr.builtInBtn)
            : h("button", { type: "button", class: "btn btn--gold", onclick: () => (howTo.hidden = !howTo.hidden) }, tr.installBtn),
          module.builtin ? h("p", { class: "hint" }, tr.builtInLong) : askButton,
        ),
      ),
      h(
        "div",
        { class: "cols" },
        h(
          "div",
          { class: "col" },
          h("p", { class: "body" }, module.description || `${module.about}. ${module.voice}`),
          h(
            "div",
            {},
            h("span", { class: "label" }, tr.toolsExposed),
            module.tools?.length
              ? h("div", { class: "tools" }, module.tools.map((tool) => h("div", { class: "tool" }, h("code", {}, tool.name), h("span", {}, tool.about))))
              : h("p", { class: "hint" }, tr.noTools),
          ),
        ),
        h(
          "div",
          { class: "col col--flush" },
          [
            [tr.metaBrain, module.brain || tr.brainAny],
            [tr.metaTransport, module.builtin ? tr.builtInTransport : "MCP / stdio"],
            [tr.metaInstalls, number(module.installs)],
            [tr.metaUpdated, when(module.updated)],
          ].map(([k, v]) => h("div", { class: "kv" }, h("span", {}, k), h("span", {}, v))),
          h("div", { class: "note" }, h("span", { class: "label" }, tr.permsLabel), perms.map((p) => h("p", {}, p))),
        ),
      ),
    ),
    howTo,
  );
}

/* ── Студия ──────────────────────────────────────────────────────────────── */

function renderStandard(page) {
  const tr = t();
  const tiles = ["#2B5BC4", "#F2C14E", "#D4564A", "#7FB069", "#2B5BC4"];
  page.replaceChildren(
    pageHead(tr.stdKicker, tr.stdTitle),
    h("p", { class: "lead" }, tr.stdBody),
    h(
      "div",
      { class: "plate doc" },
      tr.rules.map(([title, body], at) =>
        h(
          "div",
          { class: "rule" },
          h("span", { class: "num", vars: { "--accent": tiles[at] } }, at + 1),
          h("div", {}, h("strong", {}, title), h("p", {}, body)),
        ),
      ),
    ),
    h("div", { class: "doc" }, codeBlock("module.json", EXAMPLE_MANIFEST, { highlightJson: true })),
    h("a", { class: "back", href: STANDARD_DOC }, `${tr.stdFull} →`),
  );
}

/* ── Вход ────────────────────────────────────────────────────────────────── */

function renderDoc(page, name) {
  const [title, sections] = DOCS[name][state.lang];
  page.replaceChildren(
    pageHead("NOAH", title),
    h("div", { class: "plate form doc" }, sections.flatMap(([head, body]) => [h("h2", {}, head), h("p", {}, body)])),
  );
}

/* ── Маршруты ────────────────────────────────────────────────────────────── */

let routeId = 0;
/**
 * Куда вернуться после входа: страница подключения нейросети (OAuth MCP)
 * отправляет на вход с `next`. Хранится в sessionStorage, потому что вход через
 * Google или Telegram уходит со страницы и возвращается на другую.
 */
function resumeAfterLogin() {
  const query = new URLSearchParams(location.hash.split("?")[1] ?? "");
  const next = query.get("next");
  try {
    if (next && next.startsWith("/oauth/authorize?")) sessionStorage.setItem("noah_next", next);
    const saved = sessionStorage.getItem("noah_next");
    if (saved && state.user) {
      sessionStorage.removeItem("noah_next");
      location.href = saved;
      return true;
    }
  } catch {
    /* без sessionStorage вернуться нельзя — останемся на сайте */
  }
  return false;
}

async function route() {
  if (resumeAfterLogin()) return;
  const id = ++routeId;
  const [view, arg] = location.hash.replace(/^#\/?/, "").split("?")[0].split("/");
  const name = view || "home";
  document.body.classList.toggle("is-home", name === "home");
  if (name !== "home") stopHome();
  renderChrome(name);
  const page = $("page");
  try {
    if (name === "home") {
      // Главная рисуется сразу; числа и модули библиотеки подставляются, когда
      // придут. Первый заход не должен ждать ни одного запроса.
      const draw = () => renderHome(page, { h, icon, lang: state.lang, stats: state.stats, modules: state.modules ?? [], number });
      const had = Boolean(state.modules);
      if (!had) draw();
      await loadLibrary();
      if (id === routeId) {
        renderChrome("home");
        draw();
      }
    } else if (name === "library") {
      await loadLibrary();
      if (id === routeId) {
        renderChrome("library");
        renderLibrary(page);
      }
    } else if (name === "module" && arg) {
      const module = await api(`/api/modules/${encodeURIComponent(arg)}`);
      state.lastModule = module.id;
      if (id === routeId) {
        renderChrome("module");
        renderModule(page, module);
      }
    } else if (name === "studio") (await account()).renderStudio(page);
    else if (name === "seller") await (await account()).renderSeller(page, arg === "keys");
    else if (name === "account") await (await account()).renderAccount(page);
    else if (name === "connect") await (await account()).renderConnect(page);
    else if (name === "standard") renderStandard(page);
    // Документация грузится, только когда её открыли: главной она не нужна.
    else if (name === "docs") {
      const { renderDocs } = await import("./docs.js?v=43");
      await renderDocs(page, arg, { h, lang: state.lang });
    }
    else if (name === "login" || name === "signup") {
      if (state.user) location.hash = "#/seller";
      else (await account()).renderAuth(page, name);
    } else if (name === "privacy" || name === "terms") renderDoc(page, name);
    else location.hash = "#/";
  } catch (err) {
    if (id === routeId) page.replaceChildren(h("p", { class: "empty" }, err.message));
  }
  if (id === routeId && !location.hash.startsWith("#/library")) window.scrollTo(0, 0);
}

/* ── События ─────────────────────────────────────────────────────────────── */

for (const button of document.querySelectorAll("[data-lang]")) {
  button.addEventListener("click", () => {
    $("langList").hidden = true;
    $("langBtn").setAttribute("aria-expanded", "false");
    state.lang = button.dataset.lang;
    try {
      localStorage.setItem("noah.lang", state.lang);
    } catch {
      /* выбор не сохранится */
    }
    route();
  });
}

let searchTimer = 0;
$("search").addEventListener("input", (event) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    state.query = event.target.value.trim();
    if (!location.hash.startsWith("#/library")) location.hash = "#/library";
    else route();
  }, 220);
});

document.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    $("searchBox").classList.add("is-open");
    $("search").focus();
  }
});

$("langBtn").addEventListener("click", (event) => {
  event.stopPropagation();
  const list = $("langList");
  list.hidden = !list.hidden;
  $("langBtn").setAttribute("aria-expanded", String(!list.hidden));
});

$("searchBtn").addEventListener("click", () => {
  const box = $("searchBox");
  box.classList.toggle("is-open");
  if (box.classList.contains("is-open")) $("search").focus();
});

$("meBtn").addEventListener("click", (event) => {
  event.stopPropagation();
  $("menu").hidden = !$("menu").hidden;
});
document.addEventListener("click", (event) => {
  if (!$("menu").contains(event.target)) $("menu").hidden = true;
  if (!$("searchBox").contains(event.target) && !$("searchBtn").contains(event.target) && !$("search").value) $("searchBox").classList.remove("is-open");
  if (!$("langPick").contains(event.target)) {
    $("langList").hidden = true;
    $("langBtn").setAttribute("aria-expanded", "false");
  }
});
$("signOut").addEventListener("click", async () => {
  await api("/api/auth/logout", { method: "POST" }).catch(() => {});
  state.user = null;
  location.hash = "#/library";
  route();
});

window.addEventListener("hashchange", route);

(async () => {
  const ready = Promise.all([
    api("/api/me").then((r) => r.user).catch(() => null),
    api("/api/auth/providers").then((r) => r.providers).catch(() => []),
  ]);
  // Главной и документации аккаунт не нужен — они рисуются, не дожидаясь его.
  const view = location.hash.replace(/^#\/?/, "").split(/[/?]/)[0] || "home";
  if (view === "home" || view === "docs") route();
  [state.user, state.providers] = await ready;
  if (view === "home" || view === "docs") renderChrome(view);
  else route();
})();

