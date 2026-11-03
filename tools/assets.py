"""ChatGPT'nin ürettiği magenta zeminli sayfaları uygulamanın kullandığı varlıklara çevirir.

    python tools/assets.py pets      # assets/generated/pets/*.png  -> assets/pets/pixel/*.png + .json
    python tools/assets.py scenes    # assets/generated/scenes/*.png -> assets/scenes/<ad>/...
    python tools/assets.py actors    # assets/generated/actors/*.png -> assets/actors/<ad>.png + .json
    python tools/assets.py preview   # kontrol için önizlemeler: tools/preview/

Yalnızca Pillow ve numpy gerekir.
"""

import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
GEN = ROOT / "assets" / "generated"


# --------------------------------------------------------------------------- zemin silme

def key_magenta(img: Image.Image) -> Image.Image:
    """Magenta (#FF00FF) zemini saydam yapar; kenarlardaki pembe sızmayı temizler."""
    a = np.asarray(img.convert("RGB")).astype(np.float32)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    # "Magentalık": kırmızı ve mavi yüksek, yeşil düşük
    m = np.minimum(r, b) - g
    alpha = np.clip((150.0 - m) / 70.0, 0.0, 1.0)  # m>=150 tamamen zemin, m<=80 tamamen nesne
    # Sızmayı geri al: yarı saydam piksellerde magenta katkısını çıkar
    with np.errstate(divide="ignore", invalid="ignore"):
        k = np.where(alpha > 0, 1.0 / np.maximum(alpha, 1e-3), 0.0)
    rr = np.clip(255.0 - (255.0 - r) * k, 0, 255)
    bb = np.clip(255.0 - (255.0 - b) * k, 0, 255)
    gg = np.clip(g * k, 0, 255)
    semi = (alpha > 0) & (alpha < 1)
    r2 = np.where(semi, rr, r)
    g2 = np.where(semi, gg, g)
    b2 = np.where(semi, bb, b)
    # Kalan pembe sızıntı: yeşil çok düşük ve kırmızı/mavi dengeliyse soldur
    out = np.dstack([r2, g2, b2, alpha * 255.0]).astype(np.uint8)
    out[alpha <= 0.02] = 0
    return Image.fromarray(out, "RGBA")


def alpha_mask(img: Image.Image, thr: int = 40) -> np.ndarray:
    return np.asarray(img)[..., 3] > thr


# --------------------------------------------------------------------------- bölütleme

def runs(profile: np.ndarray, min_gap: int, min_len: int = 3):
    """1B profilde dolu aralıklar; `min_gap`'ten kısa boşluklar birleştirilir."""
    on = profile > 0
    segs = []
    i, n = 0, len(on)
    while i < n:
        if on[i]:
            j = i
            while j < n and on[j]:
                j += 1
            segs.append([i, j])
            i = j
        else:
            i += 1
    merged = []
    for s in segs:
        if merged and s[0] - merged[-1][1] < min_gap:
            merged[-1][1] = s[1]
        else:
            merged.append(s)
    return [s for s in merged if s[1] - s[0] >= min_len]


def fit_count(segs, want):
    """Aralık sayısını `want`a indirir: en dar boşlukları birleştirerek."""
    segs = [list(s) for s in segs]
    while len(segs) > want:
        gaps = [segs[i + 1][0] - segs[i][1] for i in range(len(segs) - 1)]
        i = int(np.argmin(gaps))
        segs[i][1] = segs[i + 1][1]
        del segs[i + 1]
    return segs


def row_bands(mask: np.ndarray, want: int, min_gap: int = 6):
    prof = mask.sum(axis=1)
    bands = runs(prof, min_gap, min_len=8)
    if len(bands) > want:
        bands = fit_count(bands, want)
    while 0 < len(bands) < want:
        # Bitişik satırlar: en yüksek bandı ortasına yakın en boş satırından böl
        i = max(range(len(bands)), key=lambda k: bands[k][1] - bands[k][0])
        a, b = bands[i]
        lo, hi = a + (b - a) // 4, b - (b - a) // 4
        cut = lo + int(np.argmin(prof[lo:hi]))
        bands[i:i + 1] = [[a, cut], [cut, b]]
    return bands


def frames_in_band(mask: np.ndarray, band, want: int, min_gap: int = 10, pitch: int = 0):
    sub = mask[band[0]:band[1]]
    prof = sub.sum(axis=0)
    segs = runs(prof, min_gap, min_len=4)
    if len(segs) > want:
        segs = fit_count(segs, want)
    if len(segs) < want and pitch:
        # Kareler birbirine değiyor (kuyruk vb.): ızgara çizgisine yakın en boş sütundan kes
        x_end = segs[-1][1] if segs else mask.shape[1]
        cuts = [segs[0][0] if segs else 0]
        for k in range(1, want):
            t = int(k * pitch)
            lo, hi = max(t - pitch // 4, cuts[-1] + 4), min(t + pitch // 4, x_end - 4)
            cuts.append(lo + int(np.argmin(prof[lo:hi])) if hi > lo else t)
        cuts.append(x_end)
        segs = [[cuts[i], cuts[i + 1]] for i in range(want)]
    while 0 < len(segs) < want and not pitch:
        # Düzensiz dizilim: en geniş parçayı, ortasına yakın en boş sütunundan böl
        i = max(range(len(segs)), key=lambda k: segs[k][1] - segs[k][0])
        a, b = segs[i]
        lo, hi = a + (b - a) // 5, b - (b - a) // 5
        cut = lo + int(np.argmin(prof[lo:hi]))
        segs[i:i + 1] = [[a, cut], [cut, b]]
    out = []
    for x0, x1 in segs:
        cols = sub[:, x0:x1]
        ys = np.where(cols.any(axis=1))[0]
        out.append((x0, band[0] + ys[0], x1, band[0] + ys[-1] + 1))
    return out


# --------------------------------------------------------------------------- ölçekleme

def resize_rgba(img: Image.Image, w: int, h: int) -> Image.Image:
    """Ön-çarpımlı alfa ile küçült (kenarlarda koyu hale olmasın)."""
    return img.convert("RGBa").resize((max(1, w), max(1, h)), Image.LANCZOS).convert("RGBA")


# --------------------------------------------------------------------------- petler

PET_ROWS = [("idle", 4), ("walk", 8), ("run", 6), ("sleep", 4), ("jump", 4), ("happy", 4), ("dangle", 4), ("special", 6)]
CELL = 96
BASE = 6  # hücre altından ayak hizasına piksel


def build_pet(src: Path, dst_png: Path, dst_json: Path):
    img = key_magenta(Image.open(src))
    mask = alpha_mask(img)
    bands = row_bands(mask, len(PET_ROWS))
    if len(bands) != len(PET_ROWS):
        raise SystemExit(f"{src.name}: {len(bands)} satır bulundu, {len(PET_ROWS)} bekleniyordu")
    rows = []
    for (name, n), band in zip(PET_ROWS, bands):
        fr = frames_in_band(mask, band, n, pitch=img.width // 8)
        if len(fr) < n:
            print(f"  ! {src.name} {name}: {len(fr)}/{n} kare")
        rows.append((name, band, fr))
    # Ortak ölçek: en geniş / en uzun kare hücreye sığsın (zemin çizgisi satırın altı)
    maxw = max(f[2] - f[0] for _, _, fr in rows for f in fr)
    maxh = max(band[1] - f[1] for _, band, fr in rows for f in fr)
    s = min((CELL - 6) / maxw, (CELL - BASE - 4) / maxh)
    sheet = Image.new("RGBA", (CELL * 8, CELL * 8), (0, 0, 0, 0))
    anims = {}
    for r, (name, band, fr) in enumerate(rows):
        anims[name] = {"row": r, "frames": len(fr)}
        for c, (x0, y0, x1, y1) in enumerate(fr):
            crop = img.crop((x0, y0, x1, y1))
            w, h = round((x1 - x0) * s), round((y1 - y0) * s)
            crop = resize_rgba(crop, w, h)
            # Zemin: satırın en alt noktası; kare o çizgiye göre yerinde kalır (zıplama yüksekliği korunur)
            lift = round((band[1] - y1) * s)
            px = c * CELL + (CELL - w) // 2
            py = r * CELL + CELL - BASE - lift - h
            sheet.alpha_composite(crop, (px, max(r * CELL, py)))
    dst_png.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(dst_png, optimize=True)
    meta = {"cell": CELL, "base": BASE, "anims": anims, "stand": {"anim": "walk", "frame": 0}}
    dst_json.write_text(json.dumps(meta, indent=1), encoding="utf-8")
    print(f"  {src.name}: ölçek {s:.3f}, kareler " + " ".join(f"{n}={a['frames']}" for n, a in anims.items()))


def pets():
    out = ROOT / "assets" / "pets" / "pixel"
    names = {
        "fox_pixel": "fox", "chameleon_pixel": "chameleon", "firefly_pixel": "firefly",
        "hedgehog_pixel": "hedgehog", "puppy_pixel": "puppy",
        "cat_orange": "cat_orange", "cat_black": "cat_black", "cat_grey": "cat_grey", "wolfdog": "wolfdog",
    }
    for src, name in names.items():
        p = GEN / "pets" / f"{src}.png"
        if p.exists():
            build_pet(p, out / f"{name}.png", out / f"{name}.json")


# --------------------------------------------------------------------------- manzaralar

SCENE_H = 112  # kaydedilen şerit yüksekliği (px); çubuk yanında ~42 DIP, %200 ölçekte de keskin

# Her sayfanın düzeni biraz farklı: hangi bantlar şerit, hangi bant parçalar; uçlar, uçanlar,
# yürüme çizgisi (gündüz şeridinde, kaynak görsel y'si). Parça sırası soldan sağa.
SCENES = {
    # mode "segments": şerit doğal sınırlarından benzersiz parçalara bölünür (cuts, kaynak x) ve her
    # parça ekranda bir kez görünür; geri kalan yer yalnızca zemin bandıyla (ground_y'den aşağısı) dolar.
    # mode "tile": şerit döşenir (tekrarın doğal olduğu manzaralar, ör. stadyum tribünü).
    "mountains": dict(strips=2, caps=(0, 7), fly=[6], walk_y=322, ground_y=300, cuts=[430, 830, 1250], pano_ground=0.85, pano_walk=0.93,
                      names=["capl", "pine", "rock", "bush", "sign", "deer", "eagle", "capr"]),
    "istanbul": dict(strips=2, caps=(0, 7), fly=[5], walk_y=378, ground_y=300, cuts=[330, 640, 900, 1190], pano_ground=0.74, pano_walk=0.95,
                     names=["capl", "simit", "cat", "tea", "lamp", "seagull", "tram", "capr"]),
    "antalya": dict(strips=2, caps=(0, 1), fly=[], walk_y=392, ground_y=338, cuts=[380, 720, 1090], pano_ground=0.78, pano_walk=0.95,
                    names=["capl", "capr", "palm", "umbrella", "orange", "turtle", "seagull", "lighthouse"]),
    "winter_cozy": dict(strips=1, caps=(0, 1), fly=[], walk_y=405, ground_y=362, cuts=[560, 1000], pano_ground=0.80, pano_walk=0.93,
                        erase=dict(rows=[1, 2, 3, 4], x=(1195, 1295), y=(0, 95)),
                        names=["capl", "capr", "snowman", "lamp", "tree", "sled", "mailbox", "lantern"]),
    "moon": dict(strips=1, caps=(0, 1), fly=[], walk_y=405, ground_y=340, cuts=[570, 1000], pano_ground=0.70, pano_walk=0.88,
                 names=["capl", "capr", "lander", "flag", "rover", "astronaut", "rock", "dish"]),
    "space": dict(strips=1, caps=None, fly=[2, 3, 6], walk_y=386, ground_y=380, cuts=[600, 1100], pano_ground=0.80, pano_walk=0.93,
                  names=["capl", "capr", "satellite", "asteroid", "buoy", "rocket", "ufo"], skip=["capl", "capr"]),
    "stadium": dict(strips=2, caps=(2, 3), fly=[], walk_y=332, mode="tile",
                    names=["goall", "goalr", "capl", "capr", "corner", "bench", "scoreboard", "light"], skip=["goall", "goalr"]),
}


def bands_all(mask: np.ndarray, min_gap: int = 6):
    return runs(mask.sum(axis=1), min_gap, min_len=8)


def bbox(mask: np.ndarray, y0: int, y1: int):
    sub = mask[y0:y1]
    ys = np.where(sub.any(axis=1))[0]
    xs = np.where(sub.any(axis=0))[0]
    return xs[0], y0 + ys[0], xs[-1] + 1, y0 + ys[-1] + 1


def pack(items, pad=2, vertical=False):
    """Görselleri tek satırlık (ya da tek sütunluk) bir atlasa dizer; [(x, y, w, h)] döndürür."""
    if vertical:
        w = max(i.width for i in items) + pad * 2
        h = sum(i.height for i in items) + pad * (len(items) + 1)
    else:
        w = sum(i.width for i in items) + pad * (len(items) + 1)
        h = max(i.height for i in items) + pad * 2
    atlas = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    out, pos = [], pad
    for i in items:
        xy = (pad, pos) if vertical else (pos, pad)
        atlas.alpha_composite(i, xy)
        out.append({"x": xy[0], "y": xy[1], "w": i.width, "h": i.height})
        pos += (i.height if vertical else i.width) + pad
    return atlas, out


PANO_ROWS = 5


def build_pano(name: str, cfg: dict, out: Path, meta: dict) -> bool:
    """Yeni biçim: <ad>_pano.png (+ _pano_night.png) — 5 satır, her biri farklı, uzun bir manzara
    parçası. Satırlar benzersiz parçalar olur; zemin bandı ilk satırın altından alınır."""
    day = GEN / "scenes" / f"{name}_pano.png"
    if not day.exists():
        return False
    night = GEN / "scenes" / f"{name}_pano_night.png"
    fade = round(SCENE_H * 0.22)
    gfrac = cfg.get("pano_ground", 0.74)
    for label, path in [("day", day), ("night", night)]:
        if not path.exists():
            continue
        img = key_magenta(Image.open(path))
        mask = alpha_mask(img)
        bands = row_bands(mask, PANO_ROWS)
        er = cfg.get("erase") if label == "day" else None
        if er and len(bands) == PANO_ROWS:
            a = np.asarray(img).astype(np.float32).copy()
            x0, x1 = er["x"]
            for r in er["rows"]:
                top = bands[r][0]
                y0, y1 = top + er["y"][0], min(bands[r][1], top + er["y"][1])
                left, right = a[y0:y1, x0 - 1], a[y0:y1, x1]
                t = np.linspace(0.0, 1.0, x1 - x0)[None, :, None]
                a[y0:y1, x0:x1] = left[:, None, :] * (1 - t) + right[:, None, :] * t
            img = Image.fromarray(a.clip(0, 255).astype(np.uint8), "RGBA")
        if len(bands) != PANO_ROWS:
            raise SystemExit(f"{path.name}: {len(bands)} satır bulundu, {PANO_ROWS} bekleniyordu")
        rows = []
        for b in bands:
            x0, y0, x1, y1 = bbox(mask, *b)
            crop = img.crop((x0 + 2, y0 + 2, x1 - 2, y1 - 2))
            rows.append(resize_rgba(crop, round(crop.width * SCENE_H / crop.height), SCENE_H))
        gy = round(SCENE_H * gfrac)
        ground = fade_edges(rows[0].crop((0, gy, rows[0].width, SCENE_H)), 0, 0, top=max(4, round((SCENE_H - gy) * 0.35)))
        ground.save(out / f"ground_{label}.png", optimize=True)
        # İki takım: soldan yumuşayan (çubuğun sağına dizilenler) ve sağdan yumuşayan (soluna dizilenler).
        # Her birleşimde yalnızca üstteki parçanın kenarı saydamlaşır, alttaki tam opak kalır:
        # böylece geçiş kusursuz olur, arada soluk şerit kalmaz.
        items = [fade_edges(r, fade, 0) for r in rows] + [fade_edges(r, 0, fade) for r in rows]
        atlas, rects = pack(items, vertical=True)
        atlas.save(out / f"segs_{label}.png", optimize=True)
        meta.update(ground={"w": ground.width, "h": ground.height}, segments=rects[:PANO_ROWS], segments_r=rects[PANO_ROWS:], fade=fade, w=rows[0].width)
    meta["night"] = night.exists()
    meta["pano"] = True
    return True


def fade_edges(img: Image.Image, left: int, right: int, top: int = 0) -> Image.Image:
    """Kenarlara saydamlık rampası uygular (yanyana/zemine yumuşak geçiş için)."""
    a = np.asarray(img).astype(np.float32)
    w, h = img.width, img.height
    ramp = np.ones(w, np.float32)
    if left:
        ramp[:left] = np.linspace(0.0, 1.0, left, endpoint=False) ** 1.5
    if right:
        ramp[w - right:] = np.linspace(1.0, 0.0, right) ** 1.5
    col = np.ones(h, np.float32)
    if top:
        col[:top] = np.linspace(0.0, 1.0, top, endpoint=False)
    a[..., 3] *= ramp[None, :] * col[:, None]
    return Image.fromarray(a.clip(0, 255).astype(np.uint8), "RGBA")


def build_scene(name: str, cfg: dict):
    src = GEN / "scenes" / f"{name}.png"
    img = key_magenta(Image.open(src))
    mask = alpha_mask(img)
    bands = bands_all(mask)
    W = img.width
    wide = [b for b in bands if (lambda bb: bb[2] - bb[0] > 0.8 * W and bb[3] - bb[1] > 180)(bbox(mask, *b))]
    if len(wide) < cfg["strips"]:
        raise SystemExit(f"{name}: {len(wide)} şerit bulundu, {cfg['strips']} bekleniyordu")
    strips = []
    for b in wide[: cfg["strips"]]:
        x0, y0, x1, y1 = bbox(mask, *b)
        # Kenardaki 2 pikseli at: magenta sızması orada kalır
        strips.append((x0 + 2, y0 + 2, x1 - 2, y1 - 2))
    day = strips[0]
    s = SCENE_H / (day[3] - day[1])
    out = ROOT / "assets" / "scenes" / name
    if out.exists():
        for f in out.iterdir():
            f.unlink()
    out.mkdir(parents=True, exist_ok=True)
    mode = cfg.get("mode", "segments")
    variants = list(zip(["day", "night"], strips))
    meta = {"mode": mode, "h": SCENE_H, "night": len(strips) > 1}

    if mode != "tile" and build_pano(name, cfg, out, meta):
        pass
    elif mode == "tile":
        for label, (x0, y0, x1, y1) in variants:
            crop = img.crop((x0, y0, x1, y1))
            resize_rgba(crop, round((x1 - x0) * s), SCENE_H).save(out / f"{label}.png", optimize=True)
        meta["w"] = round((day[2] - day[0]) * s)
    else:
        # Gece şeridi gündüzle aynı düzende ama konumu biraz kayık olabilir: kesimleri orana göre taşı
        rel = [(c - day[0]) / (day[2] - day[0]) for c in cfg["cuts"]]
        fade = round(SCENE_H * 0.22)
        for label, (x0, y0, x1, y1) in variants:
            sw = x1 - x0
            full = resize_rgba(img.crop((x0, y0, x1, y1)), round(sw * s), SCENE_H)
            # Zemin bandı: ground_y'den aşağısı, üstü yumuşakça saydamlaşır; tüm genişlik döşenir
            gy = round((cfg["ground_y"] - day[1]) * s)
            ground = fade_edges(full.crop((0, gy, full.width, SCENE_H)), 0, 0, top=max(4, round((SCENE_H - gy) * 0.35)))
            ground.save(out / f"ground_{label}.png", optimize=True)
            meta["ground"] = {"w": ground.width, "h": ground.height}
            # Benzersiz parçalar: iki yanı yumuşak, yanyana bindirilerek dizilir
            xs = [0] + [round(r * full.width) for r in rel] + [full.width]
            segs = []
            for a, b in zip(xs, xs[1:]):
                seg = full.crop((max(0, a - fade // 2), 0, min(full.width, b + fade // 2), SCENE_H))
                segs.append(fade_edges(seg, fade, fade))
            atlas, rects = pack(segs)
            atlas.save(out / f"segs_{label}.png", optimize=True)
            meta["segments"] = rects
            meta["fade"] = fade
        meta["w"] = round((day[2] - day[0]) * s)

    # Parçalar: son bant
    pb = bands[-1]
    fr = frames_in_band(mask, pb, len(cfg["names"]), min_gap=14)
    if len(fr) != len(cfg["names"]):
        raise SystemExit(f"{name}: {len(fr)} parça bulundu, {len(cfg['names'])} bekleniyordu")
    parts = []
    for (x0, y0, x1, y1) in fr:
        crop = img.crop((x0, y0, x1, y1))
        parts.append(resize_rgba(crop, round((x1 - x0) * s), round((y1 - y0) * s)))
    atlas, rects = pack(parts)
    atlas.save(out / "parts.png", optimize=True)
    names = cfg["names"]
    skip = set(cfg.get("skip", []))
    caps_i = cfg["caps"] or ()
    props = [dict(r, name=n, fly=i in cfg["fly"]) for i, (n, r) in enumerate(zip(names, rects)) if i not in caps_i and n not in skip]
    meta.update(
        walk=round((cfg["walk_y"] - day[1]) / (day[3] - day[1]), 4),
        caps=[rects[cfg["caps"][0]], rects[cfg["caps"][1]]] if cfg["caps"] else [],
        props=props,
        extra={n: r for n, r in zip(names, rects) if n in skip},
    )
    if meta.get("pano"):
        meta["walk"] = cfg.get("pano_walk", 0.88)
    (out / "scene.json").write_text(json.dumps(meta, indent=1), encoding="utf-8")
    segs = len(meta.get("segments", []))
    print(f"  {name}: {mode}, gece={meta['night']}, {segs} parça, süs {len(props)}, yürüme {meta['walk']:.2f}")


# --------------------------------------------------------------------------- karakterler (uzay savaşçısı, futbol)

ACTORS = {
    "space_fighter": dict(
        ref_h=78,  # ilk satır (savaşçı) bu yüksekliğe ölçeklenir
        rows=[
            [("idle", 4)], [("walk", 6)], [("shoot", 4), ("hit", 2)], [("laser", 3), ("impact", 4)],
            [("a_walk", 4), ("a_die", 4)], [("b_fly", 4), ("b_die", 4)], [("c_walk", 4), ("c_die", 4)], [("boom", 6)],
        ],
    ),
    "football": dict(
        ref_h=72,
        rows=[
            [("red_run", 6)], [("red_kick", 4), ("red_cheer", 2)], [("blue_run", 6)], [("blue_kick", 4), ("blue_cheer", 2)],
            [("gk_idle", 2), ("gk_dive", 4), ("gk_catch", 2)], [("ref_run", 4), ("ref_whistle", 2)],
            [("ball", 4), ("ball_squash", 2), ("shadow", 1)], [("puff", 4), ("confetti", 4)],
        ],
    ),
}


def build_actor(name: str, cfg: dict):
    src = GEN / "actors" / f"{name}.png"
    img = key_magenta(Image.open(src))
    mask = alpha_mask(img)
    rows = cfg["rows"]
    bands = row_bands(mask, len(rows))
    if len(bands) != len(rows):
        raise SystemExit(f"{name}: {len(bands)} satır, {len(rows)} bekleniyordu")
    found = []
    for spec, band in zip(rows, bands):
        want = sum(n for _, n in spec)
        fr = frames_in_band(mask, band, want, min_gap=12)
        if len(fr) != want:
            raise SystemExit(f"{name}: satırda {len(fr)}/{want} kare ({spec})")
        found.append((spec, band, fr))
    s = cfg["ref_h"] / max(found[0][1][1] - f[1] for f in found[0][2])
    groups = []  # (ad, [kırpılmış], hücre w, h)
    for spec, band, fr in found:
        i = 0
        for anim, n in spec:
            frames = fr[i:i + n]
            i += n
            cw = max(round((f[2] - f[0]) * s) for f in frames) + 2
            ch = max(round((band[1] - f[1]) * s) for f in frames) + 2
            cells = []
            for (x0, y0, x1, y1) in frames:
                crop = resize_rgba(img.crop((x0, y0, x1, y1)), round((x1 - x0) * s), round((y1 - y0) * s))
                cell = Image.new("RGBA", (cw, ch), (0, 0, 0, 0))
                lift = round((band[1] - y1) * s)
                cell.alpha_composite(crop, ((cw - crop.width) // 2, max(0, ch - 1 - lift - crop.height)))
                cells.append(cell)
            groups.append((anim, cells, cw, ch))
    # Her animasyon atlasta bir satır
    W = max(len(c) * cw for _, c, cw, _ in groups)
    H = sum(ch for *_, ch in groups)
    atlas = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    meta, y = {}, 0
    for anim, cells, cw, ch in groups:
        for k, c in enumerate(cells):
            atlas.alpha_composite(c, (k * cw, y))
        meta[anim] = {"x": 0, "y": y, "w": cw, "h": ch, "frames": len(cells)}
        y += ch
    out = ROOT / "assets" / "actors"
    out.mkdir(parents=True, exist_ok=True)
    atlas.save(out / f"{name}.png", optimize=True)
    (out / f"{name}.json").write_text(json.dumps({"scale": 3, "anims": meta}, indent=1), encoding="utf-8")
    print(f"  {name}: atlas {W}x{H}, " + ", ".join(f"{a}={m['frames']}" for a, m in meta.items()))


def actors():
    for name, cfg in ACTORS.items():
        if (GEN / "actors" / f"{name}.png").exists():
            build_actor(name, cfg)


def scenes():
    for name, cfg in SCENES.items():
        if (GEN / "scenes" / f"{name}.png").exists():
            build_scene(name, cfg)


if __name__ == "__main__":
    what = sys.argv[1] if len(sys.argv) > 1 else "pets"
    {"pets": pets, "scenes": scenes, "actors": actors}[what]()
