// Схема к разделу урока: несколько блоков и стрелки между ними.
//
// Модель присылает её в конце рассказа блоком
//   ```scheme
//   {"title": "…", "nodes": [{"id": "a", "label": "…"}], "edges": [{"from": "a", "to": "b", "label": "…"}]}
//   ```
// Рисуется своим SVG без библиотек: блоки встают слоями слева направо по
// стрелкам (что из чего следует), подписи стрелок — над ними. Вслух схема не
// читается — её смотрят, пока Ноа рассказывает.

const SVG = "http://www.w3.org/2000/svg";
const MAX_NODES = 9;

/** Текст модели → { text } без блока схемы и { scheme } — разобранная схема или null. */
export function splitScheme(source) {
  const text = String(source ?? "");
  const match = /```scheme\s*([\s\S]*?)```/i.exec(text);
  if (!match) return { text: text.trim(), scheme: null };
  return { text: (text.slice(0, match.index) + text.slice(match.index + match[0].length)).trim(), scheme: parseScheme(match[1]) };
}

/** JSON схемы → проверенная схема или null. */
export function parseScheme(raw) {
  let data;
  try {
    data = typeof raw === "string" ? JSON.parse(raw) : raw;
  } catch {
    return null;
  }
  // Главное раздела — 3–5 коротких пунктов: их видно, пока Ноа рассказывает.
  const points = (Array.isArray(data?.points) ? data.points : [])
    .map((p) => String(p ?? "").trim().slice(0, 90))
    .filter(Boolean)
    .slice(0, 5);
  let nodes = (Array.isArray(data?.nodes) ? data.nodes : [])
    .filter((n) => n && n.id != null && String(n.label ?? "").trim())
    .slice(0, MAX_NODES)
    .map((n) => ({ id: String(n.id), label: String(n.label).trim().slice(0, 40) }));
  if (nodes.length < 2) {
    if (!points.length) return null;
    nodes = [];
  }
  const known = new Set(nodes.map((n) => n.id));
  const edges = (Array.isArray(data?.edges) ? data.edges : [])
    .filter((e) => e && known.has(String(e.from)) && known.has(String(e.to)) && String(e.from) !== String(e.to))
    .map((e) => ({ from: String(e.from), to: String(e.to), label: String(e.label ?? "").trim().slice(0, 24) }));
  return { title: String(data?.title ?? "").trim().slice(0, 60), points, nodes, edges };
}

/**
 * Слои: блок стоит правее всех, из кого в него идут стрелки. Без стрелок —
 * в первом слое. Циклы не мешают: глубина ограничена числом блоков.
 */
export function layers(scheme) {
  const depth = new Map(scheme.nodes.map((n) => [n.id, 0]));
  for (let pass = 0; pass < scheme.nodes.length; pass++) {
    let moved = false;
    for (const e of scheme.edges) {
      const want = depth.get(e.from) + 1;
      if (want > depth.get(e.to) && want < scheme.nodes.length) {
        depth.set(e.to, want);
        moved = true;
      }
    }
    if (!moved) break;
  }
  const columns = [];
  for (const n of scheme.nodes) (columns[depth.get(n.id)] ??= []).push(n);
  return columns.filter(Boolean);
}

/** Перенос подписи блока по словам — до трёх строк. */
function wrap(label, width) {
  const words = label.split(/\s+/);
  const lines = [];
  let line = "";
  for (const word of words) {
    if (line && (line + " " + word).length > width) {
      lines.push(line);
      line = word;
    } else line = line ? `${line} ${word}` : word;
  }
  if (line) lines.push(line);
  return lines.slice(0, 3);
}

function node(tag, attrs = {}, text = "") {
  const el = document.createElementNS(SVG, tag);
  for (const [key, value] of Object.entries(attrs)) el.setAttribute(key, String(value));
  if (text) el.textContent = text;
  return el;
}

/**
 * Роль блока — для цвета: откуда всё начинается (в него стрелок нет), чем
 * кончается (из него стрелок нет) и что посередине. Глазу сразу видно
 * направление: зелёное → синее → золотое.
 */
export function roles(scheme) {
  const into = new Set(scheme.edges.map((e) => e.to));
  const out = new Set(scheme.edges.map((e) => e.from));
  return new Map(
    scheme.nodes.map((n) => [n.id, !scheme.edges.length ? "mid" : !into.has(n.id) ? "start" : !out.has(n.id) ? "end" : "mid"]),
  );
}

/** Рисует схему: <figure> с подписью и SVG. */
export function renderScheme(scheme) {
  const columns = layers(scheme);
  const CHAR = 7.4;
  const H = 52;
  const GAP_X = 96;
  const GAP_Y = 28;
  // Ширина колонки — по самой длинной строке её блоков.
  const widths = columns.map((column) =>
    Math.min(200, Math.max(116, ...column.map((n) => Math.max(...wrap(n.label, 22).map((l) => l.length)) * CHAR + 26))),
  );
  const tallest = Math.max(...columns.map((c) => c.length));
  const width = widths.reduce((sum, w) => sum + w, 0) + (columns.length - 1) * GAP_X + 24;
  const height = tallest * H + (tallest - 1) * GAP_Y + 24;

  const pos = new Map();
  let left = 12;
  columns.forEach((column, x) => {
    const offset = ((tallest - column.length) * (H + GAP_Y)) / 2;
    column.forEach((n, y) => pos.set(n.id, { x: left, y: 12 + offset + y * (H + GAP_Y), w: widths[x] }));
    left += widths[x] + GAP_X;
  });

  // Точки входа и выхода стрелок разнесены по краю блока: несколько стрелок
  // в один блок не сливаются в одну линию и подписи не лезут друг на друга.
  const ports = (list, side) => {
    const map = new Map();
    for (const [id, edges] of list) {
      edges.sort((a, b) => pos.get(side === "out" ? a.to : a.from).y - pos.get(side === "out" ? b.to : b.from).y);
      edges.forEach((e, k) => map.set(e, ((k + 1) * H) / (edges.length + 1)));
    }
    return map;
  };
  const group = (key) => {
    const map = new Map();
    for (const e of scheme.edges) (map.get(e[key]) ?? map.set(e[key], []).get(e[key])).push(e);
    return map;
  };
  const outPort = ports(group("from"), "out");
  const inPort = ports(group("to"), "in");

  const svg = node("svg", { viewBox: `0 0 ${width} ${height}`, class: "scheme__svg", role: "img", "aria-label": scheme.title || "Схема" });
  svg.style.maxWidth = `${width}px`;
  const defs = node("defs");
  const marker = node("marker", { id: "scheme-arrow", viewBox: "0 0 10 10", refX: 9, refY: 5, markerWidth: 7, markerHeight: 7, orient: "auto-start-reverse" });
  marker.append(node("path", { d: "M0,0 L10,5 L0,10 z", class: "scheme__head" }));
  defs.append(marker);
  svg.append(defs);

  const labels = [];
  for (const e of scheme.edges) {
    const a = pos.get(e.from);
    const b = pos.get(e.to);
    const forward = b.x > a.x;
    const x1 = forward ? a.x + a.w : a.x + a.w / 2;
    const y1 = forward ? a.y + outPort.get(e) : a.y + H;
    const x2 = forward ? b.x : b.x + b.w / 2;
    const y2 = forward ? b.y + inPort.get(e) : b.y;
    const mid = (x1 + x2) / 2;
    const d = forward ? `M${x1},${y1} C${mid},${y1} ${mid},${y2} ${x2},${y2}` : `M${x1},${y1} C${x1},${y1 + 34} ${x2},${y2 - 34} ${x2},${y2}`;
    svg.append(node("path", { d, class: "scheme__edge", "marker-end": "url(#scheme-arrow)" }));
    if (e.label) labels.push({ text: e.label, x: mid, y: (y1 + y2) / 2 });
  }

  for (const n of scheme.nodes) {
    const { x, y, w } = pos.get(n.id);
    svg.append(node("rect", { x, y, width: w, height: H, rx: 10, class: `scheme__box scheme__box--${roles(scheme).get(n.id)}` }));
    const lines = wrap(n.label, 22);
    const top = y + H / 2 - ((lines.length - 1) * 15) / 2 + 5;
    lines.forEach((text, i) => svg.append(node("text", { x: x + w / 2, y: top + i * 15, class: "scheme__label", "text-anchor": "middle" }, text)));
  }

  // Подписи стрелок — поверх, на плашке; совпавшие по месту раздвигаются.
  labels.sort((a, b) => a.y - b.y);
  for (let i = 1; i < labels.length; i++) {
    const prev = labels[i - 1];
    if (Math.abs(labels[i].x - prev.x) < 70 && labels[i].y - prev.y < 18) labels[i].y = prev.y + 18;
  }
  for (const label of labels) {
    const w = label.text.length * 6.6 + 12;
    svg.append(node("rect", { x: label.x - w / 2, y: label.y - 11, width: w, height: 16, rx: 8, class: "scheme__pill" }));
    svg.append(node("text", { x: label.x, y: label.y + 1, class: "scheme__edge-label", "text-anchor": "middle" }, label.text));
  }

  const figure = document.createElement("figure");
  figure.className = "scheme";
  if (scheme.title) {
    const caption = document.createElement("figcaption");
    caption.textContent = scheme.title;
    figure.append(caption);
  }
  figure.append(svg);
  return figure;
}

/** Просьба к модели — дописать схему в конце рассказа. */
export const SCHEME_ASK =
  "В самом конце, после вопроса, добавь блок ```scheme с JSON: " +
  '{"title": "…", "points": ["…"], "nodes": [{"id": "a", "label": "…"}], "edges": [{"from": "a", "to": "b", "label": "…"}]}. ' +
  "points — 3–5 главных мыслей раздела, каждая до 8 слов. nodes и edges — схема: 3–7 блоков по 1–4 слова, " +
  "стрелки — что из чего следует или что за чем идёт, подпись стрелки 1–2 слова или пусто. " +
  "Блок — только для экрана, вслух его не упоминай.";
