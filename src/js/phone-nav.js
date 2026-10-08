// Нижняя панель вкладок на телефоне — общая для всех экранов приложения.
//
// Экраны Ноа (обучение, практика, активы) сделаны окнами компьютера: свой
// заголовок с «свернуть» и «закрыть», и ничего, чтобы перейти к другому.
// На телефоне окно одно, и переходят в нём, как в любом приложении, —
// панелью внизу. Кнопки окна прячутся (phone.css), вместо них — вкладки.

const TABS = [
  { id: "noa", label: "Ноа", icon: "◉", href: "onboarding.html?tab=noa" },
  { id: "learning", label: "Обучение", icon: "◈", href: "learning.html" },
  { id: "practice", label: "Практика", icon: "▤", href: "practice.html" },
  { id: "watchlist", label: "Активы", icon: "◆", href: "watchlist.html" },
  { id: "more", label: "Ещё", icon: "☰", href: "onboarding.html?tab=modules" },
];

/** Какая вкладка — этот экран. */
function current() {
  const page = location.pathname.split("/").pop() || "onboarding.html";
  if (page.startsWith("learning")) return "learning";
  if (page.startsWith("practice")) return "practice";
  if (page.startsWith("watchlist")) return "watchlist";
  if (page.startsWith("onboarding")) {
    const tab = new URLSearchParams(location.search).get("tab");
    return tab === "noa" || (!tab && !document.querySelector("[data-tab][aria-selected='true']:not([data-tab='noa'])")) ? "noa" : "more";
  }
  return "more";
}

export function mountPhoneNav() {
  if (document.querySelector(".phone-nav")) return;
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = new URL("../styles/phone.css", import.meta.url).href;
  document.head.append(link);

  const nav = document.createElement("nav");
  nav.className = "phone-nav";
  nav.setAttribute("aria-label", "Разделы");
  const here = current();
  for (const tab of TABS) {
    const item = document.createElement("a");
    item.className = "phone-nav__tab";
    item.href = tab.href;
    if (tab.id === here) item.setAttribute("aria-current", "page");
    const icon = document.createElement("span");
    icon.className = "phone-nav__icon";
    icon.textContent = tab.icon;
    const label = document.createElement("span");
    label.className = "phone-nav__label";
    label.textContent = tab.label;
    item.append(icon, label);
    nav.append(item);
  }
  document.body.append(nav);
  document.documentElement.classList.add("has-phone-nav");
}
