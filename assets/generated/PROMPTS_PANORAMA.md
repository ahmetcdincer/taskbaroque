# ChatGPT promptları — Panoramalar (2. tur)

Bu promptlar, görev çubuğunun yanındaki manzaraların **tekrar etmeden** ekranı doldurması için.
Her görselde aynı manzaranın birbirinin devamı olan **5 farklı uzun şeridi** var.

## Nasıl kullanılır
1. Her görsel için **ayrı bir ChatGPT sohbeti** aç ve promptu olduğu gibi yapıştır.
2. Görseli indir ve başlıkta yazan **klasöre, yazan adla** kaydet.
3. **Gece sürümleri** için gündüz görselini aynı sohbette tutup, o bölümdeki **gece promptunu**
   yapıştır. Düzen birebir aynı kalmalı.
4. Hepsi hazır olunca bana "panoramalar hazır" de. Hepsi gelmek zorunda değil; hangileri hazırsa
   onlarla başlarım.

**Kayıt klasörü:** `D:\Repos\taskbaroque\assets\generated\scenes\`

| # | Manzara | Gündüz dosyası | Gece dosyası |
|---|---|---|---|
| 1 | İstanbul | `istanbul_pano.png` | `istanbul_pano_night.png` |
| 2 | Antalya | `antalya_pano.png` | `antalya_pano_night.png` |
| 3 | Dağlar | `mountains_pano.png` | `mountains_pano_night.png` |
| 4 | Karlı köy | `winter_cozy_pano.png` | — |
| 5 | Ay | `moon_pano.png` | — |
| 6 | Uzay (baştan) | `space_pano.png` | — |

Tam yollar, örneğin: `D:\Repos\taskbaroque\assets\generated\scenes\istanbul_pano.png`

## Her görselde kontrol et
- **Satır sayısı:** Tam **5 şerit** olmalı, aralarında ince magenta boşluk bulunmalı.
- **Arka plan:** Şeritlerin dışı **düz magenta (#FF00FF)** olmalı; şeritlerin içindeki gökyüzü normal renkte olmalı.
- **Tekrar yok:** Her simge (cami, köprü, Dünya, güneş, ay) tüm görselde **yalnızca bir kez** geçmeli.
- **Ortak çizgi:** Şeritlerin ufku ve zemini aynı yükseklikte olmalı.
- **Düzeltme cümleleri:** Kural bozulursa, sorun hangisiyse onu yaz:
  - *"Satır sayısını tam 5 yap, başka hiçbir şeyi değiştirme"*
  - *"Şeritlerin dışını tamamen düz #FF00FF magenta yap, başka hiçbir şeyi değiştirme"*
  - *"Tekrar eden simgeyi kaldır, her simge yalnızca bir kez geçsin, başka hiçbir şeyi değiştirme"*
  - *"Tüm şeritlerde ufku ve zemini aynı yüksekliğe getir, başka hiçbir şeyi değiştirme"*

---

## 1. İstanbul

### Gündüz → `scenes\istanbul_pano.png`

```
Create a pixel-art PANORAMA SHEET for a long side-scrolling landscape that will sit beside a desktop
taskbar. Image size 1536x1024.

Draw exactly 5 horizontal strips stacked vertically. Each strip fills the full width (about 1520 px
wide, about 180 px tall). Leave a thin 16 px gap of pure magenta between strips.
The 5 strips are 5 DIFFERENT, consecutive sections of ONE long continuous panorama of the Istanbul
shoreline along the Bosphorus at golden hour:
Strip 1: calm Bosphorus water, a ferry (vapur) and seagulls, distant green hills
Strip 2: Hagia Sophia with its four minarets on the hill above old red-roofed houses
Strip 3: the Blue Mosque with six minarets and Sultanahmet park trees
Strip 4: Galata Tower rising above the Karaköy rooftops, fishing boats at the quay
Strip 5: the Bosphorus Bridge stretching across the water with Ortaköy Mosque at its foot

RULES (very important):
- All 5 strips share EXACTLY the same horizon height, the same sky colors, the same lighting and the
  same ground. The bottom 24% of every strip is the same flat walkable stone seaside promenade with a
  black iron railing, its top edge perfectly horizontal and at the same height in all strips. Small
  characters will walk on it.
- The leftmost and rightmost 80 px of every strip are calm: only sky, water and the promenade.
  No building, landmark or object is cut off at a strip edge.
- Every landmark appears only ONCE in the whole sheet. Never repeat a building, a mosque, a sun or a
  moon. At most one sun in the entire sheet.
- It will be displayed very small (about 42 px tall): use bold, readable silhouettes, strong contrast,
  and keep important things in the lower two thirds of each strip.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, limited palette (about 24–32 colors), clean 1-pixel dark outlines,
consistent pixel size, side view.
BACKGROUND: everything outside the 5 strips is one flat solid pure magenta #FF00FF. Inside the strips
the sky is drawn normally (never magenta). No text, no labels, no watermark, no borders.
```

### Gece → `scenes\istanbul_pano_night.png`
Gündüz görseli üretildikten sonra **aynı sohbette** yapıştır:

```
Redraw this exact same panorama sheet at night. Keep the layout of all 5 strips identical: same
buildings in the same places, same horizon height, same promenade and railing at the same height,
same strip sizes and the same magenta gaps. Change only the lighting: deep blue night sky with stars,
one crescent moon in the whole sheet (only once), warmly lit mosques and windows, bridge lights and
their reflections on the water, glowing street lamps on the promenade. Everything outside the strips
stays flat solid pure magenta #FF00FF. Same pixel-art style, no text.
```

---

## 2. Antalya

### Gündüz → `scenes\antalya_pano.png`

```
Create a pixel-art PANORAMA SHEET for a long side-scrolling landscape that will sit beside a desktop
taskbar. Image size 1536x1024.

Draw exactly 5 horizontal strips stacked vertically. Each strip fills the full width (about 1520 px
wide, about 180 px tall). Leave a thin 16 px gap of pure magenta between strips.
The 5 strips are 5 DIFFERENT, consecutive sections of ONE long continuous panorama of the Antalya
Mediterranean coast on a sunny day:
Strip 1: turquoise open sea with a gulet sailing boat, the snowy Taurus mountains far behind
Strip 2: tall cliffs with the Düden waterfall pouring into the sea
Strip 3: Kaleiçi old town with red-tiled Ottoman houses and the fluted Yivli Minaret
Strip 4: Hadrian's Gate (three white marble arches) and the old harbor with small boats
Strip 5: Konyaaltı beach with sand, sun umbrellas, palm trees and the mountains behind

RULES (very important):
- All 5 strips share EXACTLY the same horizon height, the same sky colors, the same lighting and the
  same ground. The bottom 24% of every strip is the same flat walkable sunny stone seaside promenade
  with palm planters, its top edge perfectly horizontal and at the same height in all strips. Small
  characters will walk on it.
- The leftmost and rightmost 80 px of every strip are calm: only sky, sea and the promenade.
  No building, landmark or object is cut off at a strip edge.
- Every landmark appears only ONCE in the whole sheet. Never repeat a building, a minaret, a sun or a
  moon. At most one sun in the entire sheet.
- It will be displayed very small (about 42 px tall): use bold, readable silhouettes, strong contrast,
  and keep important things in the lower two thirds of each strip.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, limited palette (about 24–32 colors), clean 1-pixel dark outlines,
consistent pixel size, side view.
BACKGROUND: everything outside the 5 strips is one flat solid pure magenta #FF00FF. Inside the strips
the sky is drawn normally (never magenta). No text, no labels, no watermark, no borders.
```

### Gece → `scenes\antalya_pano_night.png`
Gündüz görseli üretildikten sonra **aynı sohbette** yapıştır:

```
Redraw this exact same panorama sheet at dusk turning to night. Keep the layout of all 5 strips
identical: same landmarks in the same places, same horizon height, same promenade at the same height,
same strip sizes and the same magenta gaps. Change only the lighting: purple-orange dusk sky turning
dark blue with the first stars, NO sun anywhere, one moon in the whole sheet (only once), warmly lit
windows, glowing street lamps on the promenade, lights reflecting on the sea. Everything outside the
strips stays flat solid pure magenta #FF00FF. Same pixel-art style, no text.
```

---

## 3. Dağlar

### Gündüz → `scenes\mountains_pano.png`

```
Create a pixel-art PANORAMA SHEET for a long side-scrolling landscape that will sit beside a desktop
taskbar. Image size 1536x1024.

Draw exactly 5 horizontal strips stacked vertically. Each strip fills the full width (about 1520 px
wide, about 180 px tall). Leave a thin 16 px gap of pure magenta between strips.
The 5 strips are 5 DIFFERENT, consecutive sections of ONE long continuous panorama of an alpine
valley in summer:
Strip 1: a tall pine forest with a few deer at its edge
Strip 2: one big snowy peak above a turquoise mountain lake
Strip 3: a waterfall and a small wooden bridge over a stream
Strip 4: a log cabin with smoke from the chimney and a wooden fence
Strip 5: rolling meadows with wildflowers and a distant low mountain range

RULES (very important):
- All 5 strips share EXACTLY the same horizon height, the same sky colors, the same lighting and the
  same ground. The bottom 24% of every strip is the same flat walkable green grass on brown soil, its
  top edge perfectly horizontal and at the same height in all strips. Small characters will walk on it.
- The leftmost and rightmost 80 px of every strip are calm: only sky, distant hills and the grass.
  No tree, building or object is cut off at a strip edge.
- Every landmark appears only ONCE in the whole sheet. Never repeat the cabin, the waterfall, the
  lake, the big peak, a sun or a moon. At most one sun in the entire sheet.
- It will be displayed very small (about 42 px tall): use bold, readable silhouettes, strong contrast,
  and keep important things in the lower two thirds of each strip.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, limited palette (about 24–32 colors), clean 1-pixel dark outlines,
consistent pixel size, side view.
BACKGROUND: everything outside the 5 strips is one flat solid pure magenta #FF00FF. Inside the strips
the sky is drawn normally (never magenta). No text, no labels, no watermark, no borders.
```

### Gece → `scenes\mountains_pano_night.png`
Gündüz görseli üretildikten sonra **aynı sohbette** yapıştır:

```
Redraw this exact same panorama sheet at night. Keep the layout of all 5 strips identical: same
trees, peak, lake, waterfall, cabin and meadows in the same places, same horizon height, same grass
at the same height, same strip sizes and the same magenta gaps. Change only the lighting: dark blue
night sky with many stars, one full moon in the whole sheet (only once), moonlight on the snowy peak
and the lake, warm glowing windows in the cabin, a few fireflies over the meadow. Everything outside
the strips stays flat solid pure magenta #FF00FF. Same pixel-art style, no text.
```

---

## 4. Karlı köy → `scenes\winter_cozy_pano.png`

```
Create a pixel-art PANORAMA SHEET for a long side-scrolling landscape that will sit beside a desktop
taskbar. Image size 1536x1024.

Draw exactly 5 horizontal strips stacked vertically. Each strip fills the full width (about 1520 px
wide, about 180 px tall). Leave a thin 16 px gap of pure magenta between strips.
The 5 strips are 5 DIFFERENT, consecutive sections of ONE long continuous panorama of a cozy snowy
village at dusk with warm lights:
Strip 1: a snow-covered pine forest with a few glowing lanterns
Strip 2: a wooden cabin with glowing orange windows and colorful string lights
Strip 3: a frozen pond with lantern posts and a snowy bench
Strip 4: the village square with one big decorated tree and string lights between lamp posts
Strip 5: a small chapel with a lit window and snowy hills behind

RULES (very important):
- All 5 strips share EXACTLY the same horizon height, the same sky colors, the same lighting and the
  same ground. The bottom 24% of every strip is the same flat walkable snowy path, its top edge
  perfectly horizontal and at the same height in all strips. Small characters will walk on it.
- The leftmost and rightmost 80 px of every strip are calm: only sky, snowy hills and the path.
  No building, tree or object is cut off at a strip edge.
- Every landmark appears only ONCE in the whole sheet. Never repeat the cabin, the pond, the big tree
  or the chapel. No sun; at most one moon in the entire sheet.
- The mood is warm and inviting: warm orange window light against cool blue-purple snow and sky.
- It will be displayed very small (about 42 px tall): use bold, readable silhouettes, strong contrast,
  and keep important things in the lower two thirds of each strip.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, limited palette (about 24–32 colors), clean 1-pixel dark outlines,
consistent pixel size, side view.
BACKGROUND: everything outside the 5 strips is one flat solid pure magenta #FF00FF. Inside the strips
the sky is drawn normally (never magenta). No text, no labels, no watermark, no borders.
```

---

## 5. Ay → `scenes\moon_pano.png`

```
Create a pixel-art PANORAMA SHEET for a long side-scrolling landscape that will sit beside a desktop
taskbar. Image size 1536x1024.

Draw exactly 5 horizontal strips stacked vertically. Each strip fills the full width (about 1520 px
wide, about 180 px tall). Leave a thin 16 px gap of pure magenta between strips.
The 5 strips are 5 DIFFERENT, consecutive sections of ONE long continuous panorama of the surface of
the Moon under a black starry sky:
Strip 1: rolling grey hills and craters with a distant ridge
Strip 2: planet Earth rising large and blue above the horizon
Strip 3: a small lunar base with white domes, a solar panel array and a radio dish
Strip 4: a big crater with a lunar lander and a rover parked near it
Strip 5: low hills with one shooting star crossing the sky

RULES (very important):
- All 5 strips share EXACTLY the same horizon height, the same black starry sky and the same ground.
  The bottom 24% of every strip is the same flat walkable grey lunar dust with small craters and
  footprints, its top edge perfectly horizontal and at the same height in all strips. Small characters
  will walk on it.
- The leftmost and rightmost 80 px of every strip are calm: only stars, distant hills and the ground.
  No object is cut off at a strip edge.
- Planet Earth appears only ONCE in the whole sheet (only in strip 2). Never repeat Earth, the base,
  the lander or the rover. No sun anywhere.
- It will be displayed very small (about 42 px tall): use bold, readable silhouettes, strong contrast,
  and keep important things in the lower two thirds of each strip.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, limited palette (about 24–32 colors), clean 1-pixel dark outlines,
consistent pixel size, side view.
BACKGROUND: everything outside the 5 strips is one flat solid pure magenta #FF00FF. Inside the strips
the sky is drawn black/dark navy with stars (never magenta). No text, no labels, no watermark,
no borders.
```

---

## 6. Uzay (baştan) → `scenes\space_pano.png`

```
Create a pixel-art PANORAMA SHEET for a long side-scrolling landscape that will sit beside a desktop
taskbar. Image size 1536x1024.

Draw exactly 5 horizontal strips stacked vertically. Each strip fills the full width (about 1520 px
wide, about 180 px tall). Leave a thin 16 px gap of pure magenta between strips.
The 5 strips are 5 DIFFERENT, consecutive sections of ONE long continuous panorama of a rocky
asteroid surface drifting through deep space:
Strip 1: purple-blue nebula clouds and many stars above the rocky ground
Strip 2: one big ringed planet low on the horizon
Strip 3: a space station with blinking lights and a small ship docking
Strip 4: an asteroid field with a small mining rig and floating rocks
Strip 5: a spiral galaxy and a comet streaking across the stars

RULES (very important):
- All 5 strips share EXACTLY the same horizon height, the same dark space sky colors and the same
  ground. The bottom 24% of every strip is the same flat walkable dark rocky asteroid ground with small
  glowing cyan crystals, its top edge perfectly horizontal and at the same height in all strips. Small
  characters will walk on it.
- The leftmost and rightmost 80 px of every strip are calm: only stars and the ground. No planet,
  station or object is cut off at a strip edge.
- Every landmark appears only ONCE in the whole sheet. Only ONE planet in the entire sheet (strip 2),
  only one station, only one galaxy. No sun.
- Rich but readable: deep navy and purple space, bright accent colors on the station lights and the
  crystals.
- It will be displayed very small (about 42 px tall): use bold, readable silhouettes, strong contrast,
  and keep important things in the lower two thirds of each strip.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, limited palette (about 24–32 colors), clean 1-pixel dark outlines,
consistent pixel size, side view.
BACKGROUND: everything outside the 5 strips is one flat solid pure magenta #FF00FF. Inside the strips
space is drawn dark navy/black with stars (never magenta). No text, no labels, no watermark,
no borders.
```
