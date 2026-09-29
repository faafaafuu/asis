// Значки дизайн-системы NOAH: один набор (styles/noah/icons.svg) на страницу.
//
// Набор вставляется в начало <body> один раз, дальше значок — это
// <svg><use href="#i-имя"/></svg>. Файл свой и лежит рядом, в программе он
// читается с диска, в браузере — одним запросом, который кэшируется.

const SVG = "http://www.w3.org/2000/svg";
let mounted = null;

/** Вставляет набор значков в страницу. Повторный вызов ничего не делает. */
export function mountIcons() {
  mounted ??= fetch(new URL("../styles/noah/icons.svg", import.meta.url))
    .then((response) => (response.ok ? response.text() : ""))
    .then((text) => {
      if (!text || document.getElementById("i-close")) return;
      const holder = document.createElement("div");
      holder.innerHTML = text;
      const sprite = holder.querySelector("svg");
      if (sprite) document.body.prepend(sprite);
    })
    .catch(() => {
      /* без значков кнопки остаются с подписью в title — не беда */
    });
  return mounted;
}

/** Значок `name` (без «i-») размером `size`. Линия — как в дизайне, 2.25. */
export function icon(name, size = 16) {
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("class", "icon");
  const use = document.createElementNS(SVG, "use");
  use.setAttribute("href", `#i-${name}`);
  svg.append(use);
  return svg;
}
