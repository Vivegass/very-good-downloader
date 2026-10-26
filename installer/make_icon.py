"""Рисует app.ico: скруглённый квадрат с градиентом и стрелка «скачать»."""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

S = 1024
out = Path(sys.argv[1])

grad = Image.new("RGBA", (S, S))
px = grad.load()
c1, c2 = (124, 58, 237), (236, 72, 153)        # фиолетовый → розовый, по диагонали
for y in range(S):
    for x in range(S):
        t = (x + y) / (2 * S - 2)
        px[x, y] = tuple(int(a + (b - a) * t) for a, b in zip(c1, c2)) + (255,)

mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle((40, 40, S - 40, S - 40), radius=220, fill=255)
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
img.paste(grad, (0, 0), mask)

d = ImageDraw.Draw(img)
white = (255, 255, 255, 255)
d.rounded_rectangle((452, 210, 572, 560), radius=40, fill=white)              # стержень стрелки
d.polygon([(292, 500), (732, 500), (512, 740)], fill=white)                  # наконечник
d.rounded_rectangle((262, 790, 762, 870), radius=40, fill=white)             # «полка»

img.save(out, sizes=[(256, 256), (128, 128), (64, 64), (48, 48), (32, 32), (24, 24), (16, 16)])
img.resize((256, 256), Image.LANCZOS).save(out.with_suffix(".png"))
print("icon:", out)
