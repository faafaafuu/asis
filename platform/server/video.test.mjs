import { test } from "node:test";
import assert from "node:assert/strict";
import { checkCode, parseVideo } from "./video.mjs";

const reply = `TITLE: Сайдкар в Kubernetes
SCENES:
1. Разберём сайдкар.
2. Под — общая коробка.
3. Итог.
CODE:
\`\`\`python
from noah_kit import *


class Video(NoahScene):
    def construct(self):
        self.intro("урок", "Сайдкар", "")
        self.hold(0)
        self.hold(1)
        self.hold(2)
\`\`\``;

test("ответ модели разбирается на название, фразы и код", () => {
  const parsed = parseVideo(reply);
  assert.equal(parsed.title, "Сайдкар в Kubernetes");
  assert.deepEqual(parsed.scenes, ["Разберём сайдкар.", "Под — общая коробка.", "Итог."]);
  assert.match(parsed.code, /class Video\(NoahScene\)/);
  assert.equal(checkCode(parsed.code, parsed.scenes), "");
});

test("в контейнер не проходит лишнее", () => {
  const { code, scenes } = parseVideo(reply);
  assert.match(checkCode(code.replace("from noah_kit import *", "from noah_kit import *\nimport os"), scenes), /import/);
  assert.match(checkCode(code.replace("self.hold(2)", "open('/etc/passwd')\n        self.hold(2)"), scenes), /open/);
  assert.match(checkCode(code.replace("self.hold(2)", "x = ().__class__\n        self.hold(2)"), scenes), /__class__/);
  assert.match(checkCode(code.replace("        self.hold(2)\n", ""), scenes), /hold/);
  assert.match(checkCode(code.replace("Video(NoahScene)", "Other(Scene)"), scenes), /Video/);
});
