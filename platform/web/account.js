// Страницы кабинета: вход, аккаунт, кабинет автора, подключение ИИ, студия.
// Грузятся, только когда их открыли, — главной они не нужны.

import { RELEASES, SITE, REPO, STANDARD_DOC } from "./links.js?v=37";
import { DOCS } from "./i18n.js?v=37";
import { $, ACCENTS, CATEGORY_ICON, EXAMPLE_MANIFEST, ICONS, NAV_ICON, api, codeBlock, copy, h, highlight, hooks, icon, iconFor, number, pageHead, paletteFor, pickLang, state, t, tile, toast, when } from "./core.js?v=37";

export function renderStudio(page) {
  const tr = t();
  const area = h("textarea", { rows: "4", placeholder: tr.studioPh });
  area.value = state.studioText;
  const ideas = h(
    "div",
    { class: "chips" },
    tr.ideas.map((idea) => h("button", { type: "button", class: "chip chip--quiet", onclick: () => ((area.value = state.studioText = idea), paint()) }, idea)),
  );
  const rows = h("div");
  let copiedOnce = false;
  const paint = () => {
    const described = area.value.trim().length > 0;
    const states = [
      described ? "done" : "waiting",
      copiedOnce ? "done" : described ? "running" : "waiting",
      copiedOnce ? "running" : "waiting",
      "waiting",
    ];
    const tiles = { done: "#7FB069", running: "#F2C14E", waiting: "#E7E4DD" };
    const colors = { done: "#4E7A3C", running: "#8A6614", waiting: "#6E6A61" };
    rows.replaceChildren(
      ...tr.build.map((label, at) =>
        h(
          "div",
          { class: "step__row step__row--center" },
          h("span", { class: "num", vars: { "--accent": tiles[states[at]] } }, at + 1),
          h("span", {}, label),
          h("span", { class: "state", vars: { "--state": colors[states[at]] } }, tr.states[states[at]]),
        ),
      ),
    );
  };
  area.addEventListener("input", () => {
    state.studioText = area.value;
    paint();
  });
  const assemble = h("button", {
    type: "button",
    class: "btn btn--gold btn--wide",
    onclick: async () => {
      const task = area.value.trim();
      if (!task) {
        toast(tr.promptEmpty);
        area.focus();
        return;
      }
      await copy(`${tr.promptIntro}\n${task}`, assemble, tr.assembleBtn);
      copiedOnce = true;
      paint();
      toast(tr.copied);
    },
  }, tr.assembleBtn);
  paint();

  page.replaceChildren(
    pageHead(tr.studioKicker, tr.studioTitle),
    h(
      "div",
      { class: "steps" },
      h("div", { class: "plate" }, h("div", { class: "step__head" }, `01 · ${tr.stepDescribe}`), h("div", { class: "step__body" }, area, ideas)),
      h(
        "div",
        { class: "plate" },
        h("div", { class: "step__head" }, `02 · ${tr.stepAssemble}`),
        rows,
        h(
          "div",
          { class: "step__body" },
          assemble,
          h("p", { class: "hint" }, tr.connectHint),
          codeBlock("terminal", 'claude mcp add noa -- "C:\\path\\to\\sufler.exe" --mcp'),
        ),
      ),
      codeBlock(`03 · ${tr.stepManifest}`, EXAMPLE_MANIFEST, { highlightJson: true }),
    ),
  );
}

/* ── Кабинет автора ──────────────────────────────────────────────────────── */

export async function renderSeller(page, focusKeys = false) {
  const tr = t();
  if (!state.user) {
    page.replaceChildren(
      pageHead(tr.sellerKicker, tr.sellerTitle),
      h("div", { class: "empty" }, tr.needLogin, " ", h("a", { href: "#/login" }, tr.signIn), " · ", h("a", { href: "#/signup" }, tr.signUp)),
    );
    return;
  }
  const { modules } = await api("/api/my/modules");
  state.myCount = modules.length;
  const installs = modules.reduce((sum, m) => sum + m.installs, 0);

  const stats = h(
    "div",
    { class: "strip" },
    [
      ["$0", tr.sellerStats[0], "#EAF0E6", "#3F6B30"],
      ["$0", tr.sellerStats[1], "#E9EEF9"],
      [installs, tr.sellerStats[2], "#FBF2DC"],
      [modules.length, tr.sellerStats[3], "#F8E9E6"],
    ].map(([value, label, bg, fg]) =>
      h("div", { class: "stat", vars: { "--bg": bg, "--fg": fg ?? "#171B26" } }, h("div", { class: "stat__value" }, typeof value === "number" ? number(value) : value), h("div", { class: "stat__label" }, label)),
    ),
  );

  const table = h(
    "div",
    { class: "plate" },
    h("div", { class: "table__row table__row--head" }, h("span", {}, tr.colModule), h("span", {}, tr.colPrice), h("span", {}, tr.colSales), h("span", {}, tr.colRevenue), h("span", {})),
    modules.length
      ? modules.map((m) =>
          h(
            "div",
            { class: "table__row" },
            h("a", { class: "table__name", href: `#/module/${m.id}` }, tile(m, "tile--small"), h("span", {}, m.title)),
            h("span", { class: "mono cell" }, h("span", { class: "cellk" }, tr.colPrice), tr.free),
            h("span", { class: "mono cell" }, h("span", { class: "cellk" }, tr.colSales), number(m.installs)),
            h("span", { class: "revenue cell" }, h("span", { class: "cellk" }, tr.colRevenue), "$0"),
            h("button", {
              type: "button",
              class: "btn btn--small btn--danger",
              onclick: async () => {
                if (!confirm(tr.removeConfirm)) return;
                await api(`/api/my/modules/${m.id}`, { method: "DELETE" }).catch((err) => toast(err.message));
                hooks.route();
              },
            }, tr.remove),
          ),
        )
      : h("div", { class: "step__body" }, h("p", { class: "hint" }, tr.noMine)),
  );

  const label = h("input", { type: "text", maxlength: "40", placeholder: tr.keyLabelPh });
  const fresh = h("div", { class: "secret-once", hidden: true });
  const keys = h(
    "div",
    { class: "plate", id: "keys" },
    h("div", { class: "step__head" }, tr.keysTitle),
    h(
      "div",
      { class: "step__body" },
      h("p", { class: "hint" }, tr.keysLead),
      h(
        "form",
        {
          class: "field",
          onsubmit: async (event) => {
            event.preventDefault();
            try {
              const { token } = await api("/api/tokens", { method: "POST", body: { label: label.value } });
              const copyButton = h("button", { type: "button", class: "btn btn--small", onclick: () => copy(token, copyButton, "COPY") }, "COPY");
              fresh.replaceChildren(h("span", { class: "hint" }, tr.keyOnce), h("code", {}, token), copyButton);
              fresh.hidden = false;
              label.value = "";
              list.replaceChildren(...(await keyRows()));
            } catch (err) {
              toast(err.message);
            }
          },
        },
        label,
        h("button", { type: "submit", class: "btn btn--gold" }, tr.createKey),
      ),
    ),
    fresh,
  );
  const keyRows = async () => {
    const { tokens: current } = await api("/api/tokens");
    if (!current.length) return [h("div", { class: "step__row" }, h("span", { class: "hint" }, tr.noKeys))];
    return current.map((key) =>
      h(
        "div",
        { class: "step__row" },
        h("div", { class: "grow" }, key.label, h("p", {}, `${when(key.created)} · ${key.used ? `${tr.used} ${when(key.used)}` : tr.never}`)),
        h("button", {
          type: "button",
          class: "btn btn--small btn--danger",
          onclick: async () => {
            await api(`/api/tokens/${key.id}`, { method: "DELETE" });
            list.replaceChildren(...(await keyRows()));
          },
        }, tr.remove),
      ),
    );
  };
  const list = h("div", {}, ...(await keyRows()));
  keys.append(list);

  const how = h(
    "div",
    { class: "plate" },
    h("div", { class: "step__head" }, tr.publishHow),
    tr.publishSteps.map(([title, body], at) => h("div", { class: "step__row" }, h("span", { class: "num", vars: { "--accent": "#7FB069" } }, at + 1), h("div", {}, title, h("p", {}, body)))),
  );

  const payout = h("button", { type: "button", class: "btn", disabled: true, title: tr.payoutSoon }, `${tr.payout} $0`);
  page.replaceChildren(
    pageHead(tr.sellerKicker, tr.sellerTitle, payout),
    h("p", { class: "lead" }, tr.payoutSoon),
    stats,
    table,
    h("div", { class: "steps" }, keys, how),
  );
  hooks.renderChrome("seller");
  if (focusKeys) keys.scrollIntoView({ behavior: "smooth", block: "start" });
}

/* ── Стандарт ────────────────────────────────────────────────────────────── */

export function renderAuth(page, mode) {
  const tr = t();
  const signup = mode === "signup";
  const error = h("p", { class: "error", hidden: true });
  const field = (label, input, hint) => h("label", { class: "field" }, h("span", { class: "label" }, label), input, hint ? h("p", { class: "hint" }, hint) : null);
  const email = h("input", { type: "email", autocomplete: "email", required: true });
  const name = h("input", { type: "text", autocomplete: "username", required: true, minlength: "3", maxlength: "24" });
  const password = h("input", { type: "password", autocomplete: signup ? "new-password" : "current-password", required: true, minlength: signup ? "10" : null });
  const submit = h("button", { type: "submit", class: "btn btn--gold btn--wide" }, signup ? tr.signUp : tr.signIn);

  const form = h(
    "form",
    {
      class: "form",
      onsubmit: async (event) => {
        event.preventDefault();
        submit.disabled = true;
        error.hidden = true;
        try {
          const body = signup ? { email: email.value, name: name.value, password: password.value } : { email: email.value, password: password.value };
          const { user } = await api(`/api/auth/${signup ? "register" : "login"}`, { method: "POST", body });
          state.user = user;
          location.hash = "#/seller";
        } catch (err) {
          error.textContent = err.message;
          error.hidden = false;
        } finally {
          submit.disabled = false;
        }
      },
    },
    field(tr.email, email),
    signup ? field(tr.authorName, name, tr.authorHint) : null,
    field(tr.password, password, signup ? tr.passHint : null),
    error,
    submit,
    h(
      "p",
      { class: "hint" },
      signup ? tr.haveAccount : tr.noAccount,
      " ",
      h("a", { href: signup ? "#/login" : "#/signup" }, signup ? tr.signIn : tr.signUp),
    ),
  );
  const params = new URLSearchParams(location.hash.split("?")[1] ?? "");
  if (params.get("error")) {
    error.textContent = params.get("error");
    error.hidden = false;
  }
  const providers = (state.providers ?? []).map((provider) => {
    if (provider.id === "telegram") {
      return h("button", { type: "button", class: `btn btn--wide oauth oauth--${provider.id}`, onclick: () => telegramLogin(error) }, h("span", { class: "oauth__mark" }, "✈"), tr.with(provider.title));
    }
    return h("a", { class: `btn btn--wide oauth oauth--${provider.id}`, href: `/auth/${provider.id}` }, h("span", { class: "oauth__mark" }, provider.title.slice(0, 1)), tr.with(provider.title));
  });
  const social = providers.length ? h("div", { class: "form oauth__list" }, providers, h("p", { class: "oauth__or mono" }, tr.orWith)) : null;
  page.replaceChildren(
    h(
      "div",
      { class: "auth" },
      h("div", { class: "plate plate--accent", vars: { "--accent": "#F2C14E" } }, h("div", { class: "step__head" }, signup ? tr.signupTitle : tr.loginTitle), social, form),
    ),
  );
  email.focus();
}

export async function renderAccount(page) {
  const tr = t();
  if (!state.user) {
    location.hash = "#/login";
    return;
  }
  const field = (label, input) => h("label", { class: "field" }, h("span", { class: "label" }, label), input);
  const note = () => h("p", { class: "hint", role: "status" });

  const current = h("input", { type: "password", autocomplete: "current-password", required: state.user.hasPassword !== false });
  const next = h("input", { type: "password", autocomplete: "new-password", required: true, minlength: "10" });
  const passNote = note();
  const pass = h(
    "form",
    {
      class: "plate",
      onsubmit: async (event) => {
        event.preventDefault();
        try {
          await api("/api/account/password", { method: "POST", body: { current: current.value, next: next.value } });
          current.value = next.value = "";
          passNote.textContent = tr.passDone;
          if (state.user.hasPassword === false) {
            state.user.hasPassword = true;
            hooks.route();
          }
        } catch (err) {
          passNote.textContent = err.message;
        }
      },
    },
    h("div", { class: "step__head" }, tr.passTitle),
    h("div", { class: "step__body" }, state.user.hasPassword === false ? null : field(tr.passCurrent, current), field(tr.passNew, next), h("p", { class: "hint" }, tr.passHint), h("button", { type: "submit", class: "btn btn--gold" }, tr.passSave), passNote),
  );

  const idNote = note();
  const idList = h("div", { class: "idlist" });
  const drawIdentities = (info) => {
    const linked = new Set(info.linked.map((row) => row.provider));
    const row = (title, on, action) =>
      h("div", { class: "idlist__row" }, h("strong", {}, title), h("span", { class: `mono idlist__state${on ? " is-on" : ""}` }, on ? tr.idOn : tr.idOff), action ?? h("span"));
    const rows = [row(tr.idPassword, info.hasPassword)];
    for (const provider of state.providers ?? []) {
      const on = linked.has(provider.id);
      const action = on
        ? h("button", {
            type: "button",
            class: "btn",
            onclick: async () => {
              try {
                drawIdentities(await api(`/api/my/identities/${provider.id}`, { method: "DELETE" }));
                idNote.textContent = "";
              } catch (err) {
                idNote.textContent = err.message;
              }
            },
          }, tr.idUnlink)
        : provider.id === "telegram"
          ? h("button", { type: "button", class: "btn", onclick: () => telegramLogin(idNote, "#/account") }, tr.idLink)
          : h("a", { class: "btn", href: `/auth/${provider.id}` }, tr.idLink);
      rows.push(row(provider.title, on, action));
    }
    idList.replaceChildren(...rows);
  };
  api("/api/my/identities").then(drawIdentities).catch((err) => (idNote.textContent = err.message));
  const identities = h(
    "div",
    { class: "plate" },
    h("div", { class: "step__head" }, tr.idTitle),
    h("div", { class: "step__body" }, h("p", { class: "hint" }, (state.providers ?? []).length ? tr.idLead : tr.idNone), idList, idNote),
  );

  const sessNote = note();
  const sessions = h(
    "div",
    { class: "plate" },
    h("div", { class: "step__head" }, tr.sessTitle),
    h(
      "div",
      { class: "step__body" },
      h("p", { class: "hint" }, tr.sessLead),
      h("button", {
        type: "button",
        class: "btn",
        onclick: async () => {
          try {
            await api("/api/account/logout-others", { method: "POST" });
            sessNote.textContent = tr.sessDone;
          } catch (err) {
            sessNote.textContent = err.message;
          }
        },
      }, tr.sessBtn),
      sessNote,
    ),
  );

  const delPass = h("input", { type: "password", autocomplete: "current-password", required: state.user.hasPassword !== false });
  const delNote = note();
  const remove = h(
    "form",
    {
      class: "plate plate--accent",
      vars: { "--accent": "#D4564A" },
      onsubmit: async (event) => {
        event.preventDefault();
        if (!confirm(tr.delConfirm)) return;
        try {
          await api("/api/account", { method: "DELETE", body: { password: delPass.value } });
          state.user = null;
          location.hash = "#/";
        } catch (err) {
          delNote.textContent = err.message;
        }
      },
    },
    h("div", { class: "step__head" }, tr.delTitle),
    h("div", { class: "step__body" }, h("p", { class: "hint" }, tr.delLead), state.user.hasPassword === false ? null : field(tr.password, delPass), h("button", { type: "submit", class: "btn btn--danger" }, tr.delBtn), delNote),
  );

  page.replaceChildren(
    pageHead(tr.accKicker, tr.accTitle),
    h("div", { class: "plate step__body" }, h("strong", {}, state.user.name), h("span", { class: "mono hint" }, state.user.email)),
    h("div", { class: "steps" }, identities, pass, sessions, remove),
  );
}

export async function renderConnect(page) {
  const tr = t();
  const link = (key) => `${SITE}/mcp?key=${key}`;
  const clients = h(
    "div",
    { class: "plate" },
    h("div", { class: "step__head" }, tr.connWhere),
    tr.connClients.map(([name, how], at) =>
      h("div", { class: "step__row" }, h("span", { class: "num", vars: { "--accent": ["#F2C14E", "#2B5BC4", "#5F8C4C", "#E7E4DD"][at] } }, at + 1), h("div", {}, name, h("p", {}, how))),
    ),
  );
  const prompts = h(
    "div",
    { class: "plate" },
    h("div", { class: "step__head" }, tr.connTry),
    h(
      "div",
      { class: "step__body" },
      tr.connPrompts.map((text) => {
        const button = h("button", { type: "button", class: "chip chip--quiet chip--wide", onclick: () => copy(text, button, text) }, `«${text}»`);
        return button;
      }),
    ),
  );
  const local = h(
    "div",
    { class: "plate" },
    h("div", { class: "step__head" }, tr.connLocal),
    h("div", { class: "step__body" }, h("p", { class: "hint" }, tr.connLocalLead), codeBlock("terminal", 'claude mcp add noa -- "C:\\path\\to\\sufler.exe" --mcp')),
  );

  let top;
  if (!state.user) {
    top = h(
      "div",
      { class: "plate plate--accent", vars: { "--accent": "#F2C14E" } },
      h("div", { class: "step__head" }, tr.connLinkTitle),
      h(
        "div",
        { class: "step__body" },
        h("p", { class: "hint" }, tr.connNeedLogin),
        h("div", { class: "hero__cta" }, h("a", { class: "btn btn--gold", href: "#/signup" }, tr.signUp), h("a", { class: "btn", href: "#/login" }, tr.signIn)),
      ),
    );
  } else {
    const [{ key }, device] = await Promise.all([api("/api/my/mcp"), api("/api/my/device")]);
    // Ссылка одна на аккаунт и видна всегда: вставили её в нейросеть однажды —
    // и больше не трогаете. Меняется только по кнопке, прежняя тогда перестаёт работать.
    const result = h("div", { class: "step__body" });
    const draw = (current) => {
      const url = link(current);
      const copyLink = h("button", { type: "button", class: "btn btn--small", onclick: () => copy(url, copyLink, tr.connCopy) }, tr.connCopy);
      const rotate = h("button", {
        type: "button",
        class: "btn btn--small",
        onclick: async () => {
          if (!confirm(tr.connRotateAsk)) return;
          rotate.disabled = true;
          try {
            draw((await api("/api/my/mcp/rotate", { method: "POST" })).key);
            toast(tr.connRotated);
          } catch (err) {
            toast(err.message);
            rotate.disabled = false;
          }
        },
      }, tr.connRotate);
      result.replaceChildren(
        h("div", { class: "secret-once" }, h("code", {}, url), copyLink),
        h("p", { class: "hint" }, tr.connLinkHint),
        h("div", { class: "hero__cta" }, rotate),
      );
    };
    draw(key);
    const status = device.online ? tr.connOnline : device.seen ? tr.connOffline : tr.connNever;
    top = h(
      "div",
      { class: "steps" },
      h(
        "div",
        { class: "plate plate--accent", vars: { "--accent": "#F2C14E" } },
        h("div", { class: "step__head" }, tr.connLinkTitle),
        result,
      ),
      h(
        "div",
        { class: "plate plate--accent", vars: { "--accent": device.online ? "#5F8C4C" : "#D4564A" } },
        h("div", { class: "step__head" }, tr.connStatus),
        h("div", { class: "step__body" }, h("p", { class: "hint" }, status)),
      ),
    );
  }

  page.replaceChildren(pageHead(tr.connKicker, tr.connTitle), h("p", { class: "lead" }, tr.connLead), top, h("div", { class: "steps" }, clients, prompts), local);
}

/** Вход через бота площадки: открыть бота и ждать подтверждения. */
export async function telegramLogin(error, back = "#/connect") {
  const tr = t();
  const tab = window.open("about:blank", "_blank");
  try {
    const { link } = await api("/api/auth/telegram/start", { method: "POST" });
    if (tab) tab.location.href = link;
    else location.href = link;
    toast(tr.tgWait);
    for (let i = 0; i < 100; i++) {
      await new Promise((resolve) => setTimeout(resolve, 3000));
      const status = await api("/api/auth/telegram/status");
      if (status.done) {
        state.user = (await api("/api/me")).user;
        if (location.hash === back) hooks.route();
        else location.hash = back;
        return;
      }
    }
  } catch (err) {
    tab?.close();
    error.textContent = err.message;
    error.hidden = false;
  }
}

