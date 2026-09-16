// Сцена «режимы Ноа»: тот же индикатор, что в программе, но покадрово и без
// случайностей — каждый кадр собирается заново, и частицы не должны прыгать
// между кадрами. Математика кольца и облака — из src/js/hud.js.

const FPS = 15;
const STEPS_PER_FRAME = 4; // индикатор рисуется на 60 Гц, кадр — на 15
const frame = Number(new URLSearchParams(location.search).get("f") ?? 0);

const PALETTE = {
  idle: ["oklch(0.55 0.08 270)", "oklch(0.5 0.07 250)", "oklch(0.58 0.06 230)"],
  listening: ["oklch(0.75 0.15 160)", "oklch(0.72 0.16 190)", "oklch(0.78 0.13 140)"],
  thinking: ["oklch(0.7 0.15 250)", "oklch(0.65 0.18 290)", "oklch(0.72 0.13 210)"],
  speaking: ["oklch(0.75 0.19 300)", "oklch(0.78 0.17 260)", "oklch(0.82 0.14 200)"],
};

// Раскадровка: с какого кадра какой режим, что сказано и что на экране.
const TIMELINE = [
  { from: 0, mode: "idle", step: 0, note: "Скажите «Ноа» — или зажмите Alt + Пробел" },
  { from: 22, mode: "listening", step: 1, said: "Ноа, поставь таймер на десять минут" },
  { from: 56, mode: "thinking", step: 2, said: "Ноа, поставь таймер на десять минут" },
  { from: 72, mode: "speaking", step: 3, answer: "Поставил таймер на десять минут." },
  { from: 104, mode: "listening", step: 4, said: "Спасибо!" },
  { from: 126, mode: "speaking", step: 4, answer: "Обращайтесь!" },
  { from: 140, mode: "idle", step: 0, note: "Разговор окончен — снова ждёт имени" },
];
export const FRAMES = 160;

const STEPS = ["Ждёт имени", "Слушает", "Думает", "Отвечает", "Разговор"];

const ease = (t) => 1 - Math.pow(1 - Math.min(Math.max(t, 0), 1), 3);
const beatAt = (f) => TIMELINE.filter((b) => b.from <= f).at(-1);
const typed = (text, from, frames) => text.slice(0, Math.round(text.length * ease((frame - from) / frames)));

/** Детерминированный генератор: одни и те же частицы на каждом кадре. */
function seeded(seed) {
  return () => {
    seed = (seed * 1664525 + 1013904223) % 4294967296;
    return seed / 4294967296;
  };
}

function targetLevel(mode, time) {
  switch (mode) {
    case "listening":
    case "thinking":
      return 0.32 + Math.sin(time * 2) * 0.08;
    case "speaking":
      return 0.55 + Math.sin(time * 3.1) * 0.25 + Math.sin(time * 7.3) * 0.12;
    default:
      return 0.12;
  }
}

const withAlpha = (color, alpha) => color.replace(")", ` / ${alpha})`);

/** Прогоняет индикатор с начала до этого кадра и рисует последний шаг. */
function drawHud(canvas) {
  const W = 360;
  const H = 180;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = W * dpr;
  canvas.height = H * dpr;
  const ctx = canvas.getContext("2d");
  ctx.scale(dpr, dpr);

  const random = seeded(7);
  const particles = Array.from({ length: 26 }, (_, i) => ({
    a: (i / 26) * Math.PI * 2,
    r: 60 + random() * 30,
    speed: 0.15 + random() * 0.3,
    size: 1 + random() * 1.8,
    phase: random() * Math.PI * 2,
  }));

  let time = 0;
  let level = 0;
  let mode = "idle";
  const total = frame * STEPS_PER_FRAME + 1;
  for (let step = 0; step < total; step++) {
    mode = beatAt(Math.floor(step / STEPS_PER_FRAME)).mode;
    time += 0.016;
    const chase = mode === "speaking" ? 0.35 : 0.12;
    level += (targetLevel(mode, time) - level) * chase;
    for (const p of particles) p.a += p.speed * 0.012;
  }

  const colors = PALETTE[mode];
  const cx = W / 2;
  const cy = H / 2;

  const cloudR = 70 + level * 40;
  const cloud = ctx.createRadialGradient(cx, cy, 4, cx, cy, cloudR);
  cloud.addColorStop(0, withAlpha(colors[0], 0.28 + level * 0.2));
  cloud.addColorStop(0.5, withAlpha(colors[1], 0.1));
  cloud.addColorStop(1, "transparent");
  ctx.fillStyle = cloud;
  ctx.beginPath();
  ctx.arc(cx, cy, cloudR, 0, Math.PI * 2);
  ctx.fill();

  for (const p of particles) {
    const radius = p.r + Math.sin(time * 1.5 + p.phase) * 6 + level * 18;
    const glow = Math.abs(Math.sin(time * 2 + p.phase));
    ctx.fillStyle = withAlpha(colors[2], 0.15 + glow * 0.35);
    ctx.beginPath();
    ctx.arc(cx + Math.cos(p.a) * radius, cy + Math.sin(p.a) * radius * 0.62, p.size, 0, Math.PI * 2);
    ctx.fill();
  }

  const speaking = mode === "speaking";
  const spikes = speaking ? 20 : 64;
  const baseR = 34 + level * 10;
  ctx.beginPath();
  for (let i = 0; i <= spikes; i++) {
    const a = (i / spikes) * Math.PI * 2;
    const noise = speaking
      ? Math.sin(a * 5 + time * 3) * 0.6 + Math.sin(a * 3 - time * 1.8) * 0.4
      : Math.sin(a * 9 + time * 4) * 0.5 + Math.sin(a * 5 - time * 2.6) * 0.3 + Math.sin(a * 17 + time * 6) * 0.2;
    const amount = (speaking ? 17 : mode === "idle" ? 3 : 7) * level + 2;
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

  if (!speaking) {
    ctx.beginPath();
    for (let i = 0; i <= 18; i++) {
      const a = (i / 18) * Math.PI * 2 - time * 0.6;
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
}

export function render() {
  document.getElementById("page").remove();
  document.getElementById("hudwrap").remove();
  document.getElementById("caption").remove();

  const beat = beatAt(frame);
  const root = document.createElement("div");
  root.className = "modes";
  root.innerHTML = `
    <div class="modes__brand"><b>Ноа</b><span>голосовой помощник</span></div>
    <canvas class="modes__hud"></canvas>
    <div class="modes__line"></div>
    <ol class="modes__steps">${STEPS.map((s, i) => `<li data-i="${i}">${s}</li>`).join("")}</ol>`;
  document.body.append(root);

  drawHud(root.querySelector(".modes__hud"));

  const line = root.querySelector(".modes__line");
  const since = beat.from;
  if (beat.note) {
    line.className = "modes__line modes__line--note";
    line.textContent = beat.note;
    line.style.opacity = String(ease((frame - since) / 5));
  } else if (beat.answer) {
    line.className = "modes__line modes__line--answer";
    line.textContent = typed(beat.answer, since, 14);
  } else {
    line.className = "modes__line modes__line--said";
    // В «Думает» фраза уже сказана целиком — не печатаем её заново.
    const first = TIMELINE.find((b) => b.said === beat.said);
    line.innerHTML = `<i>«</i>${typed(beat.said, first.from + 2, 18)}<i>»</i>`;
  }

  for (const li of root.querySelectorAll(".modes__steps li")) {
    li.classList.toggle("is-on", Number(li.dataset.i) === beat.step);
    li.classList.toggle("is-done", Number(li.dataset.i) < beat.step && beat.step !== 0);
  }
}
