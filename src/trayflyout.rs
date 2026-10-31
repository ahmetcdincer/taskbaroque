use windows::core::Result;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::create_layered;
use crate::config::Palette;
use crate::gfx::{Align, Rect, Surface};
use crate::i18n::tr;
use crate::monitors::Monitor;
use crate::skin;
use crate::tray;

const CELL: f32 = 40.0;
const PAD: f32 = 10.0;
const TIP_H: f32 = 26.0;
const COLS: usize = 5;

struct Item {
    id: u64,
    pid: u32,
    tip: String,
    bmp: Option<ID2D1Bitmap>,
}

pub enum FlyoutOut {
    Click(u64, bool, u32),
}

pub struct Flyout {
    pub hwnd: HWND,
    surf: Surface,
    pal: Palette,
    items: Vec<Item>,
    rects: Vec<Rect>,
    hover: Option<usize>,
    pub rect: RECT,
    gen: u64,
    anchor_x: i32,
    edge_y: i32,
    top: bool,
    mon: Monitor,
}

impl Flyout {
    pub fn new(pal: &Palette, mon: Monitor, anchor_x: i32, edge_y: i32, top: bool) -> Result<Flyout> {
        let hwnd = create_layered(WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE)?;
        let surf = Surface::new(1.0, 1.0, mon.scale, &pal.font)?;
        let mut f = Flyout {
            hwnd,
            surf,
            pal: pal.clone(),
            items: Vec::new(),
            rects: Vec::new(),
            hover: None,
            rect: RECT::default(),
            gen: 0,
            anchor_x,
            edge_y,
            top,
            mon,
        };
        f.rebuild()?;
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(f)
    }

    fn rebuild(&mut self) -> Result<()> {
        let icons: Vec<_> = tray::icons().into_iter().filter(|i| !i.promoted).collect();
        let n = icons.len().max(1);
        let cols = n.min(COLS);
        let rows = n.div_ceil(cols);
        let (w, h) = (2.0 * PAD + cols as f32 * CELL, 2.0 * PAD + rows as f32 * CELL + TIP_H);
        self.surf = Surface::new(w, h, self.mon.scale, &self.pal.font)?;
        self.items = icons
            .iter()
            .map(|i| Item { id: i.id, pid: i.pid, tip: i.tip.lines().next().unwrap_or("").trim().to_string(), bmp: self.surf.bitmap_from_icon(HICON(i.icon as _)) })
            .collect();
        self.rects = (0..self.items.len())
            .map(|k| Rect::new(PAD + (k % cols) as f32 * CELL, PAD + (k / cols) as f32 * CELL, CELL, CELL))
            .collect();
        self.gen = tray::generation();
        let (wp, hp) = (self.surf.w_px, self.surf.h_px);
        let gap = (8.0 * self.mon.scale) as i32;
        let x = (self.anchor_x - wp / 2).clamp(self.mon.rect.left + gap, self.mon.rect.right - gap - wp);
        let y = if self.top { self.edge_y + gap } else { self.edge_y - gap - hp };
        self.rect = RECT { left: x, top: y, right: x + wp, bottom: y + hp };
        self.draw();
        Ok(())
    }

    pub fn refresh(&mut self) {
        if tray::generation() != self.gen {
            let _ = self.rebuild();
        }
    }

    fn draw(&self) {
        let s = &self.surf;
        let p = &self.pal;
        let full = Rect::new(0.0, 0.0, s.w_px as f32 / s.scale, s.h_px as f32 / s.scale);
        s.begin();
        let mut pal = p.clone();
        pal.bg.a = pal.bg.a.max(0.94);
        skin::panel(s, full.inset(0.5), p.radius.min(12.0), &pal, false);
        for (i, it) in self.items.iter().enumerate() {
            let r = self.rects[i];
            if self.hover == Some(i) {
                s.fill_round(r.inset(2.0), 8.0, p.text.alpha(0.1));
            }
            if let Some(b) = &it.bmp {
                s.image(b, Rect::new(r.x + (CELL - 20.0) / 2.0, r.y + (CELL - 20.0) / 2.0, 20.0, 20.0), 0.0);
            }
        }
        let tip = match self.hover.and_then(|i| self.items.get(i)) {
            Some(it) if !it.tip.is_empty() => it.tip.as_str(),
            _ => tr("Gizli simgeler"),
        };
        s.text(tip, Rect::new(PAD, full.h - TIP_H - 4.0, full.w - 2.0 * PAD, TIP_H), 11.5, 500, p.dim, Align::Center);
        s.end();
        s.present(self.hwnd, self.rect.left, self.rect.top);
    }

    pub fn contains(&self, p: POINT) -> bool {
        p.x >= self.rect.left && p.x < self.rect.right && p.y >= self.rect.top && p.y < self.rect.bottom
    }

    fn at(&self, lp: LPARAM) -> Option<usize> {
        let x = (lp.0 & 0xFFFF) as i16 as f32 / self.surf.scale;
        let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32 / self.surf.scale;
        self.rects.iter().position(|r| r.contains(x, y))
    }

    pub fn handle(&mut self, msg: u32, lp: LPARAM) -> Option<FlyoutOut> {
        match msg {
            WM_MOUSEMOVE => {
                let h = self.at(lp);
                if h != self.hover {
                    self.hover = h;
                    self.draw();
                }
                None
            }
            WM_LBUTTONUP | WM_RBUTTONUP => {
                let it = self.items.get(self.at(lp)?)?;
                Some(FlyoutOut::Click(it.id, msg == WM_RBUTTONUP, it.pid))
            }
            _ => None,
        }
    }
}

impl Drop for Flyout {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
