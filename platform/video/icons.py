"""Иконки приложения (src/styles/noah/icons.svg) — отдельными SVG для роликов."""
import os
import re
import sys

source, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)
text = open(source, encoding="utf-8").read()
count = 0
for m in re.finditer(r'<symbol id="i-([a-z0-9-]+)" viewBox="([^"]+)"([^>]*)>(.*?)</symbol>', text, re.S):
    name, box, attrs, body = m.groups()
    attrs = attrs.replace('stroke="currentColor"', 'stroke="#FFFFFF"').replace('fill="currentColor"', 'fill="#FFFFFF"')
    with open(os.path.join(out, name + ".svg"), "w", encoding="utf-8") as f:
        f.write(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{box}" width="24" height="24"><g{attrs} stroke-width="1.75">{body}</g></svg>')
    count += 1
print(count, "иконок")
