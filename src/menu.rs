use std::cell::RefCell;

use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON};
use windows::Win32::UI::WindowsAndMessaging::*;

use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;

use crate::config::Palette;
use crate::gfx::{Align, Rect, Surface};
use crate::monitors::Monitor;
use crate::skin;

const CLASS: windows::core::PCWSTR = w!("TaskbaroqueMenu");
const ROW: f32 = 32.0;
const SEP_H: f32 = 9.0;
const HEAD_H: f32 = 28.0;
const PAD: f32 = 6.0;
const ICON_W: f32 = 36.0;
const TAIL_W: f32 = 30.0;
pub const RIGHT: u32 = 0x8000_0000;

#[allow(dead_code)]
pub enum Entry {
    Item { id: u32, label: String, glyph: Option<char>, checked: bool, icon: isize },
    Sub { label: String, glyph: Option<char>, items: Vec<Entry> },
    Sep,
    Header(String),
}

impl Entry {
    pub fn item(id: u32, label: impl Into<String>, glyph: char) -> Entry {
        Entry::Item { id, label: label.into(), glyph: Some(glyph), checked: false, icon: 0 }
    }
    pub fn check(id: u32, label: impl Into<String>, checked: bool) -> Entry {
        Entry::Item { id, label: label.into(), glyph: None, checked, icon: 0 }
    }
    #[allow(dead_code)]
    pub fn sub(label: impl Into<String>, glyph: char, items: Vec<Entry>) -> Entry {
        Entry::Sub { label: label.into(), glyph: Some(glyph), items }
    }
    fn height(&self) -> f32 {
        match self {
            Entry::Sep => SEP_H,
            Entry::Header(_) => HEAD_H,
            _ => ROW,
        }
    }
    fn selectable(&self) -> bool {
        matches!(self, Entry::Item { .. } | Entry::Sub { .. })
    }
}

struct Level {
    hwnd: HWND,
    surf: Surface,
    path: Vec<usize>,
    rects: Vec<Rect>,
    bmps: Vec<Option<ID2D1Bitmap>>,
    hover: Option<usize>,
    x: i32,
    y: i32,
}

struct State {
    root: Vec<Entry>,
    levels: Vec<Level>,
    pal: Palette,
    mon: Monitor,
    result: Option<u32>,
    done: bool,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn entries<'a>(root: &'a [Entry], path: &[usize]) -> &'a [Entry] {
    let mut cur = root;
    for &i in path {
        if let Entry::Sub { items, .. } = &cur[i] {
            cur = items;
        }
    }
    cur
}

pub fn popup(pal: &Palette, mon: Monitor, root: Vec<Entry>, at: POINT, above: bool) -> Option<u32> {
    if root.is_empty() || STATE.with(|s| s.borrow().is_some()) {
        return None;
    }
    register();
    let mut pal = pal.clone();
    pal.bg.a = pal.bg.a.max(0.94);
    STATE.with(|s| *s.borrow_mut() = Some(State { root, levels: Vec::new(), pal, mon, result: None, done: false }));
    let anchor = RECT { left: at.x, top: at.y, right: at.x, bottom: at.y };
    with_state(|st| open_level(st, Vec::new(), anchor, above));

    unsafe {
        let mut msg = MSG::default();
        loop {
            if with_state(|st| st.done || st.levels.is_empty()) {
                break;
            }
            if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                PostQuitMessage(msg.wParam.0 as i32);
                break;
            }
            match msg.message {
                WM_KEYDOWN if msg.wParam.0 == VK_ESCAPE.0 as usize => with_state(|st| st.done = true),
                WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_NCLBUTTONDOWN | WM_NCRBUTTONDOWN => {
                    let h = msg.hwnd;
                    if with_state(|st| !st.levels.iter().any(|l| l.hwnd == h)) {
                        with_state(|st| st.done = true);
                        continue;
                    }
                }
                _ => {}
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    let st = STATE.with(|s| s.borrow_mut().take())?;
    for l in st.levels.iter().rev() {
        unsafe {
            let _ = DestroyWindow(l.hwnd);
        }
    }
    st.result
}

fn with_state<R: Default>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| match s.try_borrow_mut() {
        Ok(mut g) => g.as_mut().map(f).unwrap_or_default(),
        Err(_) => R::default(),
    })
}

fn register() {
    unsafe {
        let Ok(inst) = GetModuleHandleW(None) else { return };
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: inst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&wc);
    }
}

fn open_level(st: &mut State, path: Vec<usize>, anchor: RECT, above: bool) {
    let scale = st.mon.scale;
    let font = st.pal.font.clone();
    let list = entries(&st.root, &path);
    let Ok(meas) = Surface::new(1.0, 1.0, scale, &font) else { return };
    let text_w = list
        .iter()
        .map(|e| match e {
            Entry::Item { label, .. } | Entry::Sub { label, .. } => meas.measure(label, 13.0, 400),
            Entry::Header(t) => meas.measure(t, 12.0, 600) - ICON_W + 12.0,
            Entry::Sep => 0.0,
        })
        .fold(0.0, f32::max);
    drop(meas);
    let w = (text_w + ICON_W + TAIL_W + 2.0 * PAD + 8.0).clamp(200.0, 460.0);
    let h = list.iter().map(Entry::height).sum::<f32>() + 2.0 * PAD;
    let Ok(surf) = Surface::new(w, h, scale, &font) else { return };
    let mut rects = Vec::new();
    let mut y = PAD;
    for e in list {
        rects.push(Rect::new(PAD, y, w - 2.0 * PAD, e.height()));
        y += e.height();
    }

    let area = st.mon.rect;
    let (wp, hp) = (surf.w_px, surf.h_px);
    let first = path.is_empty();
    let mut x = if first { anchor.left } else { anchor.right + 2 };
    if x + wp > area.right {
        x = if first { area.right - wp } else { anchor.left - wp - 2 };
    }
    let mut y = if first {
        if above {
            anchor.top - hp
        } else {
            anchor.top
        }
    } else {
        anchor.top - (PAD * scale) as i32
    };
    y = y.clamp(area.top, (area.bottom - hp).max(area.top));
    x = x.max(area.left);

    let ex = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | if first { WINDOW_EX_STYLE(0) } else { WS_EX_NOACTIVATE };
    let owner = st.levels.first().map(|l| l.hwnd).unwrap_or_default();
    let hwnd = unsafe {
        let Ok(inst) = GetModuleHandleW(None) else { return };
        match CreateWindowExW(ex, CLASS, w!("TaskbaroqueMenu"), WS_POPUP, x, y, wp, hp, owner, None, inst, None) {
            Ok(h) => h,
            Err(_) => return,
        }
    };
    let bmps = list
        .iter()
        .map(|e| match e {
            Entry::Item { icon, .. } if *icon != 0 => surf.bitmap_from_icon(HICON(*icon as _)),
            _ => None,
        })
        .collect();
    let level = Level { hwnd, surf, path, rects, bmps, hover: None, x, y };
    draw(st, &level);
    unsafe {
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, wp, hp, SWP_NOACTIVATE);
        if first {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            SetTimer(hwnd, 1, 50, None);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }
    st.levels.push(level);
}

fn draw(st: &State, l: &Level) {
    let s = &l.surf;
    let p = &st.pal;
    let list = entries(&st.root, &l.path);
    let full = Rect::new(0.0, 0.0, s.w_px as f32 / s.scale, s.h_px as f32 / s.scale);
    s.begin();
    skin::panel(s, full.inset(0.5), p.radius.min(10.0), p, false);
    for (i, e) in list.iter().enumerate() {
        let r = l.rects[i];
        let open_sub = st.levels.iter().any(|o| o.path.len() == l.path.len() + 1 && o.path.starts_with(&l.path) && o.path.last() == Some(&i));
        match e {
            Entry::Sep => {
                let y = r.y + r.h / 2.0;
                s.line(r.x + 8.0, y, r.x + r.w - 8.0, y, 1.0, p.text.alpha(0.1));
            }
            Entry::Header(t) => {
                s.text(t, Rect::new(r.x + 10.0, r.y, r.w - 20.0, r.h), 12.0, 600, p.dim, Align::Left);
            }
            Entry::Item { label, glyph, .. } | Entry::Sub { label, glyph, .. } => {
                let checked = matches!(e, Entry::Item { checked: true, .. });
                if l.hover == Some(i) || open_sub {
                    s.fill_round(Rect::new(r.x, r.y + 1.0, r.w, r.h - 2.0), 6.0, p.accent.alpha(0.2));
                }
                if let Some(bmp) = &l.bmps[i] {
                    s.image(bmp, Rect::new(r.x + (ICON_W - 16.0) / 2.0, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0), 0.0);
                } else if let Some(g) = glyph {
                    s.glyph(*g, Rect::new(r.x, r.y, ICON_W, r.h), 14.0, p.text);
                }
                let lr = Rect::new(r.x + ICON_W, r.y, r.w - ICON_W - TAIL_W, r.h);
                s.text(label, lr, 13.0, 400, p.text, Align::Left);
                let tail = Rect::new(r.x + r.w - TAIL_W, r.y, TAIL_W, r.h);
                if matches!(e, Entry::Sub { .. }) {
                    s.glyph('\u{E76C}', tail, 10.0, p.dim);
                } else if checked {
                    s.glyph('\u{E73E}', tail, 12.0, p.accent);
                }
            }
        }
    }
    s.end();
    s.present(l.hwnd, l.x, l.y);
}

fn level_of(st: &State, hwnd: HWND) -> Option<usize> {
    st.levels.iter().position(|l| l.hwnd == hwnd)
}

fn index_at(l: &Level, lp: LPARAM) -> Option<usize> {
    let x = (lp.0 & 0xFFFF) as i16 as f32 / l.surf.scale;
    let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32 / l.surf.scale;
    l.rects.iter().position(|r| r.contains(x, y))
}

fn close_after(st: &mut State, keep: usize) {
    while st.levels.len() > keep + 1 {
        if let Some(l) = st.levels.pop() {
            unsafe {
                let _ = DestroyWindow(l.hwnd);
            }
        }
    }
}

fn on_move(st: &mut State, li: usize, lp: LPARAM) {
    let i = index_at(&st.levels[li], lp);
    let list = entries(&st.root, &st.levels[li].path);
    let Some(i) = i.filter(|&i| list[i].selectable()) else { return };
    let i = Some(i);
    if i == st.levels[li].hover {
        return;
    }
    st.levels[li].hover = i;
    let is_sub = i.is_some_and(|i| matches!(list[i], Entry::Sub { .. }));
    let mut path = st.levels[li].path.clone();
    let already = i.is_some_and(|i| st.levels.get(li + 1).is_some_and(|n| n.path.last() == Some(&i)));
    if !already {
        close_after(st, li);
    }
    if let (true, false, Some(i)) = (is_sub, already, i) {
        path.push(i);
        let l = &st.levels[li];
        let r = l.rects[i];
        let sc = l.surf.scale;
        let anchor = RECT {
            left: l.x,
            top: l.y + (r.y * sc) as i32,
            right: l.x + l.surf.w_px,
            bottom: l.y + ((r.y + r.h) * sc) as i32,
        };
        open_level(st, path, anchor, false);
    }
    let st = &*st;
    for l in &st.levels {
        draw(st, l);
    }
}

fn on_click(st: &mut State, li: usize, lp: LPARAM, right: bool) {
    let Some(i) = index_at(&st.levels[li], lp) else { return };
    if let Entry::Item { id, .. } = entries(&st.root, &st.levels[li].path)[i] {
        st.result = Some(if right { id | RIGHT } else { id });
        st.done = true;
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEMOVE => with_state(|st| {
            if let Some(li) = level_of(st, hwnd) {
                on_move(st, li, lp);
            }
        }),
        WM_LBUTTONUP | WM_RBUTTONUP => with_state(|st| {
            if let Some(li) = level_of(st, hwnd) {
                on_click(st, li, lp, msg == WM_RBUTTONUP);
            }
        }),
        WM_TIMER => {
            let down = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON].iter().any(|k| GetAsyncKeyState(k.0 as i32) as u16 & 0x8000 != 0);
            if down {
                let mut p = POINT::default();
                let _ = GetCursorPos(&mut p);
                let under = WindowFromPoint(p);
                with_state(|st| {
                    if level_of(st, under).is_none() {
                        st.done = true;
                    }
                });
            }
        }
        WM_MOUSEACTIVATE => {
            if with_state(|st| level_of(st, hwnd).is_some_and(|i| i > 0)) {
                return LRESULT(MA_NOACTIVATE as isize);
            }
        }
        WM_ACTIVATE => {
            if (wp.0 & 0xFFFF) as u32 == WA_INACTIVE {
                let other = HWND(lp.0 as _);
                with_state(|st| {
                    if level_of(st, hwnd) == Some(0) && level_of(st, other).is_none() {
                        st.done = true;
                    }
                });
            }
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
