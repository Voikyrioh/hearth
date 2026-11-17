"""Fixtures NEUTRES du mode simulé (HRT-239) : des dégradés abstraits générés, sans texte, sans reproduction d'aucune image
d'éditeur. `python scripts/build-fixture-illustrations.py` réécrit les fichiers de `public/fixtures/illustrations/`.
Seuls quelques jeux ont une fixture : les autres n'en ont volontairement pas (le repli neutre de la page est ainsi testé)."""
from pathlib import Path
from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent.parent / "public" / "fixtures" / "illustrations"
OUT.mkdir(parents=True, exist_ok=True)

def gradient(size, top, bottom, accent):
    w, h = size
    img = Image.new("RGB", size)
    px = img.load()
    for y in range(h):
        t = y / (h - 1)
        row = tuple(round(top[i] * (1 - t) + bottom[i] * t) for i in range(3))
        for x in range(w):
            px[x, y] = row
    draw = ImageDraw.Draw(img)
    draw.ellipse((w * 0.55, h * 0.1, w * 0.95, h * 0.9), outline=accent, width=max(2, w // 120))
    draw.rectangle((w * 0.08, h * 0.62, w * 0.5, h * 0.7), fill=accent)
    return img

FIXTURES = [
    ("steam-105600.jpg", (460, 215), (38, 70, 83), (20, 30, 40), (231, 111, 81)),
    ("steam-892970.jpg", (460, 215), (60, 52, 80), (24, 22, 36), (244, 162, 97)),
    ("agent-minecraft.jpg", (920, 430), (50, 90, 60), (28, 22, 18), (233, 196, 106)),
]
for name, size, top, bottom, accent in FIXTURES:
    gradient(size, top, bottom, accent).save(OUT / name, "JPEG", quality=82)
    print(name, size, (OUT / name).stat().st_size)
