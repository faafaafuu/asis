// Индикатор голосового режима.
//
// Портирован из макета «Voice Assistant Icon» без изменений в математике:
// зубчатое кольцо из суммы синусоид, облако дрейфующих частиц, плавное
// сглаживание уровня. Состояние приходит из Rust событием `hud:mode`.

import { tauri } from "./bridge.js";

const api = tauri();
const canvas = document.getElementById("hud");
const ctx = canvas.getContext("2d");

/** Размер полотна в логических пикселях — как в макете. */
const W = 360;
const H = 180;

/** Сколько частиц в облаке. Больше — заметно дороже, меньше — пусто. */
const PARTICLE_COUNT = 26;

/** Цвета по состояниям: три оттенка на градиент кольца, облака и частиц. */
const PALETTE = {
  idle: ["oklch(0.55 0.08 270)", "oklch(0.5 0.07 250)", "oklch(0.58 0.06 230)"],
  listening: ["oklch(0.75 0.15 160)", "oklch(0.72 0.16 190)", "oklch(0.78 0.13 140)"],
  thinking: ["oklch(0.7 0.15 250)", "oklch(0.65 0.18 290)", "oklch(0.72 0.13 210)"],
  speaking: ["oklch(0.75 0.19 300)", "oklch(0.78 0.17 260)", "oklch(0.82 0.14 200)"],
};

let mode = "idle";
let time = 0;
let level = 0;

const particles = Array.from({ length: PARTICLE_COUNT }, (_, i) => ({
  a: (i / PARTICLE_COUNT) * Math.PI * 2,
  r: 60 + Math.random() * 30,
  speed: 0.15 + Math.random() * 0.3,
  size: 1 + Math.random() * 1.8,
  phase: Math.random() * Math.PI * 2,
}));

/** Целевой уровень «громкости» — он же амплитуда зубцов. */
function targetLevel() {
  switch (mode) {
    case "listening":
    case "thinking":
      // Спокойная медленная пульсация: программа ждёт, а не суетится.
      return 0.32 + Math.sin(time * 2) * 0.08;
    case "speaking":
      // Резкая многочастотная реакция — так это выглядит, когда говорят.
      return 0.55 + Math.sin(time * 3.1) * 0.25 + Math.sin(time * 7.3) * 0.12;
    default:
      return 0.12;
  }
}

/** Добавляет прозрачность к цвету, не разбирая его на части. */
function withAlpha(color, alpha) {
  if (color.startsWith("oklch")) return color.replace(")", ` / ${alpha})`);
  return color;
}

function draw() {
  const dpr = window.devicePixelRatio || 1;
  if (canvas.width !== W * dpr) {
    canvas.width = W * dpr;
    canvas.height = H * dpr;
    ctx.scale(dpr, dpr);
  }

  const cx = W / 2;
  const cy = H / 2 + 10;
  time += 0.016;

  // Уровень догоняет цель плавно: при смене состояния не должно быть рывка.
  level += (targetLevel() - level) * 0.12;

  const colors = PALETTE[mode] ?? PALETTE.idle;
  ctx.clearRect(0, 0, W, H);

  // Облако: мягкое свечение в центре.
  const cloudR = 70 + level * 40;
  const cloud = ctx.createRadialGradient(cx, cy, 4, cx, cy, cloudR);
  cloud.addColorStop(0, withAlpha(colors[0], 0.28 + level * 0.2));
  cloud.addColorStop(0.5, withAlpha(colors[1], 0.1));
  cloud.addColorStop(1, "transparent");
  ctx.fillStyle = cloud;
  ctx.beginPath();
  ctx.arc(cx, cy, cloudR, 0, Math.PI * 2);
  ctx.fill();

  // Частицы: дрейфуют по эллиптической орбите и мерцают вразнобой.
  for (const p of particles) {
    p.a += p.speed * 0.012;
    const wobble = Math.sin(time * 1.5 + p.phase) * 6;
    const radius = p.r + wobble + level * 18;
    const x = cx + Math.cos(p.a) * radius;
    const y = cy + Math.sin(p.a) * radius * 0.62;
    const alpha = 0.15 + Math.abs(Math.sin(time * 2 + p.phase)) * 0.35;
    ctx.fillStyle = withAlpha(colors[2], alpha);
    ctx.beginPath();
    ctx.arc(x, y, p.size, 0, Math.PI * 2);
    ctx.fill();
  }

  // Кольцо. В речи зубцов меньше, но каждый крупнее — движение читается как
  // артикуляция, а не как рябь.
  const spikes = mode === "speaking" ? 20 : 64;
  const baseR = 34 + level * 10;
  ctx.beginPath();
  for (let i = 0; i <= spikes; i++) {
    const a = (i / spikes) * Math.PI * 2;
    const noise =
      mode === "speaking"
        ? Math.sin(a * 5 + time * 3) * 0.6 + Math.sin(a * 3 - time * 1.8) * 0.4
        : Math.sin(a * 9 + time * 4) * 0.5 +
          Math.sin(a * 5 - time * 2.6) * 0.3 +
          Math.sin(a * 17 + time * 6) * 0.2;
    const amount =
      (mode === "speaking" ? 14 : mode === "listening" || mode === "thinking" ? 7 : 3) * level + 2;
    const r = baseR + noise * amount;
    const x = cx + Math.cos(a) * r;
    const y = cy + Math.sin(a) * r;
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  }
  ctx.closePath();

  const ring = ctx.createLinearGradient(cx - baseR, cy, cx + baseR, cy);
  ring.addColorStop(0, colors[0]);
  ring.addColorStop(0.5, colors[1]);
  ring.addColorStop(1, colors[2]);
  ctx.strokeStyle = ring;
  ctx.lineWidth = 2.2;
  ctx.shadowColor = colors[1];
  ctx.shadowBlur = 18 + level * 20;
  ctx.stroke();
  ctx.shadowBlur = 0;

  // Второе кольцо — только когда программа молчит: в речи оно мешает читать
  // главное движение.
  if (mode !== "speaking") {
    const facets = 18;
    ctx.beginPath();
    for (let i = 0; i <= facets; i++) {
      const a = (i / facets) * Math.PI * 2 - time * 0.6;
      const r = baseR + 16 + Math.sin(a * 3 + time * 3) * (3 + level * 6);
      const x = cx + Math.cos(a) * r;
      const y = cy + Math.sin(a) * r;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.closePath();
    ctx.strokeStyle = withAlpha(colors[2], 0.35);
    ctx.lineWidth = 1;
    ctx.stroke();
  }

  requestAnimationFrame(draw);
}

// Состояние могли выставить до того, как эта страница загрузилась, — тогда
// событие до нас не дошло. Спрашиваем сами.
api
  ?.invoke("hud_mode")
  .then((current) => {
    const next = String(current ?? "idle");
    if (next in PALETTE) mode = next;
  })
  .catch(() => {
    /* окно открыто вне приложения — рисуем состояние ожидания */
  });

api?.listen("hud:mode", (event) => {
  const next = String(event.payload ?? "idle");
  if (next in PALETTE) mode = next;
});

requestAnimationFrame(draw);
