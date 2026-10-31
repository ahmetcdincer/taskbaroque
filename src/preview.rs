use windows::core::Result;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::create_popup;
use crate::config::Palette;
use crate::gfx::{self, Align, Color, Rect, Surface};
use crate::monitors::Monitor;

const CARD_W: f32 = 220.0;
const TITLE_H: f32 = 30.0;
const PAD: f32 = 8.0;

pub enum PreviewOut {
    Activate(HWND),
    Close(HWND),
}

pub struct Preview {
    pub hwnd: HWND,
    pub group: u64,
    pub wins: Vec<HWND>,
    surf: Surface,
    pal: Palette,
    cards: Vec<Rect>,
    thumbs: Vec<isize>,
    icons: Vec<Option<ID2D1Bitmap>>,
    hover: Option<usize>,
    hover_close: bool,
    pub rect: RECT,
    thumb_h: f32,
    anim: Option<(RECT, std::time::Instant)>,
    alpha: u8,
}

const SLIDE_MS: f32 = 170.0;

struct Layout {
    w: f32,
    h: f32,
    card_w: f32,
    card_h: f32,
    thumb_h: f32,
}

fn layout(mon: &Monitor, n: usize) -> Layout {
    let scale = mon.scale;
    let n = n.max(1) as f32;
    let avail = (mon.rect.right - mon.rect.left) as f32 / scale - 32.0;
    let card_w = CARD_W.min((avail - PAD) / n - PAD).max(120.0);
    let thumb_h = ((card_w - 2.0 * PAD) * 0.58).round();
    let card_h = TITLE_H + thumb_h + PAD;
    Layout { w: PAD + n * (card_w + PAD), h: card_h + 2.0 * PAD, card_w, card_h, thumb_h }
}

fn place(mon: &Monitor, top: bool, w_px: i32, h_px: i32, anchor_x: i32, edge_y: i32) -> RECT {
    let gap = (10.0 * mon.scale) as i32;
    let x = (anchor_x - w_px / 2).clamp(mon.rect.left + gap, (mon.rect.right - gap - w_px).max(mon.rect.left));
    let y = if top { edge_y + gap } else { edge_y - gap - h_px };
    RECT { left: x, top: y, right: x + w_px, bottom: y + h_px }
}

impl Preview {
    pub fn new(pal: &Palette, mon: &Monitor, top: bool, group: u64, wins: Vec<HWND>, anchor_x: i32, edge_y: i32) -> Result<Preview> {
        let lay = layout(mon, wins.len());
        let hwnd = create_popup(WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE)?;
        let surf = Surface::new(lay.w, lay.h, mon.scale, &pal.font)?;
        let rect = place(mon, top, surf.w_px, surf.h_px, anchor_x, edge_y);
        let (x, y) = (rect.left, rect.top);
        let cards = cards(&lay, wins.len());
        let icons = wins.iter().map(|h| gfx::window_icon(*h).and_then(|i| surf.bitmap_from_icon(i))).collect();
        let thumb_h = lay.thumb_h;
        let mut p = Preview {
            hwnd,
            group,
            wins,
            surf,
            pal: pal.clone(),
            cards,
            thumbs: Vec::new(),
            icons,
            hover: None,
            hover_close: false,
            rect,
            thumb_h,
            anim: None,
            alpha: 255,
        };
        unsafe {
            let round = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &round as *const _ as _, 4);
            let none = 0xFFFF_FFFEu32;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &none as *const _ as _, 4);
            let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, p.surf.w_px, p.surf.h_px, SWP_NOACTIVATE);
        }
        p.register_thumbs(thumb_h);
        p.draw();
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(p)
    }

    pub fn retarget(&mut self, mon: &Monitor, top: bool, group: u64, wins: Vec<HWND>, anchor_x: i32, edge_y: i32) {
        let from = self.current_rect();
        self.unregister_thumbs();
        let lay = layout(mon, wins.len());
        let size = ((lay.w * mon.scale).ceil() as i32, (lay.h * mon.scale).ceil() as i32);
        if (self.surf.w_px, self.surf.h_px) != size || self.surf.scale != mon.scale {
            if let Ok(s) = Surface::new(lay.w, lay.h, mon.scale, &self.pal.font) {
                self.surf = s;
            }
        }
        self.cards = cards(&lay, wins.len());
        self.icons = wins.iter().map(|h| gfx::window_icon(*h).and_then(|i| self.surf.bitmap_from_icon(i))).collect();
        self.group = group;
        self.wins = wins;
        self.hover = None;
        self.hover_close = false;
        self.thumb_h = lay.thumb_h;
        self.rect = place(mon, top, self.surf.w_px, self.surf.h_px, anchor_x, edge_y);
        let start = RECT { left: from.left + ((from.right - from.left) - self.surf.w_px) / 2, top: self.rect.top, right: 0, bottom: 0 };
        unsafe {
            let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, start.left, start.top, self.surf.w_px, self.surf.h_px, SWP_NOACTIVATE);
        }
        self.alpha = 0;
        self.register_thumbs(self.thumb_h);
        self.draw();
        self.anim = Some((start, std::time::Instant::now()));
    }

    fn current_rect(&self) -> RECT {
        let mut r = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut r);
        }
        r
    }

    pub fn tick(&mut self) -> bool {
        let Some((from, t0)) = self.anim else { return false };
        let t = (t0.elapsed().as_secs_f32() * 1000.0 / SLIDE_MS).min(1.0);
        let e = 1.0 - (1.0 - t).powi(3);
        let x = from.left + ((self.rect.left - from.left) as f32 * e).round() as i32;
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, x, self.rect.top, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER);
        }
        let a = ((t * 1.6).min(1.0) * 255.0) as u8;
        if a != self.alpha {
            self.alpha = a;
            self.set_thumb_opacity(a);
            self.draw();
        }
        if t >= 1.0 {
            self.anim = None;
        }
        self.anim.is_some()
    }

    fn set_thumb_opacity(&self, a: u8) {
        for &id in &self.thumbs {
            if id == 0 {
                continue;
            }
            let props = DWM_THUMBNAIL_PROPERTIES { dwFlags: DWM_TNP_OPACITY, opacity: a, ..Default::default() };
            unsafe {
                let _ = DwmUpdateThumbnailProperties(id, &props);
            }
        }
    }

    fn unregister_thumbs(&mut self) {
        for t in self.thumbs.drain(..) {
            if t != 0 {
                unsafe {
                    let _ = DwmUnregisterThumbnail(t);
                }
            }
        }
    }

    fn register_thumbs(&mut self, thumb_h: f32) {
        let sc = self.surf.scale;
        for (i, &src) in self.wins.iter().enumerate() {
            let card = self.cards[i];
            let area = Rect::new(card.x + PAD, card.y + TITLE_H, card.w - 2.0 * PAD, thumb_h);
            if unsafe { IsIconic(src).as_bool() } {
                self.thumbs.push(0);
                continue;
            }
            let Ok(id) = (unsafe { DwmRegisterThumbnail(self.hwnd, src) }) else {
                self.thumbs.push(0);
                continue;
            };
            let size = unsafe { DwmQueryThumbnailSourceSize(id) }.unwrap_or(SIZE { cx: 16, cy: 9 });
            let (sw, sh) = (size.cx.max(1) as f32, size.cy.max(1) as f32);
            let k = (area.w / sw).min(area.h / sh);
            let (dw, dh) = (sw * k, sh * k);
            let (dx, dy) = (area.x + (area.w - dw) / 2.0, area.y + (area.h - dh) / 2.0);
            let props = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY,
                rcDestination: RECT {
                    left: (dx * sc) as i32,
                    top: (dy * sc) as i32,
                    right: ((dx + dw) * sc) as i32,
                    bottom: ((dy + dh) * sc) as i32,
                },
                opacity: self.alpha,
                fVisible: TRUE,
                ..Default::default()
            };
            unsafe {
                let _ = DwmUpdateThumbnailProperties(id, &props);
            }
            self.thumbs.push(id);
        }
    }

    fn draw(&self) {
        let s = &self.surf;
        let p = &self.pal;
        let k = self.alpha as f32 / 255.0;
        let fg = unsafe { GetForegroundWindow() };
        s.begin();
        let base = Color::rgba(24, 24, 28, 255);
        let bg = p.bg;
        let mix = |a: f32, b: f32| a * bg.a + b * (1.0 - bg.a);
        let solid = Color { r: mix(bg.r, base.r), g: mix(bg.g, base.g), b: mix(bg.b, base.b), a: 1.0 };
        let full = Rect::new(0.0, 0.0, s.w_px as f32 / s.scale, s.h_px as f32 / s.scale);
        s.fill_round(full, 0.0, solid);
        s.stroke_round_grad(full.inset(0.5), 8.0, 1.0, &p.border, p.border_vertical);
        for (i, card) in self.cards.iter().enumerate() {
            if k <= 0.0 {
                break;
            }
            let h = self.wins[i];
            let hovered = self.hover == Some(i);
            if hovered {
                s.fill_round(*card, 8.0, p.text.alpha(0.08));
            }
            if h == fg {
                s.stroke_round(card.inset(0.5), 7.5, 1.0, p.accent.alpha(0.8 * k));
            }
            let title = Rect::new(card.x + PAD, card.y, card.w - 2.0 * PAD, TITLE_H);
            let mut tx = title.x;
            if let Some(bmp) = &self.icons[i] {
                s.image(bmp, Rect::new(tx, title.y + (TITLE_H - 16.0) / 2.0, 16.0, 16.0), 0.0);
                tx += 22.0;
            }
            let close_w = if hovered { 26.0 } else { 0.0 };
            s.text(&window_title(h), Rect::new(tx, title.y, title.x + title.w - tx - close_w, TITLE_H), 12.0, 500, p.text.alpha(p.text.a * k), Align::Left);
            if hovered {
                let cr = self.close_rect(i);
                if self.hover_close {
                    s.fill_round(cr, 4.0, Color::rgba(196, 43, 28, 255));
                }
                s.glyph('\u{E711}', cr, 10.0, p.text);
            }
            if self.thumbs[i] == 0 {
                if let Some(bmp) = &self.icons[i] {
                    let (cx, cy) = (card.x + card.w / 2.0, card.y + TITLE_H + (card.h - TITLE_H - PAD) / 2.0);
                    s.image(bmp, Rect::new(cx - 20.0, cy - 20.0, 40.0, 40.0), 0.0);
                }
            }
        }
        s.end();
        unsafe {
            let dc = GetDC(self.hwnd);
            s.blit(dc);
            ReleaseDC(self.hwnd, dc);
        }
    }

    fn close_rect(&self, i: usize) -> Rect {
        let c = self.cards[i];
        Rect::new(c.x + c.w - PAD - 22.0, c.y + (TITLE_H - 22.0) / 2.0, 22.0, 22.0)
    }

    fn at(&self, lp: LPARAM) -> (Option<usize>, bool) {
        let x = (lp.0 & 0xFFFF) as i16 as f32 / self.surf.scale;
        let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32 / self.surf.scale;
        let i = self.cards.iter().position(|c| c.contains(x, y));
        (i, i.is_some_and(|i| self.close_rect(i).contains(x, y)))
    }

    pub fn contains_screen(&self, p: POINT) -> bool {
        p.x >= self.rect.left && p.x < self.rect.right && p.y >= self.rect.top && p.y < self.rect.bottom
    }

    pub fn window_at_screen(&self, p: POINT) -> Option<HWND> {
        if !self.contains_screen(p) {
            return None;
        }
        let sc = self.surf.scale;
        let (x, y) = ((p.x - self.rect.left) as f32 / sc, (p.y - self.rect.top) as f32 / sc);
        self.cards.iter().position(|r| r.contains(x, y)).and_then(|i| self.wins.get(i).copied())
    }

    pub fn handle(&mut self, msg: u32, lp: LPARAM) -> Option<PreviewOut> {
        match msg {
            WM_PAINT => unsafe {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(self.hwnd, &mut ps);
                self.surf.blit(dc);
                let _ = EndPaint(self.hwnd, &ps);
            },
            WM_MOUSEMOVE => {
                let (i, close) = self.at(lp);
                if (i, close) != (self.hover, self.hover_close) {
                    self.hover = i;
                    self.hover_close = close;
                    self.draw();
                }
            }
            WM_LBUTTONUP => {
                let (i, close) = self.at(lp);
                let h = self.wins[i?];
                return Some(if close { PreviewOut::Close(h) } else { PreviewOut::Activate(h) });
            }
            WM_MBUTTONUP => return Some(PreviewOut::Close(self.wins[self.at(lp).0?])),
            _ => {}
        }
        None
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.unregister_thumbs();
        unsafe {
            crate::droptarget::revoke(self.hwnd);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn cards(lay: &Layout, n: usize) -> Vec<Rect> {
    (0..n).map(|i| Rect::new(PAD + i as f32 * (lay.card_w + PAD), PAD, lay.card_w, lay.card_h)).collect()
}

pub fn window_title(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}
