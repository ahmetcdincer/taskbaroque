use std::f32::consts::PI;
use std::time::Instant;

use windows::core::Result;
use windows::Win32::Foundation::*;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, ReleaseCapture, SetCapture, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::{create_layered, Action, Ctx};
use crate::config::{Palette, PetCfg};
use crate::gfx::{Align, Color, Rect, Surface};
use crate::monitors::Monitor;
use crate::sprites::{self, PetSheet};
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use crate::weather::Sky;

const W: f32 = 96.0;
const H: f32 = 96.0;
const BASE: f32 = H - 4.0;
const TIMER: usize = 1;
const GRAVITY: f32 = 1500.0;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Chameleon,
    Firefly,
    Fox,
    Hedgehog,
    Puppy,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    Idle,
    Walk { to: f32 },
    Look,
    Sleep,
    Air,
    Drag,
    Land(f32),
}

#[derive(Clone, Copy, Debug)]
pub struct Ground {
    pub left: f32,
    pub right: f32,
    pub y: f32,
    pub floor: f32,
    pub min_x: f32,
    pub max_x: f32,
    pub wide: bool,
}

pub struct Pet {
    pub hwnd: HWND,
    surf: Surface,
    kind: Kind,
    size: f32,
    x: f32,
    y: f32,
    vy: f32,
    facing: f32,
    state: State,
    t: f32,
    think: f32,
    last: Instant,
    rng: u32,
    drag: Option<(POINT, f32, f32, bool)>,
    hearts: f32,
    interval: u32,
    visible: bool,
    rush: bool,
    tongue: f32,
    tongue_to: (f32, f32),
    roll: f32,
    roll_angle: f32,
    alert: f32,
    sheet: Option<(ID2D1Bitmap, PetSheet)>,
    special: f32,
    petting: f32,
    rub: f32,
    last_hover: Option<POINT>,
}

const SPECIAL_SECS: f32 = 0.8;
const SPRITE_SIZE: f32 = 64.0;

impl Pet {
    pub fn new(cfg: &PetCfg, ctx: &Ctx, mon: &Monitor, ground: Ground) -> Result<Pet> {
        let kind = match cfg.kind.as_str() {
            "firefly" => Kind::Firefly,
            "fox" => Kind::Fox,
            "hedgehog" => Kind::Hedgehog,
            "puppy" => Kind::Puppy,
            _ => Kind::Chameleon,
        };
        let size = cfg.size.clamp(0.5, 2.0);
        let hwnd = create_layered(WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE)?;
        let surf = Surface::new(W * size, H * size, mon.scale, &ctx.pal.font)?;
        let seed = unsafe { GetTickCount() } | 1;
        let mut p = Pet {
            hwnd,
            surf,
            kind,
            size,
            x: (ground.left + ground.right) / 2.0,
            y: ground.y - 120.0 * mon.scale,
            vy: 0.0,
            facing: 1.0,
            state: State::Air,
            t: 0.0,
            think: 1.5,
            last: Instant::now(),
            rng: seed,
            drag: None,
            hearts: 0.0,
            interval: 0,
            visible: true,
            rush: false,
            tongue: 0.0,
            tongue_to: (0.0, 0.0),
            roll: 0.0,
            roll_angle: 0.0,
            alert: 0.0,
            sheet: None,
            special: 0.0,
            petting: 0.0,
            rub: 0.0,
            last_hover: None,
        };
        if let Some((png, meta)) = sprites::pet(&cfg.kind) {
            p.sheet = p.surf.bitmap_from_bytes(png, 4096).map(|b| (b, meta));
        }
        p.draw(ctx);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        p.set_interval(33);
        Ok(p)
    }

    pub fn x(&self) -> f32 {
        self.x
    }

    fn sc(&self) -> f32 {
        self.surf.scale
    }

    fn flying(&self) -> bool {
        self.kind == Kind::Firefly
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x % 10_000) as f32 / 10_000.0
    }

    fn set_interval(&mut self, ms: u32) {
        if self.interval != ms {
            self.interval = ms;
            unsafe {
                SetTimer(self.hwnd, TIMER, ms, None);
            }
        }
    }

    pub fn set_visible(&mut self, v: bool) {
        if v != self.visible {
            self.visible = v;
            unsafe {
                let _ = ShowWindow(self.hwnd, if v { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
            self.last = Instant::now();
        }
    }

    pub fn go_to(&mut self, x: f32) {
        if matches!(self.state, State::Idle | State::Look | State::Walk { .. } | State::Sleep) {
            self.state = State::Walk { to: x };
            self.rush = true;
        }
    }

    pub fn alert(&mut self) {
        self.alert = 3.5;
        if self.state != State::Drag {
            if self.state == State::Sleep {
                self.state = State::Idle;
            }
            self.vy = -430.0;
            self.state = State::Air;
        }
        self.set_interval(33);
    }

    pub fn keep_on_top(&self) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    fn decide(&mut self, g: &Ground, now_hour: u16) {
        let idle = idle_seconds();
        let night = now_hour < 6;
        if idle > 120.0 || (night && self.rand() < 0.4) {
            self.state = State::Sleep;
            return;
        }
        let r = self.rand();
        let margin = 24.0 * self.sc() * self.size;
        self.rush = false;
        let (lo, hi) = if g.wide && self.rand() < 0.65 { (g.min_x, g.max_x) } else { (g.left, g.right) };
        self.state = if r < 0.5 && hi - lo > 2.0 * margin {
            let to = lo + margin + self.rand() * (hi - lo - 2.0 * margin);
            State::Walk { to }
        } else if r < 0.75 {
            State::Look
        } else {
            State::Idle
        };
        self.think = 2.0 + self.rand() * 4.0;
    }

    fn speed(&self, busy: bool) -> f32 {
        let base = match self.kind {
            Kind::Chameleon => 22.0,
            Kind::Firefly => 55.0,
            Kind::Hedgehog => 30.0,
            _ => 42.0,
        };
        let boost = if self.rush { 2.2 } else if busy { 1.8 } else { 1.0 };
        base * boost * self.sc() * self.size
    }

    pub fn step(&mut self, ctx: &Ctx, g: Ground) {
        if !self.visible {
            return;
        }
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        self.t += dt;
        self.hearts = (self.hearts - dt).max(0.0);
        self.alert = (self.alert - dt).max(0.0);
        self.tongue = (self.tongue - dt).max(0.0);
        self.special = (self.special - dt).max(0.0);
        self.petting = (self.petting - dt).max(0.0);
        self.rub *= (-dt * 1.5).exp();
        if self.roll > 0.0 {
            self.roll = (self.roll - dt).max(0.0);
            self.roll_angle += dt * 14.0 * self.facing;
        }
        let sc = self.sc();
        let busy = ctx.sys.cpu > 85.0;
        let on_bar = self.x >= g.left && self.x <= g.right;
        let mut floor = if on_bar { g.y } else { g.floor };
        let flying = self.flying();
        if flying && self.state != State::Sleep {
            floor -= (2.0 + (self.t * 1.4).sin() * 4.0) * sc * self.size;
        }

        match self.state {
            State::Drag => {}
            State::Air if flying => {
                self.y += (floor - self.y) * (dt * 4.0).min(1.0);
                if (self.y - floor).abs() < 1.5 {
                    self.state = State::Idle;
                }
            }
            State::Air => {
                self.vy += GRAVITY * dt;
                self.y += self.vy * dt * sc;
                if self.vy >= 0.0 && self.y >= floor {
                    self.y = floor;
                    self.vy = 0.0;
                    self.state = State::Land(0.22);
                }
            }
            State::Land(t) => {
                self.state = if t - dt <= 0.0 { State::Idle } else { State::Land(t - dt) };
            }
            State::Walk { to } => {
                let speed = self.speed(busy);
                let d = to - self.x;
                self.facing = d.signum();
                if d.abs() <= speed * dt {
                    self.x = to;
                    self.state = State::Look;
                    self.think = 1.0 + self.rand() * 2.0;
                } else {
                    self.x += d.signum() * speed * dt;
                }
                if flying {
                    self.y += (floor - self.y) * (dt * 4.0).min(1.0);
                } else if self.y > floor + 1.0 {
                    self.vy = -360.0;
                    self.state = State::Air;
                } else if self.y < floor - 1.0 {
                    self.state = State::Air;
                }
            }
            State::Sleep => {
                if flying {
                    self.y += (floor - self.y) * (dt * 2.0).min(1.0);
                }
                if idle_seconds() < 1.5 {
                    self.state = if flying { State::Air } else { State::Idle };
                    self.think = 2.0;
                }
            }
            State::Idle | State::Look => {
                if flying {
                    self.y += (floor - self.y) * (dt * 4.0).min(1.0);
                } else if self.y < floor - 1.0 {
                    self.state = State::Air;
                } else {
                    self.y = floor;
                }
                if self.kind == Kind::Chameleon && self.tongue <= 0.0 {
                    self.maybe_tongue(dt);
                }
                self.think -= dt;
                if self.think <= 0.0 && matches!(self.state, State::Idle | State::Look) {
                    self.decide(&g, ctx.now.hour);
                }
            }
        }
        if self.special > 0.0 && self.kind == Kind::Hedgehog && self.sheet.is_some() && self.state != State::Drag {
            self.x += self.facing * 70.0 * sc * self.size * dt;
        }
        if self.state != State::Drag {
            self.x = self.x.clamp(g.min_x + 20.0 * sc, g.max_x - 20.0 * sc);
        }

        let dancing = ctx.media.playing && matches!(self.state, State::Idle | State::Look);
        let fast = !matches!(self.state, State::Idle | State::Look | State::Sleep)
            || dancing
            || self.hearts > 0.0
            || self.alert > 0.0
            || self.tongue > 0.0
            || self.roll > 0.0
            || self.special > 0.0
            || self.petting > 0.0
            || (flying && self.state != State::Sleep);
        self.set_interval(if fast {
            33
        } else if self.state == State::Sleep {
            250
        } else {
            100
        });
        self.draw(ctx);
    }

    fn maybe_tongue(&mut self, dt: f32) {
        let mut c = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut c);
        }
        let sc = self.sc();
        let (dx, dy) = ((c.x as f32 - self.x) / sc, (c.y as f32 - self.y) / sc);
        let near = dx.abs() < 110.0 * self.size && dy < 10.0 && dy > -90.0 * self.size;
        if near && self.rand() < dt * 0.8 {
            self.facing = if dx >= 0.0 { 1.0 } else { -1.0 };
            if self.sheet.is_some() {
                self.special = SPECIAL_SECS;
            } else {
                self.tongue = 0.5;
                self.tongue_to = (dx, dy);
            }
        }
    }

    fn draw(&self, ctx: &Ctx) {
        let s = &self.surf;
        let p = &ctx.pal;
        let k = self.size;
        let cx = W * k / 2.0;
        let base = BASE * k;
        let sleeping = self.state == State::Sleep;
        let walking = matches!(self.state, State::Walk { .. });
        let dancing = ctx.media.playing && matches!(self.state, State::Idle | State::Look);
        let hour = ctx.now.hour;

        let mut a = Anim {
            facing: self.facing,
            phase: self.t * if self.rush { 14.0 } else { 9.0 },
            walking,
            bob: 0.0,
            squash: 1.0 + (self.t * if sleeping { 1.6 } else { 2.4 }).sin() * 0.03,
            sleeping,
            eyes_closed: sleeping || (self.t % 3.7) < 0.13,
            look: 0.0,
            t: self.t,
            rush: self.rush && walking,
            puff: ctx.sys.cpu > 85.0,
            night: !(6..20).contains(&hour),
            tongue: None,
            roll: (self.roll > 0.0).then_some(self.roll_angle),
        };
        match self.state {
            State::Walk { .. } => a.bob = (a.phase).sin().abs() * 2.0 * k,
            State::Land(_) => a.squash = 0.8,
            State::Air => a.squash = if self.vy < 0.0 { 1.1 } else { 1.04 },
            State::Drag => a.squash = 1.1,
            State::Look => a.look = (self.t * 1.3).sin(),
            _ => {}
        }
        if dancing {
            a.bob = (self.t * 2.0 * PI * 2.0).sin().abs() * 3.0 * k;
            a.squash = 1.0 + (self.t * 4.0 * PI).sin() * 0.05;
        }
        if self.tongue > 0.0 {
            let e = (PI * (1.0 - self.tongue / 0.5)).sin();
            a.tongue = Some((e, self.tongue_to.0, self.tongue_to.1));
        }

        s.begin();
        let sprite_top = self.draw_sprite(s, cx, base, k, ctx.sys.cpu > 85.0);
        let drawn = sprite_top.is_some();
        let top = if let Some(t) = sprite_top {
            t
        } else {
            match self.kind {
            Kind::Chameleon => draw_chameleon(s, p, cx, base, k, &a),
            Kind::Firefly => draw_firefly(s, p, cx, base, k, &a),
            Kind::Fox | Kind::Puppy => draw_fox(s, cx, base, k, &a),
            Kind::Hedgehog => draw_hedgehog(s, cx, base, k, &a),
            }
        };

        let rainy = ctx.weather.is_some_and(|w| matches!(w.sky(), Sky::Rain | Sky::Drizzle | Sky::Storm));
        if rainy && !sleeping && a.roll.is_none() {
            umbrella(s, p, cx + 4.0 * k * self.facing, top - 4.0 * k, k);
        }
        if sleeping && !drawn {
            for i in 0..3 {
                let f = (self.t * 0.6 + i as f32 / 3.0) % 1.0;
                let sz = (8.0 + 6.0 * f) * k;
                let x = cx + (10.0 + f * 14.0) * k;
                let y = top - f * 22.0 * k;
                s.text("z", Rect::new(x, y - sz, sz * 1.5, sz * 1.5), sz, 700, p.text.alpha(1.0 - f), Align::Left);
            }
        } else if a.puff && self.kind != Kind::Hedgehog {
            let f = (self.t * 1.8) % 1.0;
            let (x, y) = (cx - 12.0 * k * self.facing, top + 6.0 * k + f * 10.0 * k);
            s.fill_poly(&[(x, y - 4.0 * k), (x - 2.2 * k, y), (x + 2.2 * k, y)], Color::rgba(120, 200, 255, 230));
            s.fill_circle(x, y + 0.5 * k, 2.3 * k, Color::rgba(120, 200, 255, 230));
        }
        if dancing {
            let f = (self.t * 0.8) % 1.0;
            let x = cx - (14.0 + f * 8.0) * k;
            let y = top - f * 18.0 * k;
            s.text("♪", Rect::new(x - 6.0 * k, y - 14.0 * k, 16.0 * k, 16.0 * k), 13.0 * k, 600, p.accent.alpha(1.0 - f), Align::Center);
        }
        if self.alert > 0.0 {
            let (bx, by) = (cx + 12.0 * k * self.facing, top - 12.0 * k + (self.t * 8.0).sin() * 1.5 * k);
            let a = self.alert.min(1.0);
            s.fill_circle(bx, by, 8.0 * k, Color::rgba(255, 255, 255, (a * 240.0) as u8));
            s.fill_poly(&[(bx - 3.0 * k, by + 6.0 * k), (bx - 6.0 * k * self.facing, by + 11.0 * k), (bx + 2.0 * k, by + 7.0 * k)], Color::rgba(255, 255, 255, (a * 240.0) as u8));
            s.text("!", Rect::new(bx - 8.0 * k, by - 8.0 * k, 16.0 * k, 16.0 * k), 12.0 * k, 800, Color { a, ..opaque(p.accent) }, Align::Center);
        }
        if self.hearts > 0.0 && !(drawn && self.petting > 0.0) {
            let f = 1.0 - self.hearts / 1.2;
            heart(s, cx, top - 6.0 * k - f * 16.0 * k, 5.0 * k, Color::rgba(255, 92, 138, ((1.0 - f) * 255.0) as u8));
        }
        s.end();

        let sc = self.sc();
        let wx = (self.x - cx * sc).round() as i32;
        let wy = (self.y - base * sc).round() as i32;
        s.present(self.hwnd, wx, wy);
    }

    fn draw_sprite(&self, s: &Surface, cx: f32, base: f32, k: f32, busy: bool) -> Option<f32> {
        let (bmp, sheet) = self.sheet.as_ref()?;
        let t = self.t;
        let (name, frame): (&str, Option<f32>) = match self.state {
            State::Drag => ("dangle", Some(t * 8.0)),
            _ if self.special > 0.0 => ("special", Some((1.0 - self.special / SPECIAL_SECS) * 0.999 * 6.0)),
            _ if self.petting > 0.0 => ("happy", Some(t * 6.0)),
            State::Sleep => ("sleep", Some(t * 1.2)),
            State::Air if self.flying() => ("walk", Some(t * 14.0)),
            State::Air => {
                let f = if self.vy < -200.0 { 1.0 } else if self.vy < 150.0 { 2.0 } else { 3.0 };
                ("jump", Some(f))
            }
            State::Land(_) => ("jump", Some(0.0)),
            State::Walk { .. } if self.rush || busy => ("run", Some(t * 13.0)),
            State::Walk { .. } => ("walk", Some(t * if self.kind == Kind::Chameleon { 7.0 } else { 11.0 })),
            _ if self.flying() => ("walk", Some(t * 9.0)),
            _ => match &sheet.stand {
                Some(st) => (st.anim.as_str(), Some(st.frame as f32)),
                None => ("idle", Some(0.0)),
            },
        };
        let anim = sheet.anims.get(name).or_else(|| sheet.anims.get("idle"))?;
        let n = anim.frames.max(1);
        let i = frame.map_or(0, |f| f.max(0.0) as u32 % n);
        let src = Rect::new(i as f32 * sheet.cell, anim.row as f32 * sheet.cell, sheet.cell, sheet.cell);
        let d = SPRITE_SIZE * k;
        let y = base - d * (1.0 - sheet.base / sheet.cell);
        s.sprite(bmp, src, Rect::new(cx - d / 2.0, y, d, d), self.facing < 0.0, 1.0);
        Some(y + d * 0.18)
    }

    pub fn handle(&mut self, msg: u32) -> Option<Option<Action>> {
        let mut c = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut c);
        }
        match msg {
            WM_LBUTTONDOWN => {
                unsafe {
                    SetCapture(self.hwnd);
                }
                self.drag = Some((c, c.x as f32 - self.x, c.y as f32 - self.y, false));
            }
            WM_MOUSEMOVE if self.drag.is_none() => {
                if let Some(l) = self.last_hover {
                    self.rub += ((c.x - l.x).abs() + (c.y - l.y).abs()) as f32 / self.sc();
                }
                self.last_hover = Some(c);
                if self.rub > 140.0 && self.state != State::Sleep {
                    self.rub = 0.0;
                    self.petting = 1.8;
                    self.hearts = 1.2;
                    if matches!(self.state, State::Walk { .. }) {
                        self.state = State::Idle;
                        self.think = 2.5;
                    }
                    self.set_interval(33);
                }
            }
            WM_MOUSEMOVE => {
                if let Some((start, ox, oy, moved)) = &mut self.drag {
                    if (c.x - start.x).abs() + (c.y - start.y).abs() > 4 {
                        *moved = true;
                    }
                    if *moved {
                        self.x = c.x as f32 - *ox;
                        self.y = c.y as f32 - *oy;
                        self.state = State::Drag;
                        self.set_interval(16);
                    }
                }
            }
            WM_LBUTTONUP => {
                unsafe {
                    let _ = ReleaseCapture();
                }
                if let Some((_, _, _, moved)) = self.drag.take() {
                    if !moved && self.sheet.is_some() {
                        self.special = SPECIAL_SECS;
                        self.hearts = 1.2;
                        if self.state == State::Sleep {
                            self.state = State::Idle;
                        }
                        self.set_interval(33);
                        return Some(None);
                    }
                    if !moved {
                        self.hearts = 1.2;
                        if self.kind == Kind::Hedgehog {
                            self.roll = 0.9;
                        }
                        if self.flying() {
                            self.y -= 24.0 * self.sc() * self.size;
                        }
                    }
                    self.vy = if moved { 0.0 } else { -430.0 };
                    self.state = State::Air;
                    self.set_interval(33);
                }
            }
            WM_RBUTTONUP => return Some(Some(Action::PetMenu)),
            _ => return None,
        }
        Some(None)
    }
}

impl Drop for Pet {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn idle_seconds() -> f32 {
    unsafe {
        let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if !GetLastInputInfo(&mut li).as_bool() {
            return 0.0;
        }
        GetTickCount().wrapping_sub(li.dwTime) as f32 / 1000.0
    }
}

struct Anim {
    facing: f32,
    phase: f32,
    walking: bool,
    bob: f32,
    squash: f32,
    sleeping: bool,
    eyes_closed: bool,
    look: f32,
    t: f32,
    rush: bool,
    puff: bool,
    night: bool,
    tongue: Option<(f32, f32, f32)>,
    roll: Option<f32>,
}

const INK: Color = Color::rgba(28, 26, 34, 255);
const WHITE: Color = Color::rgba(255, 255, 255, 255);

fn shade(c: Color, k: f32) -> Color {
    Color { r: c.r * k, g: c.g * k, b: c.b * k, a: c.a }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: a.a + (b.a - a.a) * t }
}

fn opaque(c: Color) -> Color {
    Color { a: 1.0, ..c }
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32, rot: f32) -> Vec<(f32, f32)> {
    let (sr, cr) = rot.sin_cos();
    (0..20)
        .map(|i| {
            let a = i as f32 / 20.0 * 2.0 * PI;
            let (x, y) = (a.cos() * rx, a.sin() * ry);
            (cx + x * cr - y * sr, cy + x * sr + y * cr)
        })
        .collect()
}

fn draw_chameleon(s: &Surface, p: &Palette, cx: f32, base: f32, k: f32, a: &Anim) -> f32 {
    let c = opaque(p.accent);
    let dark = shade(c, 0.55);
    let light = mix(c, WHITE, 0.35);
    let f = a.facing;
    let lift = a.bob * 0.6;
    let by = base - 9.0 * k - lift;

    let (sx, sy) = (cx - f * 21.0 * k, by - 2.0 * k);
    let mut tail = vec![(cx - f * 12.0 * k, by - 4.0 * k)];
    for i in 0..=22 {
        let q = i as f32 / 22.0;
        let ang = q * 2.0 * PI * 1.25;
        let r = 6.0 * k * (1.0 - q * 0.8);
        tail.push((sx - f * r * ang.sin(), sy - r * ang.cos()));
    }
    s.polyline(&tail, 3.4 * k, dark);
    s.polyline(&tail, 1.8 * k, c);

    for (dx, ph) in [(-9.0, 0.0), (-4.0, PI), (6.0, PI), (11.0, 0.0)] {
        let sw = if a.walking { (a.phase * 0.6 + ph).sin() * 2.5 * k } else { 0.0 };
        let lx = cx + f * dx * k;
        s.polyline(&[(lx, by), (lx + f * 1.5 * k + sw, base - 4.0 * k), (lx + f * 3.0 * k + sw, base - 0.5)], 2.4 * k, dark);
    }

    let bh = 11.0 * k * a.squash;
    let body = Rect::new(cx - 14.0 * k, by - bh, 28.0 * k, bh);
    s.fill_round(body, bh / 2.0, c);
    s.fill_round(Rect::new(body.x + 3.0 * k, body.y + bh * 0.55, body.w - 6.0 * k, bh * 0.45), bh * 0.22, light);
    let ridge: Vec<(f32, f32)> = (0..9)
        .map(|i| {
            let x = cx + f * (-11.0 + i as f32 * 2.6) * k;
            (x, body.y + if i % 2 == 0 { 0.6 * k } else { -1.6 * k })
        })
        .collect();
    s.polyline(&ridge, 1.3 * k, dark);
    for (dx, dy) in [(-6.0, 3.5), (0.0, 2.5), (5.0, 3.8)] {
        s.fill_circle(cx + f * dx * k, body.y + dy * k, 1.3 * k, dark.alpha(0.55));
    }

    let hx = cx + f * 14.0 * k;
    let hy = by - bh * 0.55;
    s.fill_poly(&[(hx - f * 3.0 * k, hy - 6.0 * k), (hx + f * 11.0 * k, hy + 0.5 * k), (hx - f * 3.0 * k, hy + 5.0 * k)], c);
    s.fill_circle(hx, hy, 5.6 * k, c);
    s.fill_poly(&[(hx - f * 4.0 * k, hy - 4.0 * k), (hx - f * 0.5 * k, hy - 9.5 * k), (hx + f * 3.0 * k, hy - 4.5 * k)], dark);
    s.line(hx + f * 3.0 * k, hy + 2.6 * k, hx + f * 10.0 * k, hy + 0.9 * k, 0.9 * k, dark);

    let eye = |ex: f32, ey: f32, r: f32, ang: f32| {
        s.fill_circle(ex, ey, r, light);
        s.stroke_round(Rect::new(ex - r, ey - r, 2.0 * r, 2.0 * r), r, 0.9 * k, dark);
        if a.eyes_closed {
            s.line(ex - r * 0.7, ey, ex + r * 0.7, ey, 1.2 * k, dark);
        } else {
            s.fill_circle(ex + ang.cos() * r * 0.45, ey + ang.sin() * r * 0.45, r * 0.38, INK);
        }
    };
    let ang1 = (a.t * 0.9).sin() * 2.6 + if f < 0.0 { PI } else { 0.0 };
    let ang2 = (a.t * 0.7 + 1.3).cos() * 2.6 + if f < 0.0 { PI } else { 0.0 };
    eye(hx - f * 1.5 * k, hy - 4.0 * k, 2.6 * k, ang2);
    eye(hx + f * 1.5 * k, hy - 1.5 * k, 3.6 * k, ang1);

    if let Some((e, tx, ty)) = a.tongue {
        let (mx, my) = (hx + f * 10.0 * k, hy + 1.0 * k);
        let (dx, dy) = (cx + tx - mx, base + ty - my);
        let d = (dx * dx + dy * dy).sqrt().max(1.0);
        let len = d.min(55.0 * k) * e;
        let (ex, ey) = (mx + dx / d * len, my + dy / d * len);
        s.line(mx, my, ex, ey, 1.6 * k, Color::rgba(240, 110, 140, 255));
        s.fill_circle(ex, ey, 2.4 * k, Color::rgba(240, 110, 140, 255));
    }
    hy - 10.0 * k
}

fn draw_firefly(s: &Surface, p: &Palette, cx: f32, base: f32, k: f32, a: &Anim) -> f32 {
    let g = opaque(p.accent);
    let cy = if a.sleeping { base - 8.0 * k } else { base - 30.0 * k };
    let f = a.facing;
    let pulse = (a.t * 2.6).sin() * 0.5 + 0.5;
    let glow = if a.sleeping {
        0.12 + 0.12 * (a.t * 0.8).sin().abs()
    } else {
        0.35 + 0.3 * pulse + if a.night { 0.25 } else { 0.0 }
    };
    let (gx, gy) = (cx - f * 5.0 * k, cy + 2.0 * k);
    let gr = (30.0 * k).min(H * k - gy - 1.0).min(gx).max(4.0 * k);
    s.fill_round_radial(Rect::new(gx - gr, gy - gr, 2.0 * gr, 2.0 * gr), gr, gx, gy, gr, gr, g.alpha(glow.min(1.0)));

    let flap = if a.sleeping { 0.15 } else { (a.t * 32.0).sin() };
    for (dx, rot, alpha) in [(-1.5, -0.9, 0.35), (1.0, -0.5, 0.5)] {
        let ry = (1.5 + 4.5 * flap.abs()) * k;
        let pts = ellipse(cx + f * dx * k, cy - 6.0 * k, 7.5 * k, ry, rot * f);
        s.fill_poly(&pts, WHITE.alpha(alpha));
    }

    let ab = Rect::new(cx - f * 5.0 * k - 5.0 * k, cy - 2.5 * k, 10.0 * k, 7.0 * k);
    s.fill_round(ab, 3.5 * k, mix(g, WHITE, 0.25 + 0.5 * glow.min(1.0) * 0.6));
    s.fill_round(Rect::new(cx - 3.5 * k, cy - 4.5 * k, 8.0 * k, 7.0 * k), 3.0 * k, Color::rgba(52, 46, 60, 255));
    let (hx, hy) = (cx + f * 6.0 * k, cy - 2.0 * k);
    s.fill_circle(hx, hy, 3.4 * k, Color::rgba(40, 36, 48, 255));
    if !a.eyes_closed {
        s.fill_circle(hx + f * 1.3 * k, hy - 0.8 * k, 1.1 * k, WHITE);
    }
    for dy in [-1.0, 1.0] {
        let wig = (a.t * 3.0 + dy).sin() * 1.2 * k;
        s.polyline(
            &[(hx + f * 1.5 * k, hy - 2.5 * k), (hx + f * 4.0 * k, hy - 6.5 * k + wig + dy * 0.8 * k), (hx + f * 6.5 * k, hy - 7.0 * k + wig)],
            0.9 * k,
            Color::rgba(40, 36, 48, 255),
        );
    }
    for dx in [-2.0, 0.5, 3.0] {
        let x = cx + f * dx * k;
        s.line(x, cy + 2.0 * k, x - f * 0.8 * k, cy + 5.0 * k, 0.8 * k, Color::rgba(40, 36, 48, 255));
    }
    cy - 14.0 * k
}

fn draw_fox(s: &Surface, cx: f32, base: f32, k: f32, a: &Anim) -> f32 {
    let fur = Color::rgba(232, 128, 58, 255);
    let dark = Color::rgba(150, 70, 30, 255);
    let cream = Color::rgba(255, 244, 234, 255);
    let sock = Color::rgba(70, 40, 30, 255);
    let f = a.facing;

    if a.sleeping {
        let breath = a.squash;
        s.fill_round(Rect::new(cx - 15.0 * k, base - 14.0 * k * breath, 30.0 * k, 14.0 * k * breath), 7.0 * k, fur);
        s.fill_round(Rect::new(cx - 16.0 * k, base - 8.0 * k, 31.0 * k, 8.0 * k), 4.0 * k, fur);
        s.fill_circle(cx + f * 13.0 * k, base - 4.5 * k, 4.2 * k, cream);
        let (hx, hy) = (cx + f * 7.0 * k, base - 10.0 * k);
        for side in [-1.0, 1.0] {
            let ex = hx + side * 3.5 * k;
            s.fill_poly(&[(ex - 2.5 * k, hy - 3.0 * k), (ex + side * 0.5 * k, hy - 9.0 * k), (ex + 2.5 * k, hy - 3.5 * k)], fur);
        }
        s.fill_circle(hx, hy, 5.5 * k, fur);
        s.fill_poly(&[(hx, hy + 1.0 * k), (hx + f * 7.0 * k, hy + 2.5 * k), (hx, hy + 4.5 * k)], cream);
        s.line(hx + f * 0.5 * k, hy - 1.0 * k, hx + f * 3.0 * k, hy - 0.5 * k, 1.1 * k, INK);
        return hy - 9.0 * k;
    }

    let lift = a.bob;
    let by = base - 9.0 * k - lift;
    let wave = (a.t * if a.walking { 6.0 } else { 2.0 }).sin();
    let n = 9;
    for i in 0..n {
        let q = i as f32 / (n - 1) as f32;
        let x = cx - f * (12.0 + q * 17.0) * k;
        let y = by - 4.0 * k - q * 9.0 * k - wave * q * 3.0 * k;
        let r = (3.2 + 4.0 * (q * PI).sin()) * k;
        s.fill_circle(x, y, r, if q > 0.78 { cream } else { fur });
    }
    for (dx, ph) in [(-9.0, 0.0), (-4.0, PI), (6.0, PI), (11.0, 0.0)] {
        let sw = if a.walking { (a.phase + ph).sin() * 3.0 * k } else { 0.0 };
        let lx = cx + f * dx * k + sw;
        s.line(lx, by - 1.0 * k, lx, base - 1.0, 2.8 * k, sock);
    }
    let bh = 11.0 * k * a.squash;
    s.fill_round(Rect::new(cx - 14.0 * k, by - bh, 28.0 * k, bh), bh / 2.0, fur);
    s.fill_circle(cx + f * 10.0 * k, by - bh * 0.4, 4.3 * k, cream);
    let (hx, hy) = (cx + f * 15.0 * k, by - bh - 2.0 * k);
    let ear = if a.rush { 11.5 } else { 9.0 };
    for side in [-1.0, 1.0] {
        let ex = hx + side * 3.8 * k - f * 1.0 * k;
        s.fill_poly(&[(ex - 3.0 * k, hy - 3.0 * k), (ex + side * 0.6 * k, hy - ear * k), (ex + 3.0 * k, hy - 3.5 * k)], fur);
        s.fill_poly(&[(ex - 1.3 * k, hy - 4.0 * k), (ex + side * 0.4 * k, hy - (ear - 3.0) * k), (ex + 1.3 * k, hy - 4.2 * k)], dark);
    }
    s.fill_circle(hx, hy, 6.5 * k, fur);
    s.fill_poly(&[(hx - f * 1.0 * k, hy - 2.0 * k), (hx + f * 11.0 * k, hy + 2.0 * k), (hx - f * 1.0 * k, hy + 5.0 * k)], fur);
    s.fill_poly(&[(hx - f * 3.0 * k, hy + 1.5 * k), (hx + f * 10.0 * k, hy + 2.6 * k), (hx - f * 1.0 * k, hy + 6.0 * k)], cream);
    s.fill_circle(hx + f * 10.5 * k, hy + 2.0 * k, 1.4 * k, INK);
    if a.eyes_closed {
        s.line(hx + f * 1.5 * k, hy - 1.0 * k, hx + f * 4.0 * k, hy - 0.6 * k, 1.1 * k, INK);
    } else {
        s.fill_circle(hx + f * 2.8 * k, hy - 1.0 * k, 1.5 * k, INK);
        s.fill_circle(hx + f * 3.2 * k, hy - 1.5 * k, 0.5 * k, WHITE);
    }
    hy - ear * k
}

fn draw_hedgehog(s: &Surface, cx: f32, base: f32, k: f32, a: &Anim) -> f32 {
    let quill = Color::rgba(110, 78, 52, 255);
    let tip = Color::rgba(232, 214, 190, 255);
    let face = Color::rgba(236, 206, 166, 255);
    let f = a.facing;
    let spike = if a.puff { 8.5 } else { 5.0 } * k;

    if let Some(ang) = a.roll {
        let (bx, by, r) = (cx, base - 11.0 * k - a.bob, 10.0 * k);
        for i in 0..16 {
            let t = ang + i as f32 / 16.0 * 2.0 * PI;
            let (b1, b2) = (t - 0.2, t + 0.2);
            s.fill_poly(
                &[(bx + b1.cos() * r, by + b1.sin() * r), (bx + t.cos() * (r + spike), by + t.sin() * (r + spike)), (bx + b2.cos() * r, by + b2.sin() * r)],
                tip,
            );
        }
        s.fill_circle(bx, by, r, quill);
        return by - r - spike;
    }

    let lift = a.bob;
    let (bx, by) = (cx - f * 2.0 * k, base - 7.0 * k - lift);
    let r = 11.0 * k * a.squash;
    for (dx, ph) in [(-5.0, 0.0), (6.0, PI)] {
        let sw = if a.walking { (a.phase + ph).sin() * 2.0 * k } else { 0.0 };
        s.fill_round(Rect::new(cx + f * dx * k + sw - 2.5 * k, base - 3.5 * k, 5.0 * k, 3.5 * k), 1.7 * k, Color::rgba(80, 56, 40, 255));
    }
    for i in 0..11 {
        let q = i as f32 / 10.0;
        let t = PI * (1.05 + q * 0.95);
        let t = if f > 0.0 { t } else { PI - t };
        let (b1, b2) = (t - 0.16, t + 0.16);
        let len = r + spike * (0.8 + 0.2 * (i % 2) as f32);
        s.fill_poly(&[(bx + b1.cos() * r, by + b1.sin() * r), (bx + t.cos() * len, by + t.sin() * len), (bx + b2.cos() * r, by + b2.sin() * r)], tip);
    }
    s.fill_round(Rect::new(bx - r, by - r, 2.0 * r, r + 6.0 * k), r * 0.9, quill);
    let (fx, fy) = (cx + f * 8.0 * k, base - 7.5 * k - lift);
    s.fill_circle(fx, fy, 5.5 * k, face);
    s.fill_poly(&[(fx, fy - 4.0 * k), (fx + f * 10.0 * k, fy + 1.0 * k), (fx, fy + 4.5 * k)], face);
    s.fill_circle(fx + f * 10.0 * k, fy + 0.8 * k, 1.6 * k, INK);
    s.fill_circle(fx - f * 1.5 * k, fy - 4.5 * k, 1.8 * k, Color::rgba(200, 160, 120, 255));
    if a.eyes_closed {
        s.line(fx + f * 1.5 * k, fy - 1.8 * k, fx + f * 4.0 * k, fy - 1.5 * k, 1.1 * k, INK);
    } else {
        s.fill_circle(fx + f * 3.0 * k, fy - 1.8 * k, 1.3 * k, INK);
    }
    by - r - spike
}

fn umbrella(s: &Surface, p: &Palette, x: f32, y: f32, k: f32) {
    let r = 18.0 * k;
    let c = opaque(shade(p.accent, 0.8));
    let arc: Vec<(f32, f32)> = (0..=16)
        .map(|i| {
            let a = PI * (1.0 + i as f32 / 16.0);
            (x + a.cos() * r, y + a.sin() * r * 0.7)
        })
        .collect();
    s.fill_poly(&arc, c);
    s.line(x, y, x, y + 16.0 * k, 1.4 * k, Color::rgba(60, 60, 70, 255));
    s.polyline(&[(x, y + 16.0 * k), (x - 2.0 * k, y + 19.0 * k), (x - 4.0 * k, y + 17.0 * k)], 1.4 * k, Color::rgba(60, 60, 70, 255));
}

fn heart(s: &Surface, x: f32, y: f32, r: f32, c: Color) {
    s.fill_circle(x - r * 0.5, y, r * 0.6, c);
    s.fill_circle(x + r * 0.5, y, r * 0.6, c);
    s.fill_poly(&[(x - r * 1.08, y + r * 0.15), (x + r * 1.08, y + r * 0.15), (x, y + r * 1.3)], c);
}
