use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use windows::core::{w, Result, HSTRING, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::Ctx;
use crate::gfx::{Align, Color, Rect, Surface};
use crate::i18n::{self, tr};
use crate::monitors::Monitor;
use crate::settings::base_color;

pub const LAUNCHER_WND: isize = 3;
const W: f32 = 600.0;
const H: f32 = 540.0;
const COLS: usize = 6;
const LIST_ROWS: usize = 9;
const WM_MOUSELEAVE_: u32 = 0x02A3;

#[derive(Clone, Debug)]
pub struct AppEntry {
    pub name: String,
    pub id: String,
}

static APPS: Mutex<Option<Arc<Vec<AppEntry>>>> = Mutex::new(None);

thread_local! {
    static ICONS: std::cell::RefCell<HashMap<String, isize>> = std::cell::RefCell::new(HashMap::new());
}

pub fn preload() {
    std::thread::spawn(|| {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let list = enumerate();
        *APPS.lock().unwrap() = Some(Arc::new(list));
    });
}

fn enumerate() -> Vec<AppEntry> {
    let mut out = Vec::new();
    unsafe {
        let Ok(folder) = SHGetKnownFolderItem::<_, IShellItem>(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) else { return out };
        let Ok(en) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else { return out };
        loop {
            let mut items = [None];
            let mut n = 0u32;
            if en.Next(&mut items, Some(&mut n)).is_err() || n == 0 {
                break;
            }
            let Some(item) = items[0].take() else { break };
            let name = take_name(item.GetDisplayName(SIGDN_NORMALDISPLAY));
            let id = take_name(item.GetDisplayName(SIGDN_PARENTRELATIVEPARSING));
            let (Some(name), Some(id)) = (name, id) else { continue };
            let low = name.to_lowercase();
            if id.starts_with("http") || ["uninstall", "kaldır", "readme", "release notes", "help"].iter().any(|k| low.contains(k)) {
                continue;
            }
            out.push(AppEntry { name, id });
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out.dedup_by(|a, b| a.name == b.name);
    out
}

unsafe fn take_name(r: Result<PWSTR>) -> Option<String> {
    let p = r.ok()?;
    let s = p.to_string().ok();
    CoTaskMemFree(Some(p.0 as _));
    s
}

fn icon_hbitmap(id: &str) -> Option<HBITMAP> {
    if let Some(h) = ICONS.with(|m| m.borrow().get(id).copied()) {
        return (h != 0).then_some(HBITMAP(h as _));
    }
    let h = unsafe {
        SHCreateItemFromParsingName::<_, _, IShellItemImageFactory>(&HSTRING::from(format!("shell:AppsFolder\\{id}")), None)
            .ok()
            .and_then(|f| f.GetImage(SIZE { cx: 64, cy: 64 }, SIIGBF_ICONONLY).ok())
    };
    ICONS.with(|m| m.borrow_mut().insert(id.to_string(), h.map_or(0, |b| b.0 as isize)));
    h
}

fn counts_path() -> std::path::PathBuf {
    crate::config::dir().join("launcher.json")
}

fn load_counts() -> HashMap<String, u32> {
    std::fs::read_to_string(counts_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub enum LauncherOut {
    Close,
    Power,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit {
    Item(usize),
    Power,
    Settings,
    Search,
}

const TIMER_OUTSIDE: usize = 7;

pub struct Launcher {
    pub hwnd: HWND,
    surf: Surface,
    query: String,
    sel: usize,
    hover: Option<Hit>,
    tracking: bool,
    hits: Vec<(Rect, Hit)>,
    shown: Vec<AppEntry>,
    icons: HashMap<String, Option<ID2D1Bitmap>>,
    counts: HashMap<String, u32>,
}

impl Launcher {
    pub fn new(ctx: &Ctx, mon: Monitor, anchor_x: i32, top: bool) -> Result<Launcher> {
        let sc = mon.scale;
        let (wp, hp) = ((W * sc) as i32, (H * sc) as i32);
        let gap = (10.0 * sc) as i32;
        let x = (anchor_x - wp / 2).clamp(mon.work.left + gap, mon.work.right - gap - wp);
        let y = if top { mon.work.top + gap } else { mon.work.bottom - gap - hp };
        let hwnd = unsafe {
            let inst = GetModuleHandleW(None)?;
            CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_TOPMOST, w!("TaskbaroqueWnd"), w!("Taskbaroque"), WS_POPUP, x, y, wp, hp, None, None, inst, None)?
        };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, LAUNCHER_WND);
            let round: u32 = 2;
            let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(33), &round as *const _ as _, 4);
        }
        let surf = Surface::new(W, H, sc, &ctx.pal.font)?;
        let mut l = Launcher {
            hwnd,
            surf,
            query: String::new(),
            sel: 0,
            hover: None,
            tracking: false,
            hits: Vec::new(),
            shown: Vec::new(),
            icons: HashMap::new(),
            counts: load_counts(),
        };
        l.filter();
        l.draw(ctx);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let fg = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let me = windows::Win32::System::Threading::GetCurrentThreadId();
            let attach = fg != 0 && fg != me && windows::Win32::System::Threading::AttachThreadInput(me, fg, true).as_bool();
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(hwnd);
            if attach {
                let _ = windows::Win32::System::Threading::AttachThreadInput(me, fg, false);
            }
            SetTimer(hwnd, TIMER_OUTSIDE, 40, None);
        }
        Ok(l)
    }

    fn filter(&mut self) {
        let apps = APPS.lock().unwrap().clone().unwrap_or_default();
        let q = self.query.trim().to_lowercase();
        self.shown = if q.is_empty() {
            let mut v: Vec<AppEntry> = apps.iter().cloned().collect();
            v.sort_by(|a, b| {
                let ca = self.counts.get(&a.id).copied().unwrap_or(0);
                let cb = self.counts.get(&b.id).copied().unwrap_or(0);
                cb.cmp(&ca).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            v.truncate(COLS * 4);
            v
        } else {
            let mut v: Vec<(u32, AppEntry)> = apps
                .iter()
                .filter_map(|a| {
                    let n = a.name.to_lowercase();
                    let words_start = n.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(&q));
                    let score = if n.starts_with(&q) {
                        0
                    } else if words_start {
                        1
                    } else if n.contains(&q) {
                        2
                    } else if initials(&n).starts_with(&q) {
                        3
                    } else {
                        return None;
                    };
                    Some((score * 100_000 - self.counts.get(&a.id).copied().unwrap_or(0).min(99_999), a.clone()))
                })
                .collect();
            v.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.len().cmp(&b.1.name.len())));
            v.into_iter().map(|(_, a)| a).take(LIST_ROWS - 1).collect()
        };
        self.sel = 0;
    }

    fn icon(&mut self, id: &str) -> Option<ID2D1Bitmap> {
        if let Some(b) = self.icons.get(id) {
            return b.clone();
        }
        let b = icon_hbitmap(id).and_then(|h| self.surf.bitmap_from_hbitmap(h));
        self.icons.insert(id.to_string(), b.clone());
        b
    }

    fn launch(&mut self, i: usize) -> bool {
        let Some(a) = self.shown.get(i).cloned() else {
            if !self.query.trim().is_empty() {
                let q = HSTRING::from(format!("search-ms:query={}", self.query.trim()));
                unsafe {
                    ShellExecuteW(None, w!("open"), &q, None, None, SW_SHOWNORMAL);
                }
                return true;
            }
            return false;
        };
        unsafe {
            ShellExecuteW(None, w!("open"), &HSTRING::from(format!("shell:AppsFolder\\{}", a.id)), None, None, SW_SHOWNORMAL);
        }
        *self.counts.entry(a.id).or_insert(0) += 1;
        if let Ok(t) = serde_json::to_string(&self.counts) {
            let _ = std::fs::write(counts_path(), t);
        }
        true
    }

    pub fn draw(&mut self, ctx: &Ctx) {
        let p = &ctx.pal;
        let base = base_color(p);
        let white = p.text;
        let dim = p.dim;
        let accent = Color { a: 1.0, ..p.accent };
        let searching = !self.query.trim().is_empty();
        let shown = self.shown.clone();
        let icons: Vec<Option<ID2D1Bitmap>> = shown.iter().map(|a| self.icon(&a.id)).collect();
        let mut hits = Vec::new();
        let s = &self.surf;
        s.begin();
        s.fill_round(Rect::new(0.0, 0.0, W, H), 0.0, base);
        s.stroke_round(Rect::new(0.5, 0.5, W - 1.0, H - 1.0), 8.0, 1.0, white.alpha(0.08));

        let sb = Rect::new(24.0, 22.0, W - 48.0, 40.0);
        s.fill_round(sb, 20.0, white.alpha(0.07));
        s.stroke_round(sb.inset(0.5), 20.0, 1.0, accent.alpha(0.6));
        s.glyph('\u{E721}', Rect::new(sb.x + 12.0, sb.y, 22.0, sb.h), 13.0, dim);
        let tr_ = Rect::new(sb.x + 40.0, sb.y, sb.w - 56.0, sb.h);
        if self.query.is_empty() {
            s.text(tr("Uygulama ara"), tr_, 13.5, 400, dim, Align::Left);
            s.fill_round(Rect::new(tr_.x, sb.y + 11.0, 1.5, sb.h - 22.0), 0.5, white);
        } else {
            s.text(&self.query, tr_, 13.5, 500, white, Align::Left);
            let cx = tr_.x + s.measure(&self.query, 13.5, 500) + 2.0;
            s.fill_round(Rect::new(cx.min(tr_.x + tr_.w), sb.y + 11.0, 1.5, sb.h - 22.0), 0.5, white);
        }
        hits.push((sb, Hit::Search));

        let top = 84.0;
        if !searching {
            let any_used = shown.iter().any(|a| self.counts.get(&a.id).copied().unwrap_or(0) > 0);
            s.text(if any_used { tr("Sık kullanılanlar") } else { tr("Uygulamalar") }, Rect::new(30.0, top, 300.0, 20.0), 12.5, 700, white, Align::Left);
            let gw = (W - 48.0) / COLS as f32;
            let gh = 88.0;
            for (i, a) in shown.iter().enumerate() {
                let r = Rect::new(24.0 + (i % COLS) as f32 * gw, top + 30.0 + (i / COLS) as f32 * gh, gw, gh - 6.0);
                let hov = self.hover == Some(Hit::Item(i)) || self.sel == i;
                if hov {
                    s.fill_round(r.inset(2.0), 10.0, white.alpha(if self.hover == Some(Hit::Item(i)) { 0.09 } else { 0.06 }));
                }
                let ir = Rect::new(r.x + (r.w - 36.0) / 2.0, r.y + 10.0, 36.0, 36.0);
                match &icons[i] {
                    Some(b) => s.image(b, ir, 0.0),
                    None => {
                        s.fill_round(ir, 9.0, accent.alpha(0.3));
                        let letter: String = a.name.chars().next().unwrap_or('?').to_uppercase().collect();
                        s.text(&letter, ir, 16.0, 700, white, Align::Center);
                    }
                }
                s.text(&a.name, Rect::new(r.x + 4.0, r.y + 52.0, r.w - 8.0, 18.0), 11.0, 500, white, Align::Center);
                hits.push((r, Hit::Item(i)));
            }
            if shown.is_empty() {
                s.text(tr("Uygulamalar yükleniyor…"), Rect::new(0.0, top + 120.0, W, 22.0), 12.5, 500, dim, Align::Center);
            }
        } else {
            s.text(tr("En iyi eşleşmeler"), Rect::new(30.0, top, 300.0, 20.0), 12.5, 700, white, Align::Left);
            let rh = 40.0;
            let n = shown.len();
            for i in 0..=n {
                let r = Rect::new(24.0, top + 28.0 + i as f32 * (rh + 2.0), W - 48.0, rh);
                let hov = self.hover == Some(Hit::Item(i));
                if self.sel == i {
                    s.fill_round(r, 10.0, accent.alpha(0.22));
                } else if hov {
                    s.fill_round(r, 10.0, white.alpha(0.07));
                }
                if i < n {
                    let ir = Rect::new(r.x + 10.0, r.y + 6.0, 28.0, 28.0);
                    match &icons[i] {
                        Some(b) => s.image(b, ir, 0.0),
                        None => s.fill_round(ir, 7.0, accent.alpha(0.3)),
                    }
                    s.text(&shown[i].name, Rect::new(r.x + 50.0, r.y, r.w - 60.0, rh), 13.0, if self.sel == i { 600 } else { 500 }, white, Align::Left);
                } else {
                    s.glyph('\u{E721}', Rect::new(r.x + 10.0, r.y, 28.0, rh), 13.0, dim);
                    let label = if i18n::en() { format!("Search files for \"{}\"", self.query.trim()) } else { format!("Dosyalarda ara: \"{}\"", self.query.trim()) };
                    s.text(&label, Rect::new(r.x + 50.0, r.y, r.w - 60.0, rh), 12.5, 500, dim, Align::Left);
                }
                hits.push((r, Hit::Item(i)));
            }
        }

        let fy = H - 56.0;
        s.fill_round(Rect::new(0.0, fy, W, 56.0), 0.0, white.alpha(0.035));
        s.fill_round(Rect::new(0.0, fy, W, 1.0), 0.0, white.alpha(0.06));
        let user = std::env::var("USERNAME").unwrap_or_default();
        s.fill_circle(42.0, fy + 28.0, 15.0, accent.alpha(0.3));
        s.text(&user.chars().next().unwrap_or('?').to_uppercase().collect::<String>(), Rect::new(27.0, fy + 13.0, 30.0, 30.0), 13.0, 700, white, Align::Center);
        s.text(&user, Rect::new(64.0, fy, 200.0, 56.0), 12.5, 600, white, Align::Left);
        for (i, (g, hit)) in [('\u{E7E8}', Hit::Power), ('\u{E713}', Hit::Settings)].iter().enumerate() {
            let r = Rect::new(W - 24.0 - 38.0 * (i as f32 + 1.0), fy + 9.0, 36.0, 38.0);
            if self.hover == Some(*hit) {
                s.fill_round(r, 8.0, white.alpha(0.08));
            }
            s.glyph(*g, r, 14.0, white);
            hits.push((r, *hit));
        }
        s.end();
        self.hits = hits;
        unsafe {
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    fn hit(&self, lp: LPARAM) -> Option<Hit> {
        let x = (lp.0 & 0xFFFF) as i16 as f32 / self.surf.scale;
        let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32 / self.surf.scale;
        self.hits.iter().find(|(r, _)| r.contains(x, y)).map(|(_, h)| *h)
    }

    fn count(&self) -> usize {
        self.shown.len() + if self.query.trim().is_empty() { 0 } else { 1 }
    }

    pub fn handle(&mut self, msg: u32, wp: WPARAM, lp: LPARAM, ctx: &Ctx) -> Option<Option<LauncherOut>> {
        match msg {
            WM_PAINT => unsafe {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(self.hwnd, &mut ps);
                self.surf.blit(dc);
                let _ = EndPaint(self.hwnd, &ps);
            },
            WM_ERASEBKGND => return Some(None),
            WM_TIMER if wp.0 == TIMER_OUTSIDE => unsafe {
                let down = |k: VIRTUAL_KEY| GetAsyncKeyState(k.0 as i32) as u16 & 0x8000 != 0;
                if down(VK_LBUTTON) || down(VK_RBUTTON) || down(VK_MBUTTON) {
                    let mut c = POINT::default();
                    let mut r = RECT::default();
                    let _ = GetCursorPos(&mut c);
                    let _ = GetWindowRect(self.hwnd, &mut r);
                    if c.x < r.left || c.x >= r.right || c.y < r.top || c.y >= r.bottom {
                        return Some(Some(LauncherOut::Close));
                    }
                }
            },
            WM_SETCURSOR if (lp.0 & 0xFFFF) as u32 == HTCLIENT => unsafe {
                let mut c = POINT::default();
                let _ = GetCursorPos(&mut c);
                let _ = ScreenToClient(self.hwnd, &mut c);
                let lpc = LPARAM(((c.y as u16 as isize) << 16) | c.x as u16 as isize);
                let id = match self.hit(lpc) {
                    Some(Hit::Search) => IDC_IBEAM,
                    Some(_) => IDC_HAND,
                    None => IDC_ARROW,
                };
                if let Ok(cur) = LoadCursorW(None, id) {
                    SetCursor(cur);
                }
                return Some(None);
            },
            WM_ACTIVATE if (wp.0 & 0xFFFF) as u32 == WA_INACTIVE => return Some(Some(LauncherOut::Close)),
            WM_KEYDOWN => {
                let n = self.count();
                let grid = self.query.trim().is_empty();
                match VIRTUAL_KEY(wp.0 as u16) {
                    VK_ESCAPE => return Some(Some(LauncherOut::Close)),
                    VK_RETURN => {
                        if self.launch(self.sel) {
                            return Some(Some(LauncherOut::Close));
                        }
                    }
                    VK_DOWN if n > 0 => self.sel = (self.sel + if grid { COLS } else { 1 }).min(n - 1),
                    VK_UP => self.sel = self.sel.saturating_sub(if grid { COLS } else { 1 }),
                    VK_RIGHT if grid && n > 0 => self.sel = (self.sel + 1).min(n - 1),
                    VK_LEFT if grid => self.sel = self.sel.saturating_sub(1),
                    _ => return Some(None),
                }
                self.draw(ctx);
            }
            WM_CHAR => {
                let c = char::from_u32(wp.0 as u32).unwrap_or('\0');
                match c {
                    '\u{8}' => {
                        self.query.pop();
                    }
                    '\u{7f}' => self.query.clear(),
                    c if !c.is_control() => self.query.push(c),
                    _ => return Some(None),
                }
                self.filter();
                self.draw(ctx);
            }
            WM_MOUSEMOVE => {
                if !self.tracking {
                    let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: self.hwnd, dwHoverTime: 0 };
                    unsafe {
                        let _ = TrackMouseEvent(&mut tme);
                    }
                    self.tracking = true;
                }
                let h = self.hit(lp);
                if h != self.hover {
                    self.hover = h;
                    self.draw(ctx);
                }
            }
            WM_MOUSELEAVE_ => {
                self.tracking = false;
                if self.hover.take().is_some() {
                    self.draw(ctx);
                }
            }
            WM_LBUTTONUP => match self.hit(lp) {
                Some(Hit::Item(i)) => {
                    if self.launch(i) {
                        return Some(Some(LauncherOut::Close));
                    }
                }
                Some(Hit::Power) => return Some(Some(LauncherOut::Power)),
                Some(Hit::Settings) => return Some(Some(LauncherOut::Settings)),
                _ => {}
            },
            _ => return None,
        }
        Some(None)
    }
}

fn initials(n: &str) -> String {
    n.split(|c: char| !c.is_alphanumeric()).filter_map(|w| w.chars().next()).collect()
}

impl Drop for Launcher {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
