use windows::core::Result;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::create_layered;
use crate::gfx::{Align, Color, Rect, Surface};
use crate::monitors::Monitor;

const CELLS: i32 = 11;
const CELL: f32 = 10.0;
const W: f32 = CELLS as f32 * CELL + 16.0;
const H: f32 = W + 34.0;
pub const TIMER_PICK: usize = 9;

pub struct Picker {
    pub hwnd: HWND,
    surf: Surface,
    pixels: Vec<[u8; 3]>,
    last: POINT,
}

fn grab(c: POINT) -> Vec<[u8; 3]> {
    let mut out = vec![[0u8; 3]; (CELLS * CELLS) as usize];
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, CELLS, CELLS);
        let old = SelectObject(mem, bmp);
        let half = CELLS / 2;
        let _ = BitBlt(mem, 0, 0, CELLS, CELLS, screen, c.x - half, c.y - half, SRCCOPY);
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: CELLS,
                biHeight: -CELLS,
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (CELLS * CELLS * 4) as usize];
        SelectObject(mem, old);
        GetDIBits(mem, bmp, 0, CELLS as u32, Some(buf.as_mut_ptr() as _), &mut bi, DIB_RGB_COLORS);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        for (i, p) in buf.chunks_exact(4).enumerate() {
            out[i] = [p[2], p[1], p[0]];
        }
    }
    out
}

impl Picker {
    pub fn new(mon: &Monitor, font: &str) -> Result<Picker> {
        let hwnd = create_layered(WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT)?;
        let surf = Surface::new(W, H, mon.scale, font)?;
        unsafe {
            SetTimer(hwnd, TIMER_PICK, 16, None);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        let mut p = Picker { hwnd, surf, pixels: Vec::new(), last: POINT { x: i32::MIN, y: 0 } };
        p.update();
        Ok(p)
    }

    pub fn color(&self) -> Option<String> {
        let c = self.pixels.get((CELLS * CELLS / 2) as usize)?;
        Some(format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]))
    }

    pub fn update(&mut self) {
        let mut c = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut c);
        }
        if c.x == self.last.x && c.y == self.last.y {
            return;
        }
        self.last = c;
        self.pixels = grab(c);
        let s = &self.surf;
        s.begin();
        let white = Color::rgba(255, 255, 255, 255);
        let bg = Color::rgba(24, 24, 28, 240);
        s.fill_round(Rect::new(0.0, 0.0, W, H), 12.0, bg);
        s.stroke_round(Rect::new(0.5, 0.5, W - 1.0, H - 1.0), 12.0, 1.0, white.alpha(0.15));
        s.push_clip(Rect::new(8.0, 8.0, CELLS as f32 * CELL, CELLS as f32 * CELL));
        for (i, p) in self.pixels.iter().enumerate() {
            let (x, y) = ((i as i32 % CELLS) as f32, (i as i32 / CELLS) as f32);
            s.fill_round(Rect::new(8.0 + x * CELL, 8.0 + y * CELL, CELL, CELL), 0.0, Color::rgba(p[0], p[1], p[2], 255));
        }
        s.pop_clip();
        let mid = 8.0 + (CELLS / 2) as f32 * CELL;
        s.stroke_round(Rect::new(mid - 1.0, mid - 1.0, CELL + 2.0, CELL + 2.0), 1.0, 2.0, white);
        if let (Some(hex), Some(p)) = (self.color(), self.pixels.get((CELLS * CELLS / 2) as usize)) {
            let y = W + 2.0;
            s.fill_round(Rect::new(10.0, y + 4.0, 22.0, 22.0), 6.0, Color::rgba(p[0], p[1], p[2], 255));
            s.stroke_round(Rect::new(10.0, y + 4.0, 22.0, 22.0), 6.0, 1.0, white.alpha(0.3));
            s.text(&hex, Rect::new(38.0, y, W - 44.0, 30.0), 13.0, 600, white, Align::Left);
        }
        s.end();
        let sc = self.surf.scale;
        let (w, h) = ((W * sc) as i32, (H * sc) as i32);
        let mut x = c.x + (20.0 * sc) as i32;
        let mut y = c.y + (20.0 * sc) as i32;
        unsafe {
            let mon = MonitorFromPoint(c, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if GetMonitorInfoW(mon, &mut mi).as_bool() {
                if x + w > mi.rcMonitor.right {
                    x = c.x - (20.0 * sc) as i32 - w;
                }
                if y + h > mi.rcMonitor.bottom {
                    y = c.y - (20.0 * sc) as i32 - h;
                }
            }
        }
        self.surf.present(self.hwnd, x, y);
    }
}

impl Drop for Picker {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
