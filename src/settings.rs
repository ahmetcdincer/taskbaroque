use windows::core::{w, Result};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::Ctx;
use crate::config::{self, Config, Palette, BAR_STYLES, NOTCH_MODES, PET_KINDS, PRESET_KEYS, WIDGET_KINDS};
use crate::gfx::{Align, Color, Rect, Surface};
use crate::i18n::{self, tr, LANGUAGES};
use crate::menu::{self, Entry};
use crate::monitors::Monitor;
use crate::skin;
use crate::util;

pub const SETTINGS_WND: isize = 2;
const W: f32 = 860.0;
const H: f32 = 580.0;
const SIDE: f32 = 210.0;
const ROW: f32 = 60.0;
const WM_MOUSELEAVE_: u32 = 0x02A3;

const SECTIONS: [(char, &str, &str); 6] = [
    ('\u{E790}', "Görünüm", "Appearance"),
    ('\u{E8A9}', "Görev çubuğu", "Taskbar"),
    ('\u{E8A0}', "Çentik", "Notch"),
    ('\u{E76E}', "Evcil hayvan", "Pet"),
    ('\u{ECA5}', "Widget'lar", "Widgets"),
    ('\u{E713}', "Genel", "General"),
];

fn l<'a>(trk: &'a str, en: &'a str) -> &'a str {
    if i18n::en() { en } else { trk }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Id {
    Layout,
    Language,
    IconSize,
    StartIcon,
    StartMenu,
    WallAccent,
    IcsAdd,
    IcsClear,
    Github,
    StartIconReset,
    ImportPins,
    BarEnabled,
    Position,
    Monitors,
    PerMonitor,
    ShowMedia,
    ShowWeather,
    ShowSys,
    Hotkeys,
    ShowClock,
    Seconds,
    ShowTray,
    HiddenApps,
    NotchMode,
    NotchMonitor,
    PetEnabled,
    PetKind,
    PetSize,
    Terrain,
    TerrainStyle,
    Car,
    Widget(usize),
    AddWidget,
    Lock,
    Autostart,
    OpenConfig,
    Reload,
    Exit,
}

enum Ctl {
    Toggle(bool),
    Choice(String),
    Stepper(String),
    Button(String, bool),
}

struct RowDef {
    id: Id,
    title: String,
    desc: String,
    ctl: Ctl,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit {
    Section(usize),
    Theme(usize),
    Row(Id),
    Step(Id, i32),
}

pub enum SettingsOut {
    Reload,
    Exit,
}

pub struct Settings {
    pub hwnd: HWND,
    surf: Surface,
    mon: Monitor,
    section: usize,
    scroll: f32,
    content_h: f32,
    hits: Vec<(Rect, Hit)>,
    hover: Option<Hit>,
    tracking: bool,
    logo: Option<ID2D1Bitmap>,
    monitors: usize,
}

impl Settings {
    pub fn new(ctx: &Ctx, mon: Monitor, monitors: usize) -> Result<Settings> {
        let sc = mon.scale;
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut r = RECT { left: 0, top: 0, right: (W * sc) as i32, bottom: (H * sc) as i32 };
        unsafe {
            let _ = AdjustWindowRectEx(&mut r, style, false, WS_EX_APPWINDOW);
        }
        let (ww, wh) = (r.right - r.left, r.bottom - r.top);
        let x = mon.work.left + ((mon.work.right - mon.work.left) - ww) / 2;
        let y = mon.work.top + ((mon.work.bottom - mon.work.top) - wh) / 2;
        let hwnd = unsafe {
            let inst = GetModuleHandleW(None)?;
            CreateWindowExW(WS_EX_APPWINDOW, w!("TaskbaroqueWnd"), w!("Taskbaroque"), style, x, y, ww, wh, None, None, inst, None)?
        };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, SETTINGS_WND);
            let dark: u32 = 1;
            let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(20), &dark as *const _ as _, 4);
            let bg = base_color(&ctx.pal);
            let cr = (bg.r * 255.0) as u32 | ((bg.g * 255.0) as u32) << 8 | ((bg.b * 255.0) as u32) << 16;
            let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(35), &cr as *const _ as _, 4);
        }
        let surf = Surface::new(W, H, sc, &ctx.pal.font)?;
        let logo = surf.bitmap_from_bytes(include_bytes!("../assets/taskbaroque_icon.png"), 256);
        let mut s = Settings { hwnd, surf, mon, section: 0, scroll: 0.0, content_h: 0.0, hits: Vec::new(), hover: None, tracking: false, logo, monitors };
        s.draw(ctx);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(hwnd);
        }
        Ok(s)
    }

    pub fn show(&self) {
        unsafe {
            if IsIconic(self.hwnd).as_bool() {
                let _ = ShowWindow(self.hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(self.hwnd);
        }
    }

    pub fn set_monitors(&mut self, n: usize) {
        self.monitors = n;
    }

    fn rows(&self, cfg: &Config) -> Vec<RowDef> {
        let b = &cfg.bar;
        let row = |id, title: &str, desc: &str, ctl| RowDef { id, title: title.to_string(), desc: desc.to_string(), ctl };
        let label_of = |list: &[(&str, &str)], key: &str| tr(list.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or(key)).to_string();
        let lang = if cfg.general.language.is_empty() { "auto" } else { cfg.general.language.as_str() };
        match self.section {
            0 => vec![
                row(Id::Layout, l("Görünüm türü", "Layout"), l("Çubuğun şekli", "Shape of the bar"), Ctl::Choice(label_of(&BAR_STYLES, &b.style))),
                row(Id::IconSize, l("Simge boyutu", "Icon size"), l("Uygulama simgelerinin boyu", "Size of app icons"), Ctl::Stepper(format!("{:.0}", b.icon_size))),
                row(
                    Id::StartIcon,
                    l("Başlat simgesi", "Start icon"),
                    if b.start_icon.is_empty() { l("Windows logosu", "Windows logo") } else { l("Özel görsel kullanılıyor", "Using a custom image") },
                    Ctl::Button(l("Görsel seç", "Choose image").into(), false),
                ),
                row(Id::StartIconReset, l("Varsayılan Başlat simgesi", "Default Start icon"), l("Windows logosuna geri dön", "Go back to the Windows logo"), Ctl::Button(l("Sıfırla", "Reset").into(), false)),
                row(
                    Id::ImportPins,
                    l("Windows sabitlemeleri", "Windows pins"),
                    l("Windows görev çubuğunda sabitli uygulamaları al", "Import the apps pinned on the Windows taskbar"),
                    Ctl::Button(l("İçe aktar", "Import").into(), false),
                ),
                row(Id::Language, l("Dil", "Language"), l("Arayüz dili", "Interface language"), Ctl::Choice(label_of(&LANGUAGES, lang))),
                row(Id::WallAccent, l("Duvar kağıdına göre renk", "Color from wallpaper"), l("Vurgu rengi duvar kağıdından alınır", "The accent color is taken from the wallpaper"), Ctl::Toggle(cfg.general.wallpaper_accent)),
                row(
                    Id::StartMenu,
                    l("Başlat düğmesi", "Start button"),
                    l("Tıklayınca hangi menü açılsın", "Which menu opens on click"),
                    Ctl::Choice(if b.start_menu == "windows" { l("Windows menüsü", "Windows menu") } else { l("Taskbaroque başlatıcı", "Taskbaroque launcher") }.into()),
                ),
            ],
            1 => vec![
                row(Id::BarEnabled, l("Özel görev çubuğu", "Custom taskbar"), l("Kapalıyken Windows görev çubuğu geri gelir", "When off, the Windows taskbar comes back"), Ctl::Toggle(b.enabled)),
                row(Id::Position, l("Konum", "Position"), l("Ekranın altı ya da üstü", "Bottom or top of the screen"), Ctl::Choice(if b.top() { l("Üst", "Top") } else { l("Alt", "Bottom") }.into())),
                row(Id::Monitors, l("Ekranlar", "Screens"), l("Çubuk hangi ekranlarda olsun", "Which screens get a bar"), Ctl::Choice(if b.all_monitors() { l("Tüm ekranlar", "All screens") } else { l("Ana ekran", "Main screen") }.into())),
                row(Id::PerMonitor, l("Her ekranda kendi pencereleri", "Each screen shows its own windows"), l("Kapalıyken tüm uygulamalar her çubukta görünür", "When off, every bar shows all apps"), Ctl::Toggle(b.windows_per_monitor)),
                row(Id::ShowMedia, l("Çalan medya", "Playing media"), l("Şarkı ya da video adı", "Song or video title"), Ctl::Toggle(b.show_media)),
                row(Id::ShowWeather, l("Hava durumu", "Weather"), "", Ctl::Toggle(b.show_weather)),
                row(Id::ShowSys, l("CPU ve RAM", "CPU and RAM"), "", Ctl::Toggle(b.show_sysinfo)),
                row(Id::Hotkeys, l("Klavye kısayolları", "Keyboard shortcuts"), l("Win+1…9 çubuktaki uygulamaları, Alt+Boşluk başlatıcıyı açar", "Win+1…9 opens bar apps, Alt+Space opens the launcher"), Ctl::Toggle(b.hotkeys)),
                row(Id::ShowClock, l("Saat", "Clock"), "", Ctl::Toggle(b.show_clock)),
                row(Id::Seconds, l("Saniyeleri göster", "Show seconds"), "", Ctl::Toggle(b.clock_seconds)),
                row(Id::ShowTray, l("Bildirim alanı", "Notification area"), l("Arka planda çalışan programların simgeleri", "Icons of programs running in the background"), Ctl::Toggle(b.show_tray)),
                row(
                    Id::HiddenApps,
                    l("Gizlenen uygulamalar", "Hidden apps"),
                    &if i18n::en() { format!("{} app(s) hidden from the taskbar", b.hidden_apps.len()) } else { format!("Görev çubuğundan gizlenen {} uygulama", b.hidden_apps.len()) },
                    Ctl::Button(l("Hepsini göster", "Show all").into(), false),
                ),
            ],
            2 => vec![
                row(Id::NotchMode, l("Görünürlük", "Visibility"), l("Çentik ne zaman görünsün", "When the notch is shown"), Ctl::Choice(label_of(&NOTCH_MODES, &cfg.notch.mode))),
                row(Id::NotchMonitor, l("Ekran", "Screen"), l("Çentiğin bulunduğu ekran", "Screen the notch is on"), Ctl::Choice(format!("{} {}", l("Ekran", "Screen"), cfg.notch.monitor))),
                row(
                    Id::IcsAdd,
                    l("Takvim (ICS) ekle", "Add calendar (ICS)"),
                    l("Google/Outlook takviminin gizli ICS bağlantısını kopyala, sonra bas", "Copy your Google/Outlook calendar's secret ICS link, then press"),
                    Ctl::Button(l("Panodan ekle", "Add from clipboard").into(), false),
                ),
                row(
                    Id::IcsClear,
                    l("Bağlı takvimler", "Connected calendars"),
                    &if i18n::en() { format!("{} calendar(s)", cfg.calendar.ics.len()) } else { format!("{} takvim", cfg.calendar.ics.len()) },
                    Ctl::Button(l("Kaldır", "Remove").into(), false),
                ),
                row(Id::Github, "GitHub", l("Açık PR'lar ve CI durumu (gh CLI ile)", "Open PRs and CI status (via gh CLI)"), Ctl::Toggle(cfg.github.enabled)),
            ],
            3 => {
                let mut v = vec![
                    row(Id::PetEnabled, l("Evcil hayvan", "Pet"), l("Görev çubuğunda dolaşan karakter", "Character that roams the taskbar"), Ctl::Toggle(cfg.pet.enabled)),
                    row(Id::PetKind, l("Karakter", "Character"), "", Ctl::Choice(label_of(&PET_KINDS, &cfg.pet.kind))),
                    row(Id::PetSize, l("Boyut", "Size"), "", Ctl::Stepper(format!("{:.1}", cfg.pet.size))),
                    row(Id::Terrain, l("Çubuğun yanlarında arazi", "Ground beside the bar"), l("Pet'in dolaştığı alan", "Area where the pet walks"), Ctl::Toggle(b.terrain)),
                    row(
                        Id::TerrainStyle,
                        l("Arazi türü", "Ground style"),
                        l("Tünelli yol ya da çimenli arazi", "Road with tunnels or grassy ground"),
                        Ctl::Choice(if b.terrain_style == "grass" { l("Çimen", "Grass") } else { l("Yol", "Road") }.into()),
                    ),
                ];
                if b.terrain_style != "grass" {
                    v.push(row(Id::Car, l("Araba", "Car"), l("Yolda dolaşıp tünellerden geçer", "Drives along the road through the tunnels"), Ctl::Toggle(b.car)));
                }
                v
            }
            4 => {
                let mut v: Vec<RowDef> = cfg
                    .widgets
                    .iter()
                    .enumerate()
                    .map(|(i, wc)| {
                        let desc = format!("{} {}", l("Ekran", "Screen"), wc.monitor);
                        row(Id::Widget(i), tr(config::widget_label(&wc.kind)), &desc, Ctl::Toggle(wc.enabled))
                    })
                    .collect();
                v.push(row(Id::AddWidget, l("Widget ekle", "Add widget"), l("Masaüstüne yeni bir widget koy", "Put a new widget on the desktop"), Ctl::Choice(l("Seç", "Choose").into())));
                v.push(row(Id::Lock, l("Widget'ları kilitle", "Lock widgets"), l("Sürüklenerek taşınamasınlar", "Prevent moving them by dragging"), Ctl::Toggle(cfg.general.locked)));
                v
            }
            _ => vec![
                row(Id::Autostart, l("Windows ile başlat", "Start with Windows"), "", Ctl::Toggle(util::autostart_enabled())),
                row(Id::OpenConfig, l("Ayar dosyası", "Settings file"), l("Gelişmiş ayarlar için config.toml", "config.toml for advanced settings"), Ctl::Button(l("Aç", "Open").into(), false)),
                row(Id::Reload, l("Yeniden yükle", "Reload"), l("Ayarları dosyadan tekrar oku", "Read settings from the file again"), Ctl::Button(l("Yükle", "Reload").into(), false)),
                row(Id::Exit, l("Çıkış", "Exit"), l("Taskbaroque'u kapat, Windows görev çubuğunu geri getir", "Close Taskbaroque and restore the Windows taskbar"), Ctl::Button(l("Çıkış", "Exit").into(), true)),
            ],
        }
    }

    pub fn draw(&mut self, ctx: &Ctx) {
        let p = &ctx.pal;
        let base = base_color(p);
        let white = p.text;
        let dim = p.dim;
        let accent = Color { a: 1.0, ..p.accent };
        let mut hits = Vec::new();
        let rows = self.rows(ctx.cfg);
        let s = &self.surf;
        s.begin();
        s.fill_round(Rect::new(0.0, 0.0, W, H), 0.0, base);
        // Kenar çubuğu
        s.fill_round(Rect::new(0.0, 0.0, SIDE, H), 0.0, white.alpha(0.035));
        if let Some(logo) = &self.logo {
            s.image(logo, Rect::new(18.0, 16.0, 40.0, 40.0), 0.0);
        }
        s.text("Taskbaroque", Rect::new(64.0, 18.0, SIDE - 70.0, 22.0), 15.0, 700, white, Align::Left);
        s.text(l("Ayarlar", "Settings"), Rect::new(64.0, 38.0, SIDE - 70.0, 18.0), 11.5, 500, dim, Align::Left);
        for (i, (g, trk, en)) in SECTIONS.iter().enumerate() {
            let r = Rect::new(10.0, 76.0 + i as f32 * 40.0, SIDE - 20.0, 36.0);
            let sel = self.section == i;
            if sel {
                s.fill_round(r, 8.0, white.alpha(0.09));
                s.fill_round(Rect::new(r.x, r.y + 10.0, 3.0, 16.0), 1.5, accent);
            } else if self.hover == Some(Hit::Section(i)) {
                s.fill_round(r, 8.0, white.alpha(0.05));
            }
            s.glyph(*g, Rect::new(r.x + 10.0, r.y, 24.0, r.h), 14.0, if sel { accent } else { dim });
            s.text(l(trk, en), Rect::new(r.x + 42.0, r.y, r.w - 46.0, r.h), 13.0, if sel { 600 } else { 400 }, white, Align::Left);
            hits.push((r, Hit::Section(i)));
        }

        // İçerik
        let cx = SIDE + 28.0;
        let cw = W - cx - 28.0;
        s.push_clip(Rect::new(SIDE, 0.0, W - SIDE, H));
        let mut y = 24.0 - self.scroll;
        s.text(l(SECTIONS[self.section].1, SECTIONS[self.section].2), Rect::new(cx, y, cw, 32.0), 22.0, 700, white, Align::Left);
        y += 48.0;

        // Tema galerisi
        if self.section == 0 {
            s.text(l("Tema", "Theme"), Rect::new(cx, y, cw, 20.0), 12.5, 600, dim, Align::Left);
            y += 26.0;
            let cols = 4;
            let gap = 12.0;
            let tw = (cw - gap * (cols as f32 - 1.0)) / cols as f32;
            let th = 78.0;
            for (i, key) in PRESET_KEYS.iter().enumerate() {
                let r = Rect::new(cx + (i % cols) as f32 * (tw + gap), y + (i / cols) as f32 * (th + gap), tw, th);
                let mut c = ctx.cfg.clone();
                c.theme.preset = key.to_string();
                c.theme.overrides = Default::default();
                let tp = c.palette();
                let sel = ctx.cfg.theme.preset == *key;
                s.fill_round(r, 10.0, white.alpha(if self.hover == Some(Hit::Theme(i)) { 0.08 } else { 0.04 }));
                let mini = Rect::new(r.x + 12.0, r.y + 12.0, r.w - 24.0, 30.0);
                skin::panel(s, mini, tp.radius.min(15.0), &tp, true);
                for k in 0..4 {
                    let col = if k == 0 { tp.accent } else { tp.text.alpha(0.55) };
                    s.fill_round(Rect::new(mini.x + 10.0 + k as f32 * 16.0, mini.y + 9.0, 11.0, 11.0), 3.0, col);
                }
                let name = c.theme_list().into_iter().find(|(k, _)| k == key).map(|(_, n)| n).unwrap_or_default();
                s.text(&name, Rect::new(r.x + 12.0, r.y + 48.0, r.w - 24.0, 22.0), 12.0, if sel { 700 } else { 500 }, white, Align::Left);
                if sel {
                    s.stroke_round(r.inset(1.0), 9.0, 2.0, accent);
                }
                hits.push((r, Hit::Theme(i)));
            }
            y += (PRESET_KEYS.len().div_ceil(cols)) as f32 * (th + gap) + 14.0;
        }

        for rd in &rows {
            let r = Rect::new(cx, y, cw, ROW - 6.0);
            let hov = matches!(self.hover, Some(Hit::Row(id)) | Some(Hit::Step(id, _)) if id == rd.id);
            s.fill_round(r, 10.0, white.alpha(if hov { 0.07 } else { 0.04 }));
            let ty = if rd.desc.is_empty() { r.y } else { r.y + 7.0 };
            let th = if rd.desc.is_empty() { r.h } else { 20.0 };
            s.text(&rd.title, Rect::new(r.x + 16.0, ty, r.w - 260.0, th), 13.0, 600, white, Align::Left);
            if !rd.desc.is_empty() {
                s.text(&rd.desc, Rect::new(r.x + 16.0, r.y + 27.0, r.w - 260.0, 18.0), 11.5, 400, dim, Align::Left);
            }
            let my = r.y + r.h / 2.0;
            let right = r.x + r.w - 16.0;
            match &rd.ctl {
                Ctl::Toggle(on) => {
                    let t = Rect::new(right - 44.0, my - 11.0, 44.0, 22.0);
                    if *on {
                        s.fill_round(t, 11.0, accent);
                        s.fill_circle(t.x + t.w - 11.0, my, 7.0, p.on_accent);
                    } else {
                        s.stroke_round(t.inset(0.75), 10.25, 1.5, white.alpha(0.5));
                        s.fill_circle(t.x + 11.0, my, 6.0, white.alpha(0.7));
                    }
                    hits.push((r, Hit::Row(rd.id)));
                }
                Ctl::Choice(v) => {
                    let c = Rect::new(right - 190.0, my - 15.0, 190.0, 30.0);
                    s.fill_round(c, 8.0, white.alpha(0.08));
                    s.text(v, Rect::new(c.x + 12.0, c.y, c.w - 40.0, c.h), 12.5, 500, white, Align::Left);
                    s.glyph('\u{E70D}', Rect::new(c.x + c.w - 30.0, c.y, 24.0, c.h), 10.0, dim);
                    hits.push((r, Hit::Row(rd.id)));
                }
                Ctl::Stepper(v) => {
                    let plus = Rect::new(right - 30.0, my - 15.0, 30.0, 30.0);
                    let minus = Rect::new(right - 110.0, my - 15.0, 30.0, 30.0);
                    for (b, g, d) in [(minus, '\u{E738}', -1), (plus, '\u{E710}', 1)] {
                        let h = self.hover == Some(Hit::Step(rd.id, d));
                        s.fill_round(b, 8.0, white.alpha(if h { 0.16 } else { 0.08 }));
                        s.glyph(g, b, 11.0, white);
                        hits.push((b, Hit::Step(rd.id, d)));
                    }
                    s.text(v, Rect::new(minus.x + 30.0, my - 15.0, 50.0, 30.0), 13.0, 600, white, Align::Center);
                }
                Ctl::Button(label, danger) => {
                    let bw = s.measure(label, 12.5, 600) + 32.0;
                    let b = Rect::new(right - bw, my - 15.0, bw, 30.0);
                    let c = if *danger { Color::rgba(196, 43, 28, 255) } else { white.alpha(if hov { 0.16 } else { 0.1 }) };
                    s.fill_round(b, 8.0, c);
                    s.text(label, b, 12.5, 600, if *danger { Color::rgba(255, 255, 255, 255) } else { white }, Align::Center);
                    hits.push((b, Hit::Row(rd.id)));
                }
            }
            y += ROW;
        }
        if self.section == 5 {
            if let Some(logo) = &self.logo {
                s.image(logo, Rect::new(cx + cw / 2.0 - 60.0, y + 16.0, 120.0, 120.0), 0.0);
            }
            s.text("Taskbaroque 0.1.0", Rect::new(cx, y + 142.0, cw, 22.0), 13.0, 600, dim, Align::Center);
            y += 180.0;
        }
        s.pop_clip();
        s.end();
        self.content_h = y + self.scroll + 20.0;
        self.hits = hits;
        self.blit();
    }

    fn blit(&self) {
        unsafe {
            let dc = GetDC(self.hwnd);
            self.surf.blit(dc);
            ReleaseDC(self.hwnd, dc);
        }
    }

    fn hit(&self, lp: LPARAM) -> Option<Hit> {
        let x = (lp.0 & 0xFFFF) as i16 as f32 / self.surf.scale;
        let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32 / self.surf.scale;
        self.hits.iter().find(|(r, _)| r.contains(x, y)).map(|(_, h)| *h)
    }

    fn choose(&self, pal: &Palette, options: Vec<(String, bool)>) -> Option<usize> {
        let entries = options.into_iter().enumerate().map(|(i, (label, on))| Entry::check(i as u32 + 1, label, on)).collect();
        let mut c = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut c);
        }
        menu::popup(pal, self.mon, entries, c, false).map(|i| (i & !menu::RIGHT) as usize - 1)
    }

    fn click(&mut self, h: Hit, ctx: &Ctx) -> Option<SettingsOut> {
        let cfg = ctx.cfg;
        let b = &cfg.bar;
        let flag = |key: &str, v: bool| config::set_bar_flag(key, v);
        match h {
            Hit::Section(i) => {
                self.section = i;
                self.scroll = 0.0;
                return None;
            }
            Hit::Theme(i) => config::set_theme(PRESET_KEYS[i]),
            Hit::Step(id, d) => match id {
                Id::IconSize => config::set_value("bar", "icon_size", (b.icon_size as i64 + 2 * d as i64).clamp(16, 32)),
                Id::PetSize => config::set_value("pet", "size", ((cfg.pet.size + 0.1 * d as f32).clamp(0.5, 2.0) * 10.0).round() as f64 / 10.0),
                _ => return None,
            },
            Hit::Row(id) => match id {
                Id::Layout => {
                    let opts = BAR_STYLES.iter().map(|(k, v)| (tr(v).to_string(), b.style == *k)).collect();
                    config::set_bar_style(BAR_STYLES[self.choose(&ctx.pal, opts)?].0);
                }
                Id::Language => {
                    let cur = if cfg.general.language.is_empty() { "auto" } else { cfg.general.language.as_str() };
                    let opts = LANGUAGES.iter().map(|(k, v)| (tr(v).to_string(), cur == *k)).collect();
                    config::set_language(LANGUAGES[self.choose(&ctx.pal, opts)?].0);
                }
                Id::BarEnabled => config::set_bar_enabled(!b.enabled),
                Id::Position => {
                    let opts = vec![(l("Alt", "Bottom").to_string(), !b.top()), (l("Üst", "Top").to_string(), b.top())];
                    config::set_value("bar", "position", ["bottom", "top"][self.choose(&ctx.pal, opts)?]);
                }
                Id::Monitors => {
                    let opts = vec![(l("Tüm ekranlar", "All screens").to_string(), b.all_monitors()), (l("Ana ekran", "Main screen").to_string(), !b.all_monitors())];
                    config::set_value("bar", "monitors", ["all", "primary"][self.choose(&ctx.pal, opts)?]);
                }
                Id::PerMonitor => flag("windows_per_monitor", !b.windows_per_monitor),
                Id::ShowMedia => flag("show_media", !b.show_media),
                Id::ShowWeather => flag("show_weather", !b.show_weather),
                Id::ShowSys => flag("show_sysinfo", !b.show_sysinfo),
                Id::Hotkeys => flag("hotkeys", !b.hotkeys),
                Id::ShowClock => flag("show_clock", !b.show_clock),
                Id::Seconds => flag("clock_seconds", !b.clock_seconds),
                Id::ShowTray => flag("show_tray", !b.show_tray),
                Id::HiddenApps => config::clear_hidden_apps(),
                Id::NotchMode => {
                    let opts = NOTCH_MODES.iter().map(|(k, v)| (tr(v).to_string(), cfg.notch.mode == *k)).collect();
                    config::set_notch_mode(NOTCH_MODES[self.choose(&ctx.pal, opts)?].0);
                }
                Id::NotchMonitor => {
                    let opts = (1..=self.monitors.max(1)).map(|i| (format!("{} {i}", l("Ekran", "Screen")), cfg.notch.monitor == i)).collect();
                    config::set_value("notch", "monitor", self.choose(&ctx.pal, opts)? as i64 + 1);
                }
                Id::PetEnabled => config::set_pet(if cfg.pet.enabled { None } else { Some(cfg.pet.kind.as_str()) }),
                Id::PetKind => {
                    let opts = PET_KINDS.iter().map(|(k, v)| (tr(v).to_string(), cfg.pet.kind == *k)).collect();
                    config::set_pet(Some(PET_KINDS[self.choose(&ctx.pal, opts)?].0));
                }
                Id::Terrain => flag("terrain", !b.terrain),
                Id::WallAccent => config::set_value("general", "wallpaper_accent", !cfg.general.wallpaper_accent),
                Id::StartMenu => {
                    let win = b.start_menu == "windows";
                    let opts = vec![(l("Taskbaroque başlatıcı", "Taskbaroque launcher").to_string(), !win), (l("Windows menüsü", "Windows menu").to_string(), win)];
                    config::set_value("bar", "start_menu", ["launcher", "windows"][self.choose(&ctx.pal, opts)?]);
                }
                Id::IcsAdd => {
                    let text = crate::system::clipboard_text(self.hwnd).unwrap_or_default();
                    let url = text.trim();
                    if url.starts_with("http") || url.starts_with("webcal") {
                        config::add_ics(url);
                    } else {
                        crate::util::message_box(l("Panoda bir takvim bağlantısı yok. Önce ICS bağlantısını kopyala.", "No calendar link on the clipboard. Copy the ICS link first."));
                        return None;
                    }
                }
                Id::IcsClear => config::clear_ics(),
                Id::Github => config::set_value("github", "enabled", !cfg.github.enabled),
                Id::TerrainStyle => {
                    let grass = b.terrain_style == "grass";
                    let opts = vec![(l("Yol", "Road").to_string(), !grass), (l("Çimen", "Grass").to_string(), grass)];
                    config::set_value("bar", "terrain_style", ["road", "grass"][self.choose(&ctx.pal, opts)?]);
                }
                Id::Car => flag("car", !b.car),
                Id::StartIcon => config::set_start_icon(Some(&pick_image(self.hwnd)?)),
                Id::ImportPins => {
                    if crate::bar::import_windows_pins(&b.pinned, true).is_none() {
                        crate::util::message_box(l("Eklenecek yeni sabitlenmiş uygulama bulunamadı.", "No new pinned apps to import."));
                        return None;
                    }
                }
                Id::StartIconReset => {
                    if b.start_icon.is_empty() {
                        return None;
                    }
                    config::set_start_icon(None);
                }
                Id::Widget(i) => config::set_widget_enabled(i, !cfg.widgets.get(i)?.enabled),
                Id::AddWidget => {
                    let opts = WIDGET_KINDS.iter().map(|(_, v)| (tr(v).to_string(), false)).collect();
                    config::add_widget(WIDGET_KINDS[self.choose(&ctx.pal, opts)?].0, 1);
                }
                Id::Lock => config::save_locked(!cfg.general.locked),
                Id::Autostart => {
                    util::set_autostart(!util::autostart_enabled());
                    return None;
                }
                Id::OpenConfig => {
                    util::open_path(&config::path());
                    return None;
                }
                Id::Reload => {}
                Id::Exit => return Some(SettingsOut::Exit),
                Id::IconSize | Id::PetSize => return None,
            },
        }
        Some(SettingsOut::Reload)
    }

    pub fn handle(&mut self, msg: u32, wp: WPARAM, lp: LPARAM, ctx: &Ctx) -> Option<Option<SettingsOut>> {
        match msg {
            WM_PAINT => unsafe {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(self.hwnd, &mut ps);
                self.surf.blit(dc);
                let _ = EndPaint(self.hwnd, &ps);
            },
            WM_ERASEBKGND => return Some(None),
            WM_MOUSEMOVE => {
                if !self.tracking {
                    let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: self.hwnd, dwHoverTime: 0 };
                    unsafe {
                        let _ = TrackMouseEvent(&mut tme);
                    }
                    self.tracking = true;
                }
                let h = self.hit(lp);
                if h != self.hover {
                    self.hover = h;
                    self.draw(ctx);
                }
            }
            WM_MOUSELEAVE_ => {
                self.tracking = false;
                if self.hover.take().is_some() {
                    self.draw(ctx);
                }
            }
            WM_MOUSEWHEEL => {
                let delta = (wp.0 >> 16) as u16 as i16 as f32 / 120.0;
                let max = (self.content_h - H).max(0.0);
                self.scroll = (self.scroll - delta * 60.0).clamp(0.0, max);
                self.draw(ctx);
            }
            WM_LBUTTONUP => {
                let out = self.hit(lp).and_then(|h| self.click(h, ctx));
                self.draw(ctx);
                return Some(out);
            }
            _ => return None,
        }
        Some(None)
    }
}

impl Drop for Settings {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Pencere zemini: temanın arka planı koyu bir tabana karıştırılmış (opak)
pub(crate) fn base_color(p: &Palette) -> Color {
    let dark = p.text.r > 0.5;
    let base = if dark { Color::rgba(28, 28, 32, 255) } else { Color::rgba(243, 243, 245, 255) };
    let bg = p.bg;
    let mix = |a: f32, b: f32| a * bg.a * 0.35 + b * (1.0 - bg.a * 0.35);
    Color { r: mix(bg.r, base.r), g: mix(bg.g, base.g), b: mix(bg.b, base.b), a: 1.0 }
}

fn pick_image(owner: HWND) -> Option<std::path::PathBuf> {
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST, SIGDN_FILESYSPATH};
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let name = windows::core::HSTRING::from(l("Görseller", "Images"));
        let filter = [COMDLG_FILTERSPEC { pszName: windows::core::PCWSTR(name.as_ptr()), pszSpec: w!("*.png;*.jpg;*.jpeg;*.bmp;*.gif;*.ico") }];
        dlg.SetFileTypes(&filter).ok()?;
        let title = windows::core::HSTRING::from(l("Başlat simgesi seç", "Choose Start icon"));
        let _ = dlg.SetTitle(&title);
        let _ = dlg.SetOptions(dlg.GetOptions().ok()? | FOS_FILEMUSTEXIST);
        dlg.Show(owner).ok()?;
        let item = dlg.GetResult().ok()?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        path.map(std::path::PathBuf::from)
    }
}
