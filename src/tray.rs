use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Instant;

use windows::core::{w, GUID, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::{taskbar, util};

const SIGNATURE: u32 = 0x3475_3423;
const NIF_MESSAGE: u32 = 0x1;
const NIF_ICON: u32 = 0x2;
const NIF_TIP: u32 = 0x4;
const NIF_STATE: u32 = 0x8;
const NIF_GUID: u32 = 0x20;
const NIS_HIDDEN: u32 = 0x1;
const NIN_SELECT: u32 = WM_USER;

#[repr(C)]
#[derive(Clone, Copy)]
struct NotifyIconData32 {
    cb_size: u32,
    hwnd: u32,
    uid: u32,
    flags: u32,
    callback: u32,
    icon: u32,
    tip: [u16; 128],
    state: u32,
    state_mask: u32,
    info: [u16; 256],
    version: u32,
    info_title: [u16; 64],
    info_flags: u32,
    guid: GUID,
    balloon_icon: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TrayData {
    signature: u32,
    message: u32,
    nid: NotifyIconData32,
}

#[derive(Clone)]
pub struct TrayIcon {
    pub id: u64,
    pub gen: u64,
    hwnd: isize,
    uid: u32,
    callback: u32,
    version: u32,
    pub icon: isize,
    pub tip: String,
    hidden: bool,
    pub pid: u32,
    pub exe: String,
    pub promoted: bool,
}

struct State {
    hwnd: HWND,
    icons: Vec<TrayIcon>,
    gen: u64,
    notify: isize,
    promoted: HashMap<String, bool>,
    broadcast: Option<Instant>,
    ticks: u32,
}

thread_local! {
    static TRAY: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with<R: Default>(f: impl FnOnce(&mut State) -> R) -> R {
    TRAY.with(|t| match t.try_borrow_mut() {
        Ok(mut g) => g.as_mut().map(f).unwrap_or_default(),
        Err(_) => R::default(),
    })
}

pub fn set_enabled(on: bool, notify: isize) -> bool {
    let running = TRAY.with(|t| t.borrow().is_some());
    match (on, running) {
        (true, false) => {
            start(notify);
            true
        }
        (false, true) => {
            stop();
            false
        }
        _ => false,
    }
}

fn start(notify: isize) {
    unsafe {
        let Ok(inst) = GetModuleHandleW(None) else { return };
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: inst.into(),
            lpszClassName: w!("Shell_TrayWnd"),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("Shell_TrayWnd"),
            w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            inst,
            None,
        ) else {
            return;
        };
        TRAY.with(|t| {
            *t.borrow_mut() = Some(State {
                hwnd,
                icons: Vec::new(),
                gen: 1,
                notify,
                promoted: read_promoted(),
                broadcast: None,
                ticks: 0,
            })
        });
        raise();
        broadcast();
    }
}

pub fn stop() {
    if let Some(st) = TRAY.with(|t| t.borrow_mut().take()) {
        unsafe {
            for i in &st.icons {
                if i.icon != 0 {
                    let _ = DestroyIcon(HICON(i.icon as _));
                }
            }
            let _ = DestroyWindow(st.hwnd);
        }
    }
}

fn raise() {
    let Some(me) = with(|st| Some(st.hwnd)) else { return };
    unsafe {
        let mut r = RECT::default();
        let have = taskbar::explorer_tray().is_some_and(|ex| GetWindowRect(ex, &mut r).is_ok());
        let flags = if have { SWP_NOACTIVATE } else { SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE };
        let _ = SetWindowPos(me, HWND_TOPMOST, r.left, r.top, r.right - r.left, r.bottom - r.top, flags);
    }
}

pub fn broadcast() {
    with(|st| st.broadcast = Some(Instant::now()));
    unsafe {
        let msg = RegisterWindowMessageW(w!("TaskbarCreated"));
        let _ = SendNotifyMessageW(HWND_BROADCAST, msg, WPARAM(0), LPARAM(0));
    }
}

pub fn own_broadcast() -> bool {
    with(|st| st.broadcast.is_some_and(|t| t.elapsed().as_secs_f32() < 3.0))
}

pub fn tick() {
    raise();
    let changed = with(|st| {
        st.ticks += 1;
        let before = st.icons.len();
        st.icons.retain(|i| unsafe {
            let alive = IsWindow(util::hwnd(i.hwnd)).as_bool();
            if !alive && i.icon != 0 {
                let _ = DestroyIcon(HICON(i.icon as _));
            }
            alive
        });
        let mut changed = before != st.icons.len();
        if st.ticks % 15 == 0 {
            let p = read_promoted();
            if p != st.promoted {
                st.promoted = p;
                changed = true;
            }
        }
        if changed {
            st.gen += 1;
        }
        changed.then_some(st.notify)
    });
    if let Some(n) = changed {
        util::post_data_changed(n);
    }
}

pub fn generation() -> u64 {
    with(|st| st.gen)
}

pub fn icons() -> Vec<TrayIcon> {
    with(|st| {
        st.icons
            .iter()
            .filter(|i| !i.hidden && i.icon != 0)
            .map(|i| TrayIcon { promoted: st.promoted.get(&i.exe).copied().unwrap_or(false), ..i.clone() })
            .collect()
    })
}

thread_local! {
    static LAST_CLICK: std::cell::RefCell<Option<(u64, std::time::Instant, POINT)>> = const { std::cell::RefCell::new(None) };
}

pub fn click(id: u64, right: bool, pt: POINT) {
    let Some(i) = with(|st| st.icons.iter().find(|i| i.id == id).cloned()) else { return };
    let double = !right
        && LAST_CLICK.with(|l| {
            let mut l = l.borrow_mut();
            let now = std::time::Instant::now();
            let limit = std::time::Duration::from_millis(unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() } as u64);
            let d = l.is_some_and(|(lid, t, lp): (u64, std::time::Instant, POINT)| lid == id && now - t <= limit && (lp.x - pt.x).abs() < 8 && (lp.y - pt.y).abs() < 8);
            *l = if d { None } else { Some((id, now, pt)) };
            d
        });
    unsafe {
        let _ = AllowSetForegroundWindow(i.pid);
        let post = |m: u32| {
            let (wp, lp) = if i.version >= 4 {
                ((pt.x as u16 as usize) | ((pt.y as u16 as usize) << 16), (m as usize & 0xFFFF) | ((i.uid as usize & 0xFFFF) << 16))
            } else {
                (i.uid as usize, m as usize)
            };
            let _ = PostMessageW(util::hwnd(i.hwnd), i.callback, WPARAM(wp), LPARAM(lp as isize));
        };
        if right {
            post(WM_RBUTTONDOWN);
            post(WM_RBUTTONUP);
            if i.version >= 4 {
                post(WM_CONTEXTMENU);
            }
        } else if double {
            post(WM_LBUTTONDBLCLK);
            post(WM_LBUTTONUP);
        } else {
            post(WM_LBUTTONDOWN);
            post(WM_LBUTTONUP);
            if i.version >= 4 {
                post(NIN_SELECT);
            }
        }
    }
}

fn icon_id(hwnd: u32, uid: u32, guid: &GUID) -> u64 {
    let mut h = DefaultHasher::new();
    if *guid != GUID::zeroed() {
        guid.to_u128().hash(&mut h);
    } else {
        (hwnd, uid).hash(&mut h);
    }
    h.finish()
}

fn wstr(s: &[u16]) -> String {
    let n = s.iter().position(|c| *c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..n])
}

fn apply(st: &mut State, d: &TrayData) -> bool {
    let n = &d.nid;
    let guid = if n.flags & NIF_GUID != 0 { n.guid } else { GUID::zeroed() };
    let id = icon_id(n.hwnd, n.uid, &guid);
    let pos = st.icons.iter().position(|i| i.id == id);
    match d.message {
        0 | 1 => {
            let idx = match pos {
                Some(p) => p,
                None => {
                    let hwnd = n.hwnd as usize as isize;
                    let mut pid = 0u32;
                    unsafe {
                        GetWindowThreadProcessId(util::hwnd(hwnd), Some(&mut pid));
                    }
                    let exe = util::process_path(util::hwnd(hwnd))
                        .and_then(|p| p.rsplit('\\').next().map(str::to_lowercase))
                        .unwrap_or_default();
                    st.icons.push(TrayIcon {
                        id,
                        gen: 0,
                        hwnd,
                        uid: n.uid,
                        callback: 0,
                        version: 0,
                        icon: 0,
                        tip: String::new(),
                        hidden: false,
                        pid,
                        exe,
                        promoted: false,
                    });
                    st.icons.len() - 1
                }
            };
            let i = &mut st.icons[idx];
            if n.flags & NIF_MESSAGE != 0 {
                i.callback = n.callback;
            }
            if n.flags & NIF_ICON != 0 {
                unsafe {
                    if i.icon != 0 {
                        let _ = DestroyIcon(HICON(i.icon as _));
                    }
                    i.icon = match n.icon {
                        0 => 0,
                        h => CopyIcon(HICON(h as usize as _)).map(|c| c.0 as isize).unwrap_or(0),
                    };
                }
            }
            if n.flags & NIF_TIP != 0 {
                i.tip = wstr(&n.tip);
            }
            if n.flags & NIF_STATE != 0 && n.state_mask & NIS_HIDDEN != 0 {
                i.hidden = n.state & NIS_HIDDEN != 0;
            }
            st.gen += 1;
            i.gen = st.gen;
            true
        }
        2 => match pos {
            Some(p) => {
                let i = st.icons.remove(p);
                if i.icon != 0 {
                    unsafe {
                        let _ = DestroyIcon(HICON(i.icon as _));
                    }
                }
                st.gen += 1;
                true
            }
            None => false,
        },
        4 => {
            if let Some(p) = pos {
                st.icons[p].version = n.version;
            }
            false
        }
        _ => false,
    }
}

fn read_promoted() -> HashMap<String, bool> {
    let mut map = HashMap::new();
    let base = w!("Control Panel\\NotifyIconSettings");
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, base, 0, KEY_READ, &mut key).is_err() {
            return map;
        }
        let mut i = 0u32;
        loop {
            let mut name = [0u16; 64];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(key, i, PWSTR(name.as_mut_ptr()), &mut len, None, PWSTR::null(), None, None).is_err() {
                break;
            }
            i += 1;
            let sub = windows::core::HSTRING::from(String::from_utf16_lossy(&name[..len as usize]));
            let mut path = [0u16; 520];
            let mut size = (path.len() * 2) as u32;
            if RegGetValueW(key, &sub, w!("ExecutablePath"), RRF_RT_REG_SZ, None, Some(path.as_mut_ptr() as _), Some(&mut size))
                .is_err()
            {
                continue;
            }
            let mut promoted = 0u32;
            let mut size = 4u32;
            let _ = RegGetValueW(key, &sub, w!("IsPromoted"), RRF_RT_REG_DWORD, None, Some(&mut promoted as *mut _ as _), Some(&mut size));
            let exe = wstr(&path).rsplit('\\').next().unwrap_or("").to_lowercase();
            if !exe.is_empty() {
                let e = map.entry(exe).or_insert(false);
                *e |= promoted != 0;
            }
        }
        let _ = RegCloseKey(key);
    }
    map
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg != WM_COPYDATA || lp.0 == 0 {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let cds = &*(lp.0 as *const COPYDATASTRUCT);
    if cds.dwData == 1 && !cds.lpData.is_null() && cds.cbData as usize >= 8 {
        let mut d: TrayData = std::mem::zeroed();
        let n = (cds.cbData as usize).min(std::mem::size_of::<TrayData>());
        std::ptr::copy_nonoverlapping(cds.lpData as *const u8, &mut d as *mut TrayData as *mut u8, n);
        if d.signature == SIGNATURE {
            if let Some(notify) = with(|st| apply(st, &d).then_some(st.notify)) {
                util::post_data_changed(notify);
            }
        }
    }
    let Some(ex) = taskbar::explorer_tray() else { return LRESULT(1) };
    let mut res = 0usize;
    let ok = SendMessageTimeoutW(ex, WM_COPYDATA, wp, lp, SMTO_ABORTIFHUNG, 2000, Some(&mut res));
    if ok.0 == 0 && cds.dwData == 1 {
        return LRESULT(1);
    }
    LRESULT(res as isize)
}
