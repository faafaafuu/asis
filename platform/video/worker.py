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
# Рендер кусками: Manim рисует кадры в один поток, и минутный ролик шёл
# 2,5–4 минуты на одном ядре. Куски — диапазоны анимаций (manim -n), время
# сцены при пропуске идёт, поэтому озвучка не уезжает.
PARTS = int(os.environ.get("NOAH_VIDEO_PARTS", "4"))
# 720p: ролик смотрят в окне урока и на телефоне, 1080p там не видно, а
# рисуется он в полтора раза дольше и весит больше.
QUALITY = os.environ.get("NOAH_VIDEO_QUALITY", "m")


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


def manim(job, args, env, cpus, memory):
    """Процесс Manim в изолированном контейнере: без сети, без прав."""
    command = [
        "docker", "run", "--rm", "--network", "none", "--cpus", cpus, "--memory", memory, "--pids-limit", "512",
        "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
        "-v", f"{job}:/manim",
        "-v", f"{KIT}/noah_kit.py:/manim/noah_kit.py:ro",
    ]
    for key, value in env.items():
        command += ["-e", f"{key}={value}"]
    command += ["noah-manim", "manim", *args, "--disable_caching", "--progress_bar", "none", "scene.py", "Video"]
    return subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)


def tail(output):
    return "\n".join(output.strip().splitlines()[-60:])


# Доли ролика по кускам: позднему куску сначала надо догнать сцену (это
# стоит времени и без кадров), поэтому промежуток у него короче.
SHARES = [0.31, 0.27, 0.23, 0.19]


def spans(total, parts):
    """Промежутки сцены [начало, конец) в секундах; последний — до конца."""
    shares = SHARES[:parts] if parts <= len(SHARES) else [1 / parts] * parts
    scale = sum(shares)
    edges, at = [0.0], 0.0
    for share in shares[:-1]:
        at += total * share / scale
        edges.append(round(at, 3))
    edges.append(1e9)
    return list(zip(edges[:-1], edges[1:]))


def length(path):
    done = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", path],
                          capture_output=True, text=True)
    return float(done.stdout.strip() or 0)


def render(job):
    for name in os.listdir(job):
        path = os.path.join(job, name)
        if name.startswith("media") and os.path.isdir(path):
            shutil.rmtree(path, ignore_errors=True)
        elif name.startswith(("timeline", "marks")) or name == "parts.txt":
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
    deadline = time.time() + RENDER_LIMIT

    durations = json.load(open(os.path.join(job, "durations.json"), encoding="utf-8"))
    parts = spans(sum(durations), PARTS)
    workers = []
    for k, (start, end) in enumerate(parts):
        args = ["-q", QUALITY, "--fps", "30", "--media_dir", f"/manim/media-{k}", "-o", "part"]
        workers.append(manim(job, args, {"NOAH_SPAN": f"{start},{end}", "NOAH_MARKS": f"marks-{k}.json"}, "1.5", "1500m"))
    failed = None
    for worker in workers:
        try:
            output, _ = worker.communicate(timeout=max(1, deadline - time.time()))
        except subprocess.TimeoutExpired:
            failed = f"Рендер не уложился в {RENDER_LIMIT} с — слишком много анимаций."
            break
        if worker.returncode != 0 and failed is None:
            failed = tail(output)
    if failed:
        for worker in workers:
            if worker.poll() is None:
                worker.kill()
        return failed

    # Склейка по порядку; отметки сцен — от начала своего куска плюс
    # настоящая длина кусков до него, до кадра.
    pieces, starts, offset = [], {0: 0.0}, 0.0
    for k in range(len(parts)):
        found = [os.path.join(base, "part.mp4") for base, _, files in os.walk(os.path.join(job, f"media-{k}")) if "part.mp4" in files]
        try:
            marks = json.load(open(os.path.join(job, f"marks-{k}.json"), encoding="utf-8"))
        except (OSError, ValueError):
            marks = []
        for scene, since in marks:
            starts[scene] = round(offset + since, 3)
        if found:
            pieces.append(found[0])
            offset += length(found[0])
    if not pieces:
        return "Рендер не оставил видео."
    listing = os.path.join(job, "parts.txt")
    with open(listing, "w", encoding="utf-8") as out:
        out.writelines(f"file '{piece}'\n" for piece in pieces)
    target = os.path.join(job, "media", "videos")
    os.makedirs(target, exist_ok=True)
    joined = subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", listing, "-c", "copy",
                             os.path.join(target, "video.mp4")], capture_output=True, text=True)
    if joined.returncode != 0:
        return "Куски рендера не склеились: " + joined.stderr[-400:]
    timeline = [starts[n] for n in range(max(starts) + 1) if n in starts]
    if len(timeline) == max(starts) + 1:
        write(os.path.join(job, "timeline.json"), timeline)
    for k in range(len(parts)):
        shutil.rmtree(os.path.join(job, f"media-{k}"), ignore_errors=True)
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


# ── Аудиодрамы ───────────────────────────────────────────────────────────
#
# Задание с drama.json — сценарий аудиодрамы (platform/server/drama.mjs).
# Каждая реплика — своим голосом, подача — темпом, высотой и громкостью;
# между репликами — короткие паузы, между сценами — длиннее, ремарки читает
# рассказчик. Серия — episode-N.mp3 и timeline-N.json: кто и когда говорит,
# чтобы окно подсвечивало реплику и по вопросу возвращало к сцене.

# Голос: (диктор Silero, темп, высота).
CAST = {
    "junior": ("aidar", 1.10, 1.03),
    "lena": ("baya", 1.0, 1.0),
    "palych": ("eugene", 0.93, 0.88),
    "narrator": ("xenia", 0.96, 1.0),
}
# Подача: (темп, высота, громкость).
DELIVERY = {
    "быстро": (1.12, 1.0, 1.0),
    "на нервах": (1.16, 1.04, 1.05),
    "шёпотом": (0.95, 1.0, 0.55),
    "спокойно": (0.96, 1.0, 1.0),
    "с иронией": (1.0, 1.02, 1.0),
    "хрипло": (0.95, 0.94, 1.0),
    "зловеще": (0.9, 0.92, 0.95),
    "радостно": (1.06, 1.05, 1.05),
    "растерянно": (1.0, 1.02, 0.95),
    "твёрдо": (0.97, 0.98, 1.05),
}
GAP = 0.28
QUESTION_GAP = 0.45
SCENE_GAP = 1.1
RATE = 48000


def wav_length(path):
    with wave.open(path) as sound:
        return sound.getnframes() / sound.getframerate()


def silence(path, seconds):
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "lavfi", "-i", f"anullsrc=r={RATE}:cl=mono",
                    "-t", f"{seconds:.3f}", "-c:a", "pcm_s16le", path], check=True)


def voiced(text, speaker, delivery, raw, out):
    """Реплика голосом персонажа: синтез и обработка подачей."""
    voice_name, tempo, pitch = CAST.get(speaker, CAST["narrator"])
    d_tempo, d_pitch, volume = DELIVERY.get(delivery, (1.0, 1.0, 1.0))
    body = json.dumps({"text": text, "speaker": voice_name, "rate": 1.0}).encode("utf-8")
    request = urllib.request.Request(VOICE, data=body, headers={"Content-Type": "application/json"})
    with open(raw, "wb") as file:
        file.write(urllib.request.urlopen(request, timeout=180).read())
    with wave.open(raw) as sound:
        source_rate = sound.getframerate()
    pitch *= d_pitch
    tempo *= d_tempo
    # asetrate меняет высоту вместе со скоростью; atempo возвращает скорость
    # к нужной. Шёпот — ещё и без низов.
    chain = [f"asetrate={int(source_rate * pitch)}", f"aresample={RATE}", f"atempo={max(0.5, min(2.0, tempo / pitch)):.3f}", f"volume={volume:.2f}"]
    if delivery == "шёпотом":
        chain.append("highpass=f=300")
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", raw, "-af", ",".join(chain), "-ac", "1", "-ar", str(RATE),
                    "-c:a", "pcm_s16le", out], check=True)
    os.remove(raw)
    return wav_length(out)


def drama_episode(job, episode):
    """Серия: реплики по порядку, паузы, разметка по времени, MP3."""
    n = episode["n"]
    folder = os.path.join(job, f"voice-{n}")
    os.makedirs(folder, exist_ok=True)
    pieces, timeline, at = [], [], 0.0
    gaps = {}

    def gap(seconds):
        key = f"{seconds:.2f}"
        if key not in gaps:
            gaps[key] = os.path.join(folder, f"gap-{key}.wav")
            silence(gaps[key], seconds)
        return gaps[key]

    def add(text, speaker, delivery, scene, line, after):
        nonlocal at
        text = str(text or "").strip()
        if not text:
            return
        index = len(timeline)
        out = os.path.join(folder, f"{index:03d}.wav")
        length = voiced(text, speaker, delivery, os.path.join(folder, f"{index:03d}.raw.wav"), out)
        timeline.append({"scene": scene, "line": line, "speaker": speaker, "start": round(at, 2), "end": round(at + length, 2)})
        pieces.append(out)
        pieces.append(gap(after))
        at += length + after

    if episode.get("recap"):
        add(episode["recap"], "narrator", "спокойно", "", "recap", SCENE_GAP)
    for scene in episode.get("scenes", []):
        add(scene.get("setting"), "narrator", "спокойно", scene.get("id", ""), "setting", 0.6)
        lines = scene.get("lines", [])
        for k, line in enumerate(lines):
            last = k == len(lines) - 1
            text = str(line.get("spoken", ""))
            pause = SCENE_GAP if last else (QUESTION_GAP if text.rstrip().endswith("?") else GAP)
            add(text, line.get("speaker"), line.get("delivery"), scene.get("id", ""), line.get("id", ""), pause)
    if episode.get("hook"):
        add(episode["hook"], "narrator", "зловеще", "", "hook", 0.5)

    stage(job, "mix")
    listing = os.path.join(folder, "list.txt")
    with open(listing, "w", encoding="utf-8") as out:
        out.writelines(f"file '{piece}'\n" for piece in pieces)
    target = os.path.join(job, f"episode-{n}.mp3")
    done = subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", listing,
                           "-c:a", "libmp3lame", "-b:a", "64k", "-ac", "1", target + ".tmp.mp3"], capture_output=True, text=True)
    if done.returncode != 0:
        raise RuntimeError("сведение серии: " + done.stderr[-400:])
    os.replace(target + ".tmp.mp3", target)
    os.chmod(target, 0o644)
    write(os.path.join(job, f"timeline-{n}.json"), timeline)
    shutil.rmtree(folder, ignore_errors=True)
    write(os.path.join(job, f"done-{n}.json"), {"seconds": round(at, 1)})
    return at


def run_drama(job):
    started = time.time()
    try:
        drama = json.load(open(os.path.join(job, "drama.json"), encoding="utf-8"))
        for episode in drama.get("episodes", []):
            if os.path.exists(os.path.join(job, f"episode-{episode['n']}.mp3")):
                continue
            stage(job, "voice")
            seconds = drama_episode(job, episode)
            log(f"{os.path.basename(job)}: серия {episode['n']} озвучена за {time.time() - started:.0f} с, звучит {seconds:.0f} с")
    except Exception as err:  # голос, ffmpeg, диск — сайту нужен ответ
        write(os.path.join(job, "error.json"), {"log": f"{type(err).__name__}: {err}"})
        log(f"{os.path.basename(job)}: аудиодрама не озвучилась: {err}")


def run(job):
    started = time.time()
    marks = []
    try:
        stage(job, "voice")
        voice(job)
        marks.append(f"голос {time.time() - started:.0f}")
        stage(job, "render")
        failed = render(job)
        marks.append(f"рендер {time.time() - started:.0f}")
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
        log(f"{os.path.basename(job)}: готово за {time.time() - started:.0f} с ({', '.join(marks)}), {size // 1024} КБ")


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
            if os.path.exists(os.path.join(job, "drama.json")):
                run_drama(job)
            else:
                run(job)
        time.sleep(2)


if __name__ == "__main__":
    sys.exit(main())
