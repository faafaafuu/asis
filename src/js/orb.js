// Фирменный знак голоса Ноа: неровное кольцо вокруг облака частиц.
//
// Портирован из макета «Voice Assistant Icon» без изменений в математике:
// зубчатое кольцо из суммы синусоид, облако дрейфующих частиц, плавное
// сглаживание уровня. Рисует его и индикатор программы (hud.js), и разговор
// в браузере (web/talk.js) — знак один и тот же.

/** Размер полотна в логических пикселях — как в макете. */
const W = 360;
const H = 180;

/** Сколько частиц в облаке. Больше — заметно дороже, меньше — пусто. */
const PARTICLE_COUNT = 26;

/** Цвета по состояниям: три оттенка на градиент кольца, облака и частиц. */
export const PALETTE = {
  // Цвета — дизайн-система NOAH, «Кольцо Ноа».
  idle: ["#6f7fa6", "#5f6f96", "#7c8bb0"],
  listening: ["#5fd3a8", "#4fc3d8", "#7fe0b0"],
  thinking: ["#7c8cf0", "#9a7cf0", "#6fb0f0"],
  speaking: ["#e07cf0", "#b08cff", "#7cc8f5"],
  // Смотрит на окно (левый Shift + пробел): янтарное кольцо в уголках кадра.
  watching: ["#f2c14e", "#e5a93a", "#f6d27a"],
  // Микрофон молчит или недоступен: серое кольцо, перечёркнутое.
  muted: ["#8a8f99", "#7a7f89", "#9aa0ab"],
  // Загрузка распознавания: одни точки, без кольца. Кольцо значит «я тут и
  // работаю», а пока распознавание поднимается, работать ещё нечем.
  loading: ["oklch(0.7 0.15 250)", "oklch(0.65 0.18 290)", "oklch(0.8 0.12 220)"],
};

/** Сколько длится появление. Быстро, но не мгновенно. */
const APPEAR_MS = 260;

/** Добавляет прозрачность к цвету, не разбирая его на части. */
function withAlpha(color, alpha) {
  if (color.startsWith("oklch")) return color.replace(")", ` / ${alpha})`);
  if (/^#[0-9a-f]{6}$/i.test(color)) {
    return `${color}${Math.round(Math.max(0, Math.min(1, alpha)) * 255)
      .toString(16)
      .padStart(2, "0")}`;
  }
  return color;
}

/**
 * Рисует знак на `canvas`, пока не позовут `stop()`.
 * `setMode` — idle, listening, thinking, speaking, loading;
 * `setLevel` — живая громкость речи 0…1 (без неё рисуется придуманная).
 */
export function createOrb(canvas) {
  const ctx = canvas.getContext("2d");
  let mode = "idle";
  let time = 0;
  let level = 0;
  let liveLevel = 0;
  let liveAt = 0;
  let appearAt = performance.now();
  let frame = 0;

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
      case "loading":
        return 0.25;
      case "muted":
        return 0.05;
      case "listening":
      case "watching":
      case "thinking":
        // Спокойная медленная пульсация: программа ждёт, а не суетится.
        return 0.32 + Math.sin(time * 2) * 0.08;
      case "speaking": {
        // Живая громкость, если она свежая. Полсекунды без вестей — значит
        // поток прервался, и лучше вернуться к придуманной анимации.
        const fresh = performance.now() - liveAt < 500;
        if (fresh) return 0.3 + liveLevel * 0.95;
        return 0.55 + Math.sin(time * 3.1) * 0.25 + Math.sin(time * 7.3) * 0.12;
      }
      default:
        return 0.12;
    }
  }

  function draw() {
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== W * dpr) {
      canvas.width = W * dpr;
      canvas.height = H * dpr;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const cx = W / 2;
    const cy = H / 2 + 10;
    time += 0.016;

    // Появление: прозрачность и размер догоняют единицу по замедляющейся кривой.
    const progress = Math.min((performance.now() - appearAt) / APPEAR_MS, 1);
    const appear = 1 - Math.pow(1 - progress, 3);

    // В речи уровень догоняет втрое быстрее — иначе сглаживание съедает
    // как раз то, что делает движение похожим на голос.
    const chase = mode === "speaking" ? 0.35 : 0.12;
    level += (targetLevel() - level) * chase;

    const colors = PALETTE[mode] ?? PALETTE.idle;
    ctx.clearRect(0, 0, W, H);

    ctx.save();
    ctx.globalAlpha = appear;
    const scale = 0.9 + appear * 0.1;
    ctx.translate(cx, cy);
    ctx.scale(scale, scale);
    ctx.translate(-cx, -cy);

    // Облако: мягкое свечение в центре.
    const cloudR = 70 + level * 40;
    const cloud = ctx.createRadialGradient(cx, cy, 4, cx, cy, cloudR);
    cloud.addColorStop(0, withAlpha(colors[0], 0.28 + level * 0.2));
    cloud.addColorStop(0.5, withAlpha(colors[1], 0.1));
    cloud.addColorStop(1, "transparent");
    const loading = mode === "loading";
    if (!loading) {
      ctx.fillStyle = cloud;
      ctx.beginPath();
      ctx.arc(cx, cy, cloudR, 0, Math.PI * 2);
      ctx.fill();
    }

    // Частицы: дрейфуют по эллиптической орбите и мерцают вразнобой.
    for (const p of particles) {
      p.a += p.speed * (loading ? 0.035 : 0.012);
      const wobble = Math.sin(time * 1.5 + p.phase) * 6;
      const radius = p.r + wobble + level * 18;
      const x = cx + Math.cos(p.a) * radius;
      const y = cy + Math.sin(p.a) * radius * 0.62;
      const glow = Math.abs(Math.sin(time * 2 + p.phase));
      const alpha = loading ? 0.35 + glow * 0.55 : 0.15 + glow * 0.35;
      ctx.fillStyle = withAlpha(colors[2], alpha);
      ctx.beginPath();
      ctx.arc(x, y, loading ? p.size * 1.3 : p.size, 0, Math.PI * 2);
      ctx.fill();
    }

    if (!loading) {
      // Кольцо. В речи зубцов меньше, но каждый крупнее — движение читается
      // как артикуляция, а не как рябь.
      const spikes = mode === "speaking" ? 20 : 64;
      const baseR = 34 + level * 10;
      ctx.beginPath();
      for (let i = 0; i <= spikes; i++) {
        const a = (i / spikes) * Math.PI * 2;
        const noise =
          mode === "speaking"
            ? Math.sin(a * 5 + time * 3) * 0.6 + Math.sin(a * 3 - time * 1.8) * 0.4
            : Math.sin(a * 9 + time * 4) * 0.5 + Math.sin(a * 5 - time * 2.6) * 0.3 + Math.sin(a * 17 + time * 6) * 0.2;
        const amount = (mode === "speaking" ? 17 : mode === "listening" || mode === "thinking" ? 7 : 3) * level + 2;
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

      // Думает — вокруг бежит дуга: видно, что работа идёт.
      if (mode === "thinking") {
        ctx.strokeStyle = colors[2];
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(cx, cy, baseR + 18, time * 2.2, time * 2.2 + 1.1);
        ctx.stroke();
      }

      // Смотрит на окно — уголки кадра вокруг кольца.
      if (mode === "watching") {
        const box = baseR + 22;
        const arm = 12;
        ctx.strokeStyle = colors[0];
        ctx.lineWidth = 2;
        for (const [dx, dy] of [
          [-1, -1],
          [1, -1],
          [1, 1],
          [-1, 1],
        ]) {
          const x = cx + dx * box;
          const y = cy + dy * box;
          ctx.beginPath();
          ctx.moveTo(x - dx * arm, y);
          ctx.lineTo(x, y);
          ctx.lineTo(x, y - dy * arm);
          ctx.stroke();
        }
      }

      // Микрофон молчит — кольцо перечёркнуто.
      if (mode === "muted") {
        ctx.strokeStyle = colors[1];
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(cx - baseR - 6, cy + baseR + 6);
        ctx.lineTo(cx + baseR + 6, cy - baseR - 6);
        ctx.stroke();
      }
    }

    ctx.restore();
    frame = requestAnimationFrame(draw);
  }

  frame = requestAnimationFrame(draw);

  return {
    setMode(next) {
      if (next in PALETTE) mode = next;
    },
    get mode() {
      return mode;
    },
    setLevel(value) {
      if (!Number.isFinite(value)) return;
      liveLevel = Math.max(0, Math.min(1, value));
      liveAt = performance.now();
    },
    appear() {
      appearAt = performance.now();
    },
    stop() {
      cancelAnimationFrame(frame);
    },
  };
}
