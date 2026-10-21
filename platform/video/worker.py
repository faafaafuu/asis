#!/usr/bin/env python3
"""Служба рендера роликов Ноа.

Сайт кладёт задание в jobs/<id>/: scenes.json (фразы озвучки по сценам),
scene.py (код ролика на деталях noah_kit) и пустой файл ready. Служба:
1. озвучивает сцены голосом Ноа и пишет durations.json;
2. рендерит код в контейнере noah-manim — без сети, без лишних прав, с
   пределом памяти, процессов и времени;
3. ставит озвучку на отметки начала сцен (timeline.json) и склеивает с
   видео, снимает кадр-обложку;
4. пишет done.json — или error.json с хвостом журнала, по которому модель
   исправит код.

Ход — в status.json: voice, render, mux. Сайт читает его и показывает.
"""
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.request
import wave

ROOT = os.environ.get("NOAH_VIDEO_DIR", "/opt/noah-video")
JOBS = os.path.join(ROOT, "jobs")
KIT = os.path.join(ROOT, "kit")
VOICE = os.environ.get("NOAH_VOICE_URL", "http://127.0.0.1:8644") + "/tts"
SPEAKER = os.environ.get("NOAH_VIDEO_SPEAKER", "xenia")
PAD = 0.6
RENDER_LIMIT = 480


def log(message):
    print(time.strftime("[%H:%M:%S] ") + message, flush=True)


def write(path, value):
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as out:
        json.dump(value, out, ensure_ascii=False)
    os.chmod(tmp, 0o644)
    os.replace(tmp, path)


def stage(job, name):
    write(os.path.join(job, "status.json"), {"stage": name, "at": time.time()})


def say(text, path):
    body = json.dumps({"text": text, "speaker": SPEAKER, "rate": 1.1}).encode("utf-8")
    request = urllib.request.Request(VOICE, data=body, headers={"Content-Type": "application/json"})
    data = urllib.request.urlopen(request, timeout=180).read()
    with open(path, "wb") as out:
        out.write(data)
    with wave.open(path) as sound:
        return sound.getnframes() / sound.getframerate()


def voice(job):
    """Озвучка сцен. Уже есть — не переозвучиваем: при исправлении кода
    фразы те же."""
    durations_path = os.path.join(job, "durations.json")
    if os.path.exists(durations_path):
        return
    scenes = json.load(open(os.path.join(job, "scenes.json"), encoding="utf-8"))
    folder = os.path.join(job, "voice")
    os.makedirs(folder, exist_ok=True)
    durations = []
    for n, text in enumerate(scenes):
        durations.append(round(say(text, os.path.join(folder, f"{n:02d}.wav")) + PAD, 2))
    write(durations_path, durations)


def render(job):
    for stale in ("media", "timeline.json"):
        path = os.path.join(job, stale)
        if os.path.isdir(path):
            shutil.rmtree(path, ignore_errors=True)
        elif os.path.exists(path):
            os.remove(path)
    # Иконки — копией в папку задания: Manim пишет рядом с SVG свой
    # временный файл, и подключённая только для чтения папка ему не годится.
    icons = os.path.join(job, "icons")
    if not os.path.isdir(icons) or not os.listdir(icons):
        shutil.rmtree(icons, ignore_errors=True)
        shutil.copytree(os.path.join(KIT, "icons"), icons)
    for base, dirs, files in os.walk(job):
        for name in dirs:
            os.chmod(os.path.join(base, name), 0o777)
    os.chmod(job, 0o777)
    command = [
        "docker", "run", "--rm", "--network", "none", "--cpus", "4", "--memory", "3g", "--pids-limit", "512",
        "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
        "-v", f"{job}:/manim",
        "-v", f"{KIT}/noah_kit.py:/manim/noah_kit.py:ro",
        "noah-manim", "manim", "-q", "h", "--fps", "30", "--disable_caching", "--progress_bar", "none",
        "-o", "video", "scene.py", "Video",
    ]
    try:
        done = subprocess.run(command, capture_output=True, text=True, timeout=RENDER_LIMIT)
    except subprocess.TimeoutExpired:
        return f"Рендер не уложился в {RENDER_LIMIT} с — слишком много анимаций."
    if done.returncode != 0:
        lines = (done.stdout + "\n" + done.stderr).strip().splitlines()
        return "\n".join(lines[-60:])
    return None


def mux(job):
    found = None
    for base, _, files in os.walk(os.path.join(job, "media", "videos")):
        if "video.mp4" in files:
            found = os.path.join(base, "video.mp4")
    if not found:
        return "Рендер не оставил видео."
    scenes = sorted(f for f in os.listdir(os.path.join(job, "voice")) if f.endswith(".wav"))
    try:
        starts = json.load(open(os.path.join(job, "timeline.json"), encoding="utf-8"))
    except (OSError, ValueError):
        starts = []
    if len(starts) < len(scenes):
        # Отметок нет — кладём фразы подряд, как они звучат.
        durations = json.load(open(os.path.join(job, "durations.json"), encoding="utf-8"))
        starts, at = [], 0.0
        for d in durations:
            starts.append(at)
            at += d
    inputs, chains = [], []
    for n, name in enumerate(scenes):
        inputs += ["-i", os.path.join(job, "voice", name)]
        delay = int(starts[n] * 1000)
        chains.append(f"[{n + 1}:a]adelay={delay}|{delay}[a{n}]")
    mix = "".join(f"[a{n}]" for n in range(len(scenes)))
    graph = ";".join(chains) + f";{mix}amix=inputs={len(scenes)}:dropout_transition=0:normalize=0[voice]"
    out = os.path.join(job, "final.mp4")
    command = ["ffmpeg", "-loglevel", "error", "-y", "-i", found, *inputs, "-filter_complex", graph,
               "-map", "0:v", "-map", "[voice]", "-c:v", "copy", "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart", out]
    done = subprocess.run(command, capture_output=True, text=True)
    if done.returncode != 0:
        return "Склейка не удалась: " + done.stderr[-400:]
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-ss", "1.5", "-i", out, "-frames:v", "1", "-vf", "scale=640:-1",
                    os.path.join(job, "poster.jpg")], capture_output=True)
    os.replace(out, os.path.join(job, "video.mp4"))
    for path in ("video.mp4", "poster.jpg"):
        full = os.path.join(job, path)
        if os.path.exists(full):
            os.chmod(full, 0o644)
    shutil.rmtree(os.path.join(job, "media"), ignore_errors=True)
    return None


def run(job):
    started = time.time()
    try:
        stage(job, "voice")
        voice(job)
        stage(job, "render")
        failed = render(job)
        if failed is None:
            stage(job, "mux")
            failed = mux(job)
    except Exception as err:  # озвучка, диск, что угодно — сайту нужен ответ
        failed = f"{type(err).__name__}: {err}"
    if failed:
        write(os.path.join(job, "error.json"), {"log": failed})
        log(f"{os.path.basename(job)}: не вышло за {time.time() - started:.0f} с")
    else:
        size = os.path.getsize(os.path.join(job, "video.mp4"))
        write(os.path.join(job, "done.json"), {"size": size, "seconds": round(time.time() - started)})
        log(f"{os.path.basename(job)}: готово за {time.time() - started:.0f} с, {size // 1024} КБ")


def main():
    os.makedirs(JOBS, exist_ok=True)
    log("служба рендера роликов запущена")
    while True:
        for name in sorted(os.listdir(JOBS)):
            job = os.path.join(JOBS, name)
            ready = os.path.join(job, "ready")
            if not os.path.isfile(ready):
                continue
            try:
                os.remove(ready)
            except OSError:
                continue
            for old in ("error.json", "done.json"):
                path = os.path.join(job, old)
                if os.path.exists(path):
                    os.remove(path)
            run(job)
        time.sleep(2)


if __name__ == "__main__":
    sys.exit(main())
