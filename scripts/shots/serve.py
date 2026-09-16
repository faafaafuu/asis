"""Отдаёт корень репозитория без кеша — для отладки кадров README.

    python scripts/shots/serve.py [порт]
"""

import http.server
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class NoCache(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT), **kwargs)

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass


port = int(sys.argv[1]) if len(sys.argv) > 1 else 8765
http.server.ThreadingHTTPServer(("127.0.0.1", port), NoCache).serve_forever()
