use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

#[derive(Clone, Copy, Debug)]
pub struct Monitor {
    pub hmon: HMONITOR,
    pub rect: RECT,
    pub work: RECT,
    pub scale: f32,
    pub primary: bool,
}

unsafe extern "system" fn enum_proc(hmon: HMONITOR, _dc: HDC, _r: *mut RECT, lp: LPARAM) -> BOOL {
    let v = &mut *(lp.0 as *mut Vec<Monitor>);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if GetMonitorInfoW(hmon, &mut mi).as_bool() {
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        v.push(Monitor {
            hmon,
            rect: mi.rcMonitor,
            work: mi.rcWork,
            scale: dx as f32 / 96.0,
            primary: mi.dwFlags & 1 != 0,
        });
    }
    TRUE
}

pub fn list() -> Vec<Monitor> {
    let mut v: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(enum_proc), LPARAM(&mut v as *mut _ as isize));
    }
    v.sort_by_key(|m| (!m.primary, m.rect.left, m.rect.top));
    v
}

pub fn index_of(mons: &[Monitor], hwnd: HWND) -> usize {
    let h = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    mons.iter().position(|m| m.hmon == h).unwrap_or(0)
}
