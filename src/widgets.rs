use windows::core::Result;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::{create_layered, Action, Ctx};
use crate::config::WidgetCfg;
use crate::gfx::{Align, Color, Rect, Surface};
use crate::media::MediaCmd;
use crate::monitors::Monitor;
use crate::skin;
use crate::sysinfo::{fmt_speed, HISTORY};
use crate::i18n::{self, tr};
use crate::util::days_in_month;
use crate::weather::Sky;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Clock,
    Weather,
    Media,
    Sysmon,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Btn {
    Prev,
    Toggle,
    Next,
}

struct Drag {
    cursor: POINT,
    x: i32,
    y: i32,
    moved: bool,
}

pub struct Widget {
    pub hwnd: HWND,
    pub index: usize,
    surf: Surface,
    kind: Kind,
    cfg: WidgetCfg,
    pub monitor: usize,
    x: i32,
    y: i32,
    w: f32,
    h: f32,
    sig: String,
    buttons: Vec<(Rect, Btn)>,
    hover: Option<Btn>,
    drag: Option<Drag>,
    art: Option<(String, Option<ID2D1Bitmap>)>,
}

pub const DESKTOP_LAYER: isize = 1;

impl Widget {
    pub fn new(index: usize, cfg: &WidgetCfg, ctx: &Ctx, mons: &[Monitor]) -> Result<Option<Widget>> {
        if !cfg.enabled || mons.is_empty() {
            return Ok(None);
        }
        let monitor = if (1..=mons.len()).contains(&cfg.monitor) { cfg.monitor - 1 } else { 0 };
        let mon = mons[monitor].rect;
        let scale = mons[monitor].scale;
        let kind = match cfg.kind.as_str() {
            "clock" => Kind::Clock,
            "weather" => Kind::Weather,
            "media" => Kind::Media,
            "sysmon" => Kind::Sysmon,
            _ => return Ok(None),
        };
        let (w, h) = match kind {
            Kind::Clock => (280.0, if cfg.calendar { 340.0 } else { 116.0 }),
            Kind::Weather => (280.0, 150.0),
            Kind::Media => (340.0, 120.0),
            Kind::Sysmon => (280.0, 236.0),
        };
        let top = cfg.layer == "top";
        let ex = WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | if top { WS_EX_TOPMOST } else { WINDOW_EX_STYLE(0) };
        let hwnd = create_layered(ex)?;
        let surf = Surface::new(w, h, scale, &ctx.pal.font)?;
        let px = |v: f32, lo: i32, hi: i32| if v < 0.0 { hi + (v * scale) as i32 } else { lo + (v * scale) as i32 };
        let x = px(cfg.x, mon.left, mon.right);
        let y = px(cfg.y, mon.top, mon.bottom);
        unsafe {
            if !top {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, DESKTOP_LAYER);
            }
            let z = if top { HWND_TOPMOST } else { HWND_BOTTOM };
            let _ = SetWindowPos(hwnd, z, x, y, surf.w_px, surf.h_px, SWP_NOACTIVATE);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(Some(Widget {
            hwnd,
            index,
            surf,
            kind,
            cfg: cfg.clone(),
            monitor,
            x,
            y,
            w,
            h,
            sig: String::new(),
            buttons: Vec::new(),
            hover: None,
            drag: None,
            art: None,
        }))
    }

    pub fn pos(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn scale(&self) -> f32 {
        self.surf.scale
    }

    fn signature(&self, ctx: &Ctx) -> String {
        let t = &ctx.now;
        match self.kind {
            Kind::Clock => format!("{}:{}:{}.{}", t.hour, t.min, if self.cfg.seconds { t.sec } else { 0 }, t.day),
            Kind::Weather => format!("{:?}", ctx.weather),
            Kind::Media => format!(
                "{}{}{}{}{:?}",
                ctx.media.title,
                ctx.media.artist,
                ctx.media.playing,
                ctx.media.thumb.is_some(),
                self.hover
            ),
            Kind::Sysmon => format!(
                "{:.0}{:.0}{:.0}{:.0}{:.0}{}",
                ctx.sys.cpu,
                ctx.sys.ram,
                ctx.sys.disk,
                ctx.sys.net_down / 1024.0,
                ctx.sys.net_up / 1024.0,
                ctx.sys.cpu_hist.len()
            ),
        }
    }

    pub fn paint(&mut self, ctx: &Ctx, force: bool) {
        let sig = self.signature(ctx);
        if !force && sig == self.sig {
            return;
        }
        self.sig = sig;

        if self.kind == Kind::Media {
            let key = ctx.media.track_key();
            if self.art.as_ref().map(|a| &a.0) != Some(&key) || (ctx.media.thumb.is_some() && self.art.as_ref().is_some_and(|a| a.1.is_none())) {
                let bmp = ctx.media.thumb.as_ref().and_then(|b| self.surf.bitmap_from_bytes(b, 160));
                self.art = Some((key, bmp));
            }
        }

        let s = &self.surf;
        let full = Rect::new(0.0, 0.0, self.w, self.h);
        let radius = ctx.pal.radius.min(22.0);
        s.begin();
        skin::panel(s, full.inset(0.5), radius, &ctx.pal, false);
        self.buttons = match self.kind {
            Kind::Clock => {
                draw_clock(s, ctx, &self.cfg, self.w);
                vec![]
            }
            Kind::Weather => {
                draw_weather(s, ctx, self.w, self.h);
                vec![]
            }
            Kind::Media => draw_media(s, ctx, self.w, self.hover, self.art.as_ref().and_then(|a| a.1.as_ref())),
            Kind::Sysmon => {
                draw_sysmon(s, ctx, self.w);
                vec![]
            }
        };
        s.end();
        s.present(self.hwnd, self.x, self.y);
    }

    fn button_at(&self, lp: LPARAM) -> Option<Btn> {
        let x = (lp.0 & 0xFFFF) as i16 as f32 / self.surf.scale;
        let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32 / self.surf.scale;
        self.buttons.iter().find(|(r, _)| r.contains(x, y)).map(|(_, b)| *b)
    }

    pub fn handle(&mut self, msg: u32, lp: LPARAM, locked: bool) -> Option<(Option<Action>, bool)> {
        match msg {
            WM_LBUTTONDOWN => {
                if self.button_at(lp).is_none() && !locked {
                    let mut cursor = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut cursor);
                        SetCapture(self.hwnd);
                    }
                    self.drag = Some(Drag { cursor, x: self.x, y: self.y, moved: false });
                }
                Some((None, false))
            }
            WM_MOUSEMOVE => {
                if let Some(d) = &mut self.drag {
                    let mut c = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut c);
                    }
                    let (nx, ny) = (d.x + c.x - d.cursor.x, d.y + c.y - d.cursor.y);
                    if (nx - self.x).abs() + (ny - self.y).abs() > 0 {
                        d.moved = true;
                        self.x = nx;
                        self.y = ny;
                        unsafe {
                            let _ = SetWindowPos(self.hwnd, None, nx, ny, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                        }
                    }
                    return Some((None, false));
                }
                let b = self.button_at(lp);
                let repaint = b != self.hover;
                self.hover = b;
                Some((None, repaint))
            }
            WM_LBUTTONUP => {
                if let Some(d) = self.drag.take() {
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    if d.moved {
                        return Some((Some(Action::SavePos(self.index)), false));
                    }
                    return Some((None, false));
                }
                let act = self.button_at(lp).map(|b| {
                    Action::Media(match b {
                        Btn::Prev => MediaCmd::Prev,
                        Btn::Toggle => MediaCmd::Toggle,
                        Btn::Next => MediaCmd::Next,
                    })
                });
                Some((act, false))
            }
            WM_RBUTTONUP => Some((Some(Action::Menu { widget: Some(self.index) }), false)),
            _ => None,
        }
    }
}

impl Drop for Widget {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn draw_clock(s: &Surface, ctx: &Ctx, cfg: &WidgetCfg, w: f32) {
    let p = &ctx.pal;
    let t = &ctx.now;
    let time = format!("{:02}:{:02}", t.hour, t.min);
    s.text(&time, Rect::new(20.0, 10.0, 220.0, 66.0), 56.0, 300, p.text, Align::Left);
    if cfg.seconds {
        let tw = s.measure(&time, 56.0, 300);
        s.text(&format!("{:02}", t.sec), Rect::new(26.0 + tw, 34.0, 48.0, 30.0), 20.0, 400, p.accent, Align::Left);
    }
    let date = format!("{} {}, {}", t.day, i18n::month(t.month), i18n::day(t.dow));
    s.text(&date, Rect::new(22.0, 74.0, w - 44.0, 24.0), 15.0, 400, p.dim, Align::Left);

    if !cfg.calendar {
        return;
    }
    s.line(20.0, 112.0, w - 20.0, 112.0, 1.0, p.dim.alpha(0.25));
    s.text(&format!("{} {}", i18n::month(t.month), t.year), Rect::new(20.0, 120.0, w - 40.0, 24.0), 14.0, 600, p.text, Align::Left);

    let cell = (w - 40.0) / 7.0;
    for (i, d) in i18n::days_short().iter().enumerate() {
        let c = if i >= 5 { p.accent.alpha(0.8) } else { p.dim };
        s.text(d, Rect::new(20.0 + i as f32 * cell, 148.0, cell, 20.0), 11.5, 600, c, Align::Center);
    }
    let first_sun = (t.dow as i32 - (t.day as i32 - 1)).rem_euclid(7);
    let first = (first_sun + 6) % 7;
    let days = days_in_month(t.year, t.month) as i32;
    for d in 1..=days {
        let idx = first + d - 1;
        let (col, row) = (idx % 7, idx / 7);
        let cx = 20.0 + col as f32 * cell;
        let cy = 172.0 + row as f32 * 27.0;
        let r = Rect::new(cx, cy, cell, 25.0);
        let today = d == t.day as i32;
        if today {
            s.fill_circle(cx + cell / 2.0, cy + 12.5, 12.5, p.accent);
        }
        let c = if today {
            p.on_accent
        } else if col >= 5 {
            p.text.alpha(0.6)
        } else {
            p.text
        };
        s.text(&d.to_string(), r, 12.5, if today { 700 } else { 400 }, c, Align::Center);
    }
}

fn draw_weather(s: &Surface, ctx: &Ctx, w: f32, h: f32) {
    let p = &ctx.pal;
    s.text(&ctx.cfg.weather.city, Rect::new(20.0, 12.0, w - 110.0, 22.0), 13.0, 600, p.dim, Align::Left);
    let Some(wt) = ctx.weather else {
        s.text(tr("Hava durumu yükleniyor…"), Rect::new(20.0, 40.0, w - 40.0, h - 60.0), 14.0, 400, p.dim, Align::Left);
        return;
    };
    s.text(&format!("{:.0}°", wt.temp), Rect::new(18.0, 32.0, 160.0, 60.0), 50.0, 300, p.text, Align::Left);
    s.text(wt.description(), Rect::new(20.0, 92.0, w - 110.0, 22.0), 14.0, 500, p.text, Align::Left);
    let mut extra = if i18n::en() { format!("Wind {:.0} km/h", wt.wind) } else { format!("Rüzgar {:.0} km/s", wt.wind) };
    if wt.tmax.is_finite() && wt.tmin.is_finite() {
        extra = format!("Y {:.0}°  D {:.0}°  ·  {}", wt.tmax, wt.tmin, extra);
    }
    s.text(&extra, Rect::new(20.0, 116.0, w - 40.0, 20.0), 12.0, 400, p.dim, Align::Left);
    draw_sky(s, wt.sky(), wt.is_day, w - 64.0, 62.0, 84.0);
}

fn draw_media(s: &Surface, ctx: &Ctx, w: f32, hover: Option<Btn>, art: Option<&ID2D1Bitmap>) -> Vec<(Rect, Btn)> {
    let p = &ctx.pal;
    let m = ctx.media;
    let ar = Rect::new(14.0, 14.0, 92.0, 92.0);
    match art {
        Some(b) => s.image(b, ar, 10.0),
        None => {
            s.fill_round(ar, 10.0, p.accent.alpha(0.18));
            s.glyph('\u{E8D6}', ar, 34.0, p.accent);
        }
    }
    let tx = 120.0;
    if !m.active {
        s.text(tr("Çalan bir şey yok"), Rect::new(tx, 30.0, w - tx - 14.0, 24.0), 15.0, 600, p.dim, Align::Left);
        s.text(tr("Spotify, YouTube vb. başlatın"), Rect::new(tx, 54.0, w - tx - 14.0, 20.0), 12.0, 400, p.dim, Align::Left);
        return vec![];
    }
    s.text(&m.title, Rect::new(tx, 16.0, w - tx - 14.0, 26.0), 16.0, 600, p.text, Align::Left);
    s.text(&m.artist, Rect::new(tx, 41.0, w - tx - 14.0, 20.0), 13.0, 400, p.dim, Align::Left);

    let mut out = Vec::new();
    let white = p.text;
    let btns = [
        (Btn::Prev, '\u{E892}'),
        (Btn::Toggle, if m.playing { '\u{E769}' } else { '\u{E768}' }),
        (Btn::Next, '\u{E893}'),
    ];
    for (i, (b, g)) in btns.iter().enumerate() {
        let r = Rect::new(tx - 4.0 + i as f32 * 42.0, 68.0, 36.0, 36.0);
        let (cx, cy) = (r.x + 18.0, r.y + 18.0);
        if *b == Btn::Toggle {
            let bg = if hover == Some(*b) { p.accent.alpha(0.8) } else { p.accent };
            s.fill_circle(cx, cy, 17.0, bg);
            s.glyph(*g, r, 14.0, p.on_accent);
        } else {
            if hover == Some(*b) {
                s.fill_circle(cx, cy, 17.0, white.alpha(0.1));
            }
            s.glyph(*g, r, 14.0, p.text);
        }
        out.push((r, *b));
    }
    out
}

fn draw_sysmon(s: &Surface, ctx: &Ctx, w: f32) {
    let p = &ctx.pal;
    let sys = ctx.sys;
    let warn = Color::rgba(255, 107, 107, 255);
    s.text(tr("Sistem"), Rect::new(20.0, 10.0, w - 40.0, 24.0), 13.0, 600, p.dim, Align::Left);

    let rows = [
        (tr("İşlemci"), format!("{:.0}%", sys.cpu), sys.cpu),
        (tr("Bellek"), format!("{:.1} / {:.1} GB", sys.ram_used_gb, sys.ram_total_gb), sys.ram),
        ("Disk C:", if i18n::en() { format!("{:.0} GB free", sys.disk_free_gb) } else { format!("{:.0} GB boş", sys.disk_free_gb) }, sys.disk),
    ];
    for (i, (label, value, pct)) in rows.iter().enumerate() {
        let y = 38.0 + i as f32 * 40.0;
        s.text(label, Rect::new(20.0, y, 120.0, 20.0), 13.0, 400, p.text, Align::Left);
        s.text(value, Rect::new(110.0, y, w - 130.0, 20.0), 13.0, 600, p.text, Align::Right);
        let track = Rect::new(20.0, y + 23.0, w - 40.0, 6.0);
        s.fill_round(track, 3.0, p.text.alpha(0.08));
        let c = if *pct > 85.0 { warn } else { p.accent };
        s.fill_round(Rect::new(track.x, track.y, (track.w * pct / 100.0).max(6.0), 6.0), 3.0, c);
    }

    s.text(
        &format!("↓ {}    ↑ {}", fmt_speed(sys.net_down), fmt_speed(sys.net_up)),
        Rect::new(20.0, 158.0, w - 40.0, 20.0),
        12.0,
        400,
        p.dim,
        Align::Left,
    );

    let g = Rect::new(20.0, 184.0, w - 40.0, 36.0);
    s.fill_round(g.inset(-2.0), 6.0, p.text.alpha(0.03));
    let pts = |hist: &std::collections::VecDeque<f32>| -> Vec<(f32, f32)> {
        let off = HISTORY - hist.len();
        hist.iter()
            .enumerate()
            .map(|(i, v)| (g.x + (off + i) as f32 * g.w / (HISTORY - 1) as f32, g.y + g.h - g.h * v / 100.0))
            .collect()
    };
    let cpu = pts(&sys.cpu_hist);
    if cpu.len() > 1 {
        let mut area = cpu.clone();
        area.push((cpu.last().unwrap().0, g.y + g.h));
        area.push((cpu[0].0, g.y + g.h));
        s.fill_poly(&area, p.accent.alpha(0.18));
        s.polyline(&cpu, 1.5, p.accent);
    }
    let ram = pts(&sys.ram_hist);
    if ram.len() > 1 {
        s.polyline(&ram, 1.5, Color::rgba(180, 140, 255, 220));
    }
}

pub fn draw_sky(s: &Surface, sky: Sky, is_day: bool, cx: f32, cy: f32, size: f32) {
    let u = size / 100.0;
    let at = |x: f32, y: f32| (cx + (x - 50.0) * u, cy + (y - 50.0) * u);
    let sun_c = Color::rgba(255, 200, 61, 255);
    let moon_c = Color::rgba(232, 230, 217, 255);
    let cloud_c = Color::rgba(230, 233, 239, 255);

    let sun = |x: f32, y: f32, r: f32| {
        let (sx, sy) = at(x, y);
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::FRAC_PI_4;
            let (c, sn) = (a.cos(), a.sin());
            s.line(sx + c * r * 1.4 * u, sy + sn * r * 1.4 * u, sx + c * r * 1.8 * u, sy + sn * r * 1.8 * u, 3.0 * u.max(0.4), sun_c);
        }
        s.fill_circle(sx, sy, r * u, sun_c);
    };
    let moon = |x: f32, y: f32, r: f32| {
        let (mx, my) = at(x, y);
        s.crescent(mx, my, r * u, moon_c);
    };
    let cloud = |x: f32, y: f32, k: f32, c: Color| {
        let p = |dx: f32, dy: f32| at(x + dx * k, y + dy * k);
        let (ax, ay) = p(-17.0, 2.0);
        s.fill_circle(ax, ay, 13.0 * k * u, c);
        let (bx, by) = p(1.0, -6.0);
        s.fill_circle(bx, by, 19.0 * k * u, c);
        let (dx, dy) = p(18.0, 3.0);
        s.fill_circle(dx, dy, 12.0 * k * u, c);
        let (rx, ry) = p(-30.0, 2.0);
        s.fill_round(Rect::new(rx, ry, 60.0 * k * u, 14.0 * k * u), 7.0 * k * u, c);
    };
    let celestial = |x: f32, y: f32, r: f32| if is_day { sun(x, y, r) } else { moon(x, y, r * 1.15) };
    let drops = |n: usize, len: f32, c: Color, dots: bool| {
        for i in 0..n {
            let x = 30.0 + i as f32 * (40.0 / (n - 1).max(1) as f32);
            let (x1, y1) = at(x, 70.0);
            if dots {
                s.fill_circle(x1, y1 + 4.0 * u * (i % 2) as f32, 3.5 * u, c);
            } else {
                let (x2, y2) = at(x - 5.0, 70.0 + len);
                s.line(x1, y1, x2, y2, 3.0 * u.max(0.4), c);
            }
        }
    };

    match sky {
        Sky::Clear => celestial(50.0, 50.0, 20.0),
        Sky::PartlyCloudy => {
            celestial(38.0, 36.0, 15.0);
            cloud(56.0, 60.0, 0.95, cloud_c);
        }
        Sky::Cloudy => {
            cloud(38.0, 42.0, 0.75, Color::rgba(154, 163, 176, 255));
            cloud(56.0, 56.0, 1.0, cloud_c);
        }
        Sky::Fog => {
            cloud(50.0, 40.0, 0.9, Color::rgba(201, 206, 214, 255));
            for (i, y) in [64.0, 74.0, 84.0].iter().enumerate() {
                let (x1, yy) = at(24.0 + i as f32 * 6.0, *y);
                let (x2, _) = at(76.0 - i as f32 * 6.0, *y);
                s.line(x1, yy, x2, yy, 3.0 * u.max(0.4), Color::rgba(201, 206, 214, 200));
            }
        }
        Sky::Drizzle => {
            cloud(50.0, 42.0, 1.0, cloud_c);
            drops(3, 8.0, Color::rgba(90, 176, 255, 255), false);
        }
        Sky::Rain => {
            cloud(50.0, 42.0, 1.0, Color::rgba(190, 198, 210, 255));
            drops(4, 16.0, Color::rgba(90, 176, 255, 255), false);
        }
        Sky::Snow => {
            cloud(50.0, 42.0, 1.0, cloud_c);
            drops(5, 0.0, Color::rgba(255, 255, 255, 255), true);
        }
        Sky::Storm => {
            cloud(50.0, 40.0, 1.0, Color::rgba(140, 149, 163, 255));
            let bolt: Vec<(f32, f32)> =
                [(52.0, 54.0), (40.0, 74.0), (49.0, 74.0), (44.0, 92.0), (62.0, 68.0), (53.0, 68.0), (58.0, 54.0)]
                    .iter()
                    .map(|&(x, y)| at(x, y))
                    .collect();
            s.fill_poly(&bolt, sun_c);
        }
    }
}
