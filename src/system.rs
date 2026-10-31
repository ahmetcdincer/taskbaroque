use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use windows::core::{w, HSTRING, PCWSTR};
use windows::Devices::Radios::{Radio, RadioKind, RadioState};
use windows::Win32::Foundation::*;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, AudioSessionStateExpired, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, IDataObject, CLSCTX_ALL};
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE};
use windows::Win32::System::Registry::*;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CF_UNICODETEXT: u32 = 13;

fn endpoint(capture: bool) -> Option<IAudioEndpointVolume> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let dev = en.GetDefaultAudioEndpoint(if capture { eCapture } else { eRender }, eConsole).ok()?;
        dev.Activate(CLSCTX_ALL, None).ok()
    }
}

#[derive(Default)]
pub struct Audio {
    out: Option<IAudioEndpointVolume>,
    mic: Option<IAudioEndpointVolume>,
}

impl Audio {
    pub fn refresh(&mut self) {
        self.out = endpoint(false);
        self.mic = endpoint(true);
    }

    pub fn volume(&self) -> Option<(f32, bool)> {
        let v = self.out.as_ref()?;
        unsafe { Some((v.GetMasterVolumeLevelScalar().ok()?, v.GetMute().ok()?.as_bool())) }
    }

    pub fn set_volume(&self, level: f32) {
        if let Some(v) = &self.out {
            unsafe {
                let _ = v.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null());
                if level > 0.0 {
                    let _ = v.SetMute(false, std::ptr::null());
                }
            }
        }
    }

    pub fn toggle_mute(&self) {
        if let (Some(v), Some((_, m))) = (&self.out, self.volume()) {
            unsafe {
                let _ = v.SetMute(!m, std::ptr::null());
            }
        }
    }

    pub fn mic_muted(&self) -> Option<bool> {
        let v = self.mic.as_ref()?;
        unsafe { v.GetMute().ok().map(|b| b.as_bool()) }
    }

    pub fn toggle_mic(&self) {
        if let (Some(v), Some(m)) = (&self.mic, self.mic_muted()) {
            unsafe {
                let _ = v.SetMute(!m, std::ptr::null());
            }
        }
    }
}

pub fn privacy_users() -> (Vec<String>, Vec<String>) {
    (capability_users("microphone"), capability_users("webcam"))
}

fn capability_users(cap: &str) -> Vec<String> {
    let mut out = Vec::new();
    let base = format!("Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\{cap}");
    scan_consent(&base, false, &mut out);
    scan_consent(&format!("{base}\\NonPackaged"), true, &mut out);
    out.sort();
    out.dedup();
    out
}

fn scan_consent(path: &str, nonpackaged: bool, out: &mut Vec<String>) {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(path), 0, KEY_READ, &mut key).is_err() {
            return;
        }
        let mut i = 0u32;
        loop {
            let mut name = [0u16; 512];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(key, i, windows::core::PWSTR(name.as_mut_ptr()), &mut len, None, windows::core::PWSTR::null(), None, None).is_err() {
                break;
            }
            i += 1;
            let sub = String::from_utf16_lossy(&name[..len as usize]);
            if sub == "NonPackaged" {
                continue;
            }
            let (mut start, mut stop) = (0u64, 0u64);
            let mut sz = 8u32;
            let hsub = HSTRING::from(sub.as_str());
            let ok = RegGetValueW(key, &hsub, w!("LastUsedTimeStart"), RRF_RT_REG_QWORD, None, Some(&mut start as *mut _ as _), Some(&mut sz)).is_ok();
            sz = 8;
            let _ = RegGetValueW(key, &hsub, w!("LastUsedTimeStop"), RRF_RT_REG_QWORD, None, Some(&mut stop as *mut _ as _), Some(&mut sz));
            if ok && start > 0 && stop == 0 {
                let name = if nonpackaged {
                    sub.rsplit('#').next().unwrap_or(&sub).trim_end_matches(".exe").to_string()
                } else {
                    sub.split('_').next().unwrap_or(&sub).rsplit('.').next().unwrap_or(&sub).to_string()
                };
                out.push(name);
            }
        }
        let _ = RegCloseKey(key);
    }
}

const PERSONALIZE: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");

pub fn dark_mode() -> bool {
    let mut v = 1u32;
    let mut sz = 4u32;
    unsafe {
        let _ = RegGetValueW(HKEY_CURRENT_USER, PERSONALIZE, w!("AppsUseLightTheme"), RRF_RT_REG_DWORD, None, Some(&mut v as *mut _ as _), Some(&mut sz));
    }
    v == 0
}

pub fn set_dark_mode(dark: bool) {
    let v: u32 = if dark { 0 } else { 1 };
    unsafe {
        for name in [w!("AppsUseLightTheme"), w!("SystemUsesLightTheme")] {
            let _ = RegSetKeyValueW(HKEY_CURRENT_USER, PERSONALIZE, name, REG_DWORD.0, Some(&v as *const _ as _), 4);
        }
        let param: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
        let _ = SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(param.as_ptr() as isize), SMTO_ABORTIFHUNG, 200, None);
    }
}

#[derive(Clone, Copy, Default)]
pub struct Radios {
    pub bluetooth: Option<bool>,
    pub wifi: Option<bool>,
}

fn radios_state() -> &'static Arc<Mutex<Radios>> {
    static S: OnceLock<Arc<Mutex<Radios>>> = OnceLock::new();
    S.get_or_init(|| {
        let s = Arc::new(Mutex::new(Radios::default()));
        let bg = s.clone();
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
            }
            let _ = Radio::RequestAccessAsync().and_then(|op| op.get());
            loop {
                let mut r = Radios::default();
                if let Ok(list) = Radio::GetRadiosAsync().and_then(|op| op.get()) {
                    for radio in list {
                        let on = radio.State().map(|s| s == RadioState::On).ok();
                        match radio.Kind() {
                            Ok(k) if k == RadioKind::Bluetooth => r.bluetooth = on,
                            Ok(k) if k == RadioKind::WiFi => r.wifi = on,
                            _ => {}
                        }
                    }
                }
                *bg.lock().unwrap() = r;
                std::thread::sleep(Duration::from_secs(4));
            }
        });
        s
    })
}

pub fn radios() -> Radios {
    *radios_state().lock().unwrap()
}

pub fn toggle_radio(bluetooth: bool) {
    let state = radios_state().clone();
    std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let want = if bluetooth { RadioKind::Bluetooth } else { RadioKind::WiFi };
        let Ok(list) = Radio::GetRadiosAsync().and_then(|op| op.get()) else { return };
        for radio in list {
            if radio.Kind().ok() == Some(want) {
                let on = radio.State().map(|s| s == RadioState::On).unwrap_or(false);
                let next = if on { RadioState::Off } else { RadioState::On };
                if radio.SetStateAsync(next).and_then(|op| op.get()).is_ok() {
                    let mut s = state.lock().unwrap();
                    if bluetooth {
                        s.bluetooth = Some(!on);
                    } else {
                        s.wifi = Some(!on);
                    }
                }
            }
        }
    });
}

pub fn clipboard_text(owner: HWND) -> Option<String> {
    unsafe {
        IsClipboardFormatAvailable(CF_UNICODETEXT).ok()?;
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(owner).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if !opened {
            return None;
        }
        let text = (|| {
            let h = GetClipboardData(CF_UNICODETEXT).ok()?;
            let p = GlobalLock(HGLOBAL(h.0)) as *const u16;
            if p.is_null() {
                return None;
            }
            let mut n = 0;
            while *p.add(n) != 0 && n < 1_000_000 {
                n += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
            let _ = GlobalUnlock(HGLOBAL(h.0));
            Some(s)
        })();
        let _ = CloseClipboard();
        text
    }
}

pub fn set_clipboard_text(owner: HWND, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        if OpenClipboard(owner).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
            let p = GlobalLock(mem) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                let _ = GlobalUnlock(mem);
                let _ = SetClipboardData(CF_UNICODETEXT, HANDLE(mem.0));
            }
        }
        let _ = CloseClipboard();
    }
}

pub fn lock() {
    unsafe {
        let _ = LockWorkStation();
    }
}

pub fn screenshot() {
    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() },
        },
    };
    let inputs = [
        key(VK_LWIN, false),
        key(VK_SHIFT, false),
        key(VK_S, false),
        key(VK_S, true),
        key(VK_SHIFT, true),
        key(VK_LWIN, true),
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn file_icon(path: &str) -> Option<HICON> {
    unsafe {
        let mut info = SHFILEINFOW::default();
        let r = SHGetFileInfoW(&HSTRING::from(path), Default::default(), Some(&mut info), std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON);
        (r != 0 && !info.hIcon.is_invalid()).then_some(info.hIcon)
    }
}

pub fn shell_image(path: &str, px: i32) -> Option<windows::Win32::Graphics::Gdi::HBITMAP> {
    unsafe {
        SHCreateItemFromParsingName::<_, _, IShellItemImageFactory>(&HSTRING::from(path), None)
            .ok()?
            .GetImage(SIZE { cx: px, cy: px }, SIIGBF_ICONONLY)
            .ok()
    }
}

pub fn drag_file(hwnd: HWND, path: &str) -> bool {
    unsafe {
        let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(path), None) else { return false };
        let Ok(arr) = SHCreateShellItemArrayFromShellItem::<_, IShellItemArray>(&item) else { return false };
        let Ok(data) = arr.BindToHandler::<_, IDataObject>(None, &BHID_DataObject) else { return false };
        let effect = SHDoDragDrop(hwnd, &data, None, DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK).unwrap_or(DROPEFFECT(0));
        effect == DROPEFFECT_MOVE || !std::path::Path::new(path).exists()
    }
}

pub fn dropped_files(wp: WPARAM) -> Vec<String> {
    unsafe {
        let drop = HDROP(wp.0 as _);
        let n = DragQueryFileW(drop, u32::MAX, None);
        let mut out = Vec::new();
        for i in 0..n {
            let len = DragQueryFileW(drop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(drop, i, Some(&mut buf));
            out.push(String::from_utf16_lossy(&buf[..len]));
        }
        DragFinish(drop);
        out
    }
}

pub fn wallpaper_path() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETDESKWALLPAPER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS};
    let mut buf = [0u16; 520];
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETDESKWALLPAPER, buf.len() as u32, Some(buf.as_mut_ptr() as _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
    let p = String::from_utf16_lossy(&buf[..n]);
    if !p.is_empty() && std::path::Path::new(&p).exists() {
        return Some(p);
    }
    let t = std::env::var("APPDATA").ok()? + "\\Microsoft\\Windows\\Themes\\TranscodedWallpaper";
    std::path::Path::new(&t).exists().then_some(t)
}

fn hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

fn rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

pub fn wallpaper_accent() -> Option<String> {
    let px = crate::gfx::image_pixels(&wallpaper_path()?, 48)?;
    let mut bins = [(0.0f32, 0.0f32, 0.0f32); 24];
    for p in px.chunks_exact(4) {
        let (b, g, r) = (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0);
        let (h, s, v) = hsv(r, g, b);
        if s < 0.25 || v < 0.25 {
            continue;
        }
        let w = s * s * v;
        let i = ((h / 15.0) as usize).min(23);
        bins[i].0 += w;
        bins[i].1 += w * s;
        bins[i].2 += w * v;
    }
    let (i, best) = bins.iter().enumerate().max_by(|a, b| a.1 .0.total_cmp(&b.1 .0))?;
    if best.0 < 1.0 {
        return None;
    }
    let h = i as f32 * 15.0 + 7.5;
    let s = (best.1 / best.0).clamp(0.55, 0.85);
    let v = (best.2 / best.0).clamp(0.8, 1.0);
    let (r, g, b) = rgb(h, s, v);
    Some(format!("#{:02X}{:02X}{:02X}", (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8))
}

fn reg_binary(path: &str, name: &str) -> Vec<u8> {
    let (path, name) = (HSTRING::from(path), HSTRING::from(name));
    unsafe {
        let mut n = 0u32;
        if RegGetValueW(HKEY_CURRENT_USER, &path, &name, RRF_RT_REG_BINARY, None, None, Some(&mut n)).is_err() || n == 0 {
            return Vec::new();
        }
        let mut buf = vec![0u8; n as usize];
        if RegGetValueW(HKEY_CURRENT_USER, &path, &name, RRF_RT_REG_BINARY, None, Some(buf.as_mut_ptr() as _), Some(&mut n)).is_err() {
            return Vec::new();
        }
        buf.truncate(n as usize);
        buf
    }
}

fn taskband_favorites() -> Vec<u8> {
    reg_binary("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Taskband", "Favorites")
}

fn shell_link(p: &std::path::Path) -> Option<(IShellLinkW, String)> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ};
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let pf: IPersistFile = link.cast().ok()?;
        pf.Load(&HSTRING::from(p.as_os_str()), STGM_READ).ok()?;
        let mut buf = [0u16; 520];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
        let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
        (n > 0).then(|| (link, String::from_utf16_lossy(&buf[..n])))
    }
}

fn lnk_target(p: &std::path::Path) -> Option<String> {
    let (link, target) = shell_link(p)?;
    unsafe {
        if target.to_lowercase().ends_with("\\update.exe") {
            let mut args = [0u16; 1024];
            let _ = link.GetArguments(&mut args);
            let n = args.iter().position(|c| *c == 0).unwrap_or(0);
            let args = String::from_utf16_lossy(&args[..n]);
            let exe = args.split("--processStart").nth(1)?.trim().trim_matches('"').split('"').next()?.trim().to_string();
            let root = std::path::Path::new(&target).parent()?.to_string_lossy().into_owned();
            return Some(crate::bar::resolve_app(&format!("{root}\\app-0\\{exe}")));
        }
        Some(target)
    }
}

fn assoc_exe(ext: &str) -> Option<String> {
    let mut buf = [0u16; 520];
    let mut n = buf.len() as u32;
    unsafe {
        AssocQueryStringW(ASSOCF_INIT_IGNOREUNKNOWN, ASSOCSTR_EXECUTABLE, &HSTRING::from(format!(".{ext}")), None, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).ok().ok()?;
    }
    let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
    (n > 0).then(|| String::from_utf16_lossy(&buf[..n]))
}

pub fn recent_files(exe: &str, max: usize) -> Vec<String> {
    let key = crate::bar::app_key(exe);
    let folders = key.ends_with("\\explorer.exe");
    let Ok(appdata) = std::env::var("APPDATA") else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(std::path::Path::new(&appdata).join("Microsoft\\Windows\\Recent")) else { return Vec::new() };
    let mut lnks: Vec<(std::time::SystemTime, std::path::PathBuf)> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    lnks.sort_by(|a, b| b.0.cmp(&a.0));
    let mut assoc: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let mut out: Vec<String> = Vec::new();
    for (_, l) in lnks.into_iter().take(150) {
        let Some((_, target)) = shell_link(&l) else { continue };
        let tp = std::path::Path::new(&target);
        let ok = if folders {
            tp.is_dir()
        } else if tp.is_file() {
            let ext = tp.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            !ext.is_empty() && *assoc.entry(ext.clone()).or_insert_with(|| assoc_exe(&ext).is_some_and(|e| crate::bar::app_key(&e) == key))
        } else {
            false
        };
        if ok && !out.iter().any(|o| o.eq_ignore_ascii_case(&target)) {
            out.push(target);
            if out.len() >= max {
                break;
            }
        }
    }
    out
}

pub fn virtual_desktops() -> (usize, usize) {
    const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops";
    let ids = reg_binary(KEY, "VirtualDesktopIDs");
    let mut cur = reg_binary(KEY, "CurrentVirtualDesktop");
    if cur.len() != 16 {
        let mut sid = 0u32;
        unsafe {
            let _ = windows::Win32::System::RemoteDesktop::ProcessIdToSessionId(windows::Win32::System::Threading::GetCurrentProcessId(), &mut sid);
        }
        cur = reg_binary(&format!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\SessionInfo\\{sid}\\VirtualDesktops"), "CurrentVirtualDesktop");
    }
    let count = (ids.len() / 16).max(1);
    let idx = ids.chunks_exact(16).position(|c| c == cur.as_slice()).unwrap_or(0);
    (count, idx)
}

pub fn switch_desktop(delta: i32) {
    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() },
        },
    };
    let arrow = if delta > 0 { VK_RIGHT } else { VK_LEFT };
    let mut inputs = vec![key(VK_CONTROL, false), key(VK_LWIN, false)];
    for _ in 0..delta.unsigned_abs() {
        inputs.push(key(arrow, false));
        inputs.push(key(arrow, true));
    }
    inputs.push(key(VK_LWIN, true));
    inputs.push(key(VK_CONTROL, true));
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn new_desktop() {
    crate::util::send_combo(&[VK_CONTROL, VK_LWIN], VK_D);
}

#[derive(Clone, Debug)]
pub struct AppVolume {
    pub key: String,
    pub name: String,
    pub path: String,
    pub vol: f32,
    pub muted: bool,
}

fn sessions() -> Vec<(String, String, ISimpleAudioVolume)> {
    use windows::core::Interface;
    let mut out = Vec::new();
    unsafe {
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return out };
        let Ok(dev) = en.GetDefaultAudioEndpoint(eRender, eConsole) else { return out };
        let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { return out };
        let Ok(list) = mgr.GetSessionEnumerator() else { return out };
        for i in 0..list.GetCount().unwrap_or(0) {
            let Ok(c) = list.GetSession(i) else { continue };
            if c.GetState().map_or(true, |s| s == AudioSessionStateExpired) {
                continue;
            }
            let Ok(c2) = c.cast::<IAudioSessionControl2>() else { continue };
            let Ok(sv) = c.cast::<ISimpleAudioVolume>() else { continue };
            if c2.IsSystemSoundsSession() == windows::Win32::Foundation::S_OK {
                out.push(("system".into(), String::new(), sv));
                continue;
            }
            let Some(path) = c2.GetProcessId().ok().and_then(crate::util::pid_path) else { continue };
            out.push((crate::bar::app_key(&path), path, sv));
        }
    }
    out
}

pub fn app_volumes() -> Vec<AppVolume> {
    let mut out: Vec<AppVolume> = Vec::new();
    for (key, path, sv) in sessions() {
        let (vol, muted) = unsafe { (sv.GetMasterVolume().unwrap_or(1.0), sv.GetMute().map_or(false, |m| m.as_bool())) };
        if let Some(a) = out.iter_mut().find(|a| a.key == key) {
            a.muted &= muted;
            continue;
        }
        let name = if key == "system" {
            crate::i18n::tr("Sistem sesleri").to_string()
        } else {
            crate::util::file_description(&path).or_else(|| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned())).unwrap_or_default()
        };
        out.push(AppVolume { key, name, path, vol, muted });
    }
    out.sort_by_key(|a| (a.key == "system", a.name.to_lowercase()));
    out
}

pub fn set_app_volume(key: &str, vol: Option<f32>, mute: Option<bool>) {
    for (k, _, sv) in sessions() {
        if k != key {
            continue;
        }
        unsafe {
            if let Some(v) = vol {
                let _ = sv.SetMasterVolume(v.clamp(0.0, 1.0), std::ptr::null());
                if v > 0.0 {
                    let _ = sv.SetMute(false, std::ptr::null());
                }
            }
            if let Some(m) = mute {
                let _ = sv.SetMute(m, std::ptr::null());
            }
        }
    }
}

pub fn windows_pins() -> Vec<String> {
    let Ok(appdata) = std::env::var("APPDATA") else { return Vec::new() };
    let dir = std::path::Path::new(&appdata).join("Microsoft\\Internet Explorer\\Quick Launch\\User Pinned\\TaskBar");
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let fav = taskband_favorites();
    let mut found: Vec<(usize, String)> = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
            continue;
        }
        let Some(target) = lnk_target(&p) else { continue };
        if !target.to_lowercase().ends_with(".exe") || !std::path::Path::new(&target).exists() || found.iter().any(|(_, t)| t.eq_ignore_ascii_case(&target)) {
            continue;
        }
        let name: Vec<u8> = p.file_name().unwrap_or_default().to_string_lossy().encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        let pos = fav.windows(name.len().max(1)).position(|w| w == name.as_slice()).unwrap_or(usize::MAX);
        found.push((pos, target));
    }
    found.sort_by_key(|f| f.0);
    found.into_iter().map(|f| f.1).collect()
}
