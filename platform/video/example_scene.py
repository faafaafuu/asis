"""Образец ролика для модели: сайдкар в Kubernetes. Шесть сцен — шесть
фраз озвучки; сцена i кончается self.hold(i)."""
from noah_kit import *


class Video(NoahScene):
    def construct(self):
        # 0 — заставка
        self.intro("урок · паттерны kubernetes", "Сайдкар в Kubernetes", "несколько контейнеров в одном поде")
        self.hold(0)
        self.clear()

        # 1 — под и приложение
        pod = zone("pod · default", 8.75, 5.6).move_to(LEFT * 2.5 + UP * 0.25)
        app = card("Go-приложение", "порт :8080", "cpu", CY, "контейнер").move_to(LEFT * 5.15 + UP * 1.15)
        self.go(FadeIn(pod), *self.say("Под — общая коробка для контейнеров"), rt=1.2)
        self.go(FadeIn(app, scale=0.9), rt=0.8)
        self.hold(1)

        # 2 — сайдкар-логгер читает stdout
        logger = card("Логгер", "читает stdout", "log", MG, "сайдкар").move_to(RIGHT * 0.15 + UP * 1.15)
        out = wire(app[1].get_right(), logger[1].get_left(), "stdout", CY, step=1)
        self.go(FadeIn(logger, scale=0.9), *self.say("Логгер-сайдкар читает вывод приложения"), rt=0.8)
        self.go(GrowArrow(out[1]), FadeIn(out[0]), FadeIn(out[2:]), rt=0.6)
        self.send(out, ["GET 200", "WARN db"], CY)
        self.hold(2)

        # 3 — наружу, в CloudWatch
        cloud = card("CloudWatch", "хранилище логов", "cloud", OK, "внешний сервис").move_to(RIGHT * 5.45 + UP * 1.15)
        logs = wire(logger[1].get_right(), cloud[1].get_left(), "логи", OK, step=2)
        self.go(*self.focus(logger, cloud, logs), FadeIn(cloud, scale=0.9), *self.say("Логи уходят наружу — приложение об этом не знает"), rt=0.9)
        self.go(GrowArrow(logs[1]), FadeIn(logs[0]), FadeIn(logs[2:]), rt=0.6)
        self.send(logs, ["логи ×3", "логи ×5"], OK)
        self.hold(3)

        # 4 — экспортер метрик и Prometheus
        exporter = card("Экспортер", "порт :9090", "gauge", MG, "сайдкар").move_to(LEFT * 2.5 + DOWN * 1.55)
        prom = card("Prometheus", "сбор метрик", "chart", OK, "внешний сервис").move_to(RIGHT * 5.45 + DOWN * 1.55)
        pull = wire(prom[1].get_left(), exporter[1].get_right(), "забирает", OK, step=3)
        self.go(*self.focus(exporter, prom, pull), FadeIn(exporter, scale=0.9), *self.say("Prometheus сам забирает метрики с экспортера"), rt=0.9)
        self.go(FadeIn(prom, scale=0.9), GrowArrow(pull[1]), FadeIn(pull[0]), FadeIn(pull[2:]), rt=0.8)
        self.send(pull, ["cpu 42%", "rps 120"], GOLD, reverse=True)
        self.hold(4)

        # 5 — итог: схема сверху, выводы под ней
        scheme = VGroup(pod, app, logger, out, cloud, logs, exporter, prom, pull)
        self.go(*self.focus(), scheme.animate.scale(0.5).move_to(UP * 1.75), FadeOut(self.caption), rt=1.0)
        summary = self.points([
            ("cpu", CY, "один под — три контейнера"),
            ("check", OK, "у каждого своя задача"),
            ("refresh", GOLD, "обновляются отдельно"),
        ]).move_to(DOWN * 1.9)
        self.show_points(summary)
        self.hold(5)
        self.wait(0.6)
