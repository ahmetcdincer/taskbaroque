# ChatGPT görsel promptları — Taskbaroque

Her görsel için **ayrı bir ChatGPT sohbeti** aç, ilgili promptu olduğu gibi yapıştır. Bir görsel bitince
dosyayı indir ve başlıkta yazan **klasöre, yazan adla** kaydet. Gerisini (kesme, arka planı silme,
ölçekleme, JSON, koda bağlama) ben yapacağım.

Kök klasör: `D:\Repos\taskbaroque\assets\generated\`

| # | Ne | Kaydedilecek dosya |
|---|----|----|
| 1 | Dağlı manzara | `scenes\mountains.png` |
| 2 | Mevcut petlerin pixel hâlleri (5 ayrı görsel) | `pets\fox_pixel.png`, `pets\chameleon_pixel.png`, `pets\firefly_pixel.png`, `pets\hedgehog_pixel.png`, `pets\puppy_pixel.png` |
| 3 | Uzay manzarası | `scenes\space.png` |
| 4 | Kış temalı sıcak, ışıklı manzara | `scenes\winter_cozy.png` |
| 5 | 3 renkte kedi peti (3 ayrı görsel) | `pets\cat_orange.png`, `pets\cat_black.png`, `pets\cat_grey.png` |
| 6 | Kurt köpeği peti | `pets\wolfdog.png` |
| 7 | İstanbul manzarası | `scenes\istanbul.png` |
| 8 | Antalya manzarası | `scenes\antalya.png` |
| 9 | Ay ve uzay manzarası | `scenes\moon.png` |
| 10 | Uzay savaşçısı + lazer + uzaylılar | `actors\space_fighter.png` |
| 11 | Futbol sahası ve tribünler | `scenes\stadium.png` |
| 12 | Futbolcular ve top | `actors\football.png` |

## Kontrol listesi (her görselde)
- Arka plan **tamamen düz magenta (#FF00FF)** olmalı: gradyan, gölge, desen yok. Değilse: *"Arka planı tamamen düz #FF00FF magenta yap, başka hiçbir şeyi değiştirme"* de.
- Gerçek pixel art olmalı: keskin kareler, bulanıklık yok. Yazı, filigran, çerçeve, ızgara çizgisi olmamalı.
- Kareler birbirine değmemeli, aralarında magenta boşluk kalmalı.
- Pet tablolarında ChatGPT 8 satırı tek görselde beceremezse, aynı sohbette *"Bunu iki görsele böl: 1-4. satırlar ve 5-8. satırlar"* de. Dosyaları `_a.png` / `_b.png` ekiyle kaydet (ör. `fox_pixel_a.png`, `fox_pixel_b.png`).
- Beğenmediğin karede *"Sadece X satırını yeniden üret, stil aynı kalsın"* diyebilirsin.

---

## Ortak stil bloğu
Aşağıdaki promptların hepsinde bu blok zaten var; ayrıca yapıştırman gerekmez.

```
STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

---

## 1. Dağlı manzara → `scenes\mountains.png`

```
Create a pixel-art sprite sheet for a side-scrolling landscape strip that will sit beside a desktop
taskbar. Image size 1536x1024 (landscape).

ROW 1 (top, about 1450x250 px): a long horizontal, SEAMLESSLY TILEABLE landscape strip (left edge
must continue perfectly into the right edge). Daytime alpine scenery: layered snowy mountain peaks in
the back, pine forest and green hills in the middle, a small wooden cabin and a stream. The bottom 25%
of the strip is a flat walkable dirt-and-grass path running the full width, with its top edge perfectly
horizontal at the same height everywhere (small characters will walk on it).
ROW 2: the SAME strip at night — dark blue sky, moon, stars, warm lit cabin windows, same layout so
both rows line up pixel for pixel.
ROW 3: left end cap and right end cap of the strip (each about 300x250, the landscape tapering off
into a rocky edge), then 6 separate small props: pine tree, rock, bush with flowers, wooden signpost,
small deer, flying eagle.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

---

## 2. Mevcut petlerin pixel hâlleri (5 ayrı sohbet)

Her pet için **ayrı sohbet** aç ve o petin mevcut görselini promptla birlikte **ek olarak yükle**
(referans): `D:\Repos\taskbaroque\assets\pets\<ad>.png`. Aşağıdaki promptta yalnızca `{PET}` ve
`{SPECIAL}` kısmını tablodaki gibi değiştir.

| Pet | Referans dosya | `{PET}` | `{SPECIAL}` | Kaydet |
|---|---|---|---|---|
| Tilki | `assets\pets\fox.png` | a cute orange fox with a white-tipped fluffy tail | pouncing: crouch, leap forward, land, shake | `pets\fox_pixel.png` |
| Bukalemun | `assets\pets\chameleon.png` | a cute green chameleon with a curled tail | shooting its tongue out to catch a fly while its color shifts | `pets\chameleon_pixel.png` |
| Ateş böceği | `assets\pets\firefly.png` | a cute firefly with a glowing yellow abdomen | glowing brighter and pulsing with little light sparkles | `pets\firefly_pixel.png` |
| Kirpi | `assets\pets\hedgehog.png` | a cute brown hedgehog | rolling up into a spiky ball, rolling, and unrolling | `pets\hedgehog_pixel.png` |
| Köpek yavrusu | `assets\pets\puppy.png` | a cute puppy (same breed and colors as the reference) | chasing its own tail in a little circle | `pets\puppy_pixel.png` |

```
Recreate the character in the attached reference sheet as authentic pixel art: {PET}. Keep the same
character design, colors and personality, but redraw it as clean pixel art. This is an animation
sprite sheet for a small desktop pet that walks on the taskbar.

Image size 1024x1024. Arrange the frames in a grid of 8 rows and up to 8 columns, all cells the same
size (128x128), one frame per cell, the character centered in its cell and always the same size, feet
on the same baseline near the bottom of each cell, facing RIGHT in every frame.
Row 1 — idle: 4 frames (breathing, blinking)
Row 2 — walk: 8 frames (full walk cycle)
Row 3 — run: 6 frames (full run cycle)
Row 4 — sleep: 4 frames (curled up, a small "z" bubble may appear)
Row 5 — jump: 4 frames (crouch, take-off, airborne, landing)
Row 6 — happy: 4 frames (being petted by a hand from the top-right, eyes closed, little hearts)
Row 7 — dangle: 4 frames (lifted up off the ground, legs dangling and wiggling)
Row 8 — special: 6 frames ({SPECIAL})
Leave the unused cells at the end of rows empty (magenta).

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

---

## 3. Uzay manzarası → `scenes\space.png`

```
Create a pixel-art sprite sheet for a side-scrolling space scene strip that will sit beside a desktop
taskbar. Image size 1536x1024 (landscape).

ROW 1 (top, about 1450x250 px): a long horizontal, SEAMLESSLY TILEABLE strip (left edge continues
perfectly into the right edge): deep space with a purple-blue nebula, twinkling stars, a ringed planet
and a distant galaxy in the back. The bottom 25% is a flat metal space-station walkway with glowing
cyan floor lights, its top edge perfectly horizontal at the same height everywhere (small characters
will walk on it). Note: the space background itself must be drawn dark (deep navy/black), NOT magenta.
Only the area outside the strip is magenta.
ROW 2: 4 small star "twinkle" frames (a single star sparkling), 4 frames of a shooting star.
ROW 3: left end cap and right end cap of the walkway (each about 300x250), then 5 separate props:
small satellite, asteroid, space buoy with blinking light, little rocket, distant UFO.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

---

## 4. Kış temalı sıcak, ışıklı manzara → `scenes\winter_cozy.png`

```
Create a pixel-art sprite sheet for a cozy winter landscape strip that will sit beside a desktop
taskbar. Image size 1536x1024 (landscape).

ROW 1 (top, about 1450x250 px): a long horizontal, SEAMLESSLY TILEABLE strip (left edge continues
perfectly into the right edge): a warm, cozy snowy village at dusk — small wooden cabins with glowing
warm orange windows, smoke from chimneys, snow-covered pine trees, colorful string lights (warm
yellow, red, green) hanging between lamp posts, soft falling snow, a frozen pond. The bottom 25% is
a flat snowy walkable path, its top edge perfectly horizontal at the same height everywhere (small
characters will walk on it). The mood is warm and inviting despite the snow.
ROW 2: 4 frames of the string lights twinkling (a short piece of string with 5 bulbs, the glow
alternating), 4 frames of a window glow flickering, 4 frames of a falling snowflake cluster.
ROW 3: left end cap and right end cap (each about 300x250, snowdrift edge), then 6 separate props:
snowman with scarf, glowing street lamp, decorated pine tree, sled, mailbox with snow, lantern.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

---

## 5. Kedi peti — 3 renk (3 ayrı sohbet)

`{CAT}` kısmını değiştirerek üç kez kullan:

| Renk | `{CAT}` | Kaydet |
|---|---|---|
| Turuncu tekir | an orange tabby cat with darker orange stripes, white chest and paws, green eyes | `pets\cat_orange.png` |
| Siyah | a black cat with a tiny white patch on the chest, yellow eyes | `pets\cat_black.png` |
| Gri-beyaz | a grey-and-white cat (grey back and head, white face, belly and paws), blue eyes | `pets\cat_grey.png` |

```
Create an animation sprite sheet of a cute small cartoon cat as authentic pixel art: {CAT}. Chibi
proportions (big head, small body, expressive eyes), friendly and playful. This is a desktop pet that
walks on the taskbar.

Image size 1024x1024. Arrange the frames in a grid of 8 rows and up to 8 columns, all cells the same
size (128x128), one frame per cell, the cat centered in its cell and always the same size, paws on the
same baseline near the bottom of each cell, facing RIGHT in every frame.
Row 1 — idle: 4 frames (sitting, tail swishing, blinking)
Row 2 — walk: 8 frames (full walk cycle, tail up)
Row 3 — run: 6 frames (full gallop cycle)
Row 4 — sleep: 4 frames (curled into a ball, a small "z" bubble may appear)
Row 5 — jump: 4 frames (crouch with wiggle, take-off, airborne, landing)
Row 6 — happy: 4 frames (being petted by a hand from the top-right, eyes closed, purring, little hearts)
Row 7 — dangle: 4 frames (picked up, legs dangling and wiggling)
Row 8 — special: 6 frames (big stretch and yawn, then grooming its paw)
Leave the unused cells at the end of rows empty (magenta).

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

İpucu: İkinci ve üçüncü kediyi, ilk kedinin görselini ekleyip *"Aynı kediyi, aynı pozlarla, şu renkte
çiz: …"* diyerek de isteyebilirsin. Böylece üçü birbirine benzer olur.

---

## 6. Kurt köpeği peti → `pets\wolfdog.png`

```
Create an animation sprite sheet of a cute wolfdog puppy as authentic pixel art: a husky-like wolfdog
with grey-and-white fur, a darker grey saddle on the back, white mask-shaped face markings, pointy
upright ears, a bushy tail and icy blue eyes. Chibi proportions (big head, small body), friendly but
a little wild. This is a desktop pet that walks on the taskbar.

Image size 1024x1024. Arrange the frames in a grid of 8 rows and up to 8 columns, all cells the same
size (128x128), one frame per cell, the wolfdog centered in its cell and always the same size, paws on
the same baseline near the bottom of each cell, facing RIGHT in every frame.
Row 1 — idle: 4 frames (standing, tail wagging slowly, ears twitching)
Row 2 — walk: 8 frames (full walk cycle)
Row 3 — run: 6 frames (full run cycle)
Row 4 — sleep: 4 frames (curled up with the tail over the nose, a small "z" bubble may appear)
Row 5 — jump: 4 frames (crouch, take-off, airborne, landing)
Row 6 — happy: 4 frames (being petted by a hand from the top-right, eyes closed, tongue out, little hearts)
Row 7 — dangle: 4 frames (picked up, legs dangling and wiggling)
Row 8 — special: 6 frames (sitting down, raising its head and howling at the sky, small music-note-like "awoo" lines)
Leave the unused cells at the end of rows empty (magenta).

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

---

## 7. İstanbul manzarası → `scenes\istanbul.png`

```
Create a pixel-art sprite sheet for a side-scrolling Istanbul cityscape strip that will sit beside a
desktop taskbar. Image size 1536x1024 (landscape).

ROW 1 (top, about 1450x250 px): a long horizontal, SEAMLESSLY TILEABLE strip (left edge continues
perfectly into the right edge): the Istanbul skyline across the Bosphorus at golden hour — silhouettes
of mosques with domes and slender minarets (inspired by Hagia Sophia, the Blue Mosque and Süleymaniye),
Galata Tower, the Bosphorus Bridge, red-roofed old houses on the hills, seagulls, a small ferry
(vapur) on the water. The bottom 25% is a flat walkable seaside promenade with a stone embankment and
iron railing, its top edge perfectly horizontal at the same height everywhere (small characters will
walk on it).
ROW 2: the SAME strip at night — deep blue sky, crescent moon and stars, warmly lit mosques and
windows, bridge lights reflecting on the water, same layout so both rows line up pixel for pixel.
ROW 3: left end cap and right end cap (each about 300x250), then 6 separate props: simit cart,
street cat sitting, Turkish tea glass on a small table, old-style street lamp, flying seagull,
small red tram.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

---

## 8. Antalya manzarası → `scenes\antalya.png`

```
Create a pixel-art sprite sheet for a side-scrolling Antalya coastal landscape strip that will sit
beside a desktop taskbar. Image size 1536x1024 (landscape).

ROW 1 (top, about 1450x250 px): a long horizontal, SEAMLESSLY TILEABLE strip (left edge continues
perfectly into the right edge): the Mediterranean coast of Antalya on a sunny day — the Taurus
mountains in the back, turquoise sea, cliffs with a small waterfall pouring into the sea (like
Düden), the old town Kaleiçi with red-tiled roofs and the Yivli Minaret, Hadrian's Gate style stone
arch, palm trees, a small sailing boat (gulet). The bottom 25% is a flat walkable seaside promenade
with palm planters, its top edge perfectly horizontal at the same height everywhere (small characters
will walk on it).
ROW 2: the SAME strip at sunset/night — orange-pink sunset sky turning to dusk, lit windows and
street lamps, same layout so both rows line up pixel for pixel.
ROW 3: left end cap and right end cap (each about 300x250, a sandy beach edge), then 6 separate props:
palm tree, beach umbrella with sun lounger, orange tree with oranges, sea turtle, seagull, small
lighthouse.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

---

## 9. Ay ve uzay manzarası → `scenes\moon.png`

```
Create a pixel-art sprite sheet for a side-scrolling moon-surface strip that will sit beside a
desktop taskbar. Image size 1536x1024 (landscape).

ROW 1 (top, about 1450x250 px): a long horizontal, SEAMLESSLY TILEABLE strip (left edge continues
perfectly into the right edge): standing on the grey cratered surface of the Moon, with planet Earth
rising large and blue in the black starry sky, distant moon hills and craters. The bottom 25% is the
flat walkable lunar ground (grey dust with small craters and footprints), its top edge perfectly
horizontal at the same height everywhere (small characters will walk on it). Note: the sky inside the
strip must be drawn black/dark navy with stars, NOT magenta. Only the area outside the strip is magenta.
ROW 2: 4 frames of Earth slowly rotating (clouds shifting), 4 frames of a star twinkling.
ROW 3: left end cap and right end cap (each about 300x250, a crater rim edge), then 6 separate props:
small lunar lander, planted flag on a pole (plain, no country symbol), moon rover, little astronaut
waving, moon rock, satellite dish.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

---

## 10. Uzay savaşçısı, lazer ve uzaylılar → `actors\space_fighter.png`

Bu karakter bir manzaranın (ör. 3 Uzay veya 9 Ay) üzerinde yürüyüp uzaylılara ateş edecek.

```
Create an animation sprite sheet for a small side-view pixel-art action scene: a space warrior who
walks along the ground and shoots a laser gun at aliens coming from the sides. Image size 1536x1024.
All characters face RIGHT. Every frame of the same character is the same size, standing on the same
baseline. Arrange in rows like this, frames evenly spaced left to right:

Row 1 — space warrior idle: 4 frames. A brave little space soldier in a white-and-orange armored
space suit with a glass helmet visor, holding a chunky futuristic laser rifle.
Row 2 — space warrior walk: 6 frames (full walk cycle, rifle held ready).
Row 3 — space warrior shoot: 4 frames (aim, fire with a bright cyan muzzle flash, recoil, recover),
then 2 frames of getting hit (flinch, flashing).
Row 4 — laser: 3 frames of a cyan laser bolt (long thin glowing beam with a bright core), then
4 frames of the laser impact burst.
Row 5 — alien A walk: 4 frames (small green one-eyed blob alien hopping on tiny legs), then
4 frames of alien A defeated (squash, pop into green goo and sparkles).
Row 6 — alien B fly: 4 frames (purple flying saucer with a little tentacled alien in a glass dome,
lights blinking), then 4 frames of alien B exploding.
Row 7 — alien C crawl: 4 frames (red many-legged crawler alien with antennae), then 4 frames of
alien C defeated.
Row 8 — 6 frames of a generic pixel explosion (small flash, fireball, smoke puffs, fading).

The warrior is about twice as tall as alien A. Cute, game-like, not gory.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

---

## 11. Futbol sahası ve tribünler → `scenes\stadium.png`

```
Create a pixel-art sprite sheet for a side-scrolling football (soccer) stadium strip that will sit
beside a desktop taskbar. Image size 1536x1024 (landscape).

ROW 1 (top, about 1450x250 px): a long horizontal, SEAMLESSLY TILEABLE strip (left edge continues
perfectly into the right edge), seen from the side: tall stadium stands packed with a colorful
cheering crowd (tiny pixel people, some waving scarves and flags in generic colors, no logos), roof
with floodlight towers, advertising boards with plain colored panels (no text, no logos) along the
pitch. The bottom 30% is the green grass pitch with alternating light/dark mowing stripes and a white
touchline, its top edge perfectly horizontal at the same height everywhere (players will run on it).
ROW 2: the SAME strip at a night match — dark sky, bright floodlights, crowd with phone lights, same
layout so both rows line up pixel for pixel. Then 4 small frames of a crowd section doing a wave
(people standing up and sitting down in sequence).
ROW 3: left goal and right goal seen from the side (white posts and net, each about 200x200),
left end cap and right end cap of the stands, then 4 separate props: corner flag, substitutes'
bench, scoreboard with a blank dark screen, floodlight tower.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

---

## 12. Futbolcular ve top → `actors\football.png`

Bu karakterler 11'deki sahada koşup topla oynayacak.

```
Create an animation sprite sheet of small side-view pixel-art football (soccer) players and a ball,
for a tiny animated match. Image size 1536x1024. All players face RIGHT. Every frame of the same
player is the same size, feet on the same baseline. Cute chibi proportions (big head, small body).
Generic kits only: no logos, no numbers, no text. Arrange in rows, frames evenly spaced left to right:

Row 1 — red-team player run: 6 frames (red shirt, white shorts, red socks, full run cycle).
Row 2 — red-team player: 4 frames kicking the ball (wind-up, kick, follow-through, recover), then
2 frames celebrating a goal (arms up, jumping).
Row 3 — blue-team player run: 6 frames (blue shirt, white shorts, blue socks, full run cycle).
Row 4 — blue-team player: 4 frames kicking, then 2 frames celebrating.
Row 5 — goalkeeper (green jersey, gloves): 2 frames idle ready stance, 4 frames diving to the side
to save, 2 frames catching the ball.
Row 6 — referee (black kit): 4 frames running, 2 frames blowing the whistle with a raised arm.
Row 7 — ball: 4 frames of a classic black-and-white football rolling (rotation), 2 frames of the ball
squashed on a bounce, 1 small round ground shadow.
Row 8 — 4 frames of a small dust/grass puff (when a player kicks), 4 frames of confetti burst.

A player is about 4 times taller than the ball.

STYLE: authentic 16-bit pixel art (SNES / Stardew Valley quality), crisp hard-edged square pixels,
no anti-aliasing, no blur, no gradients except dithering, limited palette (about 24–32 colors),
clean 1-pixel dark outlines, consistent pixel size across the whole image, side view.
BACKGROUND: the entire background must be one flat solid pure magenta color #FF00FF — no gradient,
no shadow, no texture, no floor, no vignette. Magenta must not appear anywhere inside the artwork.
LAYOUT: every element/frame stands alone with generous magenta space around it; nothing touches or
overlaps; no grid lines, no borders, no labels, no text, no numbers, no watermark.
```

---

## Bitince
Tüm dosyaları yukarıdaki adlarla kaydettikten sonra bana "görseller hazır" de. Hepsi gelmek zorunda
değil, hangileri hazırsa onlarla başlarım.

Benim yapacaklarım:
- arka planı silmek, kareleri kesmek, petler için 96 px hücreli 768×768 tabloya çevirmek;
- manzaraları `assets\<sahne>\` altına şerit, uç parça ve süs olarak bölmek, JSON'larını yazmak;
- Ayarlar'a yeni pet ve manzara seçeneklerini eklemek;
- uzay savaşçısı ile futbol maçını animasyon olarak kodlamak.
