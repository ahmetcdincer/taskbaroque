"""README için tanıtım görselleri üretir (docs/images/).

    python tools/gallery.py

- scene-<ad>.png : manzaranın görev çubuğuyla birlikte görünüşü (varsa altında gece hâli)
- pets-walk.gif, pets-special.gif : bütün petler yürürken / özel hareketlerinde
- show-space.gif, show-football.gif : sahnelerdeki karakterlerin animasyonları

Çizim, src/scene.rs'teki yerleşimin aynısıdır: benzersiz parçalar çubuğun iki yanına dönüşümlü
dizilir, ekran kenarına taşan son parça kesilir.
"""

import json
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs" / "images"
BG = (32, 32, 38, 255)

# ------------------------------------------------------------------ manzaralar

W, H = 1920, 54          # DIP: ekran genişliği, çubuk alanı yüksekliği
PANEL = (640, 12, 640, 40)
S = 2                    # çıktı pikseli / DIP (README'de okunur olsun)


def sprite(dst, img, src, rect, flip=False):
    x, y, w, h = rect
    crop = img.crop((round(src[0]), round(src[1]), round(src[0] + src[2]), round(src[1] + src[3])))
    crop = crop.resize((max(1, round(w * S)), max(1, round(h * S))), Image.LANCZOS)
    if flip:
        crop = crop.transpose(Image.FLIP_LEFT_RIGHT)
    layer = Image.new("RGBA", dst.size)
    layer.alpha_composite(crop, (round(x * S), round(y * S)) if x >= 0 else (0, round(y * S)), (0 if x >= 0 else round(-x * S), 0))
    dst.alpha_composite(layer)


def clip_to(dst, layer, x0, x1):
    mask = Image.new("L", dst.size, 0)
    ImageDraw.Draw(mask).rectangle((round(x0 * S), 0, round(x1 * S), dst.height), fill=255)
    a = Image.composite(layer.getchannel("A"), Image.new("L", dst.size, 0), mask)
    layer.putalpha(a)
    dst.alpha_composite(layer)


def tile(dst, img, sw, sh, x0, x1, y, k):
    layer = Image.new("RGBA", dst.size)
    tw = sw * k
    cx, i = x0, 0
    while cx < x1:
        sprite(layer, img, (0, 0, sw, sh), (cx, y, tw + 0.5, sh * k), i % 2 == 1)
        cx += tw
        i += 1
    clip_to(dst, layer, x0, x1)


def render_scene(name, night=False):
    d = ROOT / "assets" / "scenes" / name
    m = json.loads((d / "scene.json").read_text(encoding="utf-8"))
    if night and not m["night"]:
        return None
    v = "night" if night else "day"
    img = Image.new("RGBA", (W * S, H * S), BG)
    top = PANEL[1]
    sh = H - top
    k = sh / m["h"]
    spans = [(0, PANEL[0] - 8), (PANEL[0] + PANEL[2] + 8, W)]
    caps = m.get("caps") or []
    inner = []
    for a, b in spans:
        cl = caps[0]["w"] * k if a > 1 and caps else 0
        cr = caps[1]["w"] * k if b < W - 1 and caps else 0
        inner.append((a + cl * 0.5, b - cr * 0.5))
    if m["mode"] == "tile":
        main = Image.open(d / f"{v}.png").convert("RGBA")
        for x0, x1 in inner:
            tile(img, main, m["w"], m["h"], x0, x1, top, k)
    else:
        g = m["ground"]
        main = Image.open(d / f"ground_{v}.png").convert("RGBA")
        segs = Image.open(d / f"segs_{v}.png").convert("RGBA")
        for x0, x1 in inner:
            tile(img, main, g["w"], g["h"], x0, x1, H - g["h"] * k, k)
        fade = m["fade"] * k
        cursors = [[1, inner[1][0], 1.0], [0, inner[0][1], -1.0]]
        started = [False, False]
        right = m.get("segments_r") or []
        ci = 0
        for n, seg in enumerate(m["segments"]):
            open_ = [c for c in range(2) if (cursors[(ci + c) % 2][2] > 0 and cursors[(ci + c) % 2][1] < inner[cursors[(ci + c) % 2][0]][1] - 1)
                     or (cursors[(ci + c) % 2][2] < 0 and cursors[(ci + c) % 2][1] > inner[cursors[(ci + c) % 2][0]][0] + 1)]
            if not open_:
                break
            slot = (ci + open_[0]) % 2
            ci = slot + 1
            si, cur, dr = cursors[slot]
            if dr < 0 and len(right) == len(m["segments"]):
                seg = right[n]
            sw = seg["w"] * k
            st = not started[si]
            x = cur - (fade * 0.5 if st else fade) if dr > 0 else cur - sw + (fade * 0.5 if st else fade)
            layer = Image.new("RGBA", img.size)
            sprite(layer, segs, (seg["x"], seg["y"], seg["w"], seg["h"]), (x, top, sw, sh))
            clip_to(img, layer, *inner[si])
            cursors[slot][1] = x + sw if dr > 0 else x
            started[si] = True
    if caps:
        parts = Image.open(d / "parts.png").convert("RGBA")
        for a, b in spans:
            if a > 1:
                c = caps[0]
                sprite(img, parts, (c["x"], c["y"], c["w"], c["h"]), (a, H - c["h"] * k, c["w"] * k, c["h"] * k))
            if b < W - 1:
                c = caps[1]
                sprite(img, parts, (c["x"], c["y"], c["w"], c["h"]), (b - c["w"] * k, H - c["h"] * k, c["w"] * k, c["h"] * k))
    # Görev çubuğunun yerine sade bir hap
    dr = ImageDraw.Draw(img)
    px, py, pw, ph = PANEL
    dr.rounded_rectangle((px * S, py * S, (px + pw) * S, (py + ph) * S), radius=12 * S, fill=(22, 22, 26, 255), outline=(60, 60, 70, 255))
    for i in range(7):
        cx = px + 24 + i * 40
        dr.rounded_rectangle(((cx) * S, (py + 8) * S, (cx + 24) * S, (py + 32) * S), radius=6 * S, fill=(70, 70, 84, 255))
    return img


def halves(img):
    """Ekran görüntüsünü sol yarı üstte, sağ yarı altta olacak şekilde ikiye böler (okunur boyut)."""
    w = img.width // 2
    out = Image.new("RGBA", (w, img.height * 2 + 6 * S), BG)
    out.alpha_composite(img.crop((0, 0, w, img.height)), (0, 0))
    out.alpha_composite(img.crop((w, 0, img.width, img.height)), (0, img.height + 6 * S))
    return out


def scenes():
    names = [p.name for p in sorted((ROOT / "assets" / "scenes").iterdir()) if (p / "scene.json").exists()]
    for name in names:
        for night, suffix in [(False, ""), (True, "-night")]:
            r = render_scene(name, night)
            if r is None:
                continue
            out = halves(r)
            out.convert("RGB").save(DOCS / f"scene-{name}{suffix}.png", optimize=True)
            print(" ", f"scene-{name}{suffix}.png", out.size)


# ------------------------------------------------------------------ petler

PETS = ["chameleon", "firefly", "fox", "hedgehog", "puppy", "cat_orange", "cat_black", "cat_grey", "wolfdog"]
CELL = 96
PET_PX = 80


def flatten(img):
    bg = Image.new("RGBA", img.size, BG)
    bg.alpha_composite(img)
    return bg.convert("RGB")


def pet_gif(anim, frames_out, file):
    sheets = {}
    for p in PETS:
        d = ROOT / "assets" / "pets" / "pixel"
        sheets[p] = (Image.open(d / f"{p}.png").convert("RGBA"), json.loads((d / f"{p}.json").read_text(encoding="utf-8")))
    frames = []
    for f in range(frames_out):
        canvas = Image.new("RGBA", (PET_PX * len(PETS) + 16, PET_PX + 12), BG)
        for i, p in enumerate(PETS):
            img, meta = sheets[p]
            a = meta["anims"][anim]
            n = a["frames"]
            cell = img.crop(((f % n) * CELL, a["row"] * CELL, (f % n + 1) * CELL, (a["row"] + 1) * CELL))
            canvas.alpha_composite(cell.resize((PET_PX, PET_PX), Image.LANCZOS), (8 + i * PET_PX, 6))
        frames.append(flatten(canvas))
    frames[0].save(DOCS / file, save_all=True, append_images=frames[1:], duration=110, loop=0, optimize=True)
    print(" ", file, frames[0].size)


# ------------------------------------------------------------------ sahne karakterleri

def show_gif(atlas, anims, file, scale=1.0):
    d = ROOT / "assets" / "actors"
    img = Image.open(d / f"{atlas}.png").convert("RGBA")
    meta = json.loads((d / f"{atlas}.json").read_text(encoding="utf-8"))["anims"]
    cells = [meta[a] for a in anims]
    h = max(c["h"] for c in cells)
    w = sum(c["w"] for c in cells) + 12 * (len(cells) + 1)
    frames = []
    for f in range(24):
        canvas = Image.new("RGBA", (w, h + 12), BG)
        x = 12
        for c in cells:
            i = f % c["frames"]
            cell = img.crop((c["x"] + i * c["w"], c["y"], c["x"] + (i + 1) * c["w"], c["y"] + c["h"]))
            canvas.alpha_composite(cell, (x, 6 + h - c["h"]))
            x += c["w"] + 12
        if scale != 1.0:
            canvas = canvas.resize((round(canvas.width * scale), round(canvas.height * scale)), Image.LANCZOS)
        frames.append(flatten(canvas))
    frames[0].save(DOCS / file, save_all=True, append_images=frames[1:], duration=110, loop=0, optimize=True)
    print(" ", file, frames[0].size)


if __name__ == "__main__":
    DOCS.mkdir(parents=True, exist_ok=True)
    scenes()
    pet_gif("walk", 8, "pets-walk.gif")
    pet_gif("special", 6, "pets-special.gif")
    show_gif("space_fighter", ["walk", "shoot", "a_walk", "b_fly", "c_walk", "boom"], "show-space.gif")
    show_gif("football", ["red_run", "blue_run", "red_kick", "gk_dive", "ref_whistle", "ball", "confetti"], "show-football.gif")
