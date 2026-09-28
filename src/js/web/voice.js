// Голос Ноа в браузере: говорит тем же голосом, что программа, и слушает,
// не перебивая саму себя.
//
// Говорит — голосом Silero с сервера сайта (/api/noa/tts), для вошедших; нет
// входа или сервер занят — голосом браузера. Слушает — распознаванием
// браузера (Chrome, Edge).
//
// Главное здесь — порядок. Микрофон открывается только когда Ноа договорила и
// прошло ещё 0.7 с: звук доигрывает из буфера, и без запаса браузер записывал
// конец её же фразы, принимал его за ответ и отвечал сам себе. То же правило,
// что DEAF_AFTER_SPEECH_MS в программе.

const Recognition = globalThis.SpeechRecognition ?? globalThis.webkitSpeechRecognition;

export const canListen = Boolean(Recognition);

/** Сколько глухоты после собственной речи. */
const DEAF_AFTER_SPEECH_MS = 700;
/** Сколько ждать, пока человек начнёт говорить. */
const WAIT_FOR_SPEECH_MS = 12_000;
/** Пауза, после которой фраза считается сказанной. */
const END_OF_PHRASE_MS = 1_400;

const read = (key, fallback) => {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
};

/** Текст для чтения вслух: без разметки, кода и ссылок. */
export function speakable(text) {
  return String(text ?? "")
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/https?:\/\/\S+/g, " ")
    .replace(/[`*_#>|]/g, "")
    .replace(/\s+/g, " ")
    .trim();
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
let playing = null;
let serverVoice = true;
/** Что Ноа сказала последним — чтобы не принять это за ответ человека. */
let lastSaid = "";

async function fetchVoice(text) {
  if (!serverVoice) return null;
  try {
    const response = await fetch("/api/noa/tts", {
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
    const audio = new Audio(url);
    playing = audio;
    const done = () => {
      URL.revokeObjectURL(url);
      if (playing === audio) playing = null;
      resolve();
    };
    audio.onended = done;
    audio.onerror = done;
    audio.play().catch(done);
    // Остановили — промис не должен висеть.
    const check = setInterval(() => {
      if (id !== generation) {
        clearInterval(check);
        audio.pause();
        done();
      }
      if (audio.ended) clearInterval(check);
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

/** Сказать вслух. Отдаёт промис, который исполнится, когда Ноа договорит. */
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
  playing?.pause();
  playing = null;
  globalThis.speechSynthesis?.cancel();
}

export const speaking = () => Boolean(playing) || Boolean(globalThis.speechSynthesis?.speaking);

/** Похоже ли услышанное на то, что Ноа сама только что сказала. */
function isEcho(heard) {
  const words = (text) => text.toLowerCase().replace(/ё/g, "е").split(/[^\p{L}\p{N}]+/u).filter((w) => w.length > 2);
  const said = new Set(words(lastSaid));
  const got = words(heard);
  if (got.length < 3 || !said.size) return false;
  return got.filter((w) => said.has(w)).length / got.length >= 0.7;
}

/**
 * Одна реплика человека. Ждёт начала речи до 12 с, конец — по паузе 1.4 с.
 * Пусто — никто ничего не сказал. `onHeard` — промежуточный текст для экрана.
 */
export function listen({ onHeard } = {}) {
  return new Promise((resolve, reject) => {
    if (!Recognition) return reject(new Error("Голос в этом браузере не работает — откройте Ноа в Chrome или Edge."));
    const started = Date.now();
    let finals = [];
    let interim = "";
    let lastVoice = 0;
    let ear = null;
    let finished = false;
    let silence = 0;

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
      ear.continuous = true;
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
        if (event.error === "not-allowed" || event.error === "service-not-allowed") {
          finish(new Error("Браузер не дал микрофон — разрешите его в адресной строке."));
        }
      };
      // Chrome сам закрывает распознавание на паузе — открываем снова, пока
      // человек не договорил и не вышло время ожидания.
      ear.onend = () => {
        if (!finished) setTimeout(() => !finished && open(), 120);
      };
      try {
        ear.start();
      } catch {
        /* уже запущено */
      }
    };

    silence = setInterval(() => {
      const now = Date.now();
      if (lastVoice && now - lastVoice > END_OF_PHRASE_MS) finish();
      else if (!lastVoice && now - started > WAIT_FOR_SPEECH_MS) finish();
    }, 200);
    open();
  });
}
