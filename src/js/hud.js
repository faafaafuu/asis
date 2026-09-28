// Индикатор голосового режима. Сам знак рисует orb.js — тот же, что в
// разговоре на сайте; состояние приходит из Rust событием `hud:mode`.

import { tauri } from "./bridge.js";
import { createOrb, PALETTE } from "./orb.js";

const api = tauri();
const canvas = document.getElementById("hud");

const orb = createOrb(canvas);

// Состояние могли выставить до того, как эта страница загрузилась, — тогда
// событие до нас не дошло. Спрашиваем сами.
api
  ?.invoke("hud_mode")
  .then((current) => {
    const next = String(current ?? "idle");
    if (next in PALETTE) orb.setMode(next);
  })
  .catch(() => {
    /* окно открыто вне приложения — рисуем состояние ожидания */
  });

// Голосовые из Telegram. Rust присылает OGG/Opus, здесь он становится WAV на
// 16 кГц: движок браузера Opus понимает, а своего декодера у программы нет.
// Окно индикатора для этого подходит — оно есть всегда и фокус не берёт.
// Rust повторяет просьбу, пока страница не ответит, — повтор не разбирается
// заново.
const decoding = new Set();
api?.listen("audio:decode", async (event) => {
  const { id, data } = event.payload ?? {};
  if (!id || decoding.has(id)) return;
  decoding.add(id);
  try {
    const bytes = Uint8Array.from(atob(data), (ch) => ch.charCodeAt(0));
    // Контекст на 16 кГц: разбирая, движок сам приводит звук к своей частоте.
    const audio = await new OfflineAudioContext(1, 1, 16000).decodeAudioData(bytes.buffer);
    const wav = wav16(audio.getChannelData(0), audio.sampleRate);
    await api.invoke("audio_decoded", { id, data: toBase64(wav), error: null });
  } catch (err) {
    api?.invoke("audio_decoded", { id, data: null, error: String(err?.message ?? err) }).catch(() => {});
  }
});

// Обратный путь: ответ Ноа голосом в Telegram. Rust присылает WAV от Piper,
// здесь он кодируется в Opus и укладывается в OGG — формат голосовых
// сообщений Telegram.
api?.listen("audio:encode", async (event) => {
  const { id, data } = event.payload ?? {};
  if (!id || decoding.has(id)) return;
  decoding.add(id);
  try {
    const bytes = Uint8Array.from(atob(data), (ch) => ch.charCodeAt(0));
    const audio = await new OfflineAudioContext(1, 1, 48000).decodeAudioData(bytes.buffer);
    const ogg = await oggOpus(audio.getChannelData(0));
    await api.invoke("audio_decoded", { id, data: toBase64(ogg), error: null });
  } catch (err) {
    api?.invoke("audio_decoded", { id, data: null, error: String(err?.message ?? err) }).catch(() => {});
  }
});

/** Моно 48 кГц в OGG/Opus: кодирует движок браузера, упаковывает этот код. */
export async function oggOpus(samples) {
  const packets = [];
  let failure = null;
  const encoder = new AudioEncoder({
    output: (chunk) => {
      const bytes = new Uint8Array(chunk.byteLength);
      chunk.copyTo(bytes);
      packets.push({ bytes, duration: chunk.duration ?? 20000 });
    },
    error: (err) => {
      failure = err;
    },
  });
  encoder.configure({ codec: "opus", sampleRate: 48000, numberOfChannels: 1, bitrate: 32000 });
  encoder.encode(
    new AudioData({
      format: "f32",
      sampleRate: 48000,
      numberOfFrames: samples.length,
      numberOfChannels: 1,
      timestamp: 0,
      data: samples,
    }),
  );
  await encoder.flush();
  encoder.close();
  if (failure) throw failure;
  return muxOgg(packets);
}

/** Контрольная сумма страницы OGG: CRC-32 с многочленом 0x04C11DB7, без отражения. */
const OGG_CRC = (() => {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let r = i << 24;
    for (let k = 0; k < 8; k++) r = r & 0x80000000 ? (r << 1) ^ 0x04c11db7 : r << 1;
    table[i] = r >>> 0;
  }
  return table;
})();

function oggCrc(bytes) {
  let crc = 0;
  for (const byte of bytes) crc = ((crc << 8) ^ OGG_CRC[((crc >>> 24) ^ byte) & 0xff]) >>> 0;
  return crc;
}

/** Одна страница OGG с одним пакетом. */
function oggPage(packet, { granule, serial, sequence, flags }) {
  const lacing = [];
  let left = packet.length;
  while (left >= 255) {
    lacing.push(255);
    left -= 255;
  }
  lacing.push(left);
  const page = new Uint8Array(27 + lacing.length + packet.length);
  const view = new DataView(page.buffer);
  page.set([0x4f, 0x67, 0x67, 0x53], 0); // «OggS»
  page[4] = 0;
  page[5] = flags;
  view.setBigInt64(6, BigInt(granule), true);
  view.setUint32(14, serial, true);
  view.setUint32(18, sequence, true);
  page[26] = lacing.length;
  page.set(lacing, 27);
  page.set(packet, 27 + lacing.length);
  view.setUint32(22, oggCrc(page), true);
  return page;
}

/** Пакеты Opus в файл OGG: заголовок, теги, по пакету на страницу. */
function muxOgg(packets) {
  const ascii = (text) => [...text].map((ch) => ch.charCodeAt(0));
  const serial = (Math.random() * 0xffffffff) >>> 0;
  // Задержка кодировщика Opus: столько отсчётов в начале проигрыватель пропускает.
  const PRE_SKIP = 312;

  const head = new Uint8Array(19);
  const headView = new DataView(head.buffer);
  head.set(ascii("OpusHead"), 0);
  head[8] = 1; // версия
  head[9] = 1; // каналов
  headView.setUint16(10, PRE_SKIP, true);
  headView.setUint32(12, 48000, true);
  headView.setInt16(16, 0, true);
  head[18] = 0; // раскладка каналов: моно или стерео

  const vendor = new TextEncoder().encode("Sufler");
  const tags = new Uint8Array(8 + 4 + vendor.length + 4);
  const tagsView = new DataView(tags.buffer);
  tags.set(ascii("OpusTags"), 0);
  tagsView.setUint32(8, vendor.length, true);
  tags.set(vendor, 12);
  tagsView.setUint32(12 + vendor.length, 0, true);

  const pages = [
    oggPage(head, { granule: 0, serial, sequence: 0, flags: 0x02 }),
    oggPage(tags, { granule: 0, serial, sequence: 1, flags: 0 }),
  ];
  let granule = 0;
  packets.forEach((packet, at) => {
    granule += Math.round((packet.duration * 48000) / 1e6);
    const last = at === packets.length - 1;
    pages.push(oggPage(packet.bytes, { granule, serial, sequence: at + 2, flags: last ? 0x04 : 0 }));
  });

  const out = new Uint8Array(pages.reduce((sum, page) => sum + page.length, 0));
  let offset = 0;
  for (const page of pages) {
    out.set(page, offset);
    offset += page.length;
  }
  return out;
}

/** Моно, 16 бит — то, что ждёт распознавание. */
function wav16(samples, rate) {
  const buffer = new ArrayBuffer(44 + samples.length * 2);
  const view = new DataView(buffer);
  const text = (at, value) => {
    for (let i = 0; i < value.length; i++) view.setUint8(at + i, value.charCodeAt(i));
  };
  text(0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  text(8, "WAVE");
  text(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, rate, true);
  view.setUint32(28, rate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  text(36, "data");
  view.setUint32(40, samples.length * 2, true);
  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(44 + i * 2, s < 0 ? s * 0x8000 : s * 0x7fff, true);
  }
  return new Uint8Array(buffer);
}

function toBase64(bytes) {
  let text = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(text);
}

api?.listen("hud:appear", () => orb.appear());

api?.listen("hud:level", (event) => {
  orb.setLevel(Number(event.payload));
});

api?.listen("hud:mode", (event) => {
  orb.setMode(String(event.payload ?? "idle"));
});
