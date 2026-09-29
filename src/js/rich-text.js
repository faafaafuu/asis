// Ответ модели — не сплошной текст: в нём бывают команды, таблицы, списки.
//
// Модели пишут их разметкой Markdown, и раньше попап показывал её как есть —
// со звёздочками, решётками и вертикальными чертами. Здесь — узкий разбор
// того, что встречается в коротких ответах: блок кода, таблица, списки,
// **жирный** и `код` в строке. Остальное остаётся текстом.
//
// Разбор (`parseRich`) отдельно от отрисовки: его проверяют тесты без
// браузера. Отрисовка (`renderRich`) строит узлы только через textContent —
// разметка из ответа модели в страницу не попадает никогда.

/** Строка → куски: текст, жирный, код. */
export function parseInline(text) {
  const out = [];
  const re = /(\*\*([^*\n]+)\*\*|`([^`\n]+)`)/g;
  let last = 0;
  for (const match of text.matchAll(re)) {
    if (match.index > last) out.push({ t: "text", v: text.slice(last, match.index) });
    if (match[2] !== undefined) out.push({ t: "b", v: match[2] });
    else out.push({ t: "code", v: match[3] });
    last = match.index + match[0].length;
  }
  if (last < text.length) out.push({ t: "text", v: text.slice(last) });
  return out;
}

const FENCE = /^\s*```\s*([\w+-]*)\s*$/;
const BULLET = /^\s*[-*•]\s+(.*)$/;
const NUMBER = /^\s*(\d{1,3})[.)]\s+(.*)$/;
const TABLE_ROW = /^\s*\|.*\|\s*$/;
const TABLE_RULE = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/;

const cells = (line) =>
  line
    .trim()
    .replace(/^\||\|$/g, "")
    .split("|")
    .map((cell) => cell.trim());

/**
 * Текст → блоки: абзац, списки, код, таблица. Строки абзаца сохраняют
 * переносы — модели часто пишут пункты просто с новой строки.
 */
export function parseRich(source) {
  const lines = String(source ?? "").replace(/\r\n?/g, "\n").split("\n");
  const blocks = [];
  let paragraph = [];
  const flush = () => {
    if (paragraph.length) blocks.push({ type: "p", text: paragraph.join("\n") });
    paragraph = [];
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    const fence = FENCE.exec(line);
    if (fence) {
      flush();
      const code = [];
      i++;
      while (i < lines.length && !FENCE.test(lines[i])) code.push(lines[i++]);
      blocks.push({ type: "code", lang: fence[1] || "", text: code.join("\n") });
      continue;
    }

    if (TABLE_ROW.test(line) && TABLE_RULE.test(lines[i + 1] ?? "")) {
      flush();
      const head = cells(line);
      const rows = [];
      i += 2;
      while (i < lines.length && TABLE_ROW.test(lines[i])) rows.push(cells(lines[i++]));
      i--;
      blocks.push({ type: "table", head, rows });
      continue;
    }

    const bullet = BULLET.exec(line);
    const number = NUMBER.exec(line);
    if (bullet || number) {
      flush();
      const type = bullet ? "ul" : "ol";
      const start = number ? Number(number[1]) : 1;
      const items = [];
      while (i < lines.length) {
        const item = (type === "ul" ? BULLET : NUMBER).exec(lines[i]);
        if (!item) break;
        items.push(type === "ul" ? item[1] : item[2]);
        i++;
      }
      i--;
      blocks.push(type === "ol" ? { type, start, items } : { type, items });
      continue;
    }

    if (!line.trim()) {
      flush();
      continue;
    }
    // Заголовки «## Что делать» — просто жирной строкой: в маленьком окне
    // крупный заголовок спорил бы с термином.
    const heading = /^\s*#{1,6}\s+(.*)$/.exec(line);
    paragraph.push(heading ? `**${heading[1].replace(/\*\*/g, "")}**` : line);
  }
  flush();
  return blocks;
}

/** Есть ли в тексте что-то кроме абзацев — тогда стоит разбирать. */
export function isRich(source) {
  return parseRich(source).some((block) => block.type !== "p") || /\*\*[^*\n]+\*\*|`[^`\n]+`/.test(String(source ?? ""));
}

function inline(doc, text) {
  const frag = doc.createDocumentFragment();
  for (const part of parseInline(text)) {
    if (part.t === "text") {
      frag.append(doc.createTextNode(part.v));
    } else {
      const node = doc.createElement(part.t === "b" ? "strong" : "code");
      node.textContent = part.v;
      frag.append(node);
    }
  }
  return frag;
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.append(area);
    area.select();
    const ok = document.execCommand?.("copy") ?? false;
    area.remove();
    return ok;
  }
}

/**
 * Блоки → узлы. `icon(name)` — фабрика значка (js/icons.js): в тестах и там,
 * где значков нет, передаётся `null`.
 */
export function renderRich(source, { icon = null, doc = document } = {}) {
  const frag = doc.createDocumentFragment();
  for (const block of parseRich(source)) {
    if (block.type === "p") {
      const p = doc.createElement("p");
      p.className = "rt-p";
      p.append(inline(doc, block.text));
      frag.append(p);
    } else if (block.type === "ul" || block.type === "ol") {
      const list = doc.createElement(block.type);
      list.className = `rt-list rt-list--${block.type}`;
      if (block.type === "ol" && block.start !== 1) list.start = block.start;
      for (const text of block.items) {
        const li = doc.createElement("li");
        li.append(inline(doc, text));
        list.append(li);
      }
      frag.append(list);
    } else if (block.type === "code") {
      const box = doc.createElement("div");
      box.className = "rt-code";
      const head = doc.createElement("div");
      head.className = "rt-code__head";
      const lang = doc.createElement("span");
      lang.textContent = block.lang || "код";
      const copy = doc.createElement("button");
      copy.type = "button";
      copy.className = "rt-code__copy";
      copy.tabIndex = -1;
      copy.title = "Скопировать";
      if (icon) copy.append(icon("copy", 14));
      const label = doc.createElement("span");
      label.textContent = "Копировать";
      copy.append(label);
      copy.addEventListener("mousedown", (event) => event.stopPropagation());
      copy.addEventListener("click", async (event) => {
        event.preventDefault();
        label.textContent = (await copyText(block.text)) ? "Скопировано" : "Не вышло";
        setTimeout(() => (label.textContent = "Копировать"), 1500);
      });
      head.append(lang, copy);
      const pre = doc.createElement("pre");
      const code = doc.createElement("code");
      code.textContent = block.text;
      pre.append(code);
      box.append(head, pre);
      frag.append(box);
    } else if (block.type === "table") {
      const wrap = doc.createElement("div");
      wrap.className = "rt-table";
      const table = doc.createElement("table");
      const thead = doc.createElement("thead");
      const headRow = doc.createElement("tr");
      for (const text of block.head) {
        const th = doc.createElement("th");
        th.append(inline(doc, text));
        headRow.append(th);
      }
      thead.append(headRow);
      const tbody = doc.createElement("tbody");
      for (const row of block.rows) {
        const tr = doc.createElement("tr");
        for (const text of row) {
          const td = doc.createElement("td");
          td.append(inline(doc, text));
          tr.append(td);
        }
        tbody.append(tr);
      }
      table.append(thead, tbody);
      wrap.append(table);
      frag.append(wrap);
    }
  }
  return frag;
}
