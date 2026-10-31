use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::*;

static ACTIVE: AtomicBool = AtomicBool::new(false);

fn find_all(class: PCWSTR) -> Vec<HWND> {
    let mut v = Vec::new();
    unsafe {
        let me = GetCurrentProcessId();
        let mut after = HWND::default();
        while let Ok(h) = FindWindowExW(None, after, class, None) {
            if h.is_invalid() {
                break;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if pid != me {
                v.push(h);
            }
            after = h;
        }
    }
    v
}

pub fn explorer_tray() -> Option<HWND> {
    find_all(w!("Shell_TrayWnd")).into_iter().next()
}

fn tray_windows() -> Vec<HWND> {
    let mut v = find_all(w!("Shell_TrayWnd"));
    v.extend(find_all(w!("Shell_SecondaryTrayWnd")));
    v
}

pub fn hide() {
    ACTIVE.store(true, Ordering::SeqCst);
    enforce();
}

pub fn enforce() {
    if !ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    unsafe {
        for h in tray_windows() {
            if IsWindowVisible(h).as_bool() {
                let _ = ShowWindow(h, SW_HIDE);
            }
        }
    }
}

pub fn is_tray(h: HWND) -> bool {
    tray_windows().contains(&h)
}

pub fn restore() {
    ACTIVE.store(false, Ordering::SeqCst);
    show_all();
}

pub fn show_all() {
    unsafe {
        for h in tray_windows() {
            let _ = ShowWindow(h, SW_SHOWNA);
        }
    }
}
