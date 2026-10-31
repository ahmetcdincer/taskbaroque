use windows::core::{w, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::Registry::*;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const WM_APP_REFRESH: u32 = WM_APP + 1;
pub const WM_APP_DATA: u32 = WM_APP + 2;
pub const WM_APP_APPBAR: u32 = WM_APP + 3;

pub fn app_icon(size: i32) -> HICON {
    unsafe {
        let inst = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default();
        LoadImageW(inst, PCWSTR(1 as _), IMAGE_ICON, size, size, LR_DEFAULTCOLOR)
            .map(|h| HICON(h.0))
            .unwrap_or_else(|_| LoadIconW(None, IDI_APPLICATION).unwrap_or_default())
    }
}

pub fn hwnd(v: isize) -> HWND {
    HWND(v as *mut _)
}

pub fn post_data_changed(host: isize) {
    unsafe {
        let _ = PostMessageW(hwnd(host), WM_APP_DATA, WPARAM(0), LPARAM(0));
    }
}

pub fn message_box(text: &str) {
    unsafe {
        MessageBoxW(None, &HSTRING::from(text), w!("Taskbaroque"), MB_OK | MB_ICONWARNING);
    }
}

pub struct Time {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub dow: u16,
    pub hour: u16,
    pub min: u16,
    pub sec: u16,
}

pub fn now() -> Time {
    let t = unsafe { GetLocalTime() };
    Time { year: t.wYear, month: t.wMonth, day: t.wDay, dow: t.wDayOfWeek, hour: t.wHour, min: t.wMinute, sec: t.wSecond }
}


pub fn fullscreen_busy() -> bool {
    use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN};
    unsafe { SHQueryUserNotificationState() }.is_ok_and(|s| s == QUNS_BUSY || s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE)
}

pub fn days_in_month(year: u16, month: u16) -> u16 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() },
        },
    }
}

pub fn send_win(extra: Option<VIRTUAL_KEY>) {
    let mut inputs = vec![key(VK_LWIN, false)];
    if let Some(k) = extra {
        inputs.push(key(k, false));
        inputs.push(key(k, true));
    }
    inputs.push(key(VK_LWIN, true));
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn send_combo(mods: &[VIRTUAL_KEY], k: VIRTUAL_KEY) {
    let mut inputs: Vec<INPUT> = mods.iter().map(|m| key(*m, false)).collect();
    inputs.push(key(k, false));
    inputs.push(key(k, true));
    inputs.extend(mods.iter().rev().map(|m| key(*m, true)));
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn open_path(p: &std::path::Path) {
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(p.as_os_str()), None, None, SW_SHOWNORMAL);
    }
}

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

pub fn migrate_autostart() {
    unsafe {
        if RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, w!("WinTheme"), RRF_RT_REG_SZ, None, None, None).is_ok() {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, w!("WinTheme"));
            set_autostart(true);
        }
    }
}

pub fn autostart_enabled() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, w!("Taskbaroque"), RRF_RT_REG_SZ, None, None, None).is_ok() }
}

pub fn autostart_target() -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    unsafe {
        RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, w!("Taskbaroque"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as _), Some(&mut len)).ok().ok()?;
    }
    let s = String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]);
    let s = s.trim();
    Some(s.strip_prefix('"').and_then(|r| r.split_once('"')).map(|(p, _)| p).unwrap_or(s).to_string())
}

pub fn set_autostart(on: bool) {
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            let val: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain(Some(0)).collect();
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                w!("Taskbaroque"),
                REG_SZ.0,
                Some(val.as_ptr() as _),
                (val.len() * 2) as u32,
            );
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, w!("Taskbaroque"));
        }
    }
}

pub fn process_path(h: HWND) -> Option<String> {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(h, Some(&mut pid));
    }
    pid_path(pid)
}

pub fn end_task(wins: &[HWND]) {
    use std::os::windows::process::CommandExt;
    let me = unsafe { GetCurrentProcessId() };
    let mut pids: Vec<u32> = wins
        .iter()
        .map(|h| {
            let mut pid = 0u32;
            unsafe {
                GetWindowThreadProcessId(*h, Some(&mut pid));
            }
            pid
        })
        .filter(|p| *p != 0 && *p != me)
        .collect();
    pids.sort_unstable();
    pids.dedup();
    for pid in pids {
        let killed = unsafe {
            match OpenProcess(PROCESS_TERMINATE, false, pid) {
                Ok(p) => {
                    let ok = TerminateProcess(p, 1).is_ok();
                    let _ = CloseHandle(p);
                    ok
                }
                Err(_) => false,
            }
        };
        if !killed {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .creation_flags(0x0800_0000)
                .spawn();
        }
    }
}

pub fn pid_path(pid: u32) -> Option<String> {
    unsafe {
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(p);
        (ok && n > 0).then(|| String::from_utf16_lossy(&buf[..n as usize]))
    }
}

pub fn file_description(path: &str) -> Option<String> {
    unsafe {
        let wpath = HSTRING::from(path);
        let size = GetFileVersionInfoSizeW(&wpath, None);
        if size == 0 {
            return None;
        }
        let mut block = vec![0u8; size as usize];
        GetFileVersionInfoW(&wpath, 0, size, block.as_mut_ptr() as _).ok()?;
        let query = |sub: &str| -> Option<(*const u8, u32)> {
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            VerQueryValueW(block.as_ptr() as _, &HSTRING::from(sub), &mut ptr, &mut len)
                .as_bool()
                .then_some((ptr as *const u8, len))
        };
        let (tr, len) = query("\\VarFileInfo\\Translation")?;
        if len < 4 {
            return None;
        }
        let (lang, cp) = (*(tr as *const u16), *(tr as *const u16).add(1));
        let (s, n) = query(&format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\FileDescription"))?;
        let text = std::slice::from_raw_parts(s as *const u16, n as usize);
        let d = String::from_utf16_lossy(text).trim_end_matches('\0').trim().to_string();
        (!d.is_empty()).then_some(d)
    }
}
