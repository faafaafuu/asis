// Голос Ноа в браузере: говорит тем же голосом, что программа, и слушает,
// не перебивая саму себя.
//
// Говорит — голосом Silero с сервера сайта (/api/noa/tts), для вошедших; нет
// входа или сервер занят — голосом браузера.
//
// Слушает двумя способами. Chrome и Edge распознают речь сами — быстро и без
// нагрузки на сервер. Safari (и любой браузер на iPhone) и Firefox — нет или
// ненадёжно: там страница сама записывает фразу — начало и конец по громкости,
// как в программе, — и распознаёт её сервер Ноа (/api/noa/stt).
//
// Главное здесь — порядок. Микрофон открывается только когда Ноа договорила и
// прошло ещё 0.7 с: звук доигрывает из буфера, и без запаса браузер записывал
// конец её же фразы, принимал его за ответ и отвечал сам себе.

import { request } from "./net.js";

const Recognition = globalThis.SpeechRecognition ?? globalThis.webkitSpeechRecognition;
const ua = globalThis.navigator?.userAgent ?? "";
/** Safari и всё на iPhone/iPad (там любой браузер — это Safari внутри). */
const webkitOnly = /iPhone|iPad|iPod/.test(ua) || (/Safari\//.test(ua) && !/Chrome|Chromium|CriOS|Edg|OPR|Android/.test(ua));
const canRecord = Boolean(globalThis.navigator?.mediaDevices?.getUserMedia && globalThis.MediaRecorder);
/** Телефон: непрерывное распознавание там работает плохо — слушаем по фразе. */
const phone = /Android|iPhone|iPad|iPod/.test(ua);
/**
 * Распознаёт ли браузер сам. Safari умеет, но капризно: на iPhone нужна
 * включённая Siri, и держать микрофон непрерывно он не любит. Поэтому в
 * Safari своё распознавание пробуется первым, по одной фразе, а откажет —
 * дальше пишем и распознаём на сервере.
 */
let native = Boolean(Recognition);

export const canListen = Boolean(Recognition) || canRecord;

/** Сколько глухоты после собственной речи. */
const DEAF_AFTER_SPEECH_MS = 700;
/** Сколько ждать, пока человек начнёт говорить. */
const WAIT_FOR_SPEECH_MS = 12_000;
/** Пауза, после которой фраза считается сказанной. */
const END_OF_PHRASE_MS = 1_300;
/** Дольше одной фразы не пишем. */
const MAX_PHRASE_MS = 30_000;

const read = (key, fallback) => {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
};

/* ── Звук: один элемент на страницу ───────────────────────────────────────
   iPhone играет звук только из элемента, который однажды запустили касанием.
   Ответ приходит с сервера через секунду после касания — к тому времени
   разрешение уже истекло. Поэтому первое же касание страницы «будит»
   единственный элемент тишиной, и дальше все фразы играют через него. */

const SILENCE = "data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAIA+AAACABAAZGF0YQAAAAA=";
const player = typeof Audio === "function" ? new Audio() : null;
let audioContext = null;

function unlock() {
  if (player && !player.dataset?.awake) {
    player.src = SILENCE;
    player.play().then(() => player.pause()).catch(() => {});
    if (player.dataset) player.dataset.awake = "1";
  }
  try {
    audioContext ??= new (globalThis.AudioContext ?? globalThis.webkitAudioContext)();
    audioContext.resume?.();
  } catch {
    audioContext = null;
  }
  // Голос браузера на iPhone тоже просыпается только от касания.
  if (globalThis.speechSynthesis && !unlock.spoke) {
    unlock.spoke = true;
    const hush = new SpeechSynthesisUtterance(" ");
    hush.volume = 0;
    speechSynthesis.speak(hush);
  }
}
for (const event of ["pointerdown", "touchend", "keydown"]) {
  globalThis.addEventListener?.(event, unlock, { capture: true, passive: true });
}

/** Текст для чтения вслух: без разметки, кода и ссылок. */
export function speakable(text) {
  const source = String(text ?? "");
  if (looksLikeCode(source)) return "Это код. Чтобы я рассказала, что он делает, нажмите «Объяснить».";
  // Код вслух не читается — так же, как в программе (voice::without_code):
  // блок — «код на экране», команда — «команду на экране», имя — словом,
  // путь — последней частью, ссылка — словом.
  return source
    .replace(/```[\s\S]*?```/g, " Код — на экране. ")
    .replace(/`([^`]*)`/g, (_, code) => (/^[\p{L}\p{N}._-]{1,24}$/u.test(code) ? code : "команду на экране"))
    .replace(/https?:\/\/\S+/g, "ссылка")
    .replace(/(^|\s)(~?\/[^\s,;:!?()«»"']*|[^\s,;:!?()«»"'/]+(?:\/[^\s,;:!?()«»"'/]+){2,})/g, (_, lead, path) => {
      const last = path.replace(/\/+$/, "").split("/").pop();
      return `${lead}${last || path}`;
    })
    .replace(/[`*_#>|]/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

/** Сам текст — код: много скобок, присваиваний и знаков, мало русских букв. */
function looksLikeCode(text) {
  const total = text.replace(/\s/g, "").length;
  if (total < 20) return false;
  const marks = (text.match(/[{}();=<>$|&[\]\\#]/g) ?? []).length;
  const russian = (text.match(/[а-яё]/gi) ?? []).length;
  return (marks * 100) / total >= 8 && (russian * 100) / total < 20;
}

/** Куски по предложениям: первый звучит, пока готовится следующий. */
function pieces(text, limit = 320) {
  const out = [];
  let part = "";
  for (const sentence of text.split(/(?<=[.!?…])\s+/)) {
    if (part && part.length + sentence.length + 1 > limit) {
      out.push(part);
      part = "";
    }
    part = `${part} ${sentence}`.trim();
  }
  if (part) out.push(part);
  return out;
}

let generation = 0;
let playingNow = false;
let serverVoice = true;
/** Что Ноа сказала последним — чтобы не принять это за ответ человека. */
let lastSaid = "";

async function fetchVoice(text) {
  if (!serverVoice) return null;
  try {
    const response = await request("/api/noa/tts", {
      timeout: 25_000,
      retries: 0,
      method: "POST",
      headers: { "Content-Type": "application/json" },
      credentials: "same-origin",
      body: JSON.stringify({ text, voice: read("noa.voice", "xenia"), rate: Number(read("noa.rate", "1.2")) }),
    });
    // Не вошли — голос сервера не для гостей: дальше без него.
    if (response.status === 401) serverVoice = false;
    if (!response.ok) return null;
    return URL.createObjectURL(await response.blob());
  } catch {
    return null;
  }
}

function playUrl(url, id) {
  return new Promise((resolve) => {
    const audio = player ?? new Audio();
    let over = false;
    const done = () => {
      if (over) return;
      over = true;
      clearInterval(check);
      URL.revokeObjectURL(url);
      playingNow = false;
      resolve();
    };
    playingNow = true;
    audio.onended = done;
    audio.onerror = done;
    audio.src = url;
    audio.play().catch(done);
    const check = setInterval(() => {
      if (id !== generation) {
        audio.pause();
        done();
      }
    }, 150);
  });
}

function browserSpeak(text, id) {
  return new Promise((resolve) => {
    const synth = globalThis.speechSynthesis;
    if (!synth) return resolve();
    const phrase = new SpeechSynthesisUtterance(text);
    phrase.lang = "ru-RU";
    phrase.rate = Math.min(Number(read("noa.rate", "1.2")), 1.6);
    const voice = synth.getVoices().find((v) => v.lang?.startsWith("ru"));
    if (voice) phrase.voice = voice;
    synth.speak(phrase);
    // onend в Chrome иногда не приходит — смотрим на сам синтез.
    const check = setInterval(() => {
      if (id !== generation) synth.cancel();
      if (!synth.speaking && !synth.pending) {
        clearInterval(check);
        resolve();
      }
    }, 200);
  });
}

/** Сказать вслух. Промис исполнится, когда Ноа договорит. */
export async function speak(text) {
  const clean = speakable(text);
  stopSpeaking();
  const id = generation;
  if (!clean) return;
  lastSaid = clean;
  const parts = pieces(clean);
  let next = fetchVoice(parts[0]);
  for (let at = 0; at < parts.length; at++) {
    const url = await next;
    if (id !== generation) return;
    // Следующий кусок готовится, пока звучит этот.
    next = at + 1 < parts.length ? fetchVoice(parts[at + 1]) : null;
    if (url) await playUrl(url, id);
    else await browserSpeak(parts[at], id);
    if (id !== generation) return;
  }
  await new Promise((resolve) => setTimeout(resolve, DEAF_AFTER_SPEECH_MS));
}

export function stopSpeaking() {
  generation++;
  player?.pause();
  playingNow = false;
  globalThis.speechSynthesis?.cancel();
}

export const speaking = () => playingNow || Boolean(globalThis.speechSynthesis?.speaking);

/** Похоже ли услышанное на то, что Ноа сама только что сказала. */
function isEcho(heard) {
  const words = (text) => text.toLowerCase().replace(/ё/g, "е").split(/[^\p{L}\p{N}]+/u).filter((w) => w.length > 2);
  const said = new Set(words(lastSaid));
  const got = words(heard);
  if (got.length < 3 || !said.size) return false;
  return got.filter((w) => said.has(w)).length / got.length >= 0.7;
}

/* ── Распознавание браузером (Chrome, Edge) ──────────────────────────────── */

function listenNative({ onHeard, signal, pause = END_OF_PHRASE_MS } = {}) {
  return new Promise((resolve, reject) => {
    // На телефоне и в Safari — по одной фразе: непрерывный режим там то
    // молчит, то повторяет сказанное, то обрывается на первой паузе.
    const continuous = !(webkitOnly || phone);
    const started = Date.now();
    const finals = [];
    let interim = "";
    let lastVoice = 0;
    let ear = null;
    let finished = false;

    const finish = (error) => {
      if (finished) return;
      finished = true;
      clearInterval(silence);
      try {
        ear?.abort();
      } catch {
        /* уже остановлено */
      }
      if (error) return reject(error);
      const text = [...finals, interim].join(" ").replace(/\s+/g, " ").trim();
      resolve(isEcho(text) ? "" : text);
    };

    const open = () => {
      ear = new Recognition();
      ear.lang = "ru-RU";
      ear.continuous = continuous;
      ear.interimResults = true;
      ear.onresult = (event) => {
        interim = "";
        for (let at = event.resultIndex; at < event.results.length; at++) {
          const text = event.results[at][0].transcript;
          if (event.results[at].isFinal) finals.push(text);
          else interim += text;
        }
        lastVoice = Date.now();
        onHeard?.([...finals, interim].join(" ").trim());
      };
      ear.onerror = (event) => {
        // Safari без Siri отвечает service-not-allowed: переходим на запись.
        if (event.error === "service-not-allowed" && canRecord) {
          native = false;
          finish(new Error(FALLBACK));
        } else if (event.error === "not-allowed" || event.error === "service-not-allowed") {
          finish(new Error("Браузер не дал микрофон — разрешите его в адресной строке."));
        }
      };
      // Chrome сам закрывает распознавание на паузе — открываем снова, пока
      // человек не договорил и не вышло время ожидания.
      ear.onend = () => {
        if (finished) return;
        // По фразе: распознавание само закончило её — значит, договорили.
        // Но в пересказе (долгая пауза) — слушаем дальше: система обрывает
        // на первом же вдохе, а человек ещё думает.
        if (!continuous && (finals.length || interim) && pause <= END_OF_PHRASE_MS) return finish();
        if (interim) {
          finals.push(interim);
          interim = "";
        }
        setTimeout(() => !finished && open(), 120);
      };
      try {
        ear.start();
      } catch {
        /* уже запущено */
      }
    };

    const silence = setInterval(() => {
      const now = Date.now();
      if (signal?.aborted) finish();
      else if (lastVoice && now - lastVoice > pause) finish();
      else if (!lastVoice && now - started > WAIT_FOR_SPEECH_MS) finish();
    }, 200);
    open();
  });
}

/* ── Запись и распознавание сервером (Safari, iPhone, Firefox) ───────────── */

async function openMic() {
  try {
    // Эхоподавление убирает из записи голос Ноа из колонок.
    return await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true } });
  } catch (err) {
    throw new Error(
      err?.name === "NotAllowedError" ? "Браузер не дал микрофон — разрешите его в настройках сайта." : "Микрофон не открылся.",
    );
  }
}

function recorderFor(stream) {
  const type = ["audio/webm;codecs=opus", "audio/mp4", "audio/ogg;codecs=opus", "audio/webm"].find((t) => MediaRecorder.isTypeSupported?.(t));
  return new MediaRecorder(stream, type ? { mimeType: type } : undefined);
}

async function recognize(blob) {
  const response = await request("/api/noa/stt", {
    timeout: 45_000,
    retries: 0,
    method: "POST",
    headers: { "Content-Type": blob.type || "application/octet-stream" },
    credentials: "same-origin",
    body: blob,
  });
  const data = await response.json().catch(() => ({}));
  if (response.status === 401) throw new Error("В этом браузере речь распознаёт сервер Ноа — войдите на сайт.");
  if (!response.ok) throw new Error(data.error ?? "Не получилось распознать.");
  return String(data.text ?? "").trim();
}

/** Громкость микрофона по кадрам: для начала и конца фразы. */
function levelMeter(stream) {
  const context = audioContext ?? new (globalThis.AudioContext ?? globalThis.webkitAudioContext)();
  audioContext = context;
  context.resume?.();
  const source = context.createMediaStreamSource(stream);
  const analyser = context.createAnalyser();
  analyser.fftSize = 1024;
  source.connect(analyser);
  const frame = new Float32Array(analyser.fftSize);
  return {
    level() {
      analyser.getFloatTimeDomainData(frame);
      let sum = 0;
      for (const sample of frame) sum += sample * sample;
      return Math.sqrt(sum / frame.length);
    },
    close: () => source.disconnect(),
  };
}

async function listenRecorded({ onHeard, signal, pause = END_OF_PHRASE_MS } = {}) {
  const stream = await openMic();
  const meter = levelMeter(stream);
  const recorder = recorderFor(stream);
  const chunks = [];
  recorder.ondataavailable = (event) => event.data.size && chunks.push(event.data);
  const stopped = new Promise((resolve) => (recorder.onstop = resolve));
  recorder.start(250);

  // Порог — от шума комнаты, как в программе: речь заметно громче тишины.
  const started = Date.now();
  let noise = 0.01;
  let speechAt = 0;
  let lastVoice = 0;
  await new Promise((resolve) => {
    const tick = setInterval(() => {
      const now = Date.now();
      const rms = meter.level();
      const threshold = Math.max(noise * 3.5, 0.015);
      if (rms > threshold) {
        speechAt ||= now;
        lastVoice = now;
        onHeard?.("…");
      } else if (!speechAt) {
        noise = noise * 0.95 + rms * 0.05;
      }
      const done =
        signal?.aborted ||
        (speechAt && now - lastVoice > pause) ||
        (!speechAt && now - started > WAIT_FOR_SPEECH_MS) ||
        now - started > MAX_PHRASE_MS;
      if (done) {
        clearInterval(tick);
        resolve();
      }
    }, 60);
  });
  recorder.stop();
  await stopped;
  meter.close();
  for (const track of stream.getTracks()) track.stop();
  // Громче шума так и не стало — никто не говорил, сервер не трогаем.
  if (signal?.aborted || !speechAt || lastVoice - speechAt < 300) return "";
  onHeard?.("распознаю…");
  const text = await recognize(new Blob(chunks, { type: recorder.mimeType || "audio/webm" }));
  return isEcho(text) ? "" : text;
}

/**
 * Одна реплика человека. Ждёт начала речи до 12 с, конец — по паузе 1.3 с.
 * Пусто — никто ничего не сказал. `onHeard` — промежуточное для экрана.
 */
const FALLBACK = "переход на запись";

export async function listen(opts = {}) {
  if (native) {
    try {
      return await listenNative(opts);
    } catch (err) {
      if (err.message !== FALLBACK) throw err;
    }
  }
  if (canRecord) return listenRecorded(opts);
  throw new Error("Голос в этом браузере не работает — напишите текстом.");
}

/**
 * Диктовка ответа: пишет, пока не вызовут `stop()`, и отдаёт текст.
 * Для кнопки «Надиктовать» в окне обучения.
 */
export async function dictate() {
  if (native) {
    const ear = new Recognition();
    ear.lang = "ru-RU";
    ear.continuous = true;
    ear.interimResults = false;
    const parts = [];
    const done = new Promise((resolve) => (ear.onend = resolve));
    ear.onresult = (event) => {
      for (let at = event.resultIndex; at < event.results.length; at++) {
        if (event.results[at].isFinal) parts.push(event.results[at][0].transcript);
      }
    };
    ear.onerror = () => {};
    ear.start();
    return {
      async stop() {
        ear.stop();
        await done;
        return parts.join(" ").trim();
      },
    };
  }
  if (!canRecord) throw new Error("Диктовка в этом браузере не работает — напишите текстом.");
  const stream = await openMic();
  const recorder = recorderFor(stream);
  const chunks = [];
  recorder.ondataavailable = (event) => event.data.size && chunks.push(event.data);
  const stopped = new Promise((resolve) => (recorder.onstop = resolve));
  recorder.start(250);
  return {
    async stop() {
      recorder.stop();
      await stopped;
      for (const track of stream.getTracks()) track.stop();
      return recognize(new Blob(chunks, { type: recorder.mimeType || "audio/webm" }));
    },
  };
}
