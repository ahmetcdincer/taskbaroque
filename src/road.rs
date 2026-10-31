use std::time::{Duration, Instant};

use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;

use crate::app::Ctx;
use crate::gfx::{Color, Rect, Surface};
use crate::sprites::{self, Part, RoadMeta};
use crate::weather::Sky;

const SPEED: f32 = 46.0;
const FRAME_MS: u32 = 66;
const PROPS: [&str; 11] = ["lamp", "sign", "cone", "bush", "tree", "fence", "mailbox", "bench", "flowers", "rock", "busstop"];

pub struct RoadArt {
    day: ID2D1Bitmap,
    night: Option<ID2D1Bitmap>,
    snow: Option<ID2D1Bitmap>,
    tunnels: Option<ID2D1Bitmap>,
    props: Option<ID2D1Bitmap>,
    car: Option<ID2D1Bitmap>,
    meta: RoadMeta,
}

impl RoadArt {
    pub fn load(s: &Surface) -> Option<RoadArt> {
        Some(RoadArt {
            day: s.bitmap_from_bytes(sprites::ROAD_DAY, 4096)?,
            night: s.bitmap_from_bytes(sprites::ROAD_NIGHT, 4096),
            snow: s.bitmap_from_bytes(sprites::ROAD_SNOW, 4096),
            tunnels: s.bitmap_from_bytes(sprites::ROAD_TUNNELS, 4096),
            props: s.bitmap_from_bytes(sprites::ROAD_PROPS, 4096),
            car: s.bitmap_from_bytes(sprites::ROAD_CAR, 4096),
            meta: sprites::road_meta()?,
        })
    }
}

pub struct Car {
    pub enabled: bool,
    active: bool,
    x: f32,
    dir: f32,
    dist: f32,
    yellow: bool,
    wait_until: Instant,
    last: Instant,
    spans: Vec<(f32, f32)>,
    rng: u32,
}

impl Car {
    pub fn new(enabled: bool, seed: u32) -> Car {
        Car {
            enabled,
            active: false,
            x: 0.0,
            dir: 1.0,
            dist: 0.0,
            yellow: false,
            wait_until: Instant::now() + Duration::from_secs(4),
            last: Instant::now(),
            spans: Vec::new(),
            rng: seed | 1,
        }
    }

    fn next(&mut self, m: u32) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng % m.max(1)
    }

    fn advance(&mut self, w: f32, half: f32, boost: f32, online: bool) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(30.0);
        self.last = now;
        if !self.active {
            if now < self.wait_until || self.spans.is_empty() || !online {
                return;
            }
            self.active = true;
            self.dir = if self.next(2) == 0 { 1.0 } else { -1.0 };
            self.x = if self.dir > 0.0 { -half } else { w + half };
            self.yellow = self.next(3) == 0;
            self.dist = 0.0;
            return;
        }
        self.x += self.dir * SPEED * boost * dt;
        self.dist += SPEED * boost * dt;
        if self.x < -half - 1.0 || self.x > w + half + 1.0 {
            self.active = false;
            self.wait_until = now + Duration::from_secs(3 + self.next(9) as u64);
        }
    }

    pub fn timer_ms(&self) -> Option<u32> {
        if !self.enabled || self.spans.is_empty() {
            return None;
        }
        let now = Instant::now();
        if !self.active {
            return Some((self.wait_until.saturating_duration_since(now).as_millis() as u32).max(FRAME_MS));
        }
        if self.spans.iter().any(|&(a, b)| self.x >= a && self.x <= b) {
            return Some(FRAME_MS);
        }
        let gap = self
            .spans
            .iter()
            .filter_map(|&(a, b)| if self.dir > 0.0 { (a > self.x).then(|| a - self.x) } else { (b < self.x).then(|| self.x - b) })
            .fold(f32::MAX, f32::min);
        if gap == f32::MAX {
            return Some(FRAME_MS);
        }
        Some(((gap / SPEED * 1000.0) as u32).clamp(FRAME_MS, 30_000))
    }
}

struct Tunnel {
    x: f32,
    piece: Part,
    flip: bool,
    hole: (f32, f32),
    floor: f32,
}

fn tile_strip(s: &Surface, strip: &ID2D1Bitmap, x0: f32, x1: f32, gy: f32, sw: f32, sh: f32, k: f32, mirrored: bool) {
    let tw = sw * k;
    if mirrored {
        let mut tx = x1;
        while tx > x0 + 0.5 {
            let part = tw.min(tx - x0);
            s.sprite(strip, Rect::new(0.0, 0.0, part / k, sh), Rect::new(tx - part, gy, part, sh * k), true, 1.0);
            tx -= part;
        }
    } else {
        let mut tx = x0;
        while tx < x1 - 0.5 {
            let part = tw.min(x1 - tx);
            s.sprite(strip, Rect::new(0.0, 0.0, part / k, sh), Rect::new(tx, gy, part, sh * k), false, 1.0);
            tx += part;
        }
    }
}

pub fn draw_road(s: &Surface, art: &RoadArt, car: &mut Car, panels: &[Rect], w: f32, h: f32, ctx: &Ctx, seed: i32) -> Option<f32> {
    let m = &art.meta;
    let k = 1.0 / m.zoom;
    let sky = ctx.weather.map(|w| w.sky());
    let snowy = sky == Some(Sky::Snow);
    let night = !(6..20).contains(&ctx.now.hour);
    let (sw, sh) = (m.strip_day.w, m.strip_day.h);
    let gh = sh * k;
    let gy = h - gh;
    let back_y = gy + gh * m.lines.back;
    let road_y = gy + gh * m.lines.road;
    let walk_y = gy + gh * m.lines.walk;
    let (variant, strip) = match (snowy, night) {
        (true, _) => ("snow", art.snow.as_ref().unwrap_or(&art.day)),
        (_, true) => ("night", art.night.as_ref().unwrap_or(&art.day)),
        _ => ("day", &art.day),
    };
    let tset = m.tunnels.get(variant).or_else(|| m.tunnels.get("day"));

    let mut spans = Vec::new();
    let mut sorted: Vec<Rect> = panels.to_vec();
    sorted.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut x = 0.0;
    for p in &sorted {
        spans.push((x, p.x - 8.0));
        x = p.x + p.w + 8.0;
    }
    spans.push((x, w));
    spans.retain(|&(a, b)| b - a >= 90.0);
    if spans.is_empty() {
        car.spans.clear();
        return None;
    }

    let mut tunnels = Vec::new();
    for (si, &(a, b)) in spans.iter().enumerate() {
        let mut x0 = a;
        let mut x1 = b;
        let mut mirrored = false;
        if let Some(t) = tset {
            let pw = t.piece.w * k;
            let top = h - t.piece.h * k;
            let floor = top + t.open_y[1] * k;
            if b < w - 1.0 {
                let tx = b - pw;
                tunnels.push(Tunnel { x: tx, piece: t.piece, flip: true, hole: (tx + pw - t.open[1] * k, tx + pw - t.open[0] * k), floor });
                x1 = tx;
                mirrored = true;
            }
            if a > 1.0 {
                tunnels.push(Tunnel { x: a, piece: t.piece, flip: false, hole: (a + t.open[0] * k, a + t.open[1] * k), floor });
                x0 = a + pw;
                mirrored = false;
            }
        }
        tile_strip(s, strip, x0, x1, gy, sw, sh, k, mirrored);

        if let Some(bmp) = &art.props {
            let mut rng = (seed as u32 ^ (si as u32 + 1).wrapping_mul(2654435761)) | 1;
            let mut next = |n: u32| {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                rng % n
            };
            let mut px = x0 + 14.0 + next(50) as f32;
            while px < x1 - 24.0 {
                let mut name = PROPS[next(PROPS.len() as u32) as usize];
                if name == "lamp" && night {
                    name = "lamp_on";
                }
                if let Some(p) = m.props.get(name) {
                    let dst = Rect::new(px, back_y + 1.0 - p.h * k, p.w * k, p.h * k);
                    if dst.x + dst.w < x1 {
                        s.sprite(bmp, Rect::new(p.x, p.y, p.w, p.h), dst, false, 1.0);
                    }
                    px += p.w * k;
                }
                px += 70.0 + next(150) as f32;
            }
        }
    }

    if let Some(bmp) = &art.tunnels {
        for t in &tunnels {
            let p = t.piece;
            s.sprite(bmp, Rect::new(p.x, p.y, p.w, p.h), Rect::new(t.x, h - p.h * k, p.w * k, p.h * k), t.flip, 1.0);
        }
    }

    let frames = m.car.get(if night { "night" } else if car.yellow { "yellow" } else { "green" });
    let body = m.car_body_w * k;
    car.spans = spans.iter().map(|&(a, b)| (a - body / 2.0, b + body / 2.0)).collect();
    if car.enabled {
        let boost = 1.0 + ((ctx.sys.cpu - 60.0) / 40.0).clamp(0.0, 1.0) * 1.5;
        car.advance(w, body / 2.0 + 30.0, boost, ctx.feeds.online);
    }
    if let (true, true, Some(bmp), Some(frames)) = (car.enabled, car.active, &art.car, frames) {
        if let (Some((rise, clip, floor)), false) = (car_pose(car.x, &spans, &tunnels, w, h), frames.is_empty()) {
            let f = frames[(car.dist / 5.0) as usize % frames.len()];
            let z = 1.0 - 0.15 * rise;
            let (fw, fh) = (f.w * k * z, f.h * k * z);
            let bw = body * z;
            let left = if car.dir > 0.0 { car.x - bw / 2.0 } else { car.x + bw / 2.0 - fw };
            let bottom = road_y + (floor - road_y) * rise;
            let dst = Rect::new(left.round(), (bottom - fh).round(), fw, fh);
            if let Some(c) = clip {
                s.push_clip(c);
            }
            s.sprite(bmp, Rect::new(f.x, f.y, f.w, f.h), dst, car.dir < 0.0, 1.0);
            if clip.is_some() {
                s.pop_clip();
            }
        }
    }

    if !ctx.feeds.online {
        if let (Some(bmp), Some(p)) = (&art.props, m.props.get("cone")) {
            for t in &tunnels {
                let x = if t.flip { t.hole.0 - 4.0 - p.w * k } else { t.hole.1 + 4.0 };
                s.sprite(bmp, Rect::new(p.x, p.y, p.w, p.h), Rect::new(x, road_y - p.h * k, p.w * k, p.h * k), false, 1.0);
            }
        }
    }

    if night {
        let c = Color::rgba(255, 236, 140, 255);
        for (si, &(a, b)) in spans.iter().enumerate() {
            let mut rng = (seed as u32 ^ (si as u32 + 7).wrapping_mul(2246822519)) | 1;
            for _ in 0..((b - a) / 140.0) as u32 {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let fx = a + (rng % (b - a).max(1.0) as u32) as f32;
                let fy = back_y - 8.0 - (rng >> 8) as f32 % 12.0;
                s.fill_circle(fx, fy, 3.5, c.alpha(0.18));
                s.fill_circle(fx, fy, 1.4, c);
            }
        }
    }
    Some(walk_y)
}

fn car_pose(cx: f32, spans: &[(f32, f32)], tunnels: &[Tunnel], w: f32, h: f32) -> Option<(f32, Option<Rect>, f32)> {
    let &(a, b) = spans.iter().find(|&&(a, b)| cx >= a - 40.0 && cx <= b + 40.0)?;
    let Some(t) = tunnels.iter().find(|t| t.x >= a - 1.0 && t.x <= b) else { return Some((0.0, None, 0.0)) };
    let (h0, h1) = t.hole;
    let hc = (h0 + h1) / 2.0;
    let out = if t.flip { hc - cx } else { cx - hc };
    let rise = (1.0 - out / 36.0).clamp(0.0, 1.0);
    let clip = if t.flip { Rect::new(-100.0, 0.0, h1 + 100.0, h) } else { Rect::new(h0, 0.0, w - h0 + 100.0, h) };
    if (t.flip && cx - 20.0 > h1) || (!t.flip && cx + 20.0 < h0) {
        return None;
    }
    Some((rise, Some(clip), t.floor))
}
