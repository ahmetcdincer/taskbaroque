use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const WM_APP_HOTKEY: u32 = WM_APP + 12;
pub const WM_APP_PICK: u32 = WM_APP + 13;
const WM_HOOK_MOUSE: u32 = WM_APP + 1;

static HOST: AtomicIsize = AtomicIsize::new(0);
static THREAD: AtomicU32 = AtomicU32::new(0);
static ENABLED: AtomicBool = AtomicBool::new(false);
static LAUNCHER: AtomicBool = AtomicBool::new(false);
static PICKING: AtomicBool = AtomicBool::new(false);

pub fn start(host: HWND) {
    HOST.store(host.0 as isize, Ordering::SeqCst);
    std::thread::spawn(|| unsafe {
        THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
        let inst = GetModuleHandleW(None).unwrap_or_default();
        let _kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), inst, 0);
        let mut mouse: Option<HHOOK> = None;
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_HOOK_MOUSE {
                match (msg.wParam.0 != 0, mouse) {
                    (true, None) => mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), inst, 0).ok(),
                    (false, Some(h)) => {
                        let _ = UnhookWindowsHookEx(h);
                        mouse = None;
                    }
                    _ => {}
                }
            }
        }
    });
}

pub fn configure(enabled: bool, launcher: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
    LAUNCHER.store(launcher, Ordering::SeqCst);
}

pub fn set_picking(on: bool) {
    PICKING.store(on, Ordering::SeqCst);
    unsafe {
        let _ = PostThreadMessageW(THREAD.load(Ordering::SeqCst), WM_HOOK_MOUSE, WPARAM(on as usize), LPARAM(0));
    }
}

fn post(msg: u32, wp: usize) {
    unsafe {
        let _ = PostMessageW(HWND(HOST.load(Ordering::SeqCst) as _), msg, WPARAM(wp), LPARAM(0));
    }
}

fn held(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000 != 0 }
}

fn mask_start_menu() {
    let key = |up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0xE8), dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() },
        },
    };
    unsafe {
        SendInput(&[key(false), key(true)], std::mem::size_of::<INPUT>() as i32);
    }
}

unsafe extern "system" fn keyboard(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        if k.flags.0 & LLKHF_INJECTED.0 == 0 {
            let vk = VIRTUAL_KEY(k.vkCode as u16);
            if PICKING.load(Ordering::SeqCst) && vk == VK_ESCAPE {
                if down {
                    post(WM_APP_PICK, 0);
                }
                return LRESULT(1);
            }
            if ENABLED.load(Ordering::SeqCst) {
                let win = held(VK_LWIN) || held(VK_RWIN);
                let (ctrl, alt, shift) = (held(VK_CONTROL), held(VK_MENU), held(VK_SHIFT));
                if win && !ctrl && !alt && !shift && (0x31..=0x39).contains(&k.vkCode) {
                    if down {
                        post(WM_APP_HOTKEY, (k.vkCode - 0x30) as usize);
                        mask_start_menu();
                    }
                    return LRESULT(1);
                }
                if LAUNCHER.load(Ordering::SeqCst) && vk == VK_SPACE && alt && !win && !ctrl && !shift {
                    if down {
                        post(WM_APP_HOTKEY, 0);
                    }
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

unsafe extern "system" fn mouse_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && PICKING.load(Ordering::SeqCst) {
        match wp.0 as u32 {
            WM_LBUTTONDOWN => {
                post(WM_APP_PICK, 1);
                return LRESULT(1);
            }
            WM_RBUTTONDOWN => {
                post(WM_APP_PICK, 0);
                return LRESULT(1);
            }
            WM_LBUTTONUP | WM_RBUTTONUP => return LRESULT(1),
            _ => {}
        }
    }
    CallNextHookEx(None, code, wp, lp)
}
