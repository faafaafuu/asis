"""Голосовой сервер Silero для Суфлёра.

Модель грузится один раз; дальше каждая фраза — один HTTP-запрос:
POST /tts {"text": ..., "speaker": ..., "rate": 1.0} -> WAV (48 кГц, моно, 16 бит).
GET /health -> 200, когда модель готова.

Слушает только 127.0.0.1. Через IDLE_SECONDS без запросов выходит сам —
держать сотни мегабайт памяти, когда голос не нужен, незачем.
"""

import array
import io
import json
import re
import sys
import threading
import time
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from xml.sax.saxutils import escape

import torch

MODEL_PATH, PORT = sys.argv[1], int(sys.argv[2])
IDLE_SECONDS = 600
SAMPLE_RATE = 48000
SPEAKERS = {"aidar", "baya", "kseniya", "xenia", "eugene"}

torch.set_num_threads(4)
model = torch.package.PackageImporter(MODEL_PATH).load_pickle("tts_models", "model")
model.to(torch.device("cpu"))
lock = threading.Lock()
last_used = time.monotonic()


def rate_word(rate: float) -> str:
    """Silero понимает скорость словами, а не множителем."""
    if rate < 0.8:
        return "slow"
    if rate < 1.15:
        return "medium"
    if rate < 1.5:
        return "fast"
    return "x-fast"


def chunks(text: str, limit: int = 800):
    """Длинный текст — кусками по границам предложений: у модели есть предел."""
    part = ""
    for sentence in re.split(r"(?<=[.!?…])\s+", text.strip()):
        if part and len(part) + len(sentence) + 1 > limit:
            yield part
            part = ""
        part = f"{part} {sentence}".strip()
    if part:
        yield part


def synthesize(text: str, speaker: str, rate: float) -> bytes:
    speaker = speaker if speaker in SPEAKERS else "xenia"
    pieces = []
    with lock:
        for part in chunks(text):
            ssml = f'<speak><prosody rate="{rate_word(rate)}">{escape(part)}</prosody></speak>'
            pieces.append(model.apply_tts(ssml_text=ssml, speaker=speaker, sample_rate=SAMPLE_RATE))
    if not pieces:
        raise ValueError("пустой текст")
    audio = torch.cat(pieces)
    # Без numpy: PyTorch для процессора его не ставит.
    samples = (audio.clamp(-1, 1) * 32767).to(torch.int16).tolist()
    pcm = array.array("h", samples).tobytes()
    out = io.BytesIO()
    with wave.open(out, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(SAMPLE_RATE)
        wav.writeframes(pcm)
    return out.getvalue()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        self.send_response(200 if self.path == "/health" else 404)
        self.end_headers()

    def do_POST(self):
        global last_used
        if self.path != "/tts":
            self.send_response(404)
            self.end_headers()
            return
        last_used = time.monotonic()
        try:
            length = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(length) or b"{}")
            wav = synthesize(str(body.get("text", "")), str(body.get("speaker", "")), float(body.get("rate", 1.0)))
        except Exception as err:  # отказ — текстом, Суфлёр прочитает своим голосом
            message = str(err).encode("utf-8")
            self.send_response(500)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(message)))
            self.end_headers()
            self.wfile.write(message)
            return
        last_used = time.monotonic()
        self.send_response(200)
        self.send_header("Content-Type", "audio/wav")
        self.send_header("Content-Length", str(len(wav)))
        self.end_headers()
        self.wfile.write(wav)


def watch_idle(server):
    while True:
        time.sleep(15)
        if time.monotonic() - last_used > IDLE_SECONDS:
            server.shutdown()
            return


server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
threading.Thread(target=watch_idle, args=(server,), daemon=True).start()
server.serve_forever()
