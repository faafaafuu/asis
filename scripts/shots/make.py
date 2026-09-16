"""Собирает анимации для README.

Кадры снимает headless-браузер с тех же файлов, что и в программе:

- сцены (`scene.html`) — попап и индикатор голоса, покадрово;
- туры по окнам (`window.html`) — настоящее окно на тестовых данных
  (`mock-tauri.js`), по состоянию на кадр, с плавным переходом между ними.

    python scripts/shots/make.py [имя ...]

Нужны Chrome (или Edge) и Pillow. Результат — анимированный WebP в `docs/`:
полный цвет без палитры в 256 оттенков, на которой неоновое свечение
расслаивается полосами, и файл меньше GIF.
"""

import http.server
import os
import shutil
import socketserver
import subprocess
import sys
import tempfile
import threading
from pathlib import Path
from urllib.parse import quote

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / "docs"
SCALE = 2
THEME = "neon"

# Сцены: имя, сцена, кадров, ширина, высота, кадров в секунду.
SCENES = [
    ("demo-select.webp", "select", 74, 880, 470, 12),
    ("demo-voice.webp", "voice", 68, 880, 430, 12),
]

# Туры: имя, окно, ширина, высота, состояния — (шаги, секунд на экране).
# Каждое состояние снимается с чистой загрузки, поэтому шаги в нём полные.
ANSWER = "Многоступенчатая сборка: собрать в образе golang, а запускать из distroless."
TOURS = [
    ("hub.webp", "onboarding", 620, 780, [
        ("", 3.0),
        ("click:Настройки", 3.0),
        ("click:Помощь", 2.2),
    ]),
    ("watchlist.webp", "watchlist", 720, 480, [
        ("", 2.6),
        ("click:Крипто", 2.2),
        ("click:Оповещения", 2.4),
        ("click:Год", 2.4),
    ]),
    ("learning.webp", "learning", 960, 640, [
        ("click:Урок", 3.2),
        ("click:Задачи", 2.2),
        (f"fill:.question textarea={ANSWER}|click:Проверить|wait:600", 4.2),
    ]),
    ("tasks.webp", "tasks", 400, 560, [
        ("", 3.0),
    ]),
]

FADE_FRAMES = 4
FADE_MS = 60


def browser() -> str:
    for path in (
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    ):
        if os.path.exists(path):
            return path
    sys.exit("не нашёл ни Chrome, ни Edge")


class Quiet(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT), **kwargs)

    def log_message(self, *args):
        pass


def serve() -> tuple[socketserver.TCPServer, int]:
    httpd = socketserver.ThreadingTCPServer(("127.0.0.1", 0), Quiet)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd, httpd.server_address[1]


def shoot(chrome: str, url: str, width: int, height: int, budget_ms: int, out: Path) -> Image.Image:
    subprocess.run(
        [
            chrome,
            "--headless=new",
            "--disable-gpu",
            "--hide-scrollbars",
            f"--force-device-scale-factor={SCALE}",
            f"--window-size={width},{height}",
            f"--virtual-time-budget={budget_ms}",
            f"--screenshot={out}",
            url,
        ],
        check=True,
        capture_output=True,
    )
    return Image.open(out).convert("RGB")


def save(name: str, frames: list[Image.Image], durations: list[int]) -> None:
    path = DOCS / name
    frames[0].save(
        path,
        save_all=True,
        append_images=frames[1:],
        duration=durations,
        loop=0,
        # На тёмном фоне сжатие сильнее заметно: ниже 95 проступают тени
        # соседних кадров.
        quality=95,
        method=6,
    )
    print(f"{name}: {len(frames)} кадров, {path.stat().st_size // 1024} КБ")


def make_scene(chrome: str, port: int, work: Path, spec) -> None:
    name, scene, count, width, height, fps = spec
    frames = []
    for i in range(count):
        url = f"http://127.0.0.1:{port}/scripts/shots/scene.html?scene={scene}&f={i}"
        # Виртуальное время: анимации отматываются ровно на нужный кадр.
        frames.append(shoot(chrome, url, width, height, 400 + i * (1000 // fps), work / f"{i:03d}.png"))
        print(".", end="", flush=True)
    print()
    save(name, frames, [1000 // fps] * len(frames))


def make_tour(chrome: str, port: int, work: Path, spec) -> None:
    name, page, width, height, states = spec
    shots = []
    for i, (steps, _) in enumerate(states):
        url = (
            f"http://127.0.0.1:{port}/scripts/shots/window.html"
            f"?page={page}&theme={THEME}&steps={quote(steps)}"
        )
        shots.append(shoot(chrome, url, width, height, 8000, work / f"{i:02d}.png"))
        print(".", end="", flush=True)
    print()

    frames, durations = [], []
    for i, (image, (_, seconds)) in enumerate(zip(shots, states)):
        frames.append(image)
        durations.append(int(seconds * 1000))
        following = shots[(i + 1) % len(shots)]
        if len(shots) > 1:
            for k in range(1, FADE_FRAMES + 1):
                frames.append(Image.blend(image, following, k / (FADE_FRAMES + 1)))
                durations.append(FADE_MS)
    save(name, frames, durations)


def main() -> None:
    wanted = set(sys.argv[1:])
    chrome = browser()
    httpd, port = serve()
    DOCS.mkdir(exist_ok=True)
    try:
        jobs = [(make_scene, spec) for spec in SCENES] + [(make_tour, spec) for spec in TOURS]
        for make, spec in jobs:
            if wanted and spec[0] not in wanted:
                continue
            work = Path(tempfile.mkdtemp(prefix="shots-"))
            print(f"{spec[0]}: снимаю")
            try:
                make(chrome, port, work, spec)
            finally:
                shutil.rmtree(work, ignore_errors=True)
    finally:
        httpd.shutdown()


if __name__ == "__main__":
    main()
