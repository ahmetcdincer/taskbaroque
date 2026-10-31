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
}

impl Preview {
    pub fn new(pal: &Palette, mon: &Monitor, top: bool, group: u64, wins: Vec<HWND>, anchor_x: i32, edge_y: i32) -> Result<Preview> {
        let scale = mon.scale;
        let n = wins.len().max(1) as f32;
        let avail = (mon.rect.right - mon.rect.left) as f32 / scale - 32.0;
        let card_w = CARD_W.min((avail - PAD) / n - PAD).max(120.0);
        let thumb_h = ((card_w - 2.0 * PAD) * 0.58).round();
        let card_h = TITLE_H + thumb_h + PAD;
        let (w, h) = (PAD + n * (card_w + PAD), card_h + 2.0 * PAD);

        let hwnd = create_popup(WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE)?;
        let surf = Surface::new(w, h, scale, &pal.font)?;
        let gap = (10.0 * scale) as i32;
        let x = (anchor_x - surf.w_px / 2).clamp(mon.rect.left + gap, (mon.rect.right - gap - surf.w_px).max(mon.rect.left));
        let y = if top { edge_y + gap } else { edge_y - gap - surf.h_px };
        let rect = RECT { left: x, top: y, right: x + surf.w_px, bottom: y + surf.h_px };

        let cards: Vec<Rect> =
            (0..wins.len()).map(|i| Rect::new(PAD + i as f32 * (card_w + PAD), PAD, card_w, card_h)).collect();
        let icons = wins.iter().map(|h| gfx::window_icon(*h).and_then(|i| surf.bitmap_from_icon(i))).collect();
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
                opacity: 255,
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
            let h = self.wins[i];
            let hovered = self.hover == Some(i);
            if hovered {
                s.fill_round(*card, 8.0, p.text.alpha(0.08));
            }
            if h == fg {
                s.stroke_round(card.inset(0.5), 7.5, 1.0, p.accent.alpha(0.8));
            }
            let title = Rect::new(card.x + PAD, card.y, card.w - 2.0 * PAD, TITLE_H);
            let mut tx = title.x;
            if let Some(bmp) = &self.icons[i] {
                s.image(bmp, Rect::new(tx, title.y + (TITLE_H - 16.0) / 2.0, 16.0, 16.0), 0.0);
                tx += 22.0;
            }
            let close_w = if hovered { 26.0 } else { 0.0 };
            s.text(&window_title(h), Rect::new(tx, title.y, title.x + title.w - tx - close_w, TITLE_H), 12.0, 500, p.text, Align::Left);
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
        unsafe {
            for &t in &self.thumbs {
                if t != 0 {
                    let _ = DwmUnregisterThumbnail(t);
                }
            }
            crate::droptarget::revoke(self.hwnd);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

pub fn window_title(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}
