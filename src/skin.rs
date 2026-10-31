use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use crate::config::{Palette, StartStyle, Tiles};
use crate::gfx::{Color, Rect, Surface};

pub fn panel(s: &Surface, r: Rect, radius: f32, pal: &Palette, bar: bool) {
    let radius = radius.min(r.h / 2.0).max(0.0);
    if bar && pal.glow > 0.0 {
        s.glow_round(r, radius, &pal.border, pal.border_vertical, pal.glow);
    }
    if bar && pal.bar_gradient.len() > 1 {
        s.fill_round_grad(r, radius, &pal.bar_gradient, pal.gradient_vertical);
    } else {
        s.fill_round(r, radius, if bar { pal.bar_bg } else { pal.bg });
    }
    let n = pal.glows.len();
    for (i, c) in pal.glows.iter().enumerate() {
        if c.a <= 0.0 {
            continue;
        }
        let seg = r.w / n as f32;
        let cx = r.x + seg * (i as f32 + 0.5);
        s.fill_round_radial(r, radius, cx, r.y + r.h, seg * 0.8, r.h.min(120.0) * 0.8, *c);
    }
    if let Some(c) = pal.sheen {
        s.fill_round_grad(r, radius, &[c, c.alpha(0.0), c.alpha(0.0)], true);
    }
    if let Some(e) = pal.edge {
        let inset = radius * 0.7;
        let y = r.y + r.h - 1.0;
        s.line(r.x + inset, y, r.x + r.w - inset, y, 1.5, e);
    }
    let bw = pal.border_width;
    s.stroke_round_grad(r.inset(bw / 2.0), (radius - bw / 2.0).max(0.0), bw, &pal.border, pal.border_vertical);
    if pal.brackets {
        brackets(s, r, pal.accent);
    }
}

fn brackets(s: &Surface, r: Rect, c: Color) {
    let (l, w) = (8.0, 2.0);
    let (x0, y0, x1, y1) = (r.x + 1.0, r.y + 1.0, r.x + r.w - 1.0, r.y + r.h - 1.0);
    for (x, y, dx, dy) in [(x0, y0, 1.0, 1.0), (x1, y0, -1.0, 1.0), (x0, y1, 1.0, -1.0), (x1, y1, -1.0, -1.0)] {
        s.line(x, y, x + dx * l, y, w, c);
        s.line(x, y, x, y + dy * l, w, c);
    }
}

pub fn tile(s: &Surface, r: Rect, pal: &Palette, active: bool, hover: bool) {
    let t = pal.text;
    match pal.tiles {
        Tiles::None => {
            if active {
                s.fill_round(r, 8.0, t.alpha(0.11));
            } else if hover {
                s.fill_round(r, 8.0, t.alpha(0.06));
            }
        }
        Tiles::Glass => {
            let fill = if active { 0.16 } else if hover { 0.12 } else { 0.07 };
            s.fill_round(r, 9.0, t.alpha(fill));
            s.fill_round_grad(r, 9.0, &[t.alpha(0.08), t.alpha(0.0)], true);
            let edge = if active { pal.accent.alpha(0.7) } else { t.alpha(0.14) };
            s.stroke_round(r.inset(0.5), 8.5, 1.0, edge);
        }
        Tiles::Solid => {
            s.fill_round(r, 9.0, Color::rgba(0, 0, 0, if hover { 70 } else { 100 }));
            s.fill_round_grad(r, 9.0, &[t.alpha(0.07), t.alpha(0.0)], true);
            let edge = if active { pal.accent.alpha(0.8) } else { t.alpha(0.07) };
            s.stroke_round(r.inset(0.5), 8.5, 1.0, edge);
        }
        Tiles::Outline => {
            let fill = if active { pal.accent.alpha(0.14) } else if hover { t.alpha(0.08) } else { t.alpha(0.03) };
            s.fill_round(r, 3.0, fill);
            let edge = if active { pal.accent } else { pal.border[0] };
            s.stroke_round(r.inset(0.75), 2.5, 1.5, edge);
        }
    }
}

pub fn start_button(s: &Surface, r: Rect, pal: &Palette, hover: bool, icon: f32, custom: Option<&ID2D1Bitmap>) {
    let t = pal.text;
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    match pal.start {
        StartStyle::Plain => {
            if hover {
                s.fill_round(r, 8.0, t.alpha(0.06));
            }
        }
        StartStyle::Circle => {
            let rad = r.w.min(r.h) / 2.0;
            s.fill_circle(cx, cy, rad, t.alpha(if hover { 0.13 } else { 0.07 }));
            let ring: Vec<Color> = if pal.glow > 0.0 { pal.border.clone() } else { vec![t.alpha(0.14)] };
            let c = Rect::new(cx - rad + 0.75, cy - rad + 0.75, 2.0 * rad - 1.5, 2.0 * rad - 1.5);
            s.stroke_round_grad(c, rad, 1.5, &ring, pal.border_vertical);
        }
        StartStyle::Square => {
            let k = if pal.tiles == Tiles::Outline { 3.0 } else { 8.0 };
            s.fill_round(r, k, t.alpha(if hover { 0.12 } else { 0.07 }));
            let edge = if pal.tiles == Tiles::Outline { pal.accent } else { t.alpha(0.1) };
            s.stroke_round(r.inset(0.75), k - 0.5, 1.5, edge);
        }
    }
    if let Some(b) = custom {
        let z = icon * 0.85;
        s.image(b, Rect::new(cx - z / 2.0, cy - z / 2.0, z, z), z * 0.18);
        return;
    }
    let q = icon * 0.38;
    let corner = if pal.tiles == Tiles::Outline { 0.5 } else { q * 0.22 };
    for (i, (dx, dy)) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)].into_iter().enumerate() {
        let sx = cx + if dx < 0.0 { -q - 1.0 } else { 1.0 };
        let sy = cy + if dy < 0.0 { -q - 1.0 } else { 1.0 };
        let c = pal.start_colors[i % pal.start_colors.len()];
        s.fill_round(Rect::new(sx, sy, q, q), corner, c);
    }
}

pub fn separator(s: &Surface, x: f32, cy: f32, h: f32, pal: &Palette) {
    let c = match (pal.edge, pal.brackets) {
        (Some(e), _) => e.alpha(0.8),
        (None, true) => pal.accent.alpha(0.7),
        _ => pal.text.alpha(0.18),
    };
    s.line(x, cy - h / 2.0, x, cy + h / 2.0, 1.0, c);
}
