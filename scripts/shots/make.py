"""Собирает гифки для README из настоящего интерфейса программы.

Кадры снимает headless-браузер со страницы `scene.html`: там живут тот же
попап, тот же индикатор и те же стили, что в самой программе. Номер кадра
задаётся ссылкой, поэтому пересобрать всё после правки вёрстки — одна команда,
а не новая запись экрана.

    python scripts/shots/make.py

Нужны Chrome (или Edge) и ffmpeg в PATH.
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

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / "docs"
FPS = 12

SCENES = [
    # имя файла, сцена, кадров, ширина, высота
    ("demo-select.gif", "select", 74, 880, 470),
    ("demo-voice.gif", "voice", 68, 880, 430),
]


def browser() -> str:
    for path in (
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    ):
        if os.path.exists(path):
            return path
    sys.exit("не нашёл ни Chrome, ни Edge")


def serve() -> tuple[socketserver.TCPServer, int]:
    handler = lambda *a, **kw: http.server.SimpleHTTPRequestHandler(*a, directory=str(ROOT), **kw)
    httpd = socketserver.TCPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd, httpd.server_address[1]


def main() -> None:
    chrome = browser()
    httpd, port = serve()
    DOCS.mkdir(exist_ok=True)

    try:
        for name, scene, frames, width, height in SCENES:
            work = Path(tempfile.mkdtemp(prefix=f"shots-{scene}-"))
            print(f"{name}: снимаю {frames} кадров")
            for i in range(frames):
                url = f"http://127.0.0.1:{port}/scripts/shots/scene.html?scene={scene}&f={i}"
                # Виртуальное время вместо настоящего: анимации индикатора
                # отматываются ровно на нужный кадр, и снимок повторяем.
                subprocess.run(
                    [
                        chrome,
                        "--headless=new",
                        "--disable-gpu",
                        "--hide-scrollbars",
                        "--force-device-scale-factor=2",
                        f"--window-size={width},{height}",
                        f"--virtual-time-budget={400 + i * (1000 // FPS)}",
                        f"--screenshot={work / f'{i:03d}.png'}",
                        url,
                    ],
                    check=True,
                    capture_output=True,
                )
                print(".", end="", flush=True)
            print()

            palette = work / "palette.png"
            frames_glob = str(work / "%03d.png")
            subprocess.run(
                ["ffmpeg", "-y", "-v", "error", "-framerate", str(FPS), "-i", frames_glob,
                 "-vf", "scale=880:-1:flags=lanczos,palettegen=stats_mode=diff", str(palette)],
                check=True,
            )
            subprocess.run(
                ["ffmpeg", "-y", "-v", "error", "-framerate", str(FPS), "-i", frames_glob,
                 "-i", str(palette), "-lavfi",
                 "scale=880:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=sierra2_4a",
                 "-loop", "0", str(DOCS / name)],
                check=True,
            )
            size = (DOCS / name).stat().st_size
            print(f"{name}: {size // 1024} КБ")
            shutil.rmtree(work, ignore_errors=True)
    finally:
        httpd.shutdown()


if __name__ == "__main__":
    main()
