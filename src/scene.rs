//! Görev çubuğunun yanındaki pixel art manzaralar (dağlar, İstanbul, Ay…) ve bazılarında
//! ara sıra oynayan kısa sahneler: Ay'da savaşçı-uzaylı çatışması, stadyumda gol atağı.
//!
//! Manzaralar iki türlü çizilir:
//! - "segments": şeridin benzersiz parçaları (cami, Dünya, kulübe…) ekranda yalnızca bir kez,
//!   çubuğun iki yanına dağıtılarak çizilir; geri kalan yerde yalnızca zemin bandı devam eder.
//! - "tile": tekrarı doğal olan şeritler (stadyum tribünü) döşenir.
//!
//! Varlıklar `tools/assets.py` ile ChatGPT sayfalarından üretilir (assets/scenes, assets/actors).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Deserialize;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;

use crate::app::Ctx;
use crate::gfx::{Rect, Surface};
use crate::sprites::{Part, Size};

/// (ayar anahtarı, görünen ad)
pub const SCENES: [(&str, &str); 7] = [
    ("mountains", "Dağlar"),
    ("winter_cozy", "Karlı köy"),
    ("istanbul", "İstanbul"),
    ("antalya", "Antalya"),
    ("moon", "Ay"),
    ("space", "Uzay"),
    ("stadium", "Stadyum"),
];

pub fn is_scene(key: &str) -> bool {
    SCENES.iter().any(|(k, _)| *k == key)
}

#[derive(Deserialize, Clone, Debug)]
struct Prop {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    #[allow(dead_code)]
    name: String,
    #[serde(default)]
    fly: bool,
}

#[derive(Deserialize, Clone, Debug)]
struct Meta {
    mode: String,
    w: f32,
    h: f32,
    walk: f32,
    night: bool,
    caps: Vec<Part>,
    props: Vec<Prop>,
    #[serde(default)]
    extra: HashMap<String, Part>,
    #[serde(default)]
    ground: Option<Size>,
    /// Soldan yumuşayan parçalar (panoramada; eski biçimde iki yandan)
    #[serde(default)]
    segments: Vec<Part>,
    /// Sağdan yumuşayan aynı parçalar (yoksa `segments` kullanılır)
    #[serde(default)]
    segments_r: Vec<Part>,
    #[serde(default)]
    fade: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
struct Anim {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    frames: u32,
}

#[derive(Deserialize, Clone, Debug)]
struct ActorMeta {
    /// Atlas pikselinin DIP karşılığı: 1/scale (46 DIP yüksekliğindeki manzarada)
    scale: f32,
    anims: HashMap<String, Anim>,
}

/// (ana gündüz, ana gece, parçalar gündüz, parçalar gece, süsler, json)
/// "tile" için ana = şerit; "segments" için ana = zemin bandı, parçalar = benzersiz parçalar atlası.
type Files = (&'static [u8], Option<&'static [u8]>, Option<&'static [u8]>, Option<&'static [u8]>, &'static [u8], &'static str);

macro_rules! asset {
    ($n:literal, $f:literal) => {
        include_bytes!(concat!("../assets/scenes/", $n, "/", $f)).as_slice()
    };
}

macro_rules! json {
    ($n:literal) => {
        include_str!(concat!("../assets/scenes/", $n, "/scene.json"))
    };
}

fn files(name: &str) -> Option<Files> {
    Some(match name {
        "stadium" => (asset!("stadium", "day.png"), Some(asset!("stadium", "night.png")), None, None, asset!("stadium", "parts.png"), json!("stadium")),
        "mountains" => (
            asset!("mountains", "ground_day.png"),
            Some(asset!("mountains", "ground_night.png")),
            Some(asset!("mountains", "segs_day.png")),
            Some(asset!("mountains", "segs_night.png")),
            asset!("mountains", "parts.png"),
            json!("mountains"),
        ),
        "istanbul" => (
            asset!("istanbul", "ground_day.png"),
            Some(asset!("istanbul", "ground_night.png")),
            Some(asset!("istanbul", "segs_day.png")),
            Some(asset!("istanbul", "segs_night.png")),
            asset!("istanbul", "parts.png"),
            json!("istanbul"),
        ),
        "antalya" => (
            asset!("antalya", "ground_day.png"),
            Some(asset!("antalya", "ground_night.png")),
            Some(asset!("antalya", "segs_day.png")),
            Some(asset!("antalya", "segs_night.png")),
            asset!("antalya", "parts.png"),
            json!("antalya"),
        ),
        "winter_cozy" => (asset!("winter_cozy", "ground_day.png"), None, Some(asset!("winter_cozy", "segs_day.png")), None, asset!("winter_cozy", "parts.png"), json!("winter_cozy")),
        "moon" => (asset!("moon", "ground_day.png"), None, Some(asset!("moon", "segs_day.png")), None, asset!("moon", "parts.png"), json!("moon")),
        "space" => (asset!("space", "ground_day.png"), None, Some(asset!("space", "segs_day.png")), None, asset!("space", "parts.png"), json!("space")),
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ShowKind {
    Space,
    Football,
}

fn show_of(name: &str) -> Option<(ShowKind, &'static [u8], &'static str)> {
    match name {
        "moon" | "space" => Some((ShowKind::Space, include_bytes!("../assets/actors/space_fighter.png"), include_str!("../assets/actors/space_fighter.json"))),
        "stadium" => Some((ShowKind::Football, include_bytes!("../assets/actors/football.png"), include_str!("../assets/actors/football.json"))),
        _ => None,
    }
}

pub struct SceneArt {
    main: ID2D1Bitmap,
    main_night: Option<ID2D1Bitmap>,
    segs: Option<ID2D1Bitmap>,
    segs_night: Option<ID2D1Bitmap>,
    parts: Option<ID2D1Bitmap>,
    meta: Meta,
    actors: Option<(ShowKind, ID2D1Bitmap, ActorMeta)>,
}

impl SceneArt {
    pub fn load(s: &Surface, name: &str) -> Option<SceneArt> {
        let (main, main_night, segs, segs_night, parts, json) = files(name)?;
        let actors = show_of(name).and_then(|(k, png, json)| Some((k, s.bitmap_from_bytes(png, 4096)?, serde_json::from_str(json).ok()?)));
        let bmp = |b: Option<&[u8]>| b.and_then(|b| s.bitmap_from_bytes(b, 4096));
        Some(SceneArt {
            main: s.bitmap_from_bytes(main, 4096)?,
            main_night: bmp(main_night),
            segs: bmp(segs),
            segs_night: bmp(segs_night),
            parts: s.bitmap_from_bytes(parts, 4096),
            meta: serde_json::from_str(json).ok()?,
            actors,
        })
    }
}

struct Rng(u32);

impl Rng {
    fn next(&mut self, m: u32) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 % m.max(1)
    }
    fn f(&mut self) -> f32 {
        self.next(10_000) as f32 / 10_000.0
    }
}

/// Görseli [x0, x1] aralığına döşer; kopyalar dönüşümlü aynalanır (birleşimler kusursuz kalır).
fn tile(s: &Surface, bmp: &ID2D1Bitmap, sw: f32, sh: f32, x0: f32, x1: f32, y: f32, k: f32, seed: f32) {
    let tw = sw * k;
    if tw < 1.0 || x1 <= x0 {
        return;
    }
    s.push_clip(Rect::new(x0, y - 1.0, x1 - x0, sh * k + 2.0));
    let phase = (x0 + seed).rem_euclid(2.0 * tw).floor();
    let mut cx = x0 - phase;
    let mut i = 0;
    while cx < x1 {
        s.sprite(bmp, Rect::new(0.0, 0.0, sw, sh), Rect::new(cx, y, tw + 0.5, sh * k), i % 2 == 1, 1.0);
        cx += tw;
        i += 1;
    }
    s.pop_clip();
}

pub fn draw_scene(s: &Surface, art: &SceneArt, show: &mut Show, panels: &[Rect], w: f32, h: f32, ctx: &Ctx, seed: i32) -> Option<f32> {
    let m = &art.meta;
    let night = m.night && !(6..20).contains(&ctx.now.hour);
    // Manzara çubuğun üst hizasından başlar, hiçbir zaman onu aşmaz
    let top = panels.iter().map(|p| p.y).fold(h, f32::min).clamp(0.0, h - 20.0);
    let sh = h - top;
    let k = sh / m.h;
    let walk_y = top + sh * m.walk;
    let segmented = m.mode != "tile";

    let mut spans = Vec::new();
    let mut sorted: Vec<Rect> = panels.to_vec();
    sorted.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut x = 0.0;
    for p in &sorted {
        spans.push((x, p.x - 8.0));
        x = p.x + p.w + 8.0;
    }
    spans.push((x, w));
    spans.retain(|&(a, b)| b - a >= 60.0);
    if spans.is_empty() {
        show.spans.clear();
        return None;
    }

    let main = if night { art.main_night.as_ref().unwrap_or(&art.main) } else { &art.main };
    let segs = if night { art.segs_night.as_ref().or(art.segs.as_ref()) } else { art.segs.as_ref() };

    // 1) Zemin (ya da döşenen şerit)
    let mut inner = Vec::new();
    for &(a, b) in &spans {
        let cap_l = m.caps.first().filter(|_| a > 1.0).map(|c| c.w * k);
        let cap_r = m.caps.get(1).filter(|_| b < w - 1.0).map(|c| c.w * k);
        let x0 = a + cap_l.map_or(0.0, |c| c * 0.5);
        let x1 = b - cap_r.map_or(0.0, |c| c * 0.5);
        inner.push((x0, x1));
        match (segmented, m.ground) {
            (true, Some(g)) => tile(s, main, g.w, g.h, x0, x1, h - g.h * k, k, seed as f32),
            _ => tile(s, main, m.w, m.h, x0, x1, top, k, seed as f32),
        }
    }

    // 2) Benzersiz parçalar: her biri bir kez, çubuğa bitişik başlayıp dışa doğru; sağ ve sol
    //    taraflara dönüşümlü dağıtılır, böylece iki yanda farklı manzara görünür
    let mut covered: Vec<Option<(f32, f32)>> = vec![None; spans.len()];
    if let (true, Some(bmp)) = (segmented, segs) {
        // (alan, imleç, yön): çubuğun sağındaki alan sağa, solundaki sola büyür
        let mut cursors: Vec<(usize, f32, f32)> = Vec::new();
        for (i, &(x0, x1)) in inner.iter().enumerate() {
            let (a, b) = spans[i];
            if a > 1.0 {
                cursors.push((i, x0, 1.0));
            } else if b < w - 1.0 {
                cursors.push((i, x1, -1.0));
            } else {
                cursors.push((i, (x0 + x1) / 2.0, 1.0));
            }
        }
        // Önce çubuğun sağı, sonra solu
        cursors.sort_by(|p, q| q.2.total_cmp(&p.2).then(p.0.cmp(&q.0)));
        let fade = m.fade * k;
        let one_sided = m.segments_r.len() == m.segments.len();
        let mut ci = 0;
        for (n, seg_l) in m.segments.iter().enumerate() {
            // Sıradaki, henüz dolmamış taraf (imleç alanın sonuna ulaşmadıysa). Son parça ekran
            // kenarına taşabilir; alan sınırında kırpılır, böylece boşluk kalmaz.
            let open = |c: &(usize, f32, f32)| {
                let (x0, x1) = inner[c.0];
                if c.2 > 0.0 {
                    c.1 < x1 - 1.0
                } else {
                    c.1 > x0 + 1.0
                }
            };
            let Some(slot) = (0..cursors.len()).map(|j| (ci + j) % cursors.len()).find(|&j| open(&cursors[j])) else { break };
            ci = slot + 1;
            let (si, cur, dir) = cursors[slot];
            let (x0, x1) = inner[si];
            // Sağa dizilen parça sol kenarından, sola dizilen sağ kenarından yumuşar (önceki parçanın
            // opak kenarının üstüne biner)
            let seg = if dir < 0.0 && one_sided { &m.segments_r[n] } else { seg_l };
            let sw = seg.w * k;
            let start = covered[si].is_none();
            let x = if dir > 0.0 { cur - if start { fade * 0.5 } else { fade } } else { cur - sw + if start { fade * 0.5 } else { fade } };
            s.push_clip(Rect::new(x0, top - 1.0, x1 - x0, sh + 2.0));
            s.sprite(bmp, Rect::new(seg.x, seg.y, seg.w, seg.h), Rect::new(x, top, sw, sh), false, 1.0);
            s.pop_clip();
            cursors[slot].1 = if dir > 0.0 { x + sw } else { x };
            covered[si] = Some(match covered[si] {
                Some((c0, c1)) => (c0.min(x), c1.max(x + sw)),
                None => (x, x + sw),
            });
        }
    }

    // 3) Uçlar, kaleler ve süsler
    let Some(parts) = &art.parts else { return Some(walk_y) };
    for (si, &(a, b)) in spans.iter().enumerate() {
        let (x0, x1) = inner[si];
        if let Some(c) = m.caps.first().filter(|_| a > 1.0) {
            s.sprite(parts, Rect::new(c.x, c.y, c.w, c.h), Rect::new(a, h - c.h * k, c.w * k, c.h * k), false, 1.0);
        }
        if let Some(c) = m.caps.get(1).filter(|_| b < w - 1.0) {
            s.sprite(parts, Rect::new(c.x, c.y, c.w, c.h), Rect::new(b - c.w * k, h - c.h * k, c.w * k, c.h * k), false, 1.0);
        }
        let pk = k * 0.8;
        let mut lo = x0 + 16.0;
        let mut hi = x1 - 24.0;
        if let (Some(gl), Some(gr)) = (m.extra.get("goall"), m.extra.get("goalr")) {
            let (gw, gh) = (gl.w * pk, gl.h * pk);
            s.sprite(parts, Rect::new(gl.x, gl.y, gl.w, gl.h), Rect::new(x0 + 4.0, walk_y + 2.0 - gh, gw, gh), false, 1.0);
            s.sprite(parts, Rect::new(gr.x, gr.y, gr.w, gr.h), Rect::new(x1 - 4.0 - gr.w * pk, walk_y + 2.0 - gr.h * pk, gr.w * pk, gr.h * pk), false, 1.0);
            lo = x0 + gw + 30.0;
            hi = x1 - gr.w * pk - 30.0;
            show.goal = (gw, gh);
        }
        // Süsler benzersiz parçaların üstüne binmesin
        if let Some((c0, c1)) = covered[si] {
            if c0 - lo > hi - c1 {
                hi = hi.min(c0 - 10.0);
            } else {
                lo = lo.max(c1 + 10.0);
            }
        }
        if m.props.is_empty() || hi - lo < 30.0 {
            continue;
        }
        let mut rng = Rng((seed as u32 ^ (si as u32 + 1).wrapping_mul(2654435761)) | 1);
        let mut px = lo + rng.next(50) as f32;
        while px < hi {
            let p = &m.props[rng.next(m.props.len() as u32) as usize];
            // Zemin bandının üstü saydam: orada uçan süs havada asılı kalırdı
            if p.fly && segmented {
                px += 40.0;
                continue;
            }
            let (pw, ph) = (p.w * pk, p.h * pk);
            if px + pw > hi {
                break;
            }
            let y = if p.fly { top + sh * (0.05 + 0.15 * rng.f()) } else { walk_y + 2.0 - ph };
            s.sprite(parts, Rect::new(p.x, p.y, p.w, p.h), Rect::new(px, y, pw, ph), rng.next(2) == 0 && p.fly, 1.0);
            px += pw + 90.0 + rng.next(180) as f32;
        }
    }

    if let Some((kind, bmp, am)) = &art.actors {
        show.kind = Some(*kind);
        show.spans = inner;
        let ka = sh / 46.0 / am.scale;
        show.advance(ka);
        show.draw(s, bmp, am, h, walk_y, ka);
    }
    Some(walk_y)
}

// ---------------------------------------------------------------------------------------- sahne

#[derive(Clone)]
struct Actor {
    x: f32,
    dir: f32,
    anim: &'static str,
    t0: f32,
    /// Uzaylı türü: 0 yürüyen, 1 uçan daire, 2 sürüngen; futbol: rol
    kind: u8,
    dead: Option<f32>,
    team_blue: bool,
}

impl Actor {
    fn new(x: f32, dir: f32, anim: &'static str, t: f32) -> Actor {
        Actor { x, dir, anim, t0: t, kind: 0, dead: None, team_blue: false }
    }
    fn set(&mut self, anim: &'static str, t: f32) {
        if self.anim != anim {
            self.anim = anim;
            self.t0 = t;
        }
    }
}

struct Fx {
    x: f32,
    y: f32,
    anim: &'static str,
    t0: f32,
}

pub struct Show {
    pub enabled: bool,
    kind: Option<ShowKind>,
    spans: Vec<(f32, f32)>,
    active: bool,
    span: (f32, f32),
    t: f32,
    last: Instant,
    wait_until: Instant,
    rng: Rng,
    phase: u8,
    phase_t: f32,
    hero: Actor,
    foes: Vec<Actor>,
    bolts: Vec<(f32, f32)>,
    fx: Vec<Fx>,
    spawn_left: u32,
    next_spawn: f32,
    cooldown: f32,
    // Futbol
    keeper: Actor,
    ball_x: f32,
    ball_vx: f32,
    ball_air: Option<(f32, f32)>,
    ball_dist: f32,
    goal: (f32, f32),
}

const FPS: f32 = 10.0;

impl Show {
    pub fn new(enabled: bool, seed: u32) -> Show {
        let now = Instant::now();
        Show {
            enabled,
            kind: None,
            spans: Vec::new(),
            active: false,
            span: (0.0, 0.0),
            t: 0.0,
            last: now,
            wait_until: now + Duration::from_secs(6),
            rng: Rng(seed | 1),
            phase: 0,
            phase_t: 0.0,
            hero: Actor::new(0.0, 1.0, "idle", 0.0),
            foes: Vec::new(),
            bolts: Vec::new(),
            fx: Vec::new(),
            spawn_left: 0,
            next_spawn: 0.0,
            cooldown: 0.0,
            keeper: Actor::new(0.0, -1.0, "gk_idle", 0.0),
            ball_x: 0.0,
            ball_vx: 0.0,
            ball_air: None,
            ball_dist: 0.0,
            goal: (40.0, 30.0),
        }
    }

    /// Bir sonraki karede ne zaman çizilmeli (ms); sahne yoksa None.
    pub fn timer_ms(&self) -> Option<u32> {
        if !self.enabled || self.kind.is_none() || self.spans.is_empty() {
            return None;
        }
        if self.active {
            return Some(50);
        }
        Some((self.wait_until.saturating_duration_since(Instant::now()).as_millis() as u32).clamp(50, 30_000))
    }

    fn end(&mut self) {
        self.active = false;
        self.foes.clear();
        self.bolts.clear();
        self.fx.clear();
        self.wait_until = Instant::now() + Duration::from_secs(25 + self.rng.next(45) as u64);
    }

    fn advance(&mut self, ka: f32) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.25);
        self.last = now;
        if !self.enabled {
            self.active = false;
            return;
        }
        if !self.active {
            if now < self.wait_until {
                return;
            }
            let Some(&span) = self.spans.iter().filter(|(a, b)| b - a >= 240.0).max_by(|x, y| (x.1 - x.0).total_cmp(&(y.1 - y.0))) else {
                self.wait_until = now + Duration::from_secs(10);
                return;
            };
            self.span = span;
            self.start(ka);
            return;
        }
        self.t += dt;
        match self.kind {
            Some(ShowKind::Space) => self.tick_space(dt, ka),
            Some(ShowKind::Football) => self.tick_football(dt, ka),
            None => self.end(),
        }
    }

    fn start(&mut self, ka: f32) {
        self.active = true;
        self.t = 0.0;
        self.phase = 0;
        self.phase_t = 0.0;
        self.foes.clear();
        self.bolts.clear();
        self.fx.clear();
        let (a, b) = self.span;
        let dir = if self.rng.next(2) == 0 { 1.0 } else { -1.0 };
        let start = if dir > 0.0 { a - 20.0 } else { b + 20.0 };
        self.hero = Actor::new(start, dir, if self.kind == Some(ShowKind::Football) { "red_run" } else { "walk" }, 0.0);
        if self.kind == Some(ShowKind::Football) {
            let blue = self.rng.next(2) == 0;
            self.hero.team_blue = blue;
            self.hero.anim = if blue { "blue_run" } else { "red_run" };
            let mut def = Actor::new(if dir > 0.0 { b - self.goal.0 - 40.0 } else { a + self.goal.0 + 40.0 }, -dir, if blue { "red_run" } else { "blue_run" }, 0.0);
            def.team_blue = !blue;
            def.kind = 1;
            self.foes = vec![def];
            let gx = if dir > 0.0 { b - 4.0 - self.goal.0 * 0.55 } else { a + 4.0 + self.goal.0 * 0.55 };
            self.keeper = Actor::new(gx, -dir, "gk_idle", 0.0);
            self.ball_x = start + dir * 12.0 * ka * 3.0;
            self.ball_vx = 0.0;
            self.ball_air = None;
            self.ball_dist = 0.0;
        }
    }

    // ------------------------------------------------------------------ uzay
    fn tick_space(&mut self, dt: f32, ka: f32) {
        let (a, b) = self.span;
        let len = b - a;
        let t = self.t;
        let sp = 38.0;
        match self.phase {
            // Savaşçı yürüyerek gelir
            0 => {
                let target = if self.hero.dir > 0.0 { a + len * 0.3 } else { b - len * 0.3 };
                self.hero.x += self.hero.dir * sp * dt;
                if (self.hero.dir > 0.0 && self.hero.x >= target) || (self.hero.dir < 0.0 && self.hero.x <= target) {
                    self.phase = 1;
                    self.hero.set("idle", t);
                    self.spawn_left = 3 + self.rng.next(3);
                    self.next_spawn = t + 0.5;
                }
            }
            // Uzaylılar karşıdan gelir, savaşçı ateş eder
            1 => {
                if self.spawn_left > 0 && t >= self.next_spawn {
                    self.spawn_left -= 1;
                    self.next_spawn = t + 1.1 + self.rng.f() * 0.9;
                    let mut f = Actor::new(if self.hero.dir > 0.0 { b + 18.0 } else { a - 18.0 }, -self.hero.dir, "a_walk", t);
                    f.kind = self.rng.next(3) as u8;
                    f.anim = ["a_walk", "b_fly", "c_walk"][f.kind as usize];
                    self.foes.push(f);
                }
                for f in self.foes.iter_mut().filter(|f| f.dead.is_none()) {
                    let v = [24.0, 34.0, 18.0][f.kind as usize];
                    f.x += f.dir * v * dt;
                }
                self.cooldown -= dt;
                let hero_x = self.hero.x;
                let dir = self.hero.dir;
                let target = self.foes.iter().filter(|f| f.dead.is_none() && (f.x - hero_x) * dir > 0.0).map(|f| (f.x - hero_x).abs()).fold(f32::MAX, f32::min);
                if target < len * 0.75 && self.cooldown <= 0.0 {
                    self.cooldown = 0.7 + self.rng.f() * 0.3;
                    self.hero.set("shoot", t);
                    self.hero.t0 = t;
                    self.bolts.push((hero_x + dir * 16.0 * ka * 3.0, dir));
                } else if self.hero.anim == "shoot" && t - self.hero.t0 > 0.4 {
                    self.hero.set("idle", t);
                } else if self.hero.anim == "hit" && t - self.hero.t0 > 0.5 {
                    self.hero.set("idle", t);
                }
                // Lazerler
                let mut hits = Vec::new();
                self.bolts.retain_mut(|(x, d)| {
                    *x += *d * 230.0 * dt;
                    if let Some(i) = self_hit(&self.foes, *x) {
                        hits.push((i, *x));
                        return false;
                    }
                    *x > a - 30.0 && *x < b + 30.0
                });
                for (i, x) in hits {
                    if let Some(f) = self.foes.get_mut(i) {
                        f.dead = Some(t);
                        f.set(["a_die", "b_die", "c_die"][f.kind as usize], t);
                        self.fx.push(Fx { x, y: if f.kind == 1 { -1.0 } else { 0.0 }, anim: "impact", t0: t });
                    }
                }
                // Savaşçıya ulaşan uzaylı patlar
                for f in self.foes.iter_mut().filter(|f| f.dead.is_none()) {
                    if (f.x - hero_x).abs() < 12.0 {
                        f.dead = Some(t);
                        f.set(["a_die", "b_die", "c_die"][f.kind as usize], t);
                        self.fx.push(Fx { x: f.x, y: 0.0, anim: "boom", t0: t });
                        self.hero.set("hit", t);
                    }
                }
                self.foes.retain(|f| f.dead.is_none_or(|d| t - d < 0.6) && f.x > a - 40.0 && f.x < b + 40.0);
                self.fx.retain(|f| t - f.t0 < 0.7);
                if self.spawn_left == 0 && self.foes.is_empty() && self.bolts.is_empty() && t > self.next_spawn {
                    self.phase = 2;
                    self.phase_t = t;
                }
            }
            // Zafer: kısa bir duruş, sonra en yakın kenardan çıkış
            _ => {
                if t - self.phase_t < 0.8 {
                    self.hero.set("idle", t);
                    return;
                }
                let mid = (a + b) / 2.0;
                self.hero.dir = if self.hero.x < mid { -1.0 } else { 1.0 };
                self.hero.set("walk", t);
                self.hero.x += self.hero.dir * sp * 1.2 * dt;
                if self.hero.x < a - 30.0 || self.hero.x > b + 30.0 {
                    self.end();
                }
            }
        }
    }

    // ------------------------------------------------------------------ futbol
    fn tick_football(&mut self, dt: f32, ka: f32) {
        let (a, b) = self.span;
        let t = self.t;
        let dir = self.hero.dir;
        let run = if self.hero.team_blue { "blue_run" } else { "red_run" };
        let goal_line = if dir > 0.0 { b - 4.0 - self.goal.0 * 0.5 } else { a + 4.0 + self.goal.0 * 0.5 };
        match self.phase {
            // Topla ilerleme; savunmacı karşılar ve peşine düşer
            0 => {
                self.hero.x += dir * 46.0 * dt;
                self.ball_x = self.hero.x + dir * 12.0 * ka * 3.0;
                self.ball_dist += 46.0 * dt;
                if let Some(d) = self.foes.first_mut() {
                    let close = (d.x - self.hero.x).abs() < 22.0;
                    if d.dir != dir && (close || (d.x - self.hero.x) * dir < 0.0) {
                        d.dir = dir; // dönüp kovalar
                    }
                    let v = if d.dir == dir { 40.0 } else { 44.0 };
                    d.x += d.dir * v * dt;
                }
                if (goal_line - self.hero.x) * dir < 110.0 {
                    self.phase = 1;
                    self.phase_t = t;
                    self.hero.set(if self.hero.team_blue { "blue_kick" } else { "red_kick" }, t);
                }
            }
            // Şut
            1 => {
                if let Some(d) = self.foes.first_mut() {
                    d.x += d.dir * 30.0 * dt;
                }
                if self.ball_air.is_none() && t - self.phase_t > 0.22 {
                    let flight = ((goal_line - self.ball_x).abs() / 170.0).max(0.3);
                    self.ball_air = Some((t, flight));
                    self.ball_vx = (goal_line - self.ball_x) / flight;
                    self.fx.push(Fx { x: self.ball_x, y: 0.0, anim: "puff", t0: t });
                }
                if let Some((t0, flight)) = self.ball_air {
                    self.ball_x += self.ball_vx * dt;
                    self.ball_dist += self.ball_vx.abs() * dt;
                    // Kaleci top yaklaşınca atlar ama yetişemez
                    if (self.ball_x - self.keeper.x).abs() < 50.0 && self.keeper.anim == "gk_idle" {
                        self.keeper.set("gk_dive", t);
                    }
                    if t - t0 >= flight {
                        self.ball_air = None;
                        self.ball_vx = 0.0;
                        self.ball_x = goal_line + dir * 6.0;
                        self.phase = 2;
                        self.phase_t = t;
                        self.fx.push(Fx { x: goal_line, y: -1.0, anim: "confetti", t0: t });
                        self.hero.set(if self.hero.team_blue { "blue_cheer" } else { "red_cheer" }, t);
                    }
                }
            }
            // Gol sevinci
            2 => {
                if t - self.phase_t > 2.2 {
                    self.phase = 3;
                    self.hero.dir = -dir;
                    self.hero.set(run, t);
                    if let Some(d) = self.foes.first_mut() {
                        d.dir = -dir;
                    }
                }
            }
            // Herkes geri döner
            _ => {
                self.hero.x += self.hero.dir * 50.0 * dt;
                if let Some(d) = self.foes.first_mut() {
                    d.x += d.dir * 46.0 * dt;
                }
                if self.hero.x < a - 30.0 || self.hero.x > b + 30.0 {
                    self.end();
                }
            }
        }
        self.fx.retain(|f| t - f.t0 < 0.9);
    }

    // ------------------------------------------------------------------ çizim
    fn draw(&self, s: &Surface, bmp: &ID2D1Bitmap, am: &ActorMeta, h: f32, walk_y: f32, ka: f32) {
        if !self.active {
            return;
        }
        let (a, b) = self.span;
        let t = self.t;
        let frame = |name: &str, t0: f32, looped: bool, x: f32, bottom: f32, flip: bool| {
            let Some(an) = am.anims.get(name) else { return };
            let n = an.frames.max(1);
            let i = ((t - t0).max(0.0) * FPS) as u32;
            let i = if looped { i % n } else { i.min(n - 1) };
            let (fw, fh) = (an.w * ka, an.h * ka);
            let src = Rect::new(an.x + i as f32 * an.w, an.y, an.w, an.h);
            s.sprite(bmp, src, Rect::new((x - fw / 2.0).round(), (bottom - fh).round(), fw, fh), flip, 1.0);
        };
        s.push_clip(Rect::new(a, 0.0, b - a, h));
        let ground = walk_y + 1.0;
        match self.kind {
            Some(ShowKind::Space) => {
                for f in &self.foes {
                    let y = if f.kind == 1 { ground - 18.0 * ka * 3.0 } else { ground };
                    frame(f.anim, f.t0, f.dead.is_none(), f.x, y, f.dir < 0.0);
                }
                let hero_loop = !matches!(self.hero.anim, "shoot" | "hit");
                frame(self.hero.anim, self.hero.t0, hero_loop, self.hero.x, ground, self.hero.dir < 0.0);
                let gun_y = ground - 11.0 * ka * 3.0;
                for &(x, d) in &self.bolts {
                    frame("laser", 0.0, true, x, gun_y + 4.0 * ka * 3.0, d < 0.0);
                }
                for f in &self.fx {
                    let y = if f.y < 0.0 { ground - 12.0 * ka * 3.0 } else { ground - 2.0 };
                    frame(f.anim, f.t0, false, f.x, y, false);
                }
            }
            Some(ShowKind::Football) => {
                frame(self.keeper.anim, self.keeper.t0, self.keeper.anim == "gk_idle", self.keeper.x, ground, self.keeper.dir < 0.0);
                for d in &self.foes {
                    frame(d.anim, d.t0, true, d.x, ground, d.dir < 0.0);
                }
                let kick = self.hero.anim.ends_with("_kick");
                frame(self.hero.anim, self.hero.t0, !kick, self.hero.x, ground, self.hero.dir < 0.0);
                // Top: şut karesinde top zaten çizimde; diğer anlarda ayrı çiz
                if !(kick && self.ball_air.is_none()) {
                    let lift = match self.ball_air {
                        Some((t0, fl)) => {
                            let u = ((t - t0) / fl).clamp(0.0, 1.0);
                            4.0 * u * (1.0 - u) * 16.0 * ka * 3.0
                        }
                        None => 0.0,
                    };
                    if lift > 0.5 {
                        frame("shadow", 0.0, true, self.ball_x, ground + 1.0, false);
                    }
                    if let Some(an) = am.anims.get("ball") {
                        let i = (self.ball_dist / (4.0 * ka * 3.0)) as u32 % an.frames.max(1);
                        let (fw, fh) = (an.w * ka, an.h * ka);
                        let src = Rect::new(an.x + i as f32 * an.w, an.y, an.w, an.h);
                        s.sprite(bmp, src, Rect::new((self.ball_x - fw / 2.0).round(), (ground - fh - lift).round(), fw, fh), self.hero.dir < 0.0, 1.0);
                    }
                }
                for f in &self.fx {
                    let y = if f.y < 0.0 { ground - self.goal.1 * 0.6 } else { ground };
                    frame(f.anim, f.t0, false, f.x, y, false);
                }
            }
            None => {}
        }
        s.pop_clip();
    }
}

fn self_hit(foes: &[Actor], x: f32) -> Option<usize> {
    foes.iter().position(|f| f.dead.is_none() && (f.x - x).abs() < 9.0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_scenes_parse() {
        for (k, _) in super::SCENES {
            let (_, _, _, _, _, json) = super::files(k).unwrap();
            let m: super::Meta = serde_json::from_str(json).unwrap();
            assert!(m.caps.is_empty() || m.caps.len() == 2, "{k}");
            assert!(!m.props.is_empty(), "{k}");
            assert!(m.walk > 0.5 && m.walk < 1.0, "{k}");
            if m.mode == "segments" {
                assert!(m.segments.len() >= 2 && m.ground.is_some(), "{k}");
            }
        }
        for (_, _, json) in [super::show_of("moon").unwrap(), super::show_of("stadium").unwrap()] {
            let a: super::ActorMeta = serde_json::from_str(json).unwrap();
            assert!(a.anims.len() >= 13);
        }
    }
}
