"""Распознавание речи для Ноа онлайн — когда браузер не распознаёт сам.

Chrome и Edge распознают речь сами. Safari (и всё на iPhone) и Firefox —
нет или ненадёжно: там страница записывает фразу и присылает её сюда. Модель
та же семья, что Whisper в программе, но для процессора: faster-whisper.

POST /stt — тело: запись в любом формате, который пишет браузер (webm, mp4,
ogg, wav) -> {"text": "..."}.
GET /health -> 200, когда модель готова.

Слушает только 127.0.0.1: снаружи к нему ходит сайт, пуская только вошедших.
Запуск: stt_server.py <модель> <порт> [папка моделей]
"""

import json
import os
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from faster_whisper import WhisperModel

MODEL, PORT = sys.argv[1], int(sys.argv[2])
ROOT = sys.argv[3] if len(sys.argv) > 3 else None
MAX_BYTES = 4 * 1024 * 1024

model = WhisperModel(MODEL, device="cpu", compute_type="int8", cpu_threads=min(4, os.cpu_count() or 4), download_root=ROOT)
lock = threading.Lock()


def transcribe(path: str) -> str:
    with lock:
        segments, _ = model.transcribe(
            path,
            language="ru",
            beam_size=1,
            # Тишина по краям и шорохи не должны превращаться в «Продолжение
            # следует» — этим Whisper заполняет пустоту.
            vad_filter=True,
            condition_on_previous_text=False,
            initial_prompt="Ноа, объясни, пожалуйста.",
        )
        return " ".join(segment.text.strip() for segment in segments).strip()


# То, что Whisper пишет на пустой записи: титры из роликов, на которых учился.
HALLUCINATIONS = ("продолжение следует", "субтитры", "редактор субтитров", "спасибо за просмотр", "подписывайтесь")


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def reply(self, code: int, body: dict) -> None:
        raw = json.dumps(body, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self) -> None:
        self.reply(200 if self.path == "/health" else 404, {"ok": self.path == "/health"})

    def do_POST(self) -> None:
        if self.path != "/stt":
            return self.reply(404, {"error": "не тот адрес"})
        size = int(self.headers.get("Content-Length") or 0)
        if not 0 < size <= MAX_BYTES:
            return self.reply(413, {"error": "запись пустая или слишком длинная"})
        data = self.rfile.read(size)
        started = time.monotonic()
        with tempfile.NamedTemporaryFile(suffix=".audio") as tmp:
            tmp.write(data)
            tmp.flush()
            try:
                text = transcribe(tmp.name)
            except Exception as err:  # noqa: BLE001 — запись не разобралась, это ответ, а не падение
                return self.reply(422, {"error": f"запись не разобралась: {err}"})
        if any(mark in text.lower() for mark in HALLUCINATIONS):
            text = ""
        print(f"распознано за {time.monotonic() - started:.1f} с: {len(text)} знаков", flush=True)
        self.reply(200, {"text": text})


server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
server.serve_forever()
