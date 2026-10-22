"""Детали роликов Ноа в стиле «Неон». Модель собирает ролик из них, а не
рисует с нуля: так ролики выходят одинаково аккуратными.

Сцена наследует NoahScene: self.go(*анимации, rt=…) — сыграть и учесть
время, self.hold(i) — дождаться конца озвучки сцены i, self.say(текст) —
субтитр, self.focus(блок) — выделить блок, остальное приглушить."""
import json
import os

from manim import *

BG = "#060809"
CY = "#3CE3F0"
MG = "#D45CC6"
TX = "#F2FDFF"
MU = "#8FB6BD"
OK = "#7DFCC6"
GOLD = "#FFD98A"
HEAD = "Exo 2"
MONO = "JetBrains Mono"

config.background_color = BG

# Рендер кусками: служба запускает несколько процессов, каждому —
# NOAH_SPAN="начало,конец" в секундах сцены по её собственным часам. Процесс
# рисует только анимации, начавшиеся в этом промежутке; до него сцена
# проигрывается без кадров, после — останавливается. Часы — сумма заявленных
# длительностей, а не время рендера: пропущенная анимация продвигает время
# ровно на свою длину, нарисованная — целыми кадрами, и границы у процессов
# разошлись бы.
SPAN = tuple(float(x) for x in os.environ["NOAH_SPAN"].split(",")) if os.environ.get("NOAH_SPAN") else None

if SPAN:
    from manim.renderer.cairo_renderer import CairoRenderer
    from manim.utils.exceptions import EndSceneEarlyException

    _update_frame = CairoRenderer.update_frame
    _save_static = CairoRenderer.save_static_frame_data

    def _update_frame_lazy(self, *args, **kwargs):
        # Кадры пропускаемых анимаций не пишутся — и рисовать их незачем.
        if self.skip_animations:
            return None
        return _update_frame(self, *args, **kwargs)

    def _save_static_lazy(self, scene, static_mobjects):
        if self.skip_animations:
            self.static_image = None
            return None
        return _save_static(self, scene, static_mobjects)

    CairoRenderer.update_frame = _update_frame_lazy
    CairoRenderer.save_static_frame_data = _save_static_lazy


def icon(name, color, size=0.6):
    """Иконка в квадратной ячейке size×size: плоская не растягивается."""
    mob = SVGMobject(f"icons/{name}.svg")
    mob.set_fill(opacity=0).set_stroke(color, width=3.2)
    mob.scale(size * 0.8 / max(mob.width, mob.height))
    cell = Square(side_length=size, stroke_width=0, fill_opacity=0)
    return VGroup(cell, mob.move_to(cell))


def glow(mob, color, layers=5, width=14):
    """Неоновое свечение: копии контура всё шире и прозрачнее."""
    return VGroup(*[
        mob.copy().set_fill(opacity=0).set_stroke(color, width=width * (i + 1) / layers, opacity=0.07 * (layers - i) / layers)
        for i in range(layers)
    ])


def card(title, sub, icon_name, color=CY, kind="контейнер", w=3.0, h=1.8):
    """Блок: шапка с типом, иконка, название, подпись, свечение."""
    frame = RoundedRectangle(corner_radius=0.14, width=w, height=h, stroke_color=color, stroke_width=2.6,
                             fill_color=color, fill_opacity=0.07)
    strip = RoundedRectangle(corner_radius=0.12, width=w - 0.06, height=0.4, stroke_width=0, fill_color=color, fill_opacity=0.2)
    strip.align_to(frame, UP).shift(DOWN * 0.03)
    tag = Text(kind.upper(), font=MONO, font_size=15, color=color).move_to(strip).align_to(frame, LEFT).shift(RIGHT * 0.22)
    pic = icon(icon_name, color, 0.62)
    name = Text(title, font=HEAD, weight=BOLD, font_size=30, color=TX)
    note = Text(sub, font=MONO, font_size=18, color=MU)
    words = VGroup(name, note).arrange(DOWN, aligned_edge=LEFT, buff=0.1)
    body = VGroup(pic, words).arrange(RIGHT, buff=0.3)
    if body.width > w - 0.45:
        body.scale_to_fit_width(w - 0.45)
    body.move_to(frame).shift(DOWN * 0.18)
    return VGroup(glow(frame, color), frame, strip, tag, body)


def zone(title, w, h, color=CY):
    """Область — под, узел, кластер: шапка и фоновая сетка."""
    frame = RoundedRectangle(corner_radius=0.2, width=w, height=h, stroke_color=color, stroke_width=1.6,
                             fill_color=color, fill_opacity=0.025)
    dots = VGroup(*[
        Dot([x, y, 0], radius=0.011, color=color, fill_opacity=0.35)
        for x in [i * 0.4 - w / 2 + 0.3 for i in range(int((w - 0.4) / 0.4) + 1)]
        for y in [j * 0.4 - h / 2 + 0.3 for j in range(int((h - 0.9) / 0.4) + 1)]
    ])
    head = Text(title.upper(), font=MONO, font_size=18, color=color)
    head.next_to(frame.get_corner(UL), DR, buff=0.2)
    return VGroup(frame, dots, head)


def badge(n, color=MG):
    circle = Circle(radius=0.22, stroke_width=0, fill_color=color, fill_opacity=1)
    return VGroup(glow(circle, color, 3, 10), circle, Text(str(n), font=MONO, weight=BOLD, font_size=20, color=BG).move_to(circle))


def wire(a, b, text="", color=CY, step=None):
    """Стрелка между блоками, подпись и номер шага. У горизонтальной подпись
    сверху, номер снизу; у вертикальной — подпись справа, номер слева."""
    arrow = Arrow(a, b, buff=0.12, stroke_width=5, color=color, max_tip_length_to_length_ratio=0.1)
    parts = [glow(arrow, color, 3, 12), arrow]
    dx, dy = abs(b[0] - a[0]), abs(b[1] - a[1])
    upright = dy > dx
    if text:
        label = Text(text, font=MONO, font_size=19, color=MU)
        parts.append(label.next_to(arrow, RIGHT if upright else UP, buff=0.16))
    if step is not None:
        parts.append(badge(step).move_to(arrow.get_center() + (LEFT * 0.42 if upright else DOWN * 0.42)))
    return VGroup(*parts)


def packet(text, color=CY):
    words = Text(text, font=MONO, font_size=21, color=color)
    frame = RoundedRectangle(corner_radius=0.1, width=words.width + 0.4, height=0.48, stroke_color=color, stroke_width=2,
                             fill_color=BG, fill_opacity=0.95)
    return VGroup(frame, words.move_to(frame))


class NoahScene(Scene):
    """Сцены по озвучке: длительности — в durations.json."""

    def setup(self):
        self.durations = json.load(open("durations.json", encoding="utf-8"))
        self.spent = 0.0
        self.index = 0
        self.caption = None
        self.lit = None
        # Когда на самом деле началась каждая сцена: по этим отметкам служба
        # рендера ставит озвучку, и звук не уезжает, даже если анимация
        # вышла длиннее речи.
        self.starts = [0.0]
        self.clock = 0.0
        # Кусок: отметки сцен внутри него — от начала куска, по кадрам.
        self.marks = []
        self.part_t0 = None

    def play(self, *args, **kwargs):
        if SPAN:
            start, end = SPAN
            if self.clock >= end - 1e-6:
                raise EndSceneEarlyException()
            mine = self.clock >= start - 1e-6
            self.renderer._original_skipping_status = not mine
            if mine and self.part_t0 is None:
                self.part_t0 = self.renderer.time
        self.keep_in_frame([a.mobject for a in args if isinstance(a, Animation) and getattr(a, "mobject", None) is not None])
        super().play(*args, **kwargs)
        self.clock += self.duration

    def keep_in_frame(self, coming=()):
        """Схема — в кадре и над субтитром. Модель раскладывает блоки по
        координатам и порой выносит крайние за рамку: тогда всё, кроме
        субтитра, ужимается и сдвигается внутрь. Одинаково во всех кусках
        рендера: состояние сцены у них общее."""
        # И то, что анимация только добавит: FadeIn показывает блок, которого на
        # сцене ещё нет, — иначе он полсекунды был бы за рамкой.
        stage = []
        for m in [*self.mobjects, *coming]:
            if m is self.caption or m in stage or getattr(m, "z_index", 0) >= 20 or getattr(m, "backdrop", False):
                continue
            if any(m in other.get_family() for other in stage):
                continue
            stage.append(m)
        if not stage:
            return
        group = Group(*stage)
        if group.width < 1e-3 or group.height < 1e-3:
            return
        left, right = -config.frame_width / 2 + 0.3, config.frame_width / 2 - 0.3
        bottom, top = -config.frame_height / 2 + 1.1, config.frame_height / 2 - 0.25
        if group.get_left()[0] >= left and group.get_right()[0] <= right and group.get_bottom()[1] >= bottom and group.get_top()[1] <= top:
            return
        scale = min(1.0, (right - left) / group.width, (top - bottom) / group.height)
        if scale < 1.0:
            group.scale(scale)
        dx = max(0.0, left - group.get_left()[0]) - max(0.0, group.get_right()[0] - right)
        dy = max(0.0, bottom - group.get_bottom()[1]) - max(0.0, group.get_top()[1] - top)
        group.shift(RIGHT * dx + UP * dy)

    def tear_down(self):
        if SPAN:
            with open(os.environ.get("NOAH_MARKS", "marks.json"), "w", encoding="utf-8") as out:
                json.dump(self.marks, out)
        else:
            with open("timeline.json", "w", encoding="utf-8") as out:
                json.dump(self.starts, out)

    def go(self, *anims, rt=1.0):
        self.play(*anims, run_time=rt)
        self.spent += rt

    def hold(self, index):
        left = self.durations[index] - self.spent
        if left > 0:
            self.wait(left)
        self.spent = 0.0
        self.index = index + 1
        self.starts.append(round(self.renderer.time, 3))
        if SPAN and SPAN[0] - 1e-6 <= self.clock < SPAN[1] - 1e-6:
            since = self.renderer.time - self.part_t0 if self.part_t0 is not None else 0.0
            self.marks.append([len(self.starts) - 1, round(since, 3)])

    def say(self, text):
        """Субтитр внизу: главная мысль сцены."""
        words = Text(text, font=MONO, font_size=24, color=TX)
        back = RoundedRectangle(corner_radius=0.1, width=words.width + 0.6, height=0.6, stroke_width=0, fill_color=BG, fill_opacity=0.85)
        line = VGroup(back, words.move_to(back)).to_edge(DOWN, buff=0.35).set_z_index(20)
        # Прежний субтитр уходит в первой трети, новый появляется после: на
        # одном месте две надписи сразу не читаются.
        anims = [FadeIn(line, shift=UP * 0.15, rate_func=lambda t: smooth(max(0.0, (t - 0.35) / 0.65)))]
        if self.caption is not None:
            anims.append(FadeOut(self.caption, rate_func=lambda t: smooth(min(1.0, t * 3))))
        self.caption = line
        return anims

    def send(self, wire_mob, texts, color=CY, reverse=False):
        """Пакеты данных идут по стрелке — видно, что и куда идёт. Каждый
        сперва стоит на месте, чтобы его прочли, потом плавно едет. Время
        делится поровну из того, что осталось до конца озвучки сцены."""
        arrow = wire_mob[1]
        start, end = (arrow.get_end(), arrow.get_start()) if reverse else (arrow.get_start(), arrow.get_end())
        # Подпись стрелки на время прохода прячется: пакет идёт по самой
        # стрелке, как по проводу, и не закрывает её.
        labels = [part for part in wire_mob if isinstance(part, Text)]
        if labels:
            self.go(*[FadeOut(label) for label in labels], rt=0.2)
        left = self.durations[self.index] - self.spent - 0.5
        each = max(1.6, min(2.6, left / max(len(texts), 1)))
        middle = arrow.get_center()
        room = np.linalg.norm(arrow.get_end() - arrow.get_start()) * 0.92
        for text in texts:
            # Появляется посередине стрелки, в свободном месте, стоит на
            # чтение и уходит в получателя. Шире зазора — ужимается под него.
            item = packet(text, color)
            if item.width > room:
                item.scale_to_fit_width(room)
            item.move_to(middle).set_z_index(15)
            self.go(FadeIn(item, scale=0.7), rt=0.25)
            self.wait(0.6)
            self.spent += 0.6
            self.go(item.animate.move_to(end).scale(0.6).set_opacity(0), rt=each - 0.85)
            self.remove(item)
        if labels:
            self.go(*[FadeIn(label) for label in labels], rt=0.2)

    def focus(self, *active):
        """Выделить то, о чём речь: остальное — под тёмной вуалью. Без
        аргументов — снять выделение."""
        anims = []
        if self.lit is not None:
            veil, raised = self.lit
            anims.append(FadeOut(veil))
            for mob in raised:
                mob.set_z_index(0)
            self.lit = None
        if active:
            veil = Rectangle(width=40, height=24, stroke_width=0, fill_color=BG, fill_opacity=0.68).set_z_index(5)
            veil.backdrop = True
            for mob in active:
                mob.set_z_index(10)
            anims.append(FadeIn(veil))
            self.lit = (veil, active)
        return anims

    # ── Заставка, терминал, итог ──────────────────────────────────────────

    def intro(self, tag, title, sub=""):
        """Заставка: метка, крупное название с неоновой чертой, подзаголовок.
        Играет и возвращает группу — её убирают в начале следующей сцены."""
        head = VGroup(Text(tag.upper(), font=MONO, font_size=22, color=MG), fit(Text(title, font=HEAD, weight=BOLD, font_size=66, color=TX), 12.5))
        rule = Line(LEFT * 3.2, RIGHT * 3.2, color=CY, stroke_width=4)
        head.add(rule)
        if sub:
            head.add(fit(Text(sub, font=MONO, font_size=26, color=MU), 12.5))
        head.arrange(DOWN, buff=0.32)
        shine = glow(rule, CY, 4, 16)
        self.go(FadeIn(head[0], shift=UP * 0.2), Write(head[1]), rt=1.3)
        self.go(GrowFromCenter(rule), FadeIn(shine), *[FadeIn(m) for m in head[3:]], rt=0.7)
        return VGroup(head, shine)

    def clear(self, rt=0.5):
        """Убрать со сцены всё, кроме субтитра."""
        gone = [m for m in self.mobjects if m is not self.caption]
        if gone:
            self.go(*[FadeOut(m) for m in gone], rt=rt)
        self.lit = None

    def type(self, term, lines, color=TX):
        """Набрать строки в терминале: команды — со знаком $, вывод — серым.
        Возвращает группу строк."""
        body = term[1]
        shown = VGroup()
        top = body.get_top() + DOWN * 0.7
        for n, line in enumerate(lines):
            command = line.startswith("$")
            text = Text(line, font=MONO, font_size=22, color=CY if command else MU)
            fit(text, body.width - 0.6)
            text.move_to(top + DOWN * 0.42 * n).align_to(body, LEFT).shift(RIGHT * 0.3)
            shown.add(text)
            if command:
                self.go(AddTextLetterByLetter(text), rt=max(0.5, min(1.6, len(line) * 0.045)))
            else:
                self.go(FadeIn(text, shift=UP * 0.1), rt=0.35)
        return shown

    def points(self, rows, title="итог"):
        """Выводы списком с иконками: rows — [(иконка, цвет, текст)].
        Играет по строке и возвращает группу."""
        lines = VGroup(*[
            VGroup(icon(name, color, 0.55), fit(Text(text, font=MONO, font_size=27, color=TX), 10.5)).arrange(RIGHT, buff=0.3)
            for name, color, text in rows
        ]).arrange(DOWN, aligned_edge=LEFT, buff=0.32)
        head = Text(title.upper(), font=MONO, font_size=22, color=MG).next_to(lines, UP, buff=0.3).align_to(lines, LEFT)
        group = VGroup(head, lines)
        return group

    def show_points(self, group):
        self.go(FadeIn(group[0]), rt=0.4)
        for row in group[1]:
            self.go(FadeIn(row, shift=RIGHT * 0.25), rt=0.5)


def fit(mob, width):
    """Не шире заданного: длинная надпись ужимается."""
    if mob.width > width:
        mob.scale_to_fit_width(width)
    return mob


def terminal(title="терминал", w=7.2, h=3.4):
    """Окно терминала с шапкой — в него набирают команды (self.type)."""
    body = RoundedRectangle(corner_radius=0.14, width=w, height=h, stroke_color=CY, stroke_width=2.2, fill_color="#020407", fill_opacity=1)
    bar = RoundedRectangle(corner_radius=0.12, width=w - 0.06, height=0.42, stroke_width=0, fill_color=CY, fill_opacity=0.12)
    bar.align_to(body, UP).shift(DOWN * 0.03)
    lights = VGroup(*[Circle(radius=0.07, stroke_width=0, fill_color=c, fill_opacity=0.9) for c in (MG, GOLD, OK)]).arrange(RIGHT, buff=0.12)
    lights.move_to(bar).align_to(body, LEFT).shift(RIGHT * 0.25)
    name = Text(title, font=MONO, font_size=16, color=MU).move_to(bar)
    return VGroup(glow(body, CY, 4, 12), body, bar, lights, name)
