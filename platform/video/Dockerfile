# Рендер роликов Ноа: Manim и шрифты темы «Неон» (JetBrains Mono, Exo 2).
FROM manimcommunity/manim:stable
USER root
COPY fonts/ /usr/share/fonts/truetype/noah/
RUN fc-cache -f >/dev/null
USER manimuser
WORKDIR /manim
