use std::collections::hash_map::DefaultHasher;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Instant;

use windows::core::Result;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{DeleteObject, MonitorFromWindow, HMONITOR, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::WindowsAndMessaging::HICON;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::{create_layered, Action, Ctx};
use crate::config::{Indicator, Palette, Tiles};
use crate::droptarget::{self, WM_APP_DRAG};
use crate::gfx::{self, Align, Color, Rect, Surface};
use crate::media::MediaCmd;
use crate::monitors::Monitor;
use crate::preview::{window_title, Preview, PreviewOut};
use crate::road::{draw_road, Car, RoadArt};
use crate::scene::{draw_scene, SceneArt, Show};
use crate::skin;
use crate::tray;
use crate::trayflyout::{Flyout, FlyoutOut};
use crate::util::{self, WM_APP_APPBAR};
use crate::sprites::{self, GroundMeta};
use crate::weather::Sky;
use crate::widgets::draw_sky;

const WM_MOUSELEAVE_: u32 = 0x02A3;
const TIMER_PREVIEW: usize = 1;
const TIMER_PREVIEW_CHECK: usize = 2;
const OVERHANG: f32 = 10.0;
const TIMER_ANIM: usize = 3;
const TIMER_FLYOUT: usize = 4;
const TIMER_CAR: usize = 5;
const TIMER_FLYOUT_CLOSE: usize = 6;
const TIMER_DRAG: usize = 7;
const TIMER_PREVIEW_ANIM: usize = 8;
const DRAG_DWELL_MS: u128 = 500;
const DESK_DOT: f32 = 12.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit {
    Start,
    Battery,
    App(u64),
    Media,
    Weather,
    Sys,
    Clock,
    TrayIcon(u64),
    TrayMore,
    Desktops,
}

struct Group {
    id: u64,
    wins: Vec<HWND>,
    pinned: Option<String>,
}

pub struct Bar {
    pub hwnd: HWND,
    surf: Surface,
    mon: Monitor,
    appbar: bool,
    only_this_monitor: bool,
    hidden: Vec<String>,
    pinned: Vec<String>,
    launcher: bool,
    full: bool,
    top: bool,
    x: i32,
    y: i32,
    w: f32,
    band: f32,
    overhang: f32,
    order: Vec<HWND>,
    groups: Vec<Group>,
    last_active: HashMap<u64, HWND>,
    icons: HashMap<u64, Option<ID2D1Bitmap>>,
    /// grup -> (rozet ikonu, bitmap) — Discord gibi uygulamaların okunmamış rozeti
    badges: HashMap<u64, (isize, Option<ID2D1Bitmap>)>,
    icon_px: i32,
    start_bmp: Option<ID2D1Bitmap>,
    tray_bmps: HashMap<u64, (u64, Option<ID2D1Bitmap>)>,
    hits: Vec<(Rect, Hit)>,
    hover: Option<Hit>,
    tracking: bool,
    sig: String,
    pal: Option<Palette>,
    pub panel: Option<RECT>,
    pub terrain_top: Option<i32>,
    ground: Option<GroundArt>,
    road: Option<RoadArt>,
    car: Car,
    scene: Option<SceneArt>,
    show: Show,
    preview: Option<Preview>,
    /// Önizleme dışında art arda geçen kontrol sayısı (kapanmadan önce kısa tolerans)
    preview_out: u8,
    pending: Option<u64>,
    drag_over: Option<((u64, Option<isize>), Instant, bool)>,
    press: Option<(u64, f32, f32)>,
    dragging: bool,
    drag_x: f32,
    drop_index: usize,
    anim: HashMap<u64, f32>,
    anim_last: Instant,
    animating: bool,
    /// Etkin pencere işaretinin geçişi: grup -> 0..1
    act: HashMap<u64, f32>,
    flyout: Option<Flyout>,
    /// Gizli simgelerden sağ tıklanan uygulama: onun menüsüne tıklamak paneli kapatmasın
    flyout_pid: u32,
    desktops: (usize, usize),
}

pub struct Out {
    pub action: Option<Action>,
    pub repaint: bool,
}

impl Bar {
    pub fn new(ctx: &Ctx, mon: Monitor, only_this_monitor: bool) -> Result<Bar> {
        let scale = mon.scale;
        let bc = &ctx.cfg.bar;
        let top = bc.top();
        let (r, wk) = (mon.rect, mon.work);
        let reserved = if top { wk.top - r.top } else { r.bottom - wk.bottom };
        let appbar = reserved <= 0;
        let band = if appbar { bc.total_height() } else { reserved as f32 / scale };
        let overhang = if bc.dock() { 22.0 } else if bc.floating() { OVERHANG } else { 0.0 };
        let h = band + overhang;
        let w = (r.right - r.left) as f32 / scale;
        let hwnd = create_layered(WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE)?;
        droptarget::register(hwnd, hwnd);
        let surf = Surface::new(w, h, scale, &ctx.pal.font)?;
        let mut bar = Bar {
            hwnd,
            mon,
            appbar,
            only_this_monitor,
            hidden: bc.hidden_apps.iter().map(|s| s.to_lowercase()).collect(),
            pinned: bc.pinned.clone(),
            launcher: bc.start_menu != "windows",
            full: false,
            top,
            x: r.left,
            y: if top { r.top } else { r.bottom - surf.h_px },
            surf,
            w,
            band,
            overhang,
            order: Vec::new(),
            groups: Vec::new(),
            last_active: HashMap::new(),
            icons: HashMap::new(),
            badges: HashMap::new(),
            icon_px: 0,
            start_bmp: None,
            tray_bmps: HashMap::new(),
            hits: Vec::new(),
            hover: None,
            tracking: false,
            sig: String::new(),
            pal: None,
            panel: None,
            terrain_top: None,
            ground: None,
            road: None,
            scene: None,
            show: Show::new(bc.show, (r.top as u32 + 7).wrapping_mul(2246822519) ^ (r.left as u32)),
            car: Car::new(bc.car, (r.left as u32).wrapping_mul(2654435761) ^ unsafe { windows::Win32::System::SystemInformation::GetTickCount() }),
            preview: None,
            preview_out: 0,
            pending: None,
            drag_over: None,
            press: None,
            dragging: false,
            drag_x: 0.0,
            drop_index: 0,
            anim: HashMap::new(),
            anim_last: Instant::now(),
            animating: false,
            act: HashMap::new(),
            flyout: None,
            flyout_pid: 0,
            desktops: (1, 0),
        };
        if !bc.start_icon.is_empty() {
            bar.start_bmp = std::fs::read(&bc.start_icon).ok().and_then(|b| bar.surf.bitmap_from_bytes(&b, (64.0 * scale) as u32));
        }
        if bc.terrain && bc.floating() && !top {
            // "grass" (çimen) kaldırıldı: eski ayarlar yola düşer
            if crate::scene::is_scene(&bc.terrain_style) {
                bar.scene = SceneArt::load(&bar.surf, &bc.terrain_style);
                if bar.scene.is_none() {
                    bar.ground = GroundArt::load(&bar.surf);
                }
            } else {
                bar.road = RoadArt::load(&bar.surf);
                if bar.road.is_none() {
                    bar.ground = GroundArt::load(&bar.surf);
                }
            }
        }
        if appbar {
            let mut abd = bar.abd(ctx);
            unsafe {
                SHAppBarMessage(ABM_NEW, &mut abd);
            }
        }
        bar.reposition(ctx);
        bar.refresh_windows();
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(bar)
    }

    pub fn owns(&self, h: HWND) -> bool {
        self.hwnd == h || self.preview.as_ref().is_some_and(|p| p.hwnd == h) || self.flyout.as_ref().is_some_and(|f| f.hwnd == h)
    }

    fn abd(&self, ctx: &Ctx) -> APPBARDATA {
        APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: self.hwnd,
            uCallbackMessage: WM_APP_APPBAR,
            uEdge: if ctx.cfg.bar.top() { ABE_TOP } else { ABE_BOTTOM },
            rc: self.mon.rect,
            lParam: LPARAM(0),
        }
    }

    pub fn reposition(&mut self, ctx: &Ctx) {
        if self.appbar {
            let band_px = (self.band * self.surf.scale).ceil() as i32;
            let top = self.top;
            let mut abd = self.abd(ctx);
            let fit = |rc: &mut RECT| {
                if top {
                    rc.bottom = rc.top + band_px
                } else {
                    rc.top = rc.bottom - band_px
                }
            };
            fit(&mut abd.rc);
            unsafe {
                SHAppBarMessage(ABM_QUERYPOS, &mut abd);
                fit(&mut abd.rc);
                SHAppBarMessage(ABM_SETPOS, &mut abd);
            }
            self.y = if top { abd.rc.top } else { abd.rc.bottom - self.surf.h_px };
        }
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                self.x,
                self.y,
                self.surf.w_px,
                self.surf.h_px,
                SWP_NOACTIVATE,
            );
        }
        self.sig.clear();
    }

    pub fn refresh_windows(&mut self) -> bool {
        let mut found: Vec<HWND> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(&mut found as *mut _ as isize));
        }
        if self.only_this_monitor {
            let me: HMONITOR = self.mon.hmon;
            found.retain(|h| unsafe { MonitorFromWindow(*h, MONITOR_DEFAULTTONEAREST) } == me);
        }
        let before = self.order.clone();
        let global = ORDER.with(|o| {
            let mut o = o.borrow_mut();
            o.retain(|h| unsafe { IsWindow(util::hwnd(*h)).as_bool() });
            KEYS.with(|k| k.borrow_mut().retain(|h, _| unsafe { IsWindow(util::hwnd(*h)).as_bool() }));
            let mut new: Vec<(u64, usize, isize)> = found
                .iter()
                .enumerate()
                .filter(|(_, h)| !o.contains(&(h.0 as isize)))
                .map(|(z, h)| (process_start(*h), usize::MAX - z, h.0 as isize))
                .collect();
            new.sort();
            let saved = saved_order();
            let rank = |h: isize| {
                let k = key_of(util::hwnd(h));
                saved.iter().position(|s| *s == k)
            };
            for (_, _, h) in new {
                match rank(h) {
                    Some(r) => {
                        let pos = o.iter().position(|w| rank(*w).map_or(true, |rw| rw > r)).unwrap_or(o.len());
                        o.insert(pos, h);
                    }
                    None => o.push(h),
                }
            }
            o.clone()
        });
        self.order = global.into_iter().map(util::hwnd).filter(|h| found.contains(h)).collect();

        let mut groups: Vec<Group> = Vec::new();
        for &h in &self.order {
            let key = key_of(h);
            let exe = key.rsplit('\\').next().unwrap_or("");
            if self.hidden.iter().any(|x| x == exe) {
                continue;
            }
            let id = hash(&key);
            match groups.iter_mut().find(|g| g.id == id) {
                Some(g) => g.wins.push(h),
                None => groups.push(Group { id, wins: vec![h], pinned: None }),
            }
        }
        let mut pinned_groups: Vec<Group> = Vec::new();
        for p in &self.pinned {
            let id = hash(&app_key(p));
            if pinned_groups.iter().any(|g| g.id == id) {
                continue;
            }
            let wins = groups.iter().position(|g| g.id == id).map(|i| groups.remove(i).wins).unwrap_or_default();
            pinned_groups.push(Group { id, wins, pinned: Some(p.clone()) });
        }
        pinned_groups.extend(groups);
        let mut groups = pinned_groups;
        let saved = saved_order();
        groups.sort_by_key(|g| {
            let k = group_key(g);
            saved.iter().position(|s| *s == k).unwrap_or(usize::MAX)
        });
        let fg = unsafe { GetForegroundWindow() };
        FLASH.with(|f| {
            let mut f = f.borrow_mut();
            f.remove(&(fg.0 as isize));
            f.retain(|h| unsafe { IsWindow(util::hwnd(*h)).as_bool() });
        });
        crate::taskband::prune();
        for g in &groups {
            if g.wins.contains(&fg) {
                self.last_active.insert(g.id, fg);
            }
        }
        self.last_active.retain(|id, h| groups.iter().any(|g| g.id == *id && g.wins.contains(h)));
        self.icons.retain(|id, _| groups.iter().any(|g| g.id == *id));
        if let Some(p) = &self.preview {
            if !groups.iter().any(|g| g.id == p.group && g.wins == p.wins) {
                self.hide_preview();
            }
        }
        self.groups = groups;
        self.update_fullscreen();
        before != self.order
    }

    fn update_fullscreen(&mut self) {
        unsafe {
            let fg = GetForegroundWindow();
            let mut r = RECT::default();
            let full = !fg.is_invalid()
                && MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST) == self.mon.hmon
                && GetWindowRect(fg, &mut r).is_ok()
                && r.left <= self.mon.rect.left
                && r.top <= self.mon.rect.top
                && r.right >= self.mon.rect.right
                && r.bottom >= self.mon.rect.bottom
                && !matches!(class_name(fg).as_str(), "Progman" | "WorkerW");
            let visible = IsWindowVisible(self.hwnd).as_bool();
            self.full = full;
            if full && visible {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            } else if !full && !visible {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.sig.clear();
            }
        }
    }

    fn signature(&self, ctx: &Ctx) -> String {
        let fg = unsafe { GetForegroundWindow() };
        let t = &ctx.now;
        let sec = if ctx.cfg.bar.clock_seconds { t.sec } else { 0 };
        let wins: Vec<String> = self
            .order
            .iter()
            .map(|h| format!("{:x}{}", h.0 as isize, unsafe { IsIconic(*h).as_bool() } as u8))
            .collect();
        format!(
            "{:?}|{}|{}|{}|{:x}|{:?}|{:?}|{}:{}:{}.{}|{:.0}/{:.0}|{}{}{}|{:?}",
            ctx.feeds.desktops,
            FLASH.with(|f| f.borrow().len()),
            wins.join(","),
            tray::generation(),
            fg.0 as isize,
            self.hover,
            self.preview.as_ref().map(|p| p.group),
            t.hour,
            t.min,
            sec,
            t.day,
            ctx.sys.cpu,
            ctx.sys.ram,
            ctx.media.title,
            ctx.media.artist,
            ctx.media.playing,
            ctx.weather.map(|w| (w.temp.round() as i32, w.code, w.is_day)),
        )
    }

    pub fn fullscreen(&self) -> bool {
        self.full
    }

    pub fn paint(&mut self, ctx: &Ctx, force: bool) {
        if self.full {
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_ANIM);
                let _ = KillTimer(self.hwnd, TIMER_CAR);
            }
            return;
        }
        let sig = self.signature(ctx);
        if !force && sig == self.sig {
            return;
        }
        self.sig = sig;
        self.pal = Some(ctx.pal.clone());
        self.draw(ctx);
        self.surf.present(self.hwnd, self.x, self.y);
        unsafe {
            if self.animating || self.dragging {
                SetTimer(self.hwnd, TIMER_ANIM, 16, None);
            } else {
                let _ = KillTimer(self.hwnd, TIMER_ANIM);
            }
            let timer = match (&self.road, &self.scene) {
                (Some(_), _) => self.car.timer_ms(),
                (None, Some(_)) => self.show.timer_ms(),
                _ => None,
            };
            match timer {
                Some(ms) => {
                    SetTimer(self.hwnd, TIMER_CAR, ms, None);
                }
                None => {
                    let _ = KillTimer(self.hwnd, TIMER_CAR);
                }
            }
        }
    }

    fn draw(&mut self, ctx: &Ctx) {
        let bc = &ctx.cfg.bar;
        let pal = &ctx.pal;
        let fl = bc.floating();
        let fg = unsafe { GetForegroundWindow() };

        let icon_px = {
            let ph = if fl { bc.height.clamp(28.0, (self.band - 2.0).max(28.0)) } else { self.band };
            let dock = bc.dock();
            let slot = if dock { (ph - 8.0).max(16.0) } else { (ph - 10.0).max(16.0) };
            let icon = if dock { slot - 6.0 } else { bc.icon_size.min(slot - 8.0) };
            (icon * self.surf.scale).round().max(16.0) as i32
        };
        if icon_px != self.icon_px {
            self.icon_px = icon_px;
            self.icons.clear();
        }
        for g in &self.groups {
            if !self.icons.contains_key(&g.id) {
                let path = match (&g.pinned, g.wins.is_empty()) {
                    (Some(p), true) => Some(resolve_app(p)),
                    _ => g.wins.first().and_then(|h| util::process_path(*h)).filter(|p| !generic_host(p)),
                };
                let bmp = path
                    .and_then(|p| {
                        let std_px = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256].into_iter().find(|s| *s >= icon_px).unwrap_or(256);
                        crate::system::shell_image(&p, std_px)
                    })
                    .and_then(|h| {
                        let b = self.surf.bitmap_from_shell_image(h, icon_px as u32);
                        unsafe {
                            let _ = DeleteObject(h);
                        }
                        b
                    })
                    .or_else(|| g.wins.iter().find_map(|h| gfx::window_icon(*h)).and_then(|i| self.surf.bitmap_from_icon_px(i, icon_px as u32)))
                    .or_else(|| {
                        let p = g.pinned.as_deref()?;
                        let h = crate::system::file_icon(&resolve_app(p))?;
                        let b = self.surf.bitmap_from_icon_px(h, icon_px as u32);
                        unsafe {
                            let _ = DestroyIcon(h);
                        }
                        b
                    });
                self.icons.insert(g.id, bmp);
            }
        }
        let badge_px = ((icon_px as f32) * 0.55).round().max(10.0) as u32;
        let mut badges = std::mem::take(&mut self.badges);
        badges.retain(|id, _| self.groups.iter().any(|g| g.id == *id));
        for g in &self.groups {
            match g.wins.iter().find_map(|h| crate::taskband::overlay(*h)) {
                Some(icon) => {
                    if badges.get(&g.id).map(|b| b.0) != Some(icon.0 as isize) {
                        badges.insert(g.id, (icon.0 as isize, self.surf.bitmap_from_icon_px(icon, badge_px)));
                    }
                }
                None => {
                    badges.remove(&g.id);
                }
            }
        }
        self.badges = badges;
        let tray_icons = if bc.show_tray { tray::icons() } else { Vec::new() };
        for i in tray_icons.iter().filter(|i| i.promoted) {
            if self.tray_bmps.get(&i.id).map(|b| b.0) != Some(i.gen) {
                let bmp = self.surf.bitmap_from_icon(HICON(i.icon as _));
                self.tray_bmps.insert(i.id, (i.gen, bmp));
            }
        }
        self.tray_bmps.retain(|id, _| tray_icons.iter().any(|i| i.id == *id && i.promoted));

        let mut anim = std::mem::take(&mut self.anim);
        let now = Instant::now();
        let dt = (now - self.anim_last).as_secs_f32().min(0.05);
        self.anim_last = now;
        let ease = 1.0 - (-dt * 18.0).exp();

        let s = &self.surf;
        s.begin();
        let mut hits = Vec::new();
        let hover = self.hover;

        let band_y = if self.top { 0.0 } else { self.overhang };
        let ph = if fl { bc.height.clamp(28.0, (self.band - 2.0).max(28.0)) } else { self.band };
        let py = band_y + (self.band - ph) / 2.0;
        let modular = bc.modular();
        let dock = bc.dock();
        let radius = if !fl {
            0.0
        } else if modular {
            pal.radius.min(ph / 2.0)
        } else if dock {
            16.0f32.min(ph / 2.0)
        } else {
            ph / 2.0
        };
        let slot = if dock { (ph - 8.0).max(16.0) } else { (ph - 10.0).max(16.0) };
        let icon = if dock { slot - 6.0 } else { bc.icon_size.min(slot - 8.0) };
        let gap = if pal.tiles == Tiles::None { 4.0 } else { 6.0 };
        let y = py + (ph - slot) / 2.0;
        let half = slot / 2.0;
        let sep_w = if pal.separators && !modular { 9.0 } else { 0.0 };
        let pad = 5.0 + (radius - 12.0).max(0.0) * 0.3;
        let hover_bg = |r: Rect, hit: Hit| {
            if hover == Some(hit) {
                s.fill_round(r, 8.0, pal.text.alpha(0.07));
            }
        };

        let t = &ctx.now;
        let time = if bc.clock_seconds {
            format!("{:02}:{:02}:{:02}", t.hour, t.min, t.sec)
        } else {
            format!("{:02}:{:02}", t.hour, t.min)
        };
        let date = format!("{:02}.{:02}.{}", t.day, t.month, t.year);
        let weather = ctx.weather.filter(|_| bc.show_weather);
        let temp = weather.map(|w| format!("{:.0}°", w.temp)).unwrap_or_default();
        let media_label = (bc.show_media && ctx.media.active).then(|| {
            if ctx.media.artist.is_empty() {
                ctx.media.title.clone()
            } else {
                format!("{} — {}", ctx.media.title, ctx.media.artist)
            }
        });
        let media_w = media_label.as_ref().map(|l| (s.measure(l, 12.0, 500) + slot + 14.0).min(220.0));

        let mut right: Vec<(Hit, f32)> = Vec::new();
        if bc.show_clock {
            right.push((Hit::Clock, s.measure(&date, 11.5, 400).max(s.measure(&time, 13.0, 600)) + 20.0));
        }
        if bc.show_sysinfo {
            right.push((Hit::Sys, s.measure("CPU 100%", 11.5, 500) + 18.0));
        }
        let battery = ctx.feeds.battery;
        let bat_text = battery.map(|b| format!("%{}", b.percent)).unwrap_or_default();
        if battery.is_some() {
            right.push((Hit::Battery, 22.0 + s.measure(&bat_text, 12.0, 600) + 10.0));
        }
        if weather.is_some() {
            right.push((Hit::Weather, icon + s.measure(&temp, 13.0, 600) + 22.0));
        }
        for i in tray_icons.iter().filter(|i| i.promoted).rev() {
            right.push((Hit::TrayIcon(i.id), 26.0));
        }
        if tray_icons.iter().any(|i| !i.promoted) {
            right.push((Hit::TrayMore, 24.0));
        }
        self.desktops = ctx.feeds.desktops;
        if self.desktops.0 > 1 {
            right.push((Hit::Desktops, self.desktops.0 as f32 * DESK_DOT + 26.0));
        }
        if let (true, Some(mw)) = (dock, media_w) {
            right.push((Hit::Media, mw));
        }
        let right_w = right.iter().map(|(_, w)| w).sum::<f32>() + 4.0 * right.len().saturating_sub(1) as f32;
        let n = self.groups.len() as f32;
        let apps_w = (n * slot + (n - 1.0) * gap).max(0.0);
        let media_mid = if dock { None } else { media_w };
        let mid_w = media_mid.map(|w| w + if n > 0.0 { gap } else { 0.0 }).unwrap_or(0.0) + apps_w;

        let left_align = bc.align == "left";
        let place = |total: f32| {
            if left_align {
                8.0
            } else {
                ((self.w - total) / 2.0).max(0.0)
            }
        };
        let extent;
        let mut panels: Vec<Rect> = Vec::new();
        let (start_area, mid_x, right_x) = if dock {
            let mid_part = if mid_w > 0.0 { gap + sep_w + mid_w } else { 0.0 };
            let main_w = (pad + slot + mid_part + pad).min(self.w - 16.0);
            let main = Rect::new(place(main_w), py, main_w, ph);
            skin::panel(s, main, radius, pal, true);
            panels.push(main);
            let a = Rect::new(main.x + pad, py, slot, ph);
            if pal.separators && mid_w > 0.0 {
                skin::separator(s, a.x + slot + (gap + sep_w) / 2.0, py + ph / 2.0, slot * 0.6, pal);
            }
            let cw = if right.is_empty() { 0.0 } else { right_w + 2.0 * pad + 4.0 };
            let c = Rect::new(self.w - 8.0 - cw, py, cw, ph);
            if cw > 0.0 {
                skin::panel(s, c, radius, pal, true);
            panels.push(c);
            }
            extent = main;
            (a, a.x + slot + gap + sep_w, c.x + pad + 2.0)
        } else if modular {
            let g = 6.0;
            let aw = ph + 4.0;
            let bw = (mid_w + 2.0 * pad + 4.0).max(ph);
            let cw = if right.is_empty() { 0.0 } else { right_w + 2.0 * pad + 4.0 };
            let total = aw + g + bw + if cw > 0.0 { g + cw } else { 0.0 };
            let a = Rect::new(place(total), py, aw, ph);
            let b = Rect::new(a.x + aw + g, py, bw, ph);
            let c = Rect::new(b.x + bw + g, py, cw, ph);
            skin::panel(s, a, radius, pal, true);
            panels.push(a);
            skin::panel(s, b, radius, pal, true);
            panels.push(b);
            if cw > 0.0 {
                skin::panel(s, c, radius, pal, true);
            panels.push(c);
            }
            extent = Rect::new(a.x, py, if cw > 0.0 { c.x + cw } else { b.x + bw } - a.x, ph);
            (a, b.x + (bw - mid_w) / 2.0, c.x + pad + 2.0)
        } else {
            let right_part = if right.is_empty() { 0.0 } else { gap + sep_w + right_w };
            let mid_part = if mid_w > 0.0 { gap + sep_w + mid_w } else { 0.0 };
            let content = pad + slot + mid_part + right_part + pad;
            let panel = if fl {
                let w = content.min(self.w - 16.0);
                Rect::new(place(w), py, w, ph)
            } else {
                Rect::new(0.0, py, self.w, ph)
            };
            skin::panel(s, panel, radius, pal, true);
            panels.push(panel);
            extent = panel;
            let start_x = if fl {
                panel.x + pad
            } else {
                ((self.w - (slot + mid_part)) / 2.0).max(panel.x + pad)
            };
            let a = Rect::new(start_x, py, slot, ph);
            let mid_x = a.x + slot + gap + sep_w;
            let rx = panel.x + panel.w - pad - right_w;
            if pal.separators {
                let cy = py + ph / 2.0;
                if mid_w > 0.0 {
                    skin::separator(s, a.x + slot + (gap + sep_w) / 2.0, cy, slot * 0.6, pal);
                }
                if !right.is_empty() {
                    skin::separator(s, rx - (gap + sep_w) / 2.0, cy, slot * 0.6, pal);
                }
            }
            (a, mid_x, rx)
        };

        let walk_y = match (&self.road, &self.ground, fl && !self.top && bc.terrain) {
            (Some(art), _, true) => draw_road(s, art, &mut self.car, &panels, self.w, self.overhang + self.band, ctx, self.mon.rect.left),
            (None, Some(art), true) => draw_terrain(s, art, &panels, self.w, self.overhang + self.band, ctx, self.mon.rect.left),
            (None, None, true) => match &self.scene {
                Some(art) => draw_scene(s, art, &mut self.show, &panels, self.w, self.overhang + self.band, ctx, self.mon.rect.left),
                None => None,
            },
            _ => None,
        };

        let r = Rect::new(start_area.x + (start_area.w - slot) / 2.0, y, slot, slot);
        skin::start_button(s, r, pal, hover == Some(Hit::Start), icon, self.start_bmp.as_ref());
        hits.push((r, Hit::Start));

        let draw_media = |r: Rect, label: &str| {
            hover_bg(r, Hit::Media);
            let g = if ctx.media.playing { '\u{E769}' } else { '\u{E768}' };
            s.fill_circle(r.x + slot / 2.0, r.y + slot / 2.0, slot / 2.0 - 6.0, pal.accent.alpha(0.2));
            s.glyph(g, Rect::new(r.x, r.y, slot, slot), 12.0, pal.accent);
            s.text(label, Rect::new(r.x + slot + 2.0, r.y, r.w - slot - 10.0, slot), 12.0, 500, pal.text, Align::Left);
        };
        let mut x = mid_x;
        if let (Some(label), Some(mw)) = (&media_label, media_mid) {
            let r = Rect::new(x, y, mw, slot);
            draw_media(r, label);
            hits.push((r, Hit::Media));
            x += mw + gap;
        }
        let tiled = pal.tiles != Tiles::None;
        let hovered_i = self.groups.iter().position(|g| hover == Some(Hit::App(g.id)));
        let step = slot + gap;
        let drag_id = if self.dragging { self.press.map(|p| p.0) } else { None };
        let others: Vec<usize> = (0..self.groups.len()).filter(|&i| Some(self.groups[i].id) != drag_id).collect();
        let last = x + self.groups.len().saturating_sub(1) as f32 * step;
        let drag_left = self.press.filter(|_| drag_id.is_some()).map(|(_, _, grab)| (self.drag_x - grab).clamp(x, last));
        let ins = drag_left
            .map(|dl| (0..others.len()).filter(|j| x + *j as f32 * step < dl).count())
            .unwrap_or(0);
        let mut animating = false;
        let mut act = std::mem::take(&mut self.act);
        let ease_act = 1.0 - (-dt * 14.0).exp();
        let mut placed: Vec<(usize, f32)> = Vec::new();
        for (j, &gi) in others.iter().enumerate() {
            let idx = if drag_id.is_some() && j >= ins { j + 1 } else { j };
            let tx = x + idx as f32 * step;
            let id = self.groups[gi].id;
            let mut cur = anim.get(&id).map_or(tx, |a| a + (tx - a) * ease);
            if (cur - tx).abs() < 0.3 {
                cur = tx;
            } else {
                animating = true;
            }
            anim.insert(id, cur);
            placed.push((gi, cur));
        }
        if let (Some(id), Some(dl)) = (drag_id, drag_left) {
            if let Some(gi) = self.groups.iter().position(|g| g.id == id) {
                anim.insert(id, dl);
                placed.push((gi, dl));
            }
        }
        anim.retain(|id, _| self.groups.iter().any(|g| g.id == *id));
        for (gi, ax) in placed {
            let g = &self.groups[gi];
            let lifted = drag_id == Some(g.id);
            let r = Rect::new(ax, y - if lifted { 3.0 } else { 0.0 }, slot, slot);
            let active = g.wins.contains(&fg);
            let hovered = hover == Some(Hit::App(g.id)) || self.preview.as_ref().is_some_and(|p| p.group == g.id);
            let mag = match (dock, hovered_i) {
                _ if lifted => 1.08,
                (true, Some(h)) if drag_id.is_none() && h == gi => 1.15,
                _ => 1.0,
            };
            let ms = slot * mag;
            let tr = if tiled { Rect::new(r.x, r.y - 2.0, slot, slot) } else { r };
            let tr = Rect::new(tr.x + (slot - ms) / 2.0, tr.y + slot - ms, ms, ms);
            let flashing = !active && FLASH.with(|f| g.wins.iter().any(|h| f.borrow().contains(&(h.0 as isize))));
            skin::tile(s, tr, pal, active, hovered && !dock);
            if flashing {
                s.fill_round(tr, 8.0, Color::rgba(255, 159, 67, 90));
                s.stroke_round(tr.inset(0.5), 8.0, 1.0, Color::rgba(255, 159, 67, 200));
            }
            let icon = icon * mag;
            let ir = Rect::new(tr.x + (ms - icon) / 2.0, tr.y + (ms - icon) / 2.0 - if tiled { 0.0 } else { 1.5 }, icon, icon);
            match self.icons.get(&g.id).and_then(|b| b.as_ref()) {
                Some(bmp) => s.icon(bmp, ir),
                None => {
                    s.fill_circle(ir.x + icon / 2.0, ir.y + icon / 2.0, icon / 2.0, pal.accent.alpha(0.35));
                    let name = g.wins.first().map(|h| window_title(*h)).or_else(|| g.pinned.as_deref().and_then(|p| std::path::Path::new(p).file_stem()).map(|s| s.to_string_lossy().into_owned())).unwrap_or_default();
                    let letter: String = name.chars().next().unwrap_or('?').to_uppercase().collect();
                    s.text(&letter, ir, icon * 0.5, 600, pal.text, Align::Center);
                }
            }
            // Uygulamanın görev çubuğu rozeti (ör. Discord okunmamış mesaj sayısı) sağ alt köşede
            if let Some(Some(b)) = self.badges.get(&g.id).map(|b| b.1.as_ref()) {
                let bs = icon * 0.55;
                s.icon(b, Rect::new(ir.x + icon - bs + bs * 0.2, ir.y + icon - bs + bs * 0.15, bs, bs));
            }
            let minimized = g.wins.iter().all(|h| unsafe { IsIconic(*h).as_bool() });
            // Etkin uygulama değişince işaret yumuşakça büyüyüp renk değiştirir
            let goal = if active { 1.0 } else { 0.0 };
            let mut v = act.get(&g.id).map_or(goal, |a| a + (goal - a) * ease_act);
            if (v - goal).abs() < 0.01 {
                v = goal;
            } else {
                animating = true;
            }
            act.insert(g.id, v);
            let base_c = if flashing { Color::rgba(255, 159, 67, 255) } else if minimized { pal.dim.alpha(0.5) } else { pal.dim };
            let ic = Color {
                r: base_c.r + (pal.accent.r - base_c.r) * v,
                g: base_c.g + (pal.accent.g - base_c.g) * v,
                b: base_c.b + (pal.accent.b - base_c.b) * v,
                a: base_c.a + (pal.accent.a - base_c.a) * v,
            };
            let cx = r.x + slot / 2.0;
            let iy = if tiled { (r.y + slot + 1.5).min(py + ph - 2.0) } else { r.y + slot - 1.5 };
            let multi = g.wins.len() > 1;
            let running = !g.wins.is_empty();
            match pal.indicator {
                _ if !running => {}
                Indicator::Pill => {
                    let iw = 6.0 + 10.0 * v + if multi { 6.0 } else { 0.0 };
                    s.fill_round(Rect::new(cx - iw / 2.0, iy - 1.5, iw, 3.0), 1.5, ic);
                }
                Indicator::Dot => {
                    if v > 0.0 {
                        s.fill_circle(cx, iy, 3.5 * v, pal.accent.alpha(0.25 * v));
                    }
                    let rad = 1.4 + 0.4 * v;
                    if multi {
                        s.fill_circle(cx - 2.8, iy, rad, ic);
                        s.fill_circle(cx + 2.8, iy, rad, ic);
                    } else {
                        s.fill_circle(cx, iy, rad, ic);
                    }
                }
                Indicator::Dash => {
                    let iw = 8.0 + 4.0 * v + if multi { 6.0 } else { 0.0 };
                    s.fill_round(Rect::new(cx - iw / 2.0, iy - 1.25, iw, 2.5), 1.0, ic);
                }
            }
            hits.push((r, Hit::App(g.id)));
        }
        act.retain(|id, _| self.groups.iter().any(|g| g.id == *id));
        self.act = act;
        self.anim = anim;
        self.animating = animating;
        self.drop_index = ins;

        let mut rx = right_x + right_w;
        let clock_c = if pal.brackets { pal.accent } else { pal.text };
        for (hit, w) in right {
            let r = Rect::new(rx - w, y, w, slot);
            hover_bg(r, hit);
            match hit {
                Hit::Clock => {
                    s.text(&time, Rect::new(r.x, r.y + 2.0, r.w, half), 13.0, 600, clock_c, Align::Center);
                    s.text(&date, Rect::new(r.x, r.y + half - 2.0, r.w, half), 11.5, 400, pal.dim, Align::Center);
                }
                Hit::Sys => {
                    let a = format!("CPU {:.0}%", ctx.sys.cpu);
                    let b = format!("RAM {:.0}%", ctx.sys.ram);
                    s.text(&a, Rect::new(r.x, r.y + 2.0, r.w, half), 11.5, 500, pal.text, Align::Center);
                    s.text(&b, Rect::new(r.x, r.y + half - 2.0, r.w, half), 11.5, 500, pal.dim, Align::Center);
                }
                Hit::TrayIcon(id) => {
                    if let Some((_, Some(bmp))) = self.tray_bmps.get(&id) {
                        s.image(bmp, Rect::new(r.x + (r.w - 16.0) / 2.0, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0), 0.0);
                    }
                }
                Hit::Battery => {
                    if let Some(b) = battery {
                        let lvl = (b.percent as u32 / 10).min(10);
                        let g = match (b.charging, lvl) {
                            (true, 10) => '\u{E83E}',
                            (true, l) => char::from_u32(0xE85A + l).unwrap_or('\u{E83E}'),
                            (false, 10) => '\u{E83F}',
                            (false, l) => char::from_u32(0xE850 + l).unwrap_or('\u{E83F}'),
                        };
                        let low = !b.charging && b.percent <= 20;
                        let c = if low { Color::rgba(255, 99, 71, 255) } else { pal.text };
                        s.glyph(g, Rect::new(r.x + 4.0, r.y, 20.0, slot), 14.0, c);
                        s.text(&bat_text, Rect::new(r.x + 24.0, r.y, r.w - 26.0, slot), 12.0, 600, c, Align::Left);
                    }
                }
                Hit::Desktops => {
                    let (n, cur) = self.desktops;
                    let cy = r.y + slot / 2.0;
                    for i in 0..n {
                        let cx = r.x + 8.0 + i as f32 * DESK_DOT + DESK_DOT / 2.0;
                        if i == cur {
                            s.fill_round(Rect::new(cx - 5.0, cy - 3.0, 10.0, 6.0), 3.0, pal.accent);
                        } else {
                            s.fill_circle(cx, cy, 2.5, pal.dim);
                        }
                    }
                    s.glyph('\u{E710}', Rect::new(r.x + r.w - 18.0, r.y, 14.0, slot), 9.0, pal.dim);
                }
                Hit::TrayMore => s.glyph(if self.flyout.is_some() { '\u{E70D}' } else { '\u{E70E}' }, r, 11.0, pal.dim),
                Hit::Media => {
                    if let Some(label) = &media_label {
                        draw_media(r, label);
                    }
                }
                Hit::Weather => {
                    if let Some(wt) = weather {
                        draw_sky(s, wt.sky(), wt.is_day, r.x + 8.0 + icon / 2.0, r.y + slot / 2.0, icon * 1.3);
                        let tr = Rect::new(r.x + icon + 12.0, r.y, r.w - icon - 12.0, slot);
                        s.text(&temp, tr, 13.0, 600, pal.text, Align::Left);
                    }
                }
                _ => {}
            }
            hits.push((r, hit));
            rx -= w + 4.0;
        }

        s.end();
        self.hits = hits;
        let sc = self.surf.scale;
        self.terrain_top = walk_y.map(|y| self.y + (y * sc) as i32);
        self.panel = Some(RECT {
            left: self.x + (extent.x * sc) as i32,
            top: self.y + (extent.y * sc) as i32,
            right: self.x + ((extent.x + extent.w) * sc) as i32,
            bottom: self.y + ((extent.y + extent.h) * sc) as i32,
        });
    }

    pub fn monitor(&self) -> &Monitor {
        &self.mon
    }

    pub fn visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }

    pub fn app_count(&self) -> usize {
        self.groups.len()
    }

    pub fn last_app_x(&self) -> Option<f32> {
        let (r, _) = self.hits.iter().rev().find(|(_, h)| matches!(h, Hit::App(_)))?;
        Some(self.x as f32 + (r.x + r.w / 2.0) * self.surf.scale)
    }

    fn hit_at(&self, lp: LPARAM) -> Option<Hit> {
        let x = (lp.0 & 0xFFFF) as i16 as f32 / self.surf.scale;
        let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32 / self.surf.scale;
        self.hit_at_logical(x, y)
    }

    fn hit_at_logical(&self, x: f32, y: f32) -> Option<Hit> {
        self.hits.iter().find(|(r, _)| r.contains(x, y)).map(|(_, h)| *h)
    }

    fn group(&self, id: u64) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    fn show_preview(&mut self, id: u64) {
        let Some(pal) = self.pal.clone() else { return };
        let Some(g) = self.group(id).filter(|g| !g.wins.is_empty()) else { return };
        let Some((r, _)) = self.hits.iter().find(|(_, h)| *h == Hit::App(id)) else { return };
        let sc = self.surf.scale;
        let anchor = self.x + ((r.x + r.w / 2.0) * sc) as i32;
        let edge = if self.top { self.y + ((r.y + r.h + 6.0) * sc) as i32 } else { self.y + ((r.y - 6.0) * sc) as i32 };
        let wins = g.wins.clone();
        if let Some(p) = self.preview.as_mut() {
            p.retarget(&self.mon, self.top, id, wins, anchor, edge);
            unsafe {
                SetTimer(self.hwnd, TIMER_PREVIEW_ANIM, 8, None);
            }
            return;
        }
        if let Ok(p) = Preview::new(&pal, &self.mon, self.top, id, wins, anchor, edge) {
            droptarget::register(p.hwnd, self.hwnd);
            self.preview = Some(p);
            unsafe {
                SetTimer(self.hwnd, TIMER_PREVIEW_CHECK, 120, None);
            }
        }
    }

    fn surf_contains(&self, c: POINT) -> bool {
        c.y >= self.y + (self.overhang * self.surf.scale) as i32 && c.y < self.y + self.surf.h_px
    }

    fn hide_preview(&mut self) {
        self.preview_out = 0;
        if self.preview.take().is_some() {
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_PREVIEW_CHECK);
                let _ = KillTimer(self.hwnd, TIMER_PREVIEW_ANIM);
            }
            self.sig.clear();
        }
    }

    fn drag_tick(&mut self) -> bool {
        let mut c = POINT::default();
        let held = unsafe {
            let _ = GetCursorPos(&mut c);
            (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 || (GetAsyncKeyState(VK_RBUTTON.0 as i32) as u16 & 0x8000) != 0
        };
        let target = if !held {
            None
        } else if let Some(p) = self.preview.as_ref().filter(|p| p.contains_screen(c)) {
            p.window_at_screen(c).map(|h| (p.group, Some(h.0 as isize)))
        } else if self.surf_contains(c) {
            let sc = self.surf.scale;
            match self.hit_at_logical((c.x - self.x) as f32 / sc, (c.y - self.y) as f32 / sc) {
                Some(Hit::App(id)) => Some((id, None)),
                _ => None,
            }
        } else {
            None
        };
        let mut repaint = false;
        let hover = target.filter(|t| t.1.is_none()).map(|t| Hit::App(t.0));
        if hover != self.hover {
            self.hover = hover;
            repaint = true;
        }
        let Some(target) = target else {
            if !held || (!self.surf_contains(c) && !self.preview.as_ref().is_some_and(|p| p.contains_screen(c))) {
                self.drag_over = None;
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_DRAG);
                }
            }
            return repaint;
        };
        match &mut self.drag_over {
            Some((t, since, _)) if *t != target => {
                *t = target;
                *since = Instant::now();
                self.drag_over.as_mut().unwrap().2 = false;
            }
            None => self.drag_over = Some((target, Instant::now(), false)),
            _ => {}
        }
        let Some((_, since, fired)) = &mut self.drag_over else { return repaint };
        if *fired || since.elapsed().as_millis() < DRAG_DWELL_MS {
            return repaint;
        }
        *fired = true;
        match target {
            (_, Some(h)) => {
                bring(util::hwnd(h));
                self.hide_preview();
            }
            (id, None) => {
                let wins = self.group(id).map(|g| g.wins.clone()).unwrap_or_default();
                match wins.len() {
                    0 => {}
                    1 => {
                        self.hide_preview();
                        bring(wins[0]);
                    }
                    _ if self.preview.as_ref().is_some_and(|p| p.group == id) => {}
                    _ => self.show_preview(id),
                }
            }
        }
        true
    }

    fn check_preview(&mut self) -> bool {
        let Some(p) = &self.preview else { return false };
        let mut c = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut c);
        }
        if p.contains_screen(c) {
            self.preview_out = 0;
            return false;
        }
        let sc = self.surf.scale;
        let (lx, ly) = ((c.x - self.x) as f32 / sc, (c.y - self.y) as f32 / sc);
        // İkon ile önizleme arasındaki köprü: ikisini birlikte kapsayan dikdörtgen. İmleç ikondan
        // önizlemeye giderken çubuğun kenar boşluğundan da geçer; orada kapanmamalı.
        let bridge = self.hits.iter().find(|(_, h)| *h == Hit::App(p.group)).is_some_and(|(r, _)| {
            let (il, it) = (self.x + (r.x * sc) as i32, self.y + (r.y * sc) as i32);
            let (ir, ib) = (il + (r.w * sc) as i32, it + (r.h * sc) as i32);
            let (l, rr) = (p.rect.left.min(il), p.rect.right.max(ir));
            let (t, b) = (p.rect.top.min(it), p.rect.bottom.max(ib));
            c.x >= l && c.x < rr && c.y >= t && c.y < b
        });
        match self.hit_at_logical(lx, ly) {
            Some(Hit::App(id)) if id == p.group => {
                self.preview_out = 0;
                false
            }
            Some(Hit::App(id)) => {
                self.preview_out = 0;
                self.show_preview(id);
                true
            }
            _ if bridge => {
                self.preview_out = 0;
                false
            }
            // Dışarıda: hemen değil, ~0.35 sn sonra kapat (anlık kaymalar affedilsin)
            _ if self.preview_out < 2 => {
                self.preview_out += 1;
                false
            }
            _ => {
                self.hide_preview();
                true
            }
        }
    }

    fn drop_at(&mut self, id: u64, ins: usize) {
        let Some(from) = self.groups.iter().position(|g| g.id == id) else { return };
        let mut seq: Vec<usize> = (0..self.groups.len()).filter(|&i| i != from).collect();
        seq.insert(ins.min(seq.len()), from);
        let wins: Vec<isize> = seq.iter().flat_map(|&i| self.groups[i].wins.iter().map(|h| h.0 as isize)).collect();
        ORDER.with(|o| {
            let mut o = o.borrow_mut();
            let rest: Vec<isize> = o.iter().copied().filter(|h| !wins.contains(h)).collect();
            *o = wins.clone();
            o.extend(rest);
        });
        merge_order(seq.iter().map(|&i| group_key(&self.groups[i])).collect());
        let pinned: Vec<String> = seq.iter().filter_map(|&i| self.groups[i].pinned.clone()).collect();
        if pinned != self.pinned {
            self.pinned = pinned.clone();
            crate::config::set_pinned(&pinned);
        }
        self.refresh_windows();
    }

    pub fn activate_nth(&mut self, i: usize) {
        if let Some(id) = self.groups.get(i).map(|g| g.id) {
            self.click_app(id);
            self.sig.clear();
        }
    }

    pub fn has_app(&self, path: &str) -> bool {
        let k = app_key(path);
        self.groups.iter().any(|g| group_key(g) == k)
    }

    pub fn freeze_order(&self) {
        merge_order(self.groups.iter().map(group_key).collect());
    }

    fn toggle_flyout(&mut self) {
        if self.flyout.is_some() {
            self.close_flyout();
            return;
        }
        self.hide_preview();
        let Some(pal) = self.pal.clone() else { return };
        let Some((r, _)) = self.hits.iter().find(|(_, h)| *h == Hit::TrayMore) else { return };
        let sc = self.surf.scale;
        let anchor = self.x + ((r.x + r.w / 2.0) * sc) as i32;
        let edge = if self.top { self.y + ((r.y + r.h + 6.0) * sc) as i32 } else { self.y + ((r.y - 6.0) * sc) as i32 };
        if let Ok(f) = Flyout::new(&pal, self.mon, anchor, edge, self.top) {
            self.flyout = Some(f);
            self.flyout_pid = 0;
            unsafe {
                SetTimer(self.hwnd, TIMER_FLYOUT, 60, None);
            }
        }
        self.sig.clear();
    }

    fn close_flyout(&mut self) {
        if self.flyout.take().is_some() {
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_FLYOUT);
            }
            self.sig.clear();
        }
    }

    /// Panelin dışına tıklanınca kapanır; ok düğmesine, panelin kendisine ve sağ tıklanan
    /// simgenin açtığı menüye yapılan tıklamalar hariç
    fn check_flyout(&mut self) -> bool {
        let Some(f) = self.flyout.as_mut() else { return false };
        f.refresh();
        let down = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON].iter().any(|k| unsafe { GetAsyncKeyState(k.0 as i32) } as u16 & 0x8000 != 0);
        if !down {
            return false;
        }
        let mut c = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut c);
        }
        if f.contains(c) {
            return false;
        }
        let sc = self.surf.scale;
        let (lx, ly) = ((c.x - self.x) as f32 / sc, (c.y - self.y) as f32 / sc);
        if self.hit_at_logical(lx, ly) == Some(Hit::TrayMore) {
            return false;
        }
        unsafe {
            let under = WindowFromPoint(c);
            let root = GetAncestor(under, GA_ROOT);
            let mut cls = [0u16; 16];
            let n = GetClassNameW(root, &mut cls);
            if String::from_utf16_lossy(&cls[..n.max(0) as usize]) == "#32768" {
                return false;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(root, Some(&mut pid));
            if self.flyout_pid != 0 && pid == self.flyout_pid {
                return false;
            }
        }
        self.close_flyout();
        true
    }

    fn click_group(&mut self, id: u64) {
        let _ = unsafe { KillTimer(self.hwnd, TIMER_PREVIEW) };
        let n = self.group(id).map_or(0, |g| g.wins.len());
        if n > 1 {
            // Birden çok pencere: her zaman seçtir. Önizleme fareyle zaten açıldıysa açık kalsın
            // (tıklama onu kapatmasın); fare uzaklaşınca kendiliğinden kapanır.
            if !self.preview.as_ref().is_some_and(|p| p.group == id) {
                self.show_preview(id);
            }
            return;
        }
        self.hide_preview();
        self.click_app(id);
    }

    fn click_app(&mut self, id: u64) {
        let Some(g) = self.group(id) else { return };
        if g.wins.is_empty() {
            if let Some(p) = &g.pinned {
                unsafe {
                    ShellExecuteW(None, windows::core::w!("open"), &windows::core::HSTRING::from(resolve_app(p)), None, None, SW_SHOWNORMAL);
                }
            }
            return;
        }
        let fg = unsafe { GetForegroundWindow() };
        if g.wins.len() == 1 {
            activate(g.wins[0]);
        } else if let Some(i) = g.wins.iter().position(|h| *h == fg) {
            bring(g.wins[(i + 1) % g.wins.len()]);
        } else {
            bring(self.last_active.get(&id).copied().unwrap_or(g.wins[0]));
        }
    }

    pub fn handle(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<Out> {
        let mut out = Out { action: None, repaint: false };
        if self.flyout.as_ref().is_some_and(|f| f.hwnd == hwnd) {
            if !matches!(msg, WM_MOUSEMOVE | WM_LBUTTONUP | WM_RBUTTONUP) {
                return None;
            }
            if let Some(FlyoutOut::Click(id, right, pid)) = self.flyout.as_mut().and_then(|f| f.handle(msg, lp)) {
                let mut c = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut c);
                }
                tray::click(id, right, c);
                if right {
                    self.flyout_pid = pid;
                } else {
                    unsafe {
                        SetTimer(self.hwnd, TIMER_FLYOUT_CLOSE, GetDoubleClickTime() + 40, None);
                    }
                }
            }
            return Some(out);
        }
        if hwnd != self.hwnd {
            let p = self.preview.as_mut()?;
            if !matches!(msg, WM_PAINT | WM_MOUSEMOVE | WM_LBUTTONUP | WM_MBUTTONUP) {
                return None;
            }
            match p.handle(msg, lp) {
                Some(PreviewOut::Activate(h)) => {
                    bring(h);
                    self.hide_preview();
                    out.repaint = true;
                }
                Some(PreviewOut::Close(h)) => unsafe {
                    let _ = PostMessageW(h, WM_CLOSE, WPARAM(0), LPARAM(0));
                },
                None => {}
            }
            return Some(out);
        }
        match msg {
            WM_LBUTTONDOWN => {
                let (x, y) = (lp_x(lp, self.surf.scale), lp_y(lp, self.surf.scale));
                let slot = self.hits.iter().find(|(r, h)| matches!(h, Hit::App(_)) && r.contains(x, y));
                if let Some((r, Hit::App(id))) = slot {
                    self.press = Some((*id, x, x - r.x));
                    self.drag_x = x;
                    unsafe {
                        SetCapture(self.hwnd);
                    }
                }
            }
            WM_MOUSEMOVE if self.press.is_some() => {
                let x0 = self.press.map_or(0.0, |p| p.1);
                self.drag_x = lp_x(lp, self.surf.scale);
                if !self.dragging && (self.drag_x - x0).abs() > 6.0 {
                    self.dragging = true;
                    self.pending = None;
                    self.hide_preview();
                    unsafe {
                        let _ = KillTimer(self.hwnd, TIMER_PREVIEW);
                    }
                }
                if self.dragging {
                    self.sig.clear();
                    out.repaint = true;
                }
            }
            WM_CAPTURECHANGED => {
                if self.press.take().is_some() && self.dragging {
                    self.dragging = false;
                    self.sig.clear();
                    out.repaint = true;
                }
            }
            WM_TIMER if wp.0 == TIMER_FLYOUT => out.repaint = self.check_flyout(),
            WM_TIMER if wp.0 == TIMER_FLYOUT_CLOSE => {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_FLYOUT_CLOSE);
                }
                self.close_flyout();
                out.repaint = true;
            }
            WM_APP_DRAG => {
                unsafe {
                    SetTimer(self.hwnd, TIMER_DRAG, 80, None);
                }
                out.repaint = self.drag_tick();
            }
            WM_TIMER if wp.0 == TIMER_DRAG => out.repaint = self.drag_tick(),
            WM_TIMER if wp.0 == TIMER_CAR => {
                self.sig.clear();
                out.repaint = true;
            }
            WM_TIMER if wp.0 == TIMER_ANIM => {
                self.sig.clear();
                out.repaint = true;
            }
            WM_MOUSEMOVE => {
                if !self.tracking {
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: self.hwnd,
                        dwHoverTime: 0,
                    };
                    unsafe {
                        let _ = TrackMouseEvent(&mut tme);
                    }
                    self.tracking = true;
                }
                let h = self.hit_at(lp);
                if h != self.hover {
                    self.hover = h;
                    out.repaint = true;
                    match h {
                        Some(Hit::App(id)) if self.preview.as_ref().map(|p| p.group) != Some(id) => {
                            self.pending = Some(id);
                            let delay = if self.preview.is_some() { 60 } else { 450 };
                            unsafe {
                                SetTimer(self.hwnd, TIMER_PREVIEW, delay, None);
                            }
                        }
                        Some(Hit::App(_)) => {}
                        _ => {
                            self.pending = None;
                            unsafe {
                                let _ = KillTimer(self.hwnd, TIMER_PREVIEW);
                            }
                        }
                    }
                }
            }
            WM_MOUSELEAVE_ => {
                self.tracking = false;
                self.pending = None;
                if self.hover.take().is_some() {
                    out.repaint = true;
                }
            }
            WM_TIMER if wp.0 == TIMER_PREVIEW => {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_PREVIEW);
                }
                if let (Some(id), Some(Hit::App(h))) = (self.pending.take(), self.hover) {
                    if id == h {
                        self.show_preview(id);
                        out.repaint = true;
                    }
                }
            }
            WM_TIMER if wp.0 == TIMER_PREVIEW_CHECK => out.repaint = self.check_preview(),
            WM_TIMER if wp.0 == TIMER_PREVIEW_ANIM => {
                if !self.preview.as_mut().is_some_and(|p| p.tick()) {
                    unsafe {
                        let _ = KillTimer(self.hwnd, TIMER_PREVIEW_ANIM);
                    }
                }
            }
            WM_LBUTTONUP if self.press.is_some() => {
                let dragged = self.dragging;
                let pressed = self.press.take().map(|p| p.0);
                self.dragging = false;
                unsafe {
                    let _ = ReleaseCapture();
                }
                if let (true, Some(id)) = (dragged, pressed) {
                    self.drop_at(id, self.drop_index);
                }
                self.sig.clear();
                if let (false, Some(id), Some(Hit::App(h))) = (dragged, pressed, self.hit_at(lp)) {
                    if id == h {
                        self.pending = None;
                        self.click_group(id);
                    }
                }
                out.repaint = true;
            }
            WM_LBUTTONUP => match self.hit_at(lp) {
                Some(Hit::Start) if self.launcher => out.action = Some(Action::Launcher),
                Some(Hit::Start) => util::send_win(None),
                Some(Hit::App(id)) => {
                    self.pending = None;
                    self.click_group(id);
                    out.repaint = true;
                }
                Some(Hit::Media) => out.action = Some(Action::Media(MediaCmd::Toggle)),
                Some(Hit::Clock) => util::send_win(Some(VK_N)),
                Some(Hit::Sys) => out.action = Some(Action::Open("taskmgr")),
                Some(Hit::Battery) => out.action = Some(Action::Open("ms-settings:batterysaver")),
                Some(Hit::Weather) => out.action = Some(Action::Open("msnweather://forecast")),
                Some(Hit::TrayIcon(id)) => out.action = Some(Action::TrayClick(id, false)),
                Some(Hit::TrayMore) => {
                    self.toggle_flyout();
                    out.repaint = true;
                }
                Some(Hit::Desktops) => {
                    let x = lp_x(lp, self.surf.scale);
                    if let Some((r, _)) = self.hits.iter().find(|(_, h)| *h == Hit::Desktops) {
                        let (n, cur) = self.desktops;
                        let i = ((x - r.x - 8.0) / DESK_DOT).floor();
                        if i >= n as f32 {
                            crate::system::new_desktop();
                        } else {
                            let i = i.clamp(0.0, n as f32 - 1.0) as i32;
                            if i != cur as i32 {
                                crate::system::switch_desktop(i - cur as i32);
                            }
                        }
                    }
                }
                None => {}
            },
            WM_MBUTTONUP => {
                if let Some(Hit::App(id)) = self.hit_at(lp) {
                    if let Some(g) = self.group(id).filter(|g| g.wins.len() == 1) {
                        unsafe {
                            let _ = PostMessageW(g.wins[0], WM_CLOSE, WPARAM(0), LPARAM(0));
                        }
                    }
                }
            }
            WM_RBUTTONUP => {
                self.hide_preview();
                self.pending = None;
                match self.hit_at(lp) {
                    Some(Hit::Start) => util::send_win(Some(VK_X)),
                    Some(Hit::TrayIcon(id)) => out.action = Some(Action::TrayClick(id, true)),
                    Some(Hit::TrayMore) => {
                        self.toggle_flyout();
                        out.repaint = true;
                    }
                    Some(Hit::App(id)) => {
                        if let Some(p) = self.group(id).filter(|g| g.wins.is_empty()).and_then(|g| g.pinned.clone()) {
                            out.action = Some(Action::PinnedMenu(p));
                        } else if let Some(g) = self.group(id) {
                            let last = self.last_active.get(&id).copied().unwrap_or(g.wins[0]);
                            let mut wins = vec![last.0 as isize];
                            wins.extend(g.wins.iter().filter(|h| **h != last).map(|h| h.0 as isize));
                            out.action = Some(Action::AppMenu(wins));
                        }
                    }
                    _ => out.action = Some(Action::Menu { widget: None }),
                }
            }
            WM_APP_APPBAR => match wp.0 as u32 {
                ABN_POSCHANGED => out.action = Some(Action::Reposition),
                ABN_FULLSCREENAPP => unsafe {
                    let z = if lp.0 != 0 { HWND_BOTTOM } else { HWND_TOPMOST };
                    let _ = SetWindowPos(self.hwnd, z, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                },
                _ => {}
            },
            _ => return None,
        }
        Some(out)
    }
}

impl Drop for Bar {
    fn drop(&mut self) {
        self.preview = None;
        self.flyout = None;
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: self.hwnd,
            ..Default::default()
        };
        unsafe {
            if self.appbar {
                SHAppBarMessage(ABM_REMOVE, &mut abd);
            }
            droptarget::revoke(self.hwnd);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

thread_local! {
    static ORDER: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
    static KEYS: RefCell<HashMap<isize, String>> = RefCell::new(HashMap::new());
    static SAVED: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

fn key_of(h: HWND) -> String {
    KEYS.with(|k| k.borrow_mut().entry(h.0 as isize).or_insert_with(|| process_key(h)).clone())
}

fn order_path() -> std::path::PathBuf {
    crate::config::dir().join("order.txt")
}

fn saved_order() -> Vec<String> {
    SAVED.with(|s| {
        s.borrow_mut()
            .get_or_insert_with(|| {
                std::fs::read_to_string(order_path())
                    .map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
                    .unwrap_or_default()
            })
            .clone()
    })
}

fn version_dir(lower: &str) -> Option<(usize, usize)> {
    let i = lower.find("\\app-")?;
    let j = lower[i + 5..].find('\\')? + i + 5;
    lower[i + 5..j].chars().all(|c| c.is_ascii_digit() || c == '.').then_some((i, j))
}

pub fn app_key(p: &str) -> String {
    let lower = p.to_lowercase();
    match version_dir(&lower) {
        Some((i, j)) => format!("{}\\app-*{}", &lower[..i], &lower[j..]),
        None => lower,
    }
}

fn generic_host(p: &str) -> bool {
    let name = std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(
        name.as_str(),
        "applicationframehost.exe" | "java.exe" | "javaw.exe" | "python.exe" | "pythonw.exe" | "rundll32.exe" | "dllhost.exe" | "mmc.exe" | "wscript.exe"
    )
}

pub fn resolve_app(p: &str) -> String {
    if std::path::Path::new(p).exists() {
        return p.to_string();
    }
    let Some((i, j)) = version_dir(&p.to_lowercase()) else { return p.to_string() };
    let (root, rest) = (&p[..i], &p[j..]);
    let ver = |n: &str| n[4..].split('.').map(|x| x.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.to_lowercase().starts_with("app-") && std::path::Path::new(&format!("{root}\\{n}{rest}")).exists())
        .max_by_key(|n| ver(n))
        .map(|n| format!("{root}\\{n}{rest}"))
        .unwrap_or_else(|| p.to_string())
}

thread_local! {
    static FLASH: RefCell<std::collections::HashSet<isize>> = RefCell::new(std::collections::HashSet::new());
}

pub fn set_flash(h: HWND, on: bool) {
    FLASH.with(|f| {
        let mut f = f.borrow_mut();
        if on && unsafe { GetForegroundWindow() } != h {
            f.insert(h.0 as isize);
        } else {
            f.remove(&(h.0 as isize));
        }
    });
}

fn group_key(g: &Group) -> String {
    match (&g.pinned, g.wins.first()) {
        (Some(p), _) => app_key(p),
        (None, Some(h)) => key_of(*h),
        (None, None) => String::new(),
    }
}

fn merge_order(keys: Vec<String>) {
    let keys: Vec<String> = keys.into_iter().filter(|k| !k.is_empty() && !k.starts_with("hwnd:")).collect();
    let mut all = keys.clone();
    all.extend(saved_order().into_iter().filter(|k| !keys.contains(k)));
    save_order(all);
}

pub fn import_windows_pins(current: &[String], force: bool) -> Option<Vec<String>> {
    let marker = crate::config::dir().join("pins_imported");
    if !force && marker.exists() {
        return None;
    }
    let _ = std::fs::write(&marker, "1");
    let new: Vec<String> = crate::system::windows_pins().into_iter().filter(|p| !current.iter().any(|x| app_key(x) == app_key(p))).collect();
    if new.is_empty() {
        return None;
    }
    let saved: Vec<String> = saved_order();
    let mut keys: Vec<String> = new.iter().map(|p| app_key(p)).collect();
    if saved.is_empty() {
        keys.extend(current.iter().map(|p| app_key(p)));
    }
    merge_order(keys);
    let mut all = new;
    all.extend(current.iter().cloned());
    crate::config::set_pinned(&all);
    Some(all)
}

fn save_order(keys: Vec<String>) {
    let _ = std::fs::write(order_path(), keys.join("\n"));
    SAVED.with(|s| *s.borrow_mut() = Some(keys));
}

fn process_start(h: HWND) -> u64 {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return u64::MAX };
        let (mut c, mut e, mut k, mut u) = Default::default();
        let ok = GetProcessTimes(p, &mut c, &mut e, &mut k, &mut u).is_ok();
        let _ = CloseHandle(p);
        let c: FILETIME = c;
        if ok {
            ((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64
        } else {
            u64::MAX
        }
    }
}

struct GroundArt {
    strip: ID2D1Bitmap,
    snow: Option<ID2D1Bitmap>,
    caps: Option<ID2D1Bitmap>,
    props: Option<ID2D1Bitmap>,
    night: Option<ID2D1Bitmap>,
    meta: GroundMeta,
    strip_size: (f32, f32),
}

impl GroundArt {
    fn load(s: &Surface) -> Option<GroundArt> {
        let meta = sprites::ground_meta()?;
        let strip = s.bitmap_from_bytes(sprites::GROUND_STRIP, 4096)?;
        let size = unsafe { strip.GetPixelSize() };
        Some(GroundArt {
            strip_size: (size.width as f32, size.height as f32),
            strip,
            snow: s.bitmap_from_bytes(sprites::GROUND_SNOW, 4096),
            caps: s.bitmap_from_bytes(sprites::GROUND_CAPS, 4096),
            props: s.bitmap_from_bytes(sprites::GROUND_PROPS, 4096),
            night: s.bitmap_from_bytes(sprites::GROUND_NIGHT, 4096),
            meta,
        })
    }
}

fn draw_terrain(s: &Surface, art: &GroundArt, panels: &[Rect], w: f32, h: f32, ctx: &Ctx, seed: i32) -> Option<f32> {
    let k = 0.5;
    let sky = ctx.weather.map(|w| w.sky());
    let snowy = sky == Some(Sky::Snow);
    let rainy = matches!(sky, Some(Sky::Rain | Sky::Drizzle | Sky::Storm));
    let night = !(6..20).contains(&ctx.now.hour);
    let (sw, sh) = art.strip_size;
    let gh = sh * k;
    let gy = h - gh;
    let walk_y = gy + gh * 0.3;

    let mut spans = Vec::new();
    let mut sorted: Vec<Rect> = panels.to_vec();
    sorted.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut x = 0.0;
    for p in &sorted {
        spans.push((x, p.x - 8.0));
        x = p.x + p.w + 8.0;
    }
    spans.push((x, w));
    let strip = if snowy { art.snow.as_ref().unwrap_or(&art.strip) } else { &art.strip };
    let mut any = false;
    for (si, &(a, b)) in spans.iter().enumerate() {
        if b - a < 50.0 {
            continue;
        }
        any = true;
        let mut x0 = a;
        let mut x1 = b;
        if let (Some(caps), [left, right, ..]) = (&art.caps, art.meta.ground_end_caps.as_slice()) {
            let frac = 0.32;
            if a > 1.0 {
                let src = Rect::new(left.x, left.y, left.w * frac, left.h);
                let dst = Rect::new(a, h - left.h * k, src.w * k, left.h * k);
                s.sprite(caps, src, dst, false, 1.0);
                x0 = a + dst.w;
            }
            if b < w - 1.0 {
                let src = Rect::new(right.x + right.w * (1.0 - frac), right.y, right.w * frac, right.h);
                let dst = Rect::new(b - src.w * k, h - right.h * k, src.w * k, right.h * k);
                s.sprite(caps, src, dst, false, 1.0);
                x1 = b - dst.w;
            }
        }
        let tw = sw * k;
        let mut tx = x0;
        let mut off = ((x0 as i32 + seed).rem_euclid(sw as i32)) as f32;
        while tx < x1 - 0.5 {
            let part = (tw - off * k).min(x1 - tx);
            let src = Rect::new(off, 0.0, part / k, sh);
            s.sprite(strip, src, Rect::new(tx, gy, part, gh), false, 1.0);
            tx += part;
            off = 0.0;
        }
        let mut rng = (seed as u32 ^ (si as u32 + 1).wrapping_mul(2654435761)) | 1;
        let mut next = |m: u32| {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            rng % m
        };
        let mut px = x0 + 20.0 + next(60) as f32;
        let mut first = true;
        while px < x1 - 30.0 {
            let pick = next(100);
            let (bmp, part) = if night && pick < 30 {
                let i = sprites::NIGHT_FIRST_USABLE + next(3) as usize;
                (art.night.as_ref(), art.meta.night_props.get(i))
            } else {
                let i = if first && si == 0 {
                    sprites::PROP_SIGN
                } else if rainy && pick < 45 {
                    sprites::PROP_PUDDLE
                } else {
                    next(9) as usize
                };
                (art.props.as_ref(), art.meta.props.get(i))
            };
            first = false;
            if let (Some(bmp), Some(p)) = (bmp, part) {
                let dst = Rect::new(px, walk_y + 3.0 - p.h * k, p.w * k, p.h * k);
                if dst.x + dst.w < x1 {
                    s.sprite(bmp, Rect::new(p.x, p.y, p.w, p.h), dst, false, 1.0);
                }
                px += p.w * k;
            }
            px += 60.0 + next(140) as f32;
        }
        if night {
            for _ in 0..((x1 - x0) / 120.0) as u32 {
                let fx = x0 + next((x1 - x0).max(1.0) as u32) as f32;
                let fy = walk_y - 6.0 - next(14) as f32;
                let c = Color::rgba(255, 236, 140, 255);
                s.fill_circle(fx, fy, 3.5, c.alpha(0.18));
                s.fill_circle(fx, fy, 1.4, c);
            }
        }
    }
    any.then_some(walk_y)
}

fn lp_x(lp: LPARAM, scale: f32) -> f32 {
    (lp.0 & 0xFFFF) as i16 as f32 / scale
}

fn lp_y(lp: LPARAM, scale: f32) -> f32 {
    ((lp.0 >> 16) & 0xFFFF) as i16 as f32 / scale
}

fn hash(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn process_key(h: HWND) -> String {
    match util::process_path(h).map(|p| app_key(&p)) {
        Some(p) if !p.ends_with("\\applicationframehost.exe") => p,
        _ => format!("hwnd:{:x}", h.0 as isize),
    }
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn is_task_window(h: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(h).as_bool() || GetWindowTextLengthW(h) == 0 {
            return false;
        }
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
        let app_window = ex & WS_EX_APPWINDOW.0 != 0;
        if ex & WS_EX_TOOLWINDOW.0 != 0 && !app_window {
            return false;
        }
        if ex & WS_EX_TRANSPARENT.0 != 0 && ex & WS_EX_LAYERED.0 != 0 {
            return false;
        }
        if ex & WS_EX_NOACTIVATE.0 != 0 && !app_window {
            return false;
        }
        if !app_window && GetWindow(h, GW_OWNER).map(|o| !o.is_invalid()).unwrap_or(false) {
            return false;
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut _ as _, 4);
        if cloaked != 0 {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        // Kendi pencerelerimizden yalnızca Ayarlar uygulaması görev çubuğunda görünür
        if pid == GetCurrentProcessId() && GetWindowLongPtrW(h, GWLP_USERDATA) != crate::settings::SETTINGS_WND {
            return false;
        }
        !matches!(class_name(h).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
    }
}

unsafe extern "system" fn enum_proc(h: HWND, lp: LPARAM) -> BOOL {
    let v = &mut *(lp.0 as *mut Vec<HWND>);
    if is_task_window(h) {
        v.push(h);
    }
    TRUE
}

fn activate(h: HWND) {
    unsafe {
        if GetForegroundWindow() == h && !IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_MINIMIZE);
            return;
        }
    }
    bring(h);
}

pub fn bring(h: HWND) {
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        if !SetForegroundWindow(h).as_bool() {
            let alt = |up: bool| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_MENU,
                        dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                        ..Default::default()
                    },
                },
            };
            SendInput(&[alt(false), alt(true)], std::mem::size_of::<INPUT>() as i32);
            let _ = SetForegroundWindow(h);
        }
    }
}
