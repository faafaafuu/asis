"""Голосовой сервер Суфлёра: Светлана (нейроголос Microsoft) и Silero.

Каждая фраза — один HTTP-запрос:
POST /tts {"text": ..., "speaker": ..., "rate": 1.0, "format": "mp3"?} -> звук.
GET /health -> 200, когда сервер готов.

Светлана и Дмитрий — нейроголоса Microsoft через edge-tts: живая интонация,
нужен интернет. Остальные — Silero на этом компьютере (PyTorch и модель).
Нет PyTorch — работают только нейроголоса; Microsoft не ответил — фразу
читает Ксения, если Silero стоит. Звук — WAV 48 кГц; нейроголос без ffmpeg
рядом — MP3, как его отдаёт Microsoft (программа разбирает оба).

Слушает только 127.0.0.1. Через IDLE_SECONDS без запросов выходит сам —
держать сотни мегабайт памяти, когда голос не нужен, незачем.
"""

import array
import asyncio
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from importlib import metadata
from xml.sax.saxutils import escape

try:
    import torch
except ImportError:  # поставлены только нейроголоса — без PyTorch
    torch = None
try:
    import edge_tts
except ImportError:
    edge_tts = None

MODEL_PATH, PORT = sys.argv[1], int(sys.argv[2])
# Третий параметр — сколько ждать без запросов; 0 — не уходить вовсе. Так
# сервер работает на сайте, где его держит служба, а не программа.
IDLE_SECONDS = int(sys.argv[3]) if len(sys.argv) > 3 else 600
SAMPLE_RATE = 48000
SPEAKERS = {"aidar", "baya", "kseniya", "xenia", "eugene"}
EDGE = {"svetlana": "ru-RU-SvetlanaNeural", "dmitry": "ru-RU-DmitryNeural"}
FFMPEG = shutil.which("ffmpeg")

model = None
if torch is not None and os.path.exists(MODEL_PATH):
    torch.set_num_threads(4)
    model = torch.package.PackageImporter(MODEL_PATH).load_pickle("tts_models", "model")
    model.to(torch.device("cpu"))
if model is None and edge_tts is None:
    sys.exit("нет голосов: ни Silero (PyTorch и модель), ни edge-tts")
lock = threading.Lock()
last_used = time.monotonic()
# Обновили edge-tts — сервер уходит, новая версия подхватится при запуске.
restart = threading.Event()
upgrading = threading.Event()


def rate_word(rate: float) -> str:
    """Silero понимает скорость словами, а не множителем."""
    if rate < 0.8:
        return "slow"
    if rate < 1.15:
        return "medium"
    if rate < 1.5:
        return "fast"
    return "x-fast"


# Текст для чтения вслух — то же, что src/speech.rs в программе. Модель
# молча выбрасывает цифры («порт 443 открыт» звучит «порт открыт»), а
# латиницу не знает вовсе. Числа — словами, с процентами и единицами;
# термины — по словарю pronounce.json (рядом со скриптом) и выученным словам
# (NOAH_PRONOUNCE_LEARNED); аббревиатуры — по английским буквам; пути и имена
# команд — по частям; незнакомое слово — по звучанию.
HERE = os.path.dirname(os.path.abspath(__file__))
try:
    with open(os.path.join(HERE, "pronounce.json"), encoding="utf-8") as source:
        _DICT = json.load(source)
except (OSError, ValueError):
    _DICT = {"words": {}, "letters": {}}
WORDS = {k.lower(): v for k, v in _DICT.get("words", {}).items()}
LETTER_NAMES = _DICT.get("letters", {})
LEARNED_PATH = os.environ.get("NOAH_PRONOUNCE_LEARNED", "")
_learned = {"at": 0.0, "words": {}}


def known(word: str):
    lower = word.lower()
    if lower in WORDS:
        return WORDS[lower]
    if LEARNED_PATH:
        try:
            at = os.path.getmtime(LEARNED_PATH)
            if at != _learned["at"]:
                with open(LEARNED_PATH, encoding="utf-8") as source:
                    _learned["words"] = {k.lower(): v for k, v in json.load(source).items()}
                _learned["at"] = at
        except (OSError, ValueError):
            pass
    return _learned["words"].get(lower)


ONES = ["ноль", "один", "два", "три", "четыре", "пять", "шесть", "семь", "восемь", "девять", "десять", "одиннадцать",
        "двенадцать", "тринадцать", "четырнадцать", "пятнадцать", "шестнадцать", "семнадцать", "восемнадцать", "девятнадцать"]
TENS = ["", "", "двадцать", "тридцать", "сорок", "пятьдесят", "шестьдесят", "семьдесят", "восемьдесят", "девяносто"]
HUNDREDS = ["", "сто", "двести", "триста", "четыреста", "пятьсот", "шестьсот", "семьсот", "восемьсот", "девятьсот"]


def plural(n: int, one: str, few: str, many: str) -> str:
    if 11 <= n % 100 <= 14:
        return many
    if n % 10 == 1:
        return one
    if 2 <= n % 10 <= 4:
        return few
    return many


def _hundreds(n: int, feminine: bool, out: list):
    h, rest = divmod(n, 100)
    if h:
        out.append(HUNDREDS[h])
    unit = lambda d: ("одна" if d == 1 else "две" if d == 2 else ONES[d]) if feminine else ONES[d]
    if rest >= 20:
        out.append(TENS[rest // 10])
        if rest % 10:
            out.append(unit(rest % 10))
    elif rest:
        out.append(unit(rest))


def number(n: int) -> str:
    if n == 0:
        return ONES[0]
    out = []
    for size, one, few, many, feminine in [
        (10 ** 12, "триллион", "триллиона", "триллионов", False),
        (10 ** 9, "миллиард", "миллиарда", "миллиардов", False),
        (10 ** 6, "миллион", "миллиона", "миллионов", False),
        (1000, "тысяча", "тысячи", "тысяч", True),
    ]:
        count, n = divmod(n, size)
        if count:
            _hundreds(count % 1000, feminine, out)
            out.append(plural(count, one, few, many))
    _hundreds(n, False, out)
    return " ".join(out)


UNITS = {}
for _names, _forms in [
    (["%"], ("процент", "процента", "процентов")),
    (["ГБ", "Гб", "GB", "Gb", "G", "Gi", "GiB"], ("гигабайт", "гигабайта", "гигабайт")),
    (["МБ", "Мб", "MB", "Mb", "M", "Mi", "MiB"], ("мегабайт", "мегабайта", "мегабайт")),
    (["КБ", "Кб", "KB", "Kb", "K", "Ki", "KiB"], ("килобайт", "килобайта", "килобайт")),
    (["ТБ", "Тб", "TB", "Tb"], ("терабайт", "терабайта", "терабайт")),
    (["мс", "ms"], ("миллисекунда", "миллисекунды", "миллисекунд")),
    (["ГГц", "GHz"], ("гигагерц", "гигагерца", "гигагерц")),
    (["МГц", "MHz"], ("мегагерц", "мегагерца", "мегагерц")),
    (["°C", "°С", "°"], ("градус", "градуса", "градусов")),
    (["₽", "руб"], ("рубль", "рубля", "рублей")),
    (["€"], ("евро", "евро", "евро")),
    (["vCPU", "CPU"], ("ядро", "ядра", "ядер")),
]:
    for _name in _names:
        UNITS[_name] = _forms

PAIRS = [
    ("sch", "ш"), ("tch", "ч"), ("sh", "ш"), ("ch", "ч"), ("zh", "ж"), ("th", "т"), ("ph", "ф"), ("kh", "х"), ("ts", "ц"),
    ("ck", "к"), ("oo", "у"), ("ee", "и"), ("ea", "и"), ("ou", "ау"), ("qu", "кв"), ("ya", "я"), ("yu", "ю"), ("yo", "ё"), ("wh", "в"),
]
LATIN = dict(zip("abcdefghijklmnopqrstuvwxyz", [
    "а", "б", "к", "д", "е", "ф", "г", "х", "и", "дж", "к", "л", "м",
    "н", "о", "п", "к", "р", "с", "т", "у", "в", "в", "кс", "и", "з",
]))


def transliterate(word: str) -> str:
    lower = word.lower()
    if len(lower) > 3 and lower.endswith("e") and not lower.endswith("ee") and lower[-2] not in "aeiouy":
        lower = lower[:-1]
    out, i = "", 0
    if lower.startswith("e") and not lower.startswith(("ee", "ea")):
        out, i = "э", 1
    while i < len(lower):
        for latin, cyr in PAIRS:
            if lower.startswith(latin, i):
                out += cyr
                i += len(latin)
                break
        else:
            ch = lower[i]
            out += "с" if ch == "c" and lower[i + 1:i + 2] in ("e", "i", "y") else LATIN.get(ch, "")
            i += 1
    return out


def spell(word: str) -> str:
    return "-".join(LETTER_NAMES[c] for c in word.lower() if c in LETTER_NAMES)


def parts(word: str):
    return re.findall(r"[0-9]+|[A-Z]+(?![a-z])|[A-Z]?[a-z]+|[^A-Za-z0-9]+", word) or [word]


def read_word(word: str) -> str:
    found = known(word)
    if found:
        return found
    if word.isdigit():
        return number(int(word)) if len(word) <= 15 else " ".join(ONES[int(d)] for d in word)
    pieces = [p for p in parts(word) if p]
    if len(pieces) > 1:
        return " ".join(read_word(p) for p in pieces)
    letters = sum(c.isalpha() for c in word)
    upper = sum(c.isupper() for c in word)
    if letters == 1 or (upper == letters and letters <= 6):
        return spell(word)
    if not any(c in "aeiouy" for c in word.lower()):
        return spell(word)
    return transliterate(word)


def read_chunk(chunk: str) -> str:
    found = known(chunk)
    if found:
        return found
    sides = chunk.split("/")
    if len(sides) == 2 and not chunk.startswith("/") and all(s.isalpha() and s.isascii() for s in sides):
        return " или ".join(read_word(s) for s in sides)
    chunk = chunk.replace("++", " плюс плюс ").replace("+", " плюс ").replace("#", " шарп ")
    return " ".join(read_word(p) if re.search(r"[A-Za-z0-9]", p) else p
                    for p in re.split(r"[/._\-\\:~@=\s]+", chunk) if p)


NUMBER = re.compile(r"(?<![A-Za-z0-9])(\$)?(-(?=\d))?(\d+(?:[.,:]\d+)*)(?:( ?)(%|°C|°С|°|[A-Za-zА-Яа-я₽€]+))?")
CHUNK = re.compile(r"(?:[/~](?=[A-Za-z/]))?[A-Za-z0-9][A-Za-z0-9+#._\-/\\:~@=]*")


def _number(match: re.Match) -> str:
    dollar, minus, digits, space, unit = match.groups()
    tail = ""
    if unit and unit not in UNITS:
        # Не единица — число приклеено к имени («3s», «2xx») или просто стоит
        # перед словом: слово остаётся как было.
        if not space:
            return read_chunk(digits + unit)
        tail = space + unit
        unit = None
    groups = re.split(r"([.,:])", digits)
    spoken = ["минус"] if minus else []
    for at in range(0, len(groups), 2):
        group = groups[at]
        spoken.append(" ".join(ONES[int(d)] for d in group) if (len(group) > 1 and group.startswith("0")) or len(group) > 15 else number(int(group)))
        if at + 1 < len(groups) and groups[at + 1] != ":":
            spoken.append("запятая" if groups[at + 1] == "," else "точка")
    last = int(groups[0]) if len(groups) == 1 else 5
    text = " ".join(spoken)
    if dollar:
        text += " " + plural(last, "доллар", "доллара", "долларов")
    if unit:
        text += " " + plural(last, *UNITS[unit])
    return text + tail


def _chunk(match: re.Match) -> str:
    chunk = match.group(0)
    trail = re.search(r"[.,:\-/=]+$", chunk)
    end = trail.group(0) if trail else ""
    core = chunk[: len(chunk) - len(end)] if end else chunk
    if not re.search(r"[A-Za-z]", core):
        return chunk
    return read_chunk(core) + end


def speakable(text: str) -> str:
    text = NUMBER.sub(_number, text)
    text = CHUNK.sub(_chunk, text)
    text = text.replace("&", " и ").replace("→", " — ").replace("->", " — ").replace("№", "номер ")
    text = re.sub(r"[|{}\[\]<>\\^]", " ", text)
    return re.sub(r"[ \t]{2,}", " ", text).strip()


def chunks(text: str, limit: int = 800):
    """Длинный текст — кусками по границам предложений: у модели есть предел."""
    part = ""
    pieces = re.split(r"(?<=[.!?…:])(\s+)", text.strip())
    # Разделители сохраняются: перенос строки — граница пункта, по ней
    # marked() ставит паузу. Склеенные пробелом пункты звучали одной кашей.
    for at in range(0, len(pieces), 2):
        sentence = pieces[at]
        glue = "\n" if at > 0 and "\n" in pieces[at - 1] else " "
        if part and len(part) + len(sentence) + 1 > limit:
            yield part
            part = ""
        part = f"{part}{glue}{sentence}" if part else sentence
    if part:
        yield part


def marked(part: str, rate: float) -> str:
    """Разметка для живой интонации: предложения — <s>, между абзацами — пауза.

    Без неё модель читала текст сплошным потоком, ровно: интонация не
    опускалась в конце предложения и не начиналась заново в следующем.
    <p> внутри <prosody> модель v5 не разбирает — абзац отмечается паузой.
    """
    paragraphs = []
    for paragraph in re.split(r"\n\s*\n|\n", part):
        sentences = [s for s in re.split(r"(?<=[.!?…])\s+", paragraph.strip()) if s]
        if sentences:
            paragraphs.append("".join(f"<s>{escape(s)}</s>" for s in sentences))
    # Между пунктами и абзацами — заметная пауза: шаг закончился, начинается
    # следующий.
    body = '<break time="550ms"/>'.join(paragraphs) or escape(part)
    return f'<speak><prosody rate="{rate_word(rate)}">{body}</prosody></speak>'


def tts(ssml: str, speaker: str):
    """Ударения и «ё» — моделью: неверное ударение сильнее всего выдаёт робота.
    Старые модели этих настроек не знают — тогда без них."""
    try:
        return model.apply_tts(
            ssml_text=ssml, speaker=speaker, sample_rate=SAMPLE_RATE,
            put_accent=True, put_yo=True, put_stress_homo=True, put_yo_homo=True,
        )
    except TypeError:
        return model.apply_tts(ssml_text=ssml, speaker=speaker, sample_rate=SAMPLE_RATE)


def upgrade_edge():
    """Microsoft время от времени меняет подпись запросов, и edge-tts догоняет
    её новой версией. Отказ — раз за запуск пробуем обновить библиотеку;
    вышла новая — сервер перезапускается (программа или служба поднимут)."""
    if upgrading.is_set():
        return
    upgrading.set()

    def run():
        before = metadata.version("edge-tts")
        subprocess.run(
            [sys.executable, "-m", "pip", "install", "-q", "-U", "--disable-pip-version-check",
             "--no-warn-script-location", "edge-tts"],
            capture_output=True, timeout=300,
        )
        if metadata.version("edge-tts") != before:
            print(f"edge-tts {before} -> {metadata.version('edge-tts')}, перезапуск", file=sys.stderr, flush=True)
            restart.set()

    threading.Thread(target=run, daemon=True).start()


def better_quality():
    """MP3 96 кбит/с вместо 48: edge-tts просит у Microsoft 48 кбит/с, и в
    наушниках это слышно — шипящие и верха «как в телефонной трубке». Формат
    вшит в библиотеку строкой — подменяем её в коде. Не нашли (библиотека
    изменилась) — остаётся как было."""
    target = "audio-24khz-48kbitrate-mono-mp3"

    def patched(code):
        consts = []
        changed = False
        for const in code.co_consts:
            if isinstance(const, str) and target in const:
                const, changed = const.replace(target, "audio-24khz-96kbitrate-mono-mp3"), True
            elif hasattr(const, "co_consts"):
                inner = patched(const)
                changed = changed or inner is not const
                const = inner
            consts.append(const)
        return code.replace(co_consts=tuple(consts)) if changed else code

    try:
        from edge_tts import communicate
        for name, value in vars(communicate.Communicate).items():
            if hasattr(value, "__code__"):
                value.__code__ = patched(value.__code__)
    except Exception as err:  # качество — не повод не говорить
        print(f"качество MP3 не поднять: {err}", file=sys.stderr, flush=True)


def edge_speech(text: str, speaker: str, rate: float, raw: bool = False):
    """Нейроголос Microsoft: MP3 от Microsoft, с ffmpeg рядом — WAV 48 кГц.
    Скорость — чуть бодрее заданной, тон чуть выше: так Светлана звучит живее."""
    # Пункты списка — с паузой: строка без точки в конце читалась слитно со следующей.
    words = re.sub(r"(?<![.!?…:;,])\n", ".\n", speakable(text))
    if not words:
        raise ValueError("пустой текст")
    percent = round((rate - 1) * 60 + 6)

    async def stream():
        audio = bytearray()
        talk = edge_tts.Communicate(words, EDGE.get(speaker, speaker), rate=f"{percent:+d}%", pitch="+2Hz")
        async for chunk in talk.stream():
            if chunk["type"] == "audio":
                audio += chunk["data"]
        return bytes(audio)

    mp3 = asyncio.run(asyncio.wait_for(stream(), 40))
    if not mp3:
        raise ValueError("Microsoft не прислал звук")
    # raw — как прислал Microsoft: телефону и браузеру MP3 и нужен, а
    # перекодирование только портит звук.
    if raw or not FFMPEG:
        return mp3, "audio/mpeg"
    # Через файлы, а не трубу: в трубу ffmpeg пишет WAV без длины, и чтение
    # длительности (ролики, аудиодрамы) ломается.
    with tempfile.TemporaryDirectory() as folder:
        source, target = os.path.join(folder, "in.mp3"), os.path.join(folder, "out.wav")
        with open(source, "wb") as file:
            file.write(mp3)
        subprocess.run([FFMPEG, "-loglevel", "error", "-y", "-i", source, "-ar", str(SAMPLE_RATE), "-ac", "1", target],
                       check=True, timeout=60)
        with open(target, "rb") as file:
            return file.read(), "audio/wav"


def edge_module():
    """edge-tts — и тогда, когда её поставили уже после запуска сервера:
    программа ставит Светлану в фоне, а сервер к тому времени уже работал и
    молча читал Ксенией вместо неё."""
    global edge_tts
    if edge_tts is None:
        try:
            import edge_tts as module
            edge_tts = module
        except ImportError:
            return None
    if not getattr(edge_module, "tuned", False):
        better_quality()
        edge_module.tuned = True
    return edge_tts


def synthesize(text: str, speaker: str, rate: float, raw: bool = False):
    """Звук фразы и его тип. Нейроголос не вышел — Ксения, если Silero стоит."""
    neural = speaker in EDGE or speaker.endswith("Neural")
    if neural or model is None:
        if edge_module() is None:
            print("edge-tts не стоит — читает Ксения", file=sys.stderr, flush=True)
            speaker = "xenia"
        else:
            try:
                return edge_speech(text, speaker if neural else "svetlana", rate, raw)
            except Exception as err:
                if model is None:
                    upgrade_edge()
                    raise
                print(f"нейроголос не вышел ({err}) — читает Ксения", file=sys.stderr, flush=True)
                upgrade_edge()
                speaker = "xenia"
    return silero_wav(text, speaker, rate), "audio/wav"


def silero_wav(text: str, speaker: str, rate: float) -> bytes:
    speaker = speaker if speaker in SPEAKERS else "xenia"
    pieces = []
    with lock:
        for part in chunks(speakable(text)):
            pieces.append(tts(marked(part, rate), speaker))
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
            sound, kind = synthesize(str(body.get("text", "")), str(body.get("speaker", "")), float(body.get("rate", 1.0)),
                                     body.get("format") == "mp3")
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
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(sound)))
        self.end_headers()
        self.wfile.write(sound)


def watch_idle(server):
    while True:
        time.sleep(15)
        if restart.is_set() or IDLE_SECONDS and time.monotonic() - last_used > IDLE_SECONDS:
            server.shutdown()
            return


server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
threading.Thread(target=watch_idle, args=(server,), daemon=True).start()
server.serve_forever()
# Ушли ради новой edge-tts — с кодом ошибки: служба на сайте поднимет заново.
sys.exit(3 if restart.is_set() else 0)
