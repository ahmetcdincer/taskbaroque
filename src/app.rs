use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use windows::core::{w, Result, HSTRING};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use std::path::Path;

use crate::bar::{self, Bar};
use crate::menu::{self, Entry};
use crate::preview::window_title;
use crate::config::{self, Config, Palette, WeatherCfg};
use crate::feeds::{self, FeedCfg, FeedData, Feeds};
use crate::i18n::{self, tr};
use crate::media::{self, MediaCmd, MediaInfo};
use crate::monitors::{self, Monitor};
use crate::notch::{self, Notch};
use crate::notify;
use crate::settings::{Settings, SettingsOut, SETTINGS_WND};
use crate::launcher::{self, Launcher, LauncherOut, LAUNCHER_WND};
use crate::pet::{Ground, Pet};
use crate::sysinfo::SysInfo;
use crate::util::{self, Time, WM_APP_DATA, WM_APP_REFRESH};
use crate::weather::{self, Weather};
use crate::widgets::{Widget, DESKTOP_LAYER};
use crate::{gfx, taskbar, tray};

const CLASS: windows::core::PCWSTR = w!("TaskbaroqueWnd");
const WM_APP_REBUILD: u32 = WM_APP + 4;
const WM_APP_TRAY: u32 = WM_APP + 5;
const TIMER_TICK: usize = 1;
const TIMER_REBUILD: usize = 2;
const RECENT_BASE: u32 = 200;

static HOST: AtomicIsize = AtomicIsize::new(0);

/// Ana (gizli) pencere; arka plan işleri bitince buraya mesaj atar.
pub fn host() -> isize {
    HOST.load(Ordering::SeqCst)
}
static REFRESH_PENDING: AtomicBool = AtomicBool::new(false);

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

pub struct Ctx<'a> {
    pub cfg: &'a Config,
    pub pal: Palette,
    pub sys: &'a SysInfo,
    pub media: &'a MediaInfo,
    pub weather: Option<&'a Weather>,
    pub now: Time,
    pub feeds: &'a Feeds,
}

pub enum Action {
    Media(MediaCmd),
    Menu { widget: Option<usize> },
    NotchMenu,
    PetMenu,
    AppMenu(Vec<isize>),
    TrayClick(u64, bool),
    Open(&'static str),
    SavePos(usize),
    Reposition,
    Focus(Option<u32>),
    PinnedMenu(String),
    Launcher,
    OpenUrl(String),
    ClearHistory,
    PickColor,
}

struct App {
    host: HWND,
    cfg: Config,
    cfg_mtime: Option<SystemTime>,
    monitors: Vec<Monitor>,
    bars: Vec<Bar>,
    widgets: Vec<Widget>,
    pet: Option<Pet>,
    settings: Option<Settings>,
    launcher: Option<Launcher>,
    launcher_closed: Option<std::time::Instant>,
    notch: Option<Notch>,
    app_count: usize,
    sys: SysInfo,
    media_state: Arc<Mutex<MediaInfo>>,
    media: MediaInfo,
    media_tx: Sender<MediaCmd>,
    weather_state: Arc<Mutex<Option<Weather>>>,
    weather: Option<Weather>,
    weather_tx: Sender<WeatherCfg>,
    taskbar_created: u32,
    feeds: Feeds,
    feed_state: Arc<Mutex<FeedData>>,
    feed_tx: Sender<FeedCfg>,
    reminded: Vec<(i64, String)>,
    battery_alert: u8,
    slow_tick: u32,
    shellhook: u32,
    tracker: crate::screentime::Tracker,
    picker: Option<crate::picker::Picker>,
    bt_state: Arc<Mutex<Vec<crate::devices::BtDevice>>>,
    bt_alerted: Vec<String>,
}

macro_rules! ctx {
    ($s:expr) => {
        Ctx {
            cfg: &$s.cfg,
            pal: $s.cfg.palette(),
            sys: &$s.sys,
            media: &$s.media,
            weather: $s.weather.as_ref(),
            now: util::now(),
            feeds: &$s.feeds,
        }
    };
}

impl App {
    fn build(&mut self) {
        self.monitors = monitors::list();
        crate::hotkeys::configure(self.cfg.bar.enabled && self.cfg.bar.hotkeys, self.cfg.bar.start_menu != "windows");

        if tray::set_enabled(self.cfg.bar.enabled && self.cfg.bar.show_tray, self.host.0 as isize) {
            tray_icon(self.host, true);
        }
        if self.cfg.bar.enabled {
            taskbar::hide();
            let all = self.cfg.bar.all_monitors();
            let per_mon = self.cfg.bar.windows_per_monitor && all && self.monitors.len() > 1;
            let ctx = ctx!(self);
            let mut bars = Vec::new();
            for m in self.monitors.iter().filter(|m| all || m.primary) {
                match Bar::new(&ctx, *m, per_mon) {
                    Ok(b) => bars.push(b),
                    Err(e) => util::message_box(&if i18n::en() { format!("Could not create the taskbar: {e}") } else { format!("Görev çubuğu oluşturulamadı: {e}") }),
                }
            }
            self.bars = bars;
        } else {
            taskbar::restore();
        }

        let ctx = ctx!(self);
        let widgets: Vec<Widget> = self
            .cfg
            .widgets
            .iter()
            .enumerate()
            .filter_map(|(i, c)| Widget::new(i, c, &ctx, &self.monitors).ok().flatten())
            .collect();
        self.widgets = widgets;
        self.paint_all(true);
        if self.settings.is_some() {
            let n = self.monitors.len();
            let ctx = ctx!(self);
            if let Some(st) = self.settings.as_mut() {
                st.set_monitors(n);
                st.draw(&ctx);
            }
        }

        self.notch = None;
        if !self.monitors.is_empty() {
            let i = self.cfg.notch.monitor.clamp(1, self.monitors.len()) - 1;
            let ctx = ctx!(self);
            let notch = Notch::new(&ctx, self.monitors[i], &self.cfg.notch.mode, &self.cfg.notch.edge, self.cfg.notch.offset, self.cfg.notch.locked).ok();
            self.notch = notch;
        }

        self.pet = None;
        self.app_count = self.bars.first().map(|b| b.app_count()).unwrap_or(0);
        if self.cfg.pet.enabled && !self.monitors.is_empty() {
            let i = self.cfg.pet.monitor.clamp(1, self.monitors.len()) - 1;
            let m = self.monitors[i];
            let mid = (m.rect.left + m.rect.right) as f32 / 2.0;
            if let Some((g, m)) = self.pet_ground(mid) {
                let ctx = ctx!(self);
                let pet = Pet::new(&self.cfg.pet, &ctx, &m, g).ok();
                self.pet = pet;
            }
        }
    }

    fn refresh_wallpaper_accent(&mut self) {
        let c = if self.cfg.general.wallpaper_accent { crate::system::wallpaper_accent() } else { None };
        *config::WALL_ACCENT.lock().unwrap() = c;
    }

    fn feed_cfg(cfg: &Config) -> FeedCfg {
        let gh_widget = cfg.widgets.iter().any(|w| w.enabled && w.kind == "github");
        FeedCfg { ics: cfg.calendar.ics.clone(), github: cfg.github.enabled || gh_widget }
    }

    fn set_focus(&mut self, on: bool, minutes: Option<u32>) {
        self.feeds.focus = on;
        self.feeds.focus_until = minutes.filter(|_| on).map(|m| std::time::Instant::now() + std::time::Duration::from_secs(m as u64 * 60));
        feeds::set_windows_dnd(on);
        self.paint_all(true);
        if let Some(n) = self.notch.as_mut() {
            n.wake();
        }
    }

    fn notify(&mut self, n: notify::Notice) {
        self.feeds.push_history(&n.title, &n.message, n.target);
        if self.feeds.focus || self.feeds.quiet {
            return;
        }
        self.show_notice(n);
    }

    fn show_notice(&mut self, n: notify::Notice) {
        if n.sound {
            crate::sound::notify(&self.cfg.sound, &n.kind);
        }
        if let Some(nt) = self.notch.as_mut() {
            nt.alert(&n.title, &n.message, n.target);
        }
        if let Some(p) = self.pet.as_mut() {
            p.alert();
        }
    }

    fn pet_ground(&self, x: f32) -> Option<(Ground, Monitor)> {
        let m = *self
            .monitors
            .iter()
            .find(|m| x >= m.rect.left as f32 && x < m.rect.right as f32)
            .or(self.monitors.first())?;
        let bar = self.bars.iter().find(|b| b.monitor().hmon == m.hmon && b.visible());
        let terrain = bar.and_then(|b| b.terrain_top);
        let floor = terrain.map_or(m.rect.bottom as f32 - 1.0, |y| y as f32);
        let g = match bar.and_then(|b| b.panel) {
            Some(p) => {
                let inset = 16.0 * m.scale;
                Ground {
                    left: p.left as f32 + inset,
                    right: p.right as f32 - inset,
                    y: p.top as f32 + m.scale,
                    floor,
                    min_x: m.rect.left as f32,
                    max_x: m.rect.right as f32,
                    wide: terrain.is_some(),
                }
            }
            None => Ground {
                left: m.work.left as f32,
                right: m.work.right as f32,
                y: m.work.bottom as f32,
                floor: m.work.bottom as f32,
                min_x: m.rect.left as f32,
                max_x: m.rect.right as f32,
                wide: false,
            },
        };
        Some((g, m))
    }

    fn rebuild(&mut self) {
        self.pet = None;
        self.notch = None;
        self.bars.clear();
        self.widgets.clear();
        self.build();
    }

    fn reload(&mut self) {
        self.cfg_mtime = config::mtime();
        match config::load() {
            Ok(c) => {
                i18n::set(&c.general.language);
                if c.weather != self.cfg.weather {
                    let _ = self.weather_tx.send(c.weather.clone());
                }
                if c.calendar != self.cfg.calendar || c.github != self.cfg.github {
                    let _ = self.feed_tx.send(Self::feed_cfg(&c));
                }
                self.cfg = c;
                self.refresh_wallpaper_accent();
                self.rebuild();
            }
            Err(e) => util::message_box(&e),
        }
    }

    fn paint_all(&mut self, force: bool) {
        let ctx = ctx!(self);
        for b in &mut self.bars {
            b.paint(&ctx, force);
        }
        if self.feeds.quiet {
            return;
        }
        for w in &mut self.widgets {
            w.paint(&ctx, force);
        }
    }

    fn tick_feeds(&mut self) {
        self.slow_tick = self.slow_tick.wrapping_add(1);
        if self.slow_tick % 5 == 1 {
            self.feeds.online = feeds::online();
            self.feeds.battery = feeds::battery();
        }
        if let Some(b) = self.feeds.battery {
            let level = if b.charging || b.percent > 20 { 0 } else if b.percent > 10 { 20 } else { 10 };
            if b.charging || b.percent > 25 {
                self.battery_alert = 0;
            } else if level != 0 && (self.battery_alert == 0 || level < self.battery_alert) {
                self.battery_alert = level;
                let msg = if i18n::en() { format!("{}% left, plug in the charger", b.percent) } else { format!("%{} kaldı, şarja tak", b.percent) };
                self.notify(notify::Notice { title: tr("Pil azaldı").into(), message: msg, sound: true, target: 0, kind: String::new() });
            }
        }
        if self.feeds.focus_until.is_some_and(|t| std::time::Instant::now() >= t) {
            self.set_focus(false, None);
            self.show_notice(notify::Notice { title: tr("Odak bitti").into(), message: tr("Bildirimler yeniden açık").into(), sound: true, target: 0, kind: String::new() });
        }
        let now = feeds::local_now();
        let soon: Vec<(i64, String)> = self
            .feeds
            .data
            .events
            .iter()
            .filter(|e| !e.all_day && e.start > now && e.start - now <= 5 * 60)
            .map(|e| (e.start, e.title.clone()))
            .filter(|k| !self.reminded.contains(k))
            .collect();
        for (start, title) in soon {
            self.reminded.push((start, title.clone()));
            let mins = ((start - now) as f32 / 60.0).ceil() as i64;
            let msg = if i18n::en() { format!("Starts in {mins} min") } else { format!("{mins} dk sonra başlıyor") };
            self.notify(notify::Notice { title, message: msg, sound: true, target: 0, kind: String::new() });
        }
        self.reminded.retain(|(s, _)| *s > now - 3600);
        let quiet = util::fullscreen_busy() || self.bars.iter().any(|b| b.fullscreen());
        if quiet != self.feeds.quiet {
            self.feeds.quiet = quiet;
            if let Some(p) = self.pet.as_mut() {
                p.set_visible(!quiet);
            }
        }
    }

    fn start_picker(&mut self) {
        if self.picker.is_some() {
            return;
        }
        let c = cursor();
        let Some(mon) = self.monitors.iter().find(|m| c.x >= m.rect.left && c.x < m.rect.right && c.y >= m.rect.top && c.y < m.rect.bottom).or(self.monitors.first()).copied() else { return };
        if let Ok(p) = crate::picker::Picker::new(&mon, &self.cfg.palette().font) {
            self.picker = Some(p);
            crate::hotkeys::set_picking(true);
        }
    }

    fn bt_arrived(&mut self) {
        let bt = self.bt_state.lock().unwrap().clone();
        for d in &bt {
            let low = d.percent <= 15;
            let known = self.bt_alerted.contains(&d.name);
            if low && !known {
                self.bt_alerted.push(d.name.clone());
                let msg = if i18n::en() { format!("{} is at {}%", d.name, d.percent) } else { format!("{} pili %{}", d.name, d.percent) };
                self.notify(notify::Notice { title: tr("Cihaz pili azaldı").into(), message: msg, sound: true, target: 0, kind: String::new() });
            } else if !low && d.percent > 25 && known {
                self.bt_alerted.retain(|n| *n != d.name);
            }
        }
        self.feeds.bt = bt;
    }

    fn tick_screen(&mut self) {
        self.tracker.tick(self.media.playing);
        if self.slow_tick % 5 == 0 || self.feeds.screen_top.is_empty() {
            self.feeds.screen_total = self.tracker.total();
            self.feeds.screen_top = self.tracker.top(5);
        }
        let now = util::now();
        let day = self.tracker.day();
        if now.hour >= 21 && self.tracker.summary_day != day {
            self.tracker.summary_day = day;
            self.tracker.save();
            let total = self.tracker.total();
            if total >= 30 * 60 {
                let top = self.tracker.top(1);
                let most = top.first().map(|u| format!(" · {} {} ({})", tr("En çok:"), u.name, crate::screentime::fmt_dur(u.secs))).unwrap_or_default();
                let msg = if i18n::en() {
                    format!("{} at the screen today{most}", crate::screentime::fmt_dur(total))
                } else {
                    format!("Bugün {} ekran başındaydın{most}", crate::screentime::fmt_dur(total))
                };
                self.notify(notify::Notice { title: tr("Günün özeti").into(), message: msg, sound: false, target: 0, kind: String::new() });
            }
        }
        let d = crate::system::virtual_desktops();
        if d != self.feeds.desktops {
            self.feeds.desktops = d;
            for b in &mut self.bars {
                b.refresh_windows();
            }
        }
    }

    fn feeds_arrived(&mut self) {
        let data = self.feed_state.lock().unwrap().clone();
        let mut changes = Vec::new();
        for pr in &data.prs {
            if let Some(old) = self.feeds.data.prs.iter().find(|o| o.url == pr.url) {
                if old.ci != pr.ci && matches!(pr.ci.as_str(), "SUCCESS" | "FAILURE" | "ERROR") {
                    changes.push(pr.clone());
                }
            }
        }
        self.feeds.data = data;
        for pr in changes {
            let ok = pr.ci == "SUCCESS";
            let title = if ok { tr("CI geçti") } else { tr("CI başarısız") };
            self.notify(notify::Notice { title: title.into(), message: format!("{} #{} · {}", pr.repo, pr.number, pr.title), sound: !ok, target: 0, kind: String::new() });
        }
    }

    fn tick(&mut self) {
        self.tick_feeds();
        self.tick_screen();
        let need_net = self.notch.is_some() || self.cfg.widgets.iter().any(|w| w.enabled && w.kind == "sysmon");
        self.sys.update(need_net);
        taskbar::enforce();
        tray::tick();
        crate::taskband::apply();
        if let Some(x) = self.pet.as_ref().map(|p| p.x()) {
            let mon = self.pet_ground(x).map(|(_, m)| m);
            let bar = mon.and_then(|m| self.bars.iter().find(|b| b.monitor().hmon == m.hmon));
            let visible = bar.map_or(true, |b| b.visible());
            let count = self.bars.first().map(|b| b.app_count()).unwrap_or(0);
            let target = (count > self.app_count).then(|| bar.and_then(|b| b.last_app_x())).flatten();
            self.app_count = count;
            let quiet = self.feeds.quiet;
            if let Some(p) = self.pet.as_mut() {
                p.set_visible(visible && !quiet);
                p.keep_on_top();
                if let Some(t) = target {
                    p.go_to(t);
                }
            }
        }
        if config::mtime() != self.cfg_mtime {
            self.reload();
            return;
        }
        for b in &mut self.bars {
            b.refresh_windows();
        }
        self.paint_all(false);
    }

    fn act(&mut self, _owner: HWND, a: Action) {
        match a {
            Action::Media(c) => {
                let _ = self.media_tx.send(c);
            }
            Action::Menu { widget } => self.menu(widget, false),
            Action::PetMenu => self.menu(None, true),
            Action::NotchMenu => self.notch_menu(),
            Action::AppMenu(wins) => self.app_menu(wins),
            Action::TrayClick(id, right) => tray::click(id, right, cursor()),
            Action::Open(target) => unsafe {
                ShellExecuteW(None, w!("open"), &HSTRING::from(target), None, None, SW_SHOWNORMAL);
            },
            Action::SavePos(i) => {
                let Some(wg) = self.widgets.iter().find(|w| w.index == i) else { return };
                let mon_idx = monitors::index_of(&self.monitors, wg.hwnd);
                let mon = self.monitors[mon_idx];
                let (x, y) = wg.pos();
                let (lx, ly) = ((x - mon.rect.left) as f32 / mon.scale, (y - mon.rect.top) as f32 / mon.scale);
                let moved_screen = mon_idx != wg.monitor;
                if let Some(c) = self.cfg.widgets.get_mut(i) {
                    c.monitor = mon_idx + 1;
                    c.x = lx;
                    c.y = ly;
                }
                config::save_widget_pos(i, mon_idx + 1, lx, ly);
                self.cfg_mtime = config::mtime();
                if moved_screen || (wg.scale() - mon.scale).abs() > 0.01 {
                    self.rebuild();
                }
            }
            Action::Focus(m) => {
                let on = m.is_some() || !self.feeds.focus;
                self.set_focus(on, m);
            }
            Action::PinnedMenu(p) => self.pinned_menu(p),
            Action::PickColor => self.start_picker(),
            Action::ClearHistory => self.feeds.history.clear(),
            Action::Launcher => self.toggle_launcher(),
            Action::OpenUrl(u) => unsafe {
                ShellExecuteW(None, w!("open"), &HSTRING::from(u), None, None, SW_SHOWNORMAL);
            },
            Action::Reposition => {
                let ctx = ctx!(self);
                for b in &mut self.bars {
                    b.reposition(&ctx);
                    b.paint(&ctx, true);
                }
            }
        }
    }

    fn popup(&self, entries: Vec<Entry>) -> Option<u32> {
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        let mon = self
            .monitors
            .iter()
            .find(|m| p.x >= m.rect.left && p.x < m.rect.right && p.y >= m.rect.top && p.y < m.rect.bottom)
            .or(self.monitors.first())
            .copied()?;
        let above = p.y > (mon.rect.top + mon.rect.bottom) / 2;
        menu::popup(&self.cfg.palette(), mon, entries, p, above)
    }

    fn app_menu(&mut self, wins: Vec<isize>) {
        const NEW: u32 = 1;
        const LOCATION: u32 = 2;
        const MIN: u32 = 3;
        const MAX: u32 = 4;
        const RESTORE: u32 = 5;
        const CLOSE: u32 = 6;
        const MIN_ALL: u32 = 7;
        const HIDE_APP: u32 = 8;
        const PIN: u32 = 9;
        const END_TASK: u32 = 10;
        const WIN_BASE: u32 = 100;
        let Some(&first) = wins.first() else { return };
        let h = util::hwnd(first);
        let path = util::process_path(h);
        let name = path
            .as_deref()
            .and_then(util::file_description)
            .or_else(|| path.as_deref().and_then(|p| Path::new(p).file_stem()).map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| window_title(h));

        let mut e = vec![Entry::Header(name)];
        if wins.len() > 1 {
            for (i, w) in wins.iter().enumerate() {
                e.push(Entry::item(WIN_BASE + i as u32, window_title(util::hwnd(*w)), '\u{E737}'));
            }
            e.push(Entry::Sep);
        }
        let pinned = path.as_deref().is_some_and(|p| self.cfg.bar.pinned.iter().any(|x| crate::bar::app_key(x) == crate::bar::app_key(p)));
        let recent = path.as_deref().map(|p| Self::recent_menu(p, &mut e)).unwrap_or_default();
        if path.is_some() {
            e.push(Entry::item(NEW, tr("Yeni pencere"), '\u{E710}'));
            e.push(Entry::item(PIN, if pinned { tr("Sabitlemeyi kaldır") } else { tr("Görev çubuğuna sabitle") }, if pinned { '\u{E77A}' } else { '\u{E718}' }));
            e.push(Entry::item(LOCATION, tr("Dosya konumunu aç"), '\u{E838}'));
            e.push(Entry::item(HIDE_APP, tr("Görev çubuğunda gösterme"), '\u{ED1A}'));
            e.push(Entry::Sep);
        }
        let (iconic, zoomed) = unsafe { (IsIconic(h).as_bool(), IsZoomed(h).as_bool()) };
        if wins.len() == 1 {
            if iconic || zoomed {
                e.push(Entry::item(RESTORE, tr("Geri yükle"), '\u{E923}'));
            }
            if !iconic {
                e.push(Entry::item(MIN, tr("Simge durumuna küçült"), '\u{E921}'));
            }
            if !zoomed {
                e.push(Entry::item(MAX, tr("Ekranı kapla"), '\u{E922}'));
            }
            e.push(Entry::Sep);
            e.push(Entry::item(CLOSE, tr("Pencereyi kapat"), '\u{E8BB}'));
        } else {
            e.push(Entry::item(MIN_ALL, tr("Tümünü küçült"), '\u{E921}'));
            e.push(Entry::Sep);
            e.push(Entry::item(CLOSE, if i18n::en() { format!("Close all windows ({})", wins.len()) } else { format!("Tüm pencereleri kapat ({})", wins.len()) }, '\u{E8BB}'));
        }
        let shell = path.as_deref().is_some_and(|p| Path::new(p).file_name().is_some_and(|n| n.eq_ignore_ascii_case("explorer.exe")));
        if !shell {
            e.push(Entry::item(END_TASK, tr("Görevi sonlandır"), '\u{E894}'));
        }

        let Some(cmd) = self.popup(e) else { return };
        if let Some(f) = cmd.checked_sub(RECENT_BASE).and_then(|i| recent.get(i as usize)) {
            util::open_path(Path::new(f));
            return;
        }
        unsafe {
            match cmd {
                NEW => {
                    if let Some(p) = &path {
                        ShellExecuteW(None, w!("open"), &HSTRING::from(p.as_str()), None, None, SW_SHOWNORMAL);
                    }
                }
                LOCATION => {
                    if let Some(p) = &path {
                        let args = HSTRING::from(format!("/select,\"{p}\""));
                        ShellExecuteW(None, w!("open"), w!("explorer.exe"), &args, None, SW_SHOWNORMAL);
                    }
                }
                PIN => {
                    if let Some(p) = &path {
                        self.toggle_pin(p);
                    }
                }
                HIDE_APP => {
                    if let Some(exe) = path.as_deref().and_then(|p| Path::new(p).file_name()) {
                        config::hide_app(&exe.to_string_lossy());
                        self.reload();
                    }
                }
                MIN | MIN_ALL => {
                    for w in &wins {
                        let _ = ShowWindow(util::hwnd(*w), SW_MINIMIZE);
                    }
                }
                MAX => {
                    let _ = ShowWindow(h, SW_MAXIMIZE);
                    bar::bring(h);
                }
                RESTORE => {
                    let _ = ShowWindow(h, SW_RESTORE);
                    bar::bring(h);
                }
                END_TASK => {
                    util::end_task(&wins.iter().map(|w| util::hwnd(*w)).collect::<Vec<_>>());
                }
                CLOSE => {
                    for w in &wins {
                        let _ = PostMessageW(util::hwnd(*w), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                }
                i if (WIN_BASE..WIN_BASE + wins.len() as u32).contains(&i) => {
                    bar::bring(util::hwnd(wins[(i - WIN_BASE) as usize]));
                }
                _ => {}
            }
        }
    }

    fn recent_menu(exe: &str, e: &mut Vec<Entry>) -> Vec<String> {
        let files = crate::system::recent_files(exe, 8);
        if !files.is_empty() {
            let items = files
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let name = Path::new(f).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| f.clone());
                    let glyph = if Path::new(f).is_dir() { '\u{E8B7}' } else { '\u{E8A5}' };
                    Entry::item(RECENT_BASE + i as u32, name, glyph)
                })
                .collect();
            e.push(Entry::Sub { label: tr("Son kullanılanlar").into(), glyph: Some('\u{E81C}'), items });
            e.push(Entry::Sep);
        }
        files
    }

    fn toggle_launcher(&mut self) {
        if self.launcher.is_some() || self.launcher_closed.is_some_and(|t| t.elapsed().as_millis() < 300) {
            self.close_launcher();
            return;
        }
        let c = cursor();
        let Some(mon) = self
            .monitors
            .iter()
            .find(|m| c.x >= m.rect.left && c.x < m.rect.right && c.y >= m.rect.top && c.y < m.rect.bottom)
            .or(self.monitors.first())
            .copied()
        else {
            return;
        };
        let ctx = ctx!(self);
        let top = self.cfg.bar.top();
        let l = Launcher::new(&ctx, mon, c.x, top).ok();
        self.launcher = l;
    }

    fn close_launcher(&mut self) {
        if self.launcher.take().is_some() {
            self.launcher_closed = Some(std::time::Instant::now());
        }
    }

    fn power_menu(&mut self) {
        const SLEEP: u32 = 1;
        const RESTART: u32 = 2;
        const SHUTDOWN: u32 = 3;
        const SIGNOUT: u32 = 4;
        const LOCK: u32 = 5;
        let e = vec![
            Entry::item(LOCK, tr("Kilitle"), '\u{E72E}'),
            Entry::item(SIGNOUT, tr("Oturumu kapat"), '\u{F3B1}'),
            Entry::Sep,
            Entry::item(SLEEP, tr("Uyku"), '\u{E708}'),
            Entry::item(RESTART, tr("Yeniden başlat"), '\u{E777}'),
            Entry::item(SHUTDOWN, tr("Bilgisayarı kapat"), '\u{E7E8}'),
        ];
        let pick = self.popup(e);
        self.close_launcher();
        let run = |args: &[&str]| {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("shutdown").args(args).creation_flags(0x0800_0000).spawn();
        };
        match pick {
            Some(LOCK) => crate::system::lock(),
            Some(SIGNOUT) => run(&["/l"]),
            Some(SLEEP) => unsafe {
                let _ = windows::Win32::System::Power::SetSuspendState(false, false, false);
            },
            Some(RESTART) => run(&["/r", "/t", "0"]),
            Some(SHUTDOWN) => run(&["/s", "/t", "0"]),
            _ => {}
        }
    }

    fn toggle_pin(&mut self, path: &str) {
        let mut list = self.cfg.bar.pinned.clone();
        let key = crate::bar::app_key(path);
        if list.iter().any(|x| crate::bar::app_key(x) == key) {
            list.retain(|x| crate::bar::app_key(x) != key);
        } else {
            if let Some(b) = self.bars.iter().find(|b| b.has_app(path)) {
                b.freeze_order();
            }
            list.push(path.to_string());
        }
        config::set_pinned(&list);
        self.reload();
    }

    fn pinned_menu(&mut self, path: String) {
        const OPEN: u32 = 1;
        const UNPIN: u32 = 2;
        const LOCATION: u32 = 3;
        let name = util::file_description(&path)
            .or_else(|| Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| path.clone());
        let mut e = vec![Entry::Header(name)];
        let recent = Self::recent_menu(&path, &mut e);
        e.extend([
            Entry::item(OPEN, tr("Aç"), '\u{E8A7}'),
            Entry::item(LOCATION, tr("Dosya konumunu aç"), '\u{E838}'),
            Entry::Sep,
            Entry::item(UNPIN, tr("Sabitlemeyi kaldır"), '\u{E77A}'),
        ]);
        let pick = self.popup(e);
        if let Some(f) = pick.and_then(|i| i.checked_sub(RECENT_BASE)).and_then(|i| recent.get(i as usize)) {
            util::open_path(Path::new(f));
            return;
        }
        match pick {
            Some(OPEN) => unsafe {
                ShellExecuteW(None, w!("open"), &HSTRING::from(crate::bar::resolve_app(&path)), None, None, SW_SHOWNORMAL);
            },
            Some(LOCATION) => unsafe {
                let args = HSTRING::from(format!("/select,\"{path}\""));
                ShellExecuteW(None, w!("open"), w!("explorer.exe"), &args, None, SW_SHOWNORMAL);
            },
            Some(UNPIN) => self.toggle_pin(&path),
            _ => {}
        }
    }

    fn open_settings(&mut self) {
        if let Some(st) = &self.settings {
            st.show();
            return;
        }
        let Some(mon) = self.monitors.iter().find(|m| m.primary).or(self.monitors.first()).copied() else { return };
        let n = self.monitors.len();
        let ctx = ctx!(self);
        let st = Settings::new(&ctx, mon, n).ok();
        self.settings = st;
    }

    /// Çentiğe sağ tık: kenar seçimi ve konum kilidi
    fn notch_menu(&mut self) {
        const LOCK: u32 = 1;
        const SETTINGS: u32 = 2;
        const EDGE: u32 = 10;
        let n = &self.cfg.notch;
        let mut e = vec![Entry::Header(tr("Çentik ayarları").to_string())];
        for (i, (k, v)) in config::NOTCH_EDGES.iter().enumerate() {
            e.push(Entry::check(EDGE + i as u32, tr(v), n.edge == *k));
        }
        e.push(Entry::Sep);
        e.push(Entry::check(LOCK, tr("Konumu kilitle"), n.locked));
        e.push(Entry::item(SETTINGS, tr("Ayarlar"), '\u{E713}'));
        match self.popup(e) {
            Some(LOCK) => config::set_value("notch", "locked", !n.locked),
            Some(SETTINGS) => self.open_settings(),
            Some(id) if id >= EDGE => {
                if let Some((k, _)) = config::NOTCH_EDGES.get((id - EDGE) as usize) {
                    if *k != n.edge {
                        config::set_notch_place(k, 0.5, n.monitor);
                    }
                }
            }
            _ => {}
        }
    }

    /// Sağ tık menüsü: kısa yollar; ayarların hepsi Ayarlar uygulamasında
    fn menu(&mut self, widget: Option<usize>, pet: bool) {
        const TASKMGR: u32 = 1;
        const SETTINGS: u32 = 2;
        const HIDE_WIDGET: u32 = 3;
        const HIDE_PET: u32 = 4;
        const EXIT: u32 = 9;
        const PHOTO_DIR: u32 = 20;
        const PHOTO_NEXT: u32 = 21;
        const CLEAR_DONE: u32 = 22;
        const REFRESH: u32 = 23;
        const NOTE_COLOR: u32 = 30;
        let kind = widget.and_then(|i| self.cfg.widgets.get(i)).map(|w| w.kind.clone()).unwrap_or_default();
        let note_color = widget.and_then(|i| self.cfg.widgets.get(i)).map(|w| w.color.clone()).unwrap_or_default();
        let mut e = Vec::new();
        match kind.as_str() {
            "photo" => {
                e.push(Entry::item(PHOTO_DIR, tr("Fotoğraf klasörü seç…"), '\u{E838}'));
                e.push(Entry::item(PHOTO_NEXT, tr("Sonraki fotoğraf"), '\u{E893}'));
                e.push(Entry::Sep);
            }
            "todo" => {
                e.push(Entry::item(CLEAR_DONE, tr("Tamamlananları temizle"), '\u{E74D}'));
                e.push(Entry::Sep);
            }
            "markets" => {
                e.push(Entry::item(REFRESH, tr("Yenile"), '\u{E72C}'));
                e.push(Entry::Sep);
            }
            "notes" => {
                let names = ["Sarı", "Pembe", "Yeşil", "Mavi", "Mor", "Tema rengi"];
                let items = crate::widgets::NOTE_COLORS
                    .iter()
                    .zip(names)
                    .enumerate()
                    .map(|(i, (k, n))| Entry::check(NOTE_COLOR + i as u32, tr(n), note_color == *k))
                    .collect();
                e.push(Entry::Sub { label: tr("Not rengi").to_string(), glyph: Some('\u{E790}'), items });
                e.push(Entry::Sep);
            }
            _ => {}
        }
        if widget.is_some() {
            e.push(Entry::item(HIDE_WIDGET, tr("Bu widget'ı gizle"), '\u{ED1A}'));
            e.push(Entry::Sep);
        }
        if pet {
            e.push(Entry::item(HIDE_PET, tr("Evcil hayvanı kapat"), '\u{ED1A}'));
            e.push(Entry::Sep);
        }
        e.push(Entry::item(TASKMGR, tr("Görev Yöneticisi"), '\u{E9D9}'));
        e.push(Entry::item(SETTINGS, tr("Ayarlar"), '\u{E713}'));
        e.push(Entry::Sep);
        e.push(Entry::item(EXIT, tr("Çıkış (Windows görev çubuğunu geri getir)"), '\u{E7E8}'));
        match self.popup(e) {
            Some(TASKMGR) => self.act(self.host, Action::Open("taskmgr")),
            Some(SETTINGS) => self.open_settings(),
            Some(HIDE_WIDGET) => {
                if let Some(i) = widget {
                    config::set_widget_enabled(i, false);
                    self.reload();
                }
            }
            Some(HIDE_PET) => {
                config::set_pet(None);
                self.reload();
            }
            Some(PHOTO_DIR) => {
                if let (Some(i), Some(dir)) = (widget, pick_folder(self.host)) {
                    config::set_widget_value(i, "path", dir.to_string_lossy().as_ref());
                    self.reload();
                }
            }
            Some(PHOTO_NEXT) | Some(CLEAR_DONE) => {
                if let Some(w) = widget.and_then(|i| self.widgets.iter_mut().find(|w| w.index == i)) {
                    if kind == "todo" {
                        w.clear_done();
                    } else {
                        w.next_photo();
                    }
                }
                self.paint_all(false);
            }
            Some(REFRESH) => self.reload(),
            Some(id) if (NOTE_COLOR..NOTE_COLOR + crate::widgets::NOTE_COLORS.len() as u32).contains(&id) => {
                if let Some(i) = widget {
                    config::set_widget_value(i, "color", crate::widgets::NOTE_COLORS[(id - NOTE_COLOR) as usize]);
                    self.reload();
                }
            }
            Some(EXIT) => unsafe { PostQuitMessage(0) },
            _ => {}
        }
    }

    fn handle(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        if msg == WM_CLOSE && hwnd == self.host {
            unsafe { PostQuitMessage(0) };
            return Some(LRESULT(0));
        }
        if msg == self.taskbar_created {
            if hwnd == self.host {
                tray_icon(self.host, true);
                if !tray::own_broadcast() {
                    taskbar::enforce();
                    crate::taskband::apply();
                    self.rebuild();
                    tray::broadcast();
                }
            }
            return Some(LRESULT(0));
        }
        if msg == self.shellhook && hwnd == self.host {
            let target = HWND(lp.0 as _);
            match wp.0 {
                0x8006 => crate::bar::set_flash(target, true),
                4 | 0x8004 | 2 => crate::bar::set_flash(target, false),
                _ => return Some(LRESULT(0)),
            }
            self.paint_all(false);
            return Some(LRESULT(0));
        }
        if self.picker.as_ref().is_some_and(|p| p.hwnd == hwnd) {
            if msg == WM_TIMER {
                if let Some(p) = self.picker.as_mut() {
                    p.update();
                }
                return Some(LRESULT(0));
            }
            return None;
        }
        if msg == WM_DISPLAYCHANGE || msg == WM_DPICHANGED {
            unsafe {
                let _ = SetTimer(self.host, TIMER_REBUILD, 800, None);
            }
            return if hwnd == self.host { Some(LRESULT(0)) } else { None };
        }

        if hwnd == self.host {
            match msg {
                WM_TIMER if wp.0 == TIMER_REBUILD => unsafe {
                    let _ = KillTimer(self.host, TIMER_REBUILD);
                    self.rebuild();
                },
                WM_TIMER => self.tick(),
                WM_APP_REFRESH => {
                    REFRESH_PENDING.store(false, Ordering::SeqCst);
                    for b in &mut self.bars {
                        b.refresh_windows();
                    }
                    self.paint_all(false);
                }
                WM_APP_DATA => {
                    self.feeds_arrived();
                    self.bt_arrived();
                    self.media = self.media_state.lock().unwrap().clone();
                    self.weather = self.weather_state.lock().unwrap().clone();
                    self.paint_all(false);
                }
                WM_APP_REBUILD => self.rebuild(),
                crate::hotkeys::WM_APP_HOTKEY => match wp.0 {
                    0 => self.toggle_launcher(),
                    n => {
                        let fg = unsafe { GetForegroundWindow() };
                        let mon = unsafe { windows::Win32::Graphics::Gdi::MonitorFromWindow(fg, windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST) };
                        let i = self.bars.iter().position(|b| b.monitor().hmon == mon).unwrap_or(0);
                        if let Some(b) = self.bars.get_mut(i) {
                            b.activate_nth(n - 1);
                        }
                        self.paint_all(false);
                    }
                },
                crate::hotkeys::WM_APP_PICK => {
                    crate::hotkeys::set_picking(false);
                    let color = self.picker.take().and_then(|p| p.color());
                    if let (1, Some(hex)) = (wp.0, color) {
                        crate::system::set_clipboard_text(self.host, &hex);
                        self.show_notice(notify::Notice { title: tr("Renk kopyalandı").into(), message: hex, sound: false, target: 0, kind: String::new() });
                    }
                }
                WM_SETTINGCHANGE if wp.0 == 0x14 && self.cfg.general.wallpaper_accent => {
                    self.refresh_wallpaper_accent();
                    self.rebuild();
                }
                WM_COPYDATA => match notify::parse(lp) {
                    Some(notify::Incoming::Notice(n)) => {
                        self.notify(n);
                        return Some(LRESULT(1));
                    }
                    Some(notify::Incoming::Agent(ev)) => {
                        if let Some(nt) = self.notch.as_mut() {
                            nt.agent_event(ev);
                        }
                        return Some(LRESULT(1));
                    }
                    None => return None,
                },
                WM_APP_TRAY => {
                    let ev = (lp.0 & 0xFFFF) as u32;
                    if ev == WM_LBUTTONUP {
                        self.open_settings();
                    } else if ev == WM_RBUTTONUP {
                        self.menu(None, false);
                    }
                }
                _ => return None,
            }
            return Some(LRESULT(0));
        }

        if self.launcher.as_ref().is_some_and(|l| l.hwnd == hwnd) {
            let ctx = ctx!(self);
            let out = self.launcher.as_mut()?.handle(msg, wp, lp, &ctx)?;
            match out {
                Some(LauncherOut::Close) => self.close_launcher(),
                Some(LauncherOut::Settings) => {
                    self.close_launcher();
                    self.open_settings();
                }
                Some(LauncherOut::Power) => self.power_menu(),
                None => {}
            }
            return Some(LRESULT(0));
        }

        if self.settings.as_ref().is_some_and(|s| s.hwnd == hwnd) {
            if msg == WM_CLOSE {
                self.settings = None;
                return Some(LRESULT(0));
            }
            let ctx = ctx!(self);
            let out = self.settings.as_mut()?.handle(msg, wp, lp, &ctx)?;
            match out {
                Some(SettingsOut::Reload) => self.reload(),
                Some(SettingsOut::Exit) => unsafe { PostQuitMessage(0) },
                None => {}
            }
            return Some(LRESULT(0));
        }

        if self.notch.as_ref().is_some_and(|n| n.hwnd == hwnd) {
            if msg == WM_TIMER || msg == notch::WM_APP_FRAME {
                let ctx = ctx!(self);
                let mut done = false;
                if let Some(n) = self.notch.as_mut() {
                    if msg == notch::WM_APP_FRAME {
                        n.step(&ctx);
                    } else {
                        n.on_timer(wp.0, &ctx);
                    }
                    done = n.take_timer_done();
                }
                if done && self.feeds.focus_until.is_some() {
                    self.set_focus(false, None);
                }
                if done {
                    self.show_notice(notify::Notice {
                        title: tr("Zamanlayıcı").into(),
                        message: tr("Süre doldu").into(),
                        sound: true,
                        target: 0,
                        kind: String::new(),
                    });
                }
                return Some(LRESULT(0));
            }
            let action = self.notch.as_mut()?.handle(msg, wp, lp)?;
            if let Some(a) = action {
                self.act(hwnd, a);
                if let Some(n) = self.notch.as_mut() {
                    n.recheck_hover();
                }
            }
            return Some(LRESULT(0));
        }

        if self.pet.as_ref().is_some_and(|p| p.hwnd == hwnd) {
            if msg == WM_TIMER {
                let x = self.pet.as_ref().map(|p| p.x()).unwrap_or(0.0);
                if let Some((g, _)) = self.pet_ground(x) {
                    let ctx = ctx!(self);
                    if let Some(p) = self.pet.as_mut() {
                        p.step(&ctx, g);
                    }
                }
                return Some(LRESULT(0));
            }
            let action = self.pet.as_mut()?.handle(msg)?;
            if let Some(a) = action {
                self.act(hwnd, a);
            }
            return Some(LRESULT(0));
        }

        if let Some(i) = self.bars.iter().position(|b| b.owns(hwnd)) {
            let out = self.bars[i].handle(hwnd, msg, wp, lp)?;
            if out.repaint {
                let ctx = ctx!(self);
                self.bars[i].paint(&ctx, false);
            }
            if let Some(a) = out.action {
                self.act(hwnd, a);
            }
            return Some(LRESULT(0));
        }

        let locked = self.cfg.general.locked;
        if let Some(i) = self.widgets.iter().position(|w| w.hwnd == hwnd) {
            let (action, repaint) = self.widgets[i].handle(msg, wp, lp, locked)?;
            if repaint {
                let ctx = ctx!(self);
                self.widgets[i].paint(&ctx, false);
            }
            if let Some(a) = action {
                self.act(hwnd, a);
            }
            return Some(LRESULT(0));
        }
        None
    }
}

fn tray_icon(host: HWND, add: bool) {
    unsafe {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: host,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_APP_TRAY,
            hIcon: util::app_icon(GetSystemMetrics(SM_CXSMICON)),
            ..Default::default()
        };
        for (d, s) in nid.szTip.iter_mut().zip("Taskbaroque".encode_utf16()) {
            *d = s;
        }
        let _ = Shell_NotifyIconW(if add { NIM_ADD } else { NIM_DELETE }, &nid);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_WINDOWPOSCHANGING if GetWindowLongPtrW(hwnd, GWLP_USERDATA) == DESKTOP_LAYER => {
            let pos = &mut *(lp.0 as *mut WINDOWPOS);
            pos.hwndInsertAfter = HWND_BOTTOM;
            return LRESULT(0);
        }
        WM_MOUSEACTIVATE if !matches!(GetWindowLongPtrW(hwnd, GWLP_USERDATA), SETTINGS_WND | LAUNCHER_WND) => return LRESULT(MA_NOACTIVATE as isize),
        _ => {}
    }
    if msg == notch::WM_APP_FRAME {
        notch::frame_ack();
    }
    let r = APP.with(|a| match a.try_borrow_mut() {
        Ok(mut g) => g.as_mut().and_then(|app| app.handle(hwnd, msg, wp, lp)),
        Err(_) => None,
    });
    r.unwrap_or_else(|| DefWindowProcW(hwnd, msg, wp, lp))
}

unsafe extern "system" fn win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if id_object != OBJID_WINDOW.0 || id_child != 0 {
        return;
    }
    if event == EVENT_OBJECT_SHOW && taskbar::is_tray(hwnd) {
        taskbar::enforce();
    }
    if !REFRESH_PENDING.swap(true, Ordering::SeqCst) {
        let _ = PostMessageW(util::hwnd(HOST.load(Ordering::SeqCst)), WM_APP_REFRESH, WPARAM(0), LPARAM(0));
    }
}

pub fn create_layered(ex: WINDOW_EX_STYLE) -> Result<HWND> {
    unsafe {
        let inst = GetModuleHandleW(None)?;
        CreateWindowExW(
            ex | WS_EX_LAYERED,
            CLASS,
            w!("TaskbaroqueLayer"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            inst,
            None,
        )
    }
}

pub fn create_popup(ex: WINDOW_EX_STYLE) -> Result<HWND> {
    unsafe {
        let inst = GetModuleHandleW(None)?;
        CreateWindowExW(ex, CLASS, w!("TaskbaroqueLayer"), WS_POPUP, 0, 0, 1, 1, None, None, inst, None)
    }
}

pub fn run() -> Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let _ = windows::Win32::System::Ole::OleInitialize(None);

        let _mutex = CreateMutexW(None, true, w!("Local\\TaskbaroqueSingleInstance"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            i18n::set("auto");
            util::message_box(if i18n::en() { "Taskbaroque is already running." } else { "Taskbaroque zaten çalışıyor." });
            return Ok(());
        }

        gfx::init()?;

        let inst = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: inst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hIcon: util::app_icon(GetSystemMetrics(SM_CXICON)),
            hIconSm: util::app_icon(GetSystemMetrics(SM_CXSMICON)),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let host = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            CLASS,
            w!("TaskbaroqueHost"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            inst,
            None,
        )?;
        HOST.store(host.0 as isize, Ordering::SeqCst);
        tray_icon(host, true);

        util::migrate_autostart();
        crate::integrations::heal();
        i18n::set("auto");
        let cfg = match config::load() {
            Ok(c) => c,
            Err(e) => {
                util::message_box(&e);
                Config::default()
            }
        };
        i18n::set(&cfg.general.language);
        let mut cfg = cfg;
        if let Some(p) = crate::bar::import_windows_pins(&cfg.bar.pinned, false) {
            cfg.bar.pinned = p;
        }

        let media_state = Arc::new(Mutex::new(MediaInfo::default()));
        let weather_state = Arc::new(Mutex::new(None));
        let media_tx = media::spawn(media_state.clone(), host.0 as isize);
        let weather_tx = weather::spawn(weather_state.clone(), cfg.weather.clone(), host.0 as isize);

        let mut sys = SysInfo::default();
        sys.update(true);
        launcher::preload();
        feeds::restore_windows_dnd();
        let feed_state = Arc::new(Mutex::new(FeedData::default()));
        let feed_tx = feeds::spawn(feed_state.clone(), App::feed_cfg(&cfg), host.0 as isize);
        let feeds_init = Feeds { online: feeds::online(), battery: feeds::battery(), desktops: crate::system::virtual_desktops(), ..Default::default() };
        let bt_state = Arc::new(Mutex::new(Vec::new()));
        crate::devices::spawn(bt_state.clone(), host.0 as isize);
        crate::hotkeys::start(host);
        let _ = RegisterShellHookWindow(host);
        // Claude Code bildirimleri için hook'ları kur/güncelle (başkalarının kayıtlarına dokunmaz)
        if let Err(e) = crate::integrations::ensure_hooks() {
            let _ = std::fs::write(config::dir().join("hooks-error.log"), e);
        }
        crate::taskband::start(host.0 as isize);

        let mut app = App {
            host,
            cfg,
            cfg_mtime: config::mtime(),
            monitors: Vec::new(),
            bars: Vec::new(),
            widgets: Vec::new(),
            pet: None,
            settings: None,
            launcher: None,
            launcher_closed: None,
            notch: None,
            app_count: 0,
            sys,
            media_state,
            media: MediaInfo::default(),
            media_tx,
            weather_state,
            weather: None,
            weather_tx,
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
            feeds: feeds_init,
            feed_state,
            feed_tx,
            reminded: Vec::new(),
            battery_alert: 0,
            slow_tick: 0,
            shellhook: RegisterWindowMessageW(w!("SHELLHOOK")),
            tracker: crate::screentime::Tracker::load(),
            picker: None,
            bt_state,
            bt_alerted: Vec::new(),
        };
        app.refresh_wallpaper_accent();
        app.build();
        APP.with(|a| *a.borrow_mut() = Some(app));

        SetTimer(host, TIMER_TICK, 1000, None);
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        for (a, b) in [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
            (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
            (EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE),
            (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
            (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
        ] {
            SetWinEventHook(a, b, None, Some(win_event), 0, 0, flags);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        tray_icon(host, false);
        APP.with(|a| {
            if let Some(app) = a.borrow_mut().as_mut() {
                app.tracker.save();
            }
        });
        feeds::restore_windows_dnd();
        crate::taskband::stop();
        tray::stop();
        APP.with(|a| a.borrow_mut().take());
        taskbar::restore();
    }
    Ok(())
}

/// Klasör seçme penceresi.
fn pick_folder(owner: HWND) -> Option<std::path::PathBuf> {
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_PICKFOLDERS, SIGDN_FILESYSPATH};
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let _ = dlg.SetOptions(dlg.GetOptions().ok()? | FOS_PICKFOLDERS);
        let title = HSTRING::from(tr("Fotoğraf klasörü seç…"));
        let _ = dlg.SetTitle(&title);
        dlg.Show(owner).ok()?;
        let item = dlg.GetResult().ok()?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        path.map(std::path::PathBuf::from)
    }
}

fn cursor() -> POINT {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    p
}
