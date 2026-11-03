//! ITaskbarList3 mesajlarını yakalar: başka uygulamaların görev çubuğu düğmesine
//! koyduğu rozet (overlay) ikonları — ör. Discord'un kırmızı okunmamış rozeti.
//!
//! Windows'un ITaskbarList uygulaması, `Shell_TrayWnd` penceresinin "TaskbandHWND"
//! özelliğindeki pencereye WM_USER+N mesajları gönderir (ManagedShell/RetroBar ile aynı yöntem).
//! Bu özelliği kendi gizli penceremize yönlendirir, çıkışta eski değerine geri alırız.

use std::cell::RefCell;
use std::collections::HashMap;

use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::util;

const PROP: windows::core::PCWSTR = w!("TaskbandHWND");
const SET_OVERLAY: u32 = WM_USER + 79;

struct State {
    hwnd: HWND,
    notify: isize,
    /// (tray penceresi, önceki TaskbandHWND değeri)
    saved: Vec<(isize, isize)>,
    /// pencere -> kendi kopyamız olan rozet ikonu
    overlays: HashMap<isize, isize>,
}

thread_local! {
    static ST: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn trays() -> Vec<HWND> {
    let mut v = Vec::new();
    unsafe {
        let mut after = HWND::default();
        while let Ok(h) = FindWindowExW(None, after, w!("Shell_TrayWnd"), None) {
            if h.is_invalid() {
                break;
            }
            v.push(h);
            after = h;
        }
    }
    v
}

pub fn start(notify: isize) {
    if ST.with(|s| s.borrow().is_some()) {
        apply();
        return;
    }
    unsafe {
        let Ok(inst) = GetModuleHandleW(None) else { return };
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: inst.into(),
            lpszClassName: w!("TaskbaroqueTaskband"),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let Ok(hwnd) = CreateWindowExW(WS_EX_TOOLWINDOW, w!("TaskbaroqueTaskband"), w!(""), WS_POPUP, 0, 0, 0, 0, None, None, inst, None) else {
            return;
        };
        ST.with(|s| *s.borrow_mut() = Some(State { hwnd, notify, saved: Vec::new(), overlays: HashMap::new() }));
    }
    apply();
}

/// Tüm `Shell_TrayWnd` pencerelerini (Explorer'ınki ve bizimki) kendi penceremize yönlendirir.
/// Ucuz; Explorer yeniden başladığında veya tray penceremiz sonradan oluştuğunda tekrar çağrılır.
pub fn apply() {
    ST.with(|s| {
        let mut s = s.borrow_mut();
        let Some(st) = s.as_mut() else { return };
        unsafe {
            st.saved.retain(|(t, _)| IsWindow(util::hwnd(*t)).as_bool());
            for t in trays() {
                let cur = GetPropW(t, PROP);
                if cur.0 as isize == st.hwnd.0 as isize {
                    continue;
                }
                if !st.saved.iter().any(|(x, _)| *x == t.0 as isize) {
                    st.saved.push((t.0 as isize, cur.0 as isize));
                }
                let _ = SetPropW(t, PROP, HANDLE(st.hwnd.0));
            }
        }
    });
}

fn explorer_tasklist(tray: HWND) -> isize {
    unsafe {
        FindWindowExW(tray, None, w!("ReBarWindow32"), None)
            .and_then(|rebar| FindWindowExW(rebar, None, w!("MSTaskSwWClass"), None))
            .map_or(0, |h| h.0 as isize)
    }
}

pub fn stop() {
    let Some(st) = ST.with(|s| s.borrow_mut().take()) else { return };
    unsafe {
        for (t, prev) in &st.saved {
            let t = util::hwnd(*t);
            if !IsWindow(t).as_bool() || GetPropW(t, PROP).0 as isize != st.hwnd.0 as isize {
                continue;
            }
            // Önceki değer ölü bir pencereyse (ör. önceki çalışmamız çöktüyse) Explorer'ın kendi
            // görev listesine geri bağla
            let prev = if *prev != 0 && IsWindow(util::hwnd(*prev)).as_bool() { *prev } else { explorer_tasklist(t) };
            if prev != 0 {
                let _ = SetPropW(t, PROP, HANDLE(prev as _));
            } else {
                let _ = RemovePropW(t, PROP);
            }
        }
        for icon in st.overlays.values() {
            let _ = DestroyIcon(HICON(*icon as _));
        }
        let _ = DestroyWindow(st.hwnd);
    }
}

/// Pencerenin rozet ikonu (bize ait kopya; çağıran yok etmemeli).
pub fn overlay(h: HWND) -> Option<HICON> {
    ST.with(|s| s.borrow().as_ref()?.overlays.get(&(h.0 as isize)).map(|i| HICON(*i as _)))
}

/// Kapanmış pencerelerin rozetlerini bırakır.
pub fn prune() {
    ST.with(|s| {
        let mut s = s.borrow_mut();
        let Some(st) = s.as_mut() else { return };
        st.overlays.retain(|h, icon| unsafe {
            let alive = IsWindow(util::hwnd(*h)).as_bool();
            if !alive {
                let _ = DestroyIcon(HICON(*icon as _));
            }
            alive
        });
    });
}

fn set_overlay(h: isize, icon: HICON) -> Option<isize> {
    ST.with(|s| {
        let mut s = s.borrow_mut();
        let st = s.as_mut()?;
        unsafe {
            if let Some(old) = st.overlays.remove(&h) {
                let _ = DestroyIcon(HICON(old as _));
            }
            if !icon.is_invalid() {
                if let Ok(copy) = CopyIcon(icon) {
                    st.overlays.insert(h, copy.0 as isize);
                }
            }
        }
        Some(st.notify)
    })
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == SET_OVERLAY {
        if let Some(notify) = set_overlay(wp.0 as isize, HICON(lp.0 as _)) {
            let _ = PostMessageW(util::hwnd(notify), util::WM_APP_REFRESH, WPARAM(0), LPARAM(0));
        }
        return LRESULT(0);
    }
    // Diğer ITaskbarList mesajları (ilerleme, küçük resim düğmeleri…) şimdilik yok sayılır
    if (WM_USER..WM_USER + 100).contains(&msg) {
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
