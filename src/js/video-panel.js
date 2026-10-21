// Ролик к уроку: кнопка «🎬 Видео», ход сборки и плеер. Один и тот же — в
// окне обучения на компьютере, в приложении на телефоне и в Ноа онлайн.
//
// Собирает ролик сервер (около двух минут): модель пишет сценарий и код, служба
// озвучивает и рендерит. Пока собирается — можно заниматься дальше: панель
// сама следит за ходом, ролик появится, когда будет готов. Готовый лежит в
// кэше программы и второй раз открывается сразу.

const STAGE_HINT = "около двух минут — можно заниматься дальше";

/** Ролик строкой base64 (программа) или адресом (браузер) — в src для <video>. */
function source(value) {
  if (/^(https?:|\/|blob:)/.test(value)) return value;
  const bytes = Uint8Array.from(atob(value), (c) => c.charCodeAt(0));
  return URL.createObjectURL(new Blob([bytes], { type: "video/mp4" }));
}

function clock(ms) {
  const s = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/**
 * Панель ролика. `section` — номер раздела урока, `null` — ролик по всей
 * теме. `button(text, onClick)` — кнопка в стиле окна.
 */
export function videoBox({ api, el, button, course, topic, section = null }) {
  const box = el("div", "video");
  const row = el("div", "video__row");
  const whole = section == null;
  const go = button(whole ? "🎬 Ролик по теме" : "🎬 Видео по разделу", () => (ready ? play(ready) : start(false)));
  go.classList.add("video__go");
  const status = el("span", "video__status");
  row.append(go, status);
  const player = el("div", "video__player");
  player.hidden = true;
  box.append(row, player);

  let ready = null;
  let timer = 0;
  let started = 0;

  const say = (text, bad = false) => {
    status.textContent = text;
    status.classList.toggle("is-bad", bad);
  };

  const done = (job) => {
    ready = job;
    go.disabled = false;
    go.textContent = whole ? "▶ Ролик по теме" : "▶ Видео по разделу";
    say(job.title ? `«${job.title}»` : "");
  };

  const watch = (job) => {
    clearTimeout(timer);
    if (!box.isConnected) return;
    if (job.status === "done") return done(job);
    if (job.status === "failed") {
      go.disabled = false;
      go.textContent = "🎬 Собрать ещё раз";
      ready = null;
      return say(job.message || "Ролик не собрался.", true);
    }
    go.disabled = true;
    go.textContent = "🎬 Собираю ролик…";
    started ||= Date.parse(`${job.created?.replace(" ", "T")}Z`) || Date.now();
    say(`${job.stageText || "Собираю"} · ${clock(Date.now() - started)} · ${STAGE_HINT}`);
    timer = setTimeout(async () => {
      try {
        watch(await api.invoke("learn_video_status", { id: job.id }));
      } catch {
        // Связь моргнула — спросим ещё раз.
        watch(job);
      }
    }, 4000);
  };

  async function start(again) {
    player.hidden = true;
    player.replaceChildren();
    go.disabled = true;
    say("Заказываю ролик…");
    started = Date.now();
    try {
      watch(await api.invoke("learn_video_make", { course, topic, section, again }));
    } catch (err) {
      go.disabled = false;
      say(String(err), true);
    }
  }

  async function play(job) {
    if (!player.hidden && player.firstChild) {
      player.hidden = true;
      return;
    }
    go.disabled = true;
    say("Загружаю ролик…");
    try {
      const file = await api.invoke("learn_video_file", { id: job.id });
      const video = el("video", "video__screen");
      video.controls = true;
      video.playsInline = true;
      video.src = source(file);
      const tools = el("div", "video__tools");
      const redo = button("↻ Пересобрать", () => start(true));
      const hide = button("Свернуть", () => {
        video.pause();
        player.hidden = true;
      });
      tools.append(redo, hide);
      player.replaceChildren(video, tools);
      player.hidden = false;
      say(job.title ? `«${job.title}»` : "");
      video.play().catch(() => {});
    } catch (err) {
      say(String(err), true);
    } finally {
      go.disabled = false;
    }
  }

  // Уже собирали — показать, что есть: готовый или ещё собирающийся.
  (async () => {
    try {
      const { videos } = await api.invoke("learn_videos", { course, topic });
      const mine = (videos ?? []).find((v) => (v.section ?? null) === section);
      if (mine) watch(mine);
    } catch {
      /* без связи — кнопка «собрать» всё равно есть */
    }
  })();

  return box;
}
