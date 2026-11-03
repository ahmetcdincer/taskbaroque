use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use windows::core::Result;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, SetCapture, SetFocus, VIRTUAL_KEY, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_HOME, VK_LEFT, VK_RIGHT, VK_UP};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::{create_layered, Action, Ctx};
use crate::config::WidgetCfg;
use crate::gfx::{Align, Color, Rect, Surface};
use crate::markets;
use crate::media::MediaCmd;
use crate::monitors::Monitor;
use crate::skin;
use crate::sysinfo::{fmt_speed, HISTORY};
use crate::i18n::{self, tr};
use crate::util::days_in_month;
use crate::weather::Sky;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Clock,
    Weather,
    Media,
    Sysmon,
    Notes,
    Todo,
    Markets,
    Photo,
    Github,
}

#[derive(Clone, Copy, PartialEq, Debug, Hash)]
enum Btn {
    Prev,
    Toggle,
    Next,
    /// Not metin alanı
    NoteArea,
    /// Yapılacaklar: onay kutusu, sil, metin, yeni satır, tamamlananları temizle
    Check(usize),
    Remove(usize),
    Row(usize),
    AddRow,
    ClearDone,
    /// GitHub PR satırı
    Pr(usize),
}

struct Drag {
    cursor: POINT,
    x: i32,
    y: i32,
    moved: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq)]
struct Todo {
    text: String,
    done: bool,
}

#[derive(Clone, Copy, PartialEq, Debug, Hash)]
enum EditTarget {
    Note,
    /// Var olan madde ya da (None) yeni madde
    Todo(Option<usize>),
}

/// Widget içinde yazı düzenleme: metin ve imleç (karakter sırası).
#[derive(Clone, Debug, Hash)]
struct Edit {
    target: EditTarget,
    text: String,
    caret: usize,
}

impl Edit {
    fn byte(&self, c: usize) -> usize {
        self.text.char_indices().nth(c).map_or(self.text.len(), |(b, _)| b)
    }
    fn len(&self) -> usize {
        self.text.chars().count()
    }
    fn utf16(&self) -> u32 {
        self.text.chars().take(self.caret).map(|c| c.len_utf16() as u32).sum()
    }
    fn from_utf16(&self, u: u32) -> usize {
        let mut acc = 0u32;
        for (i, c) in self.text.chars().enumerate() {
            if acc >= u {
                return i;
            }
            acc += c.len_utf16() as u32;
        }
        self.len()
    }
    fn insert(&mut self, s: &str) {
        let b = self.byte(self.caret);
        self.text.insert_str(b, s);
        self.caret += s.chars().count();
    }
    fn backspace(&mut self) {
        if self.caret > 0 {
            let (a, b) = (self.byte(self.caret - 1), self.byte(self.caret));
            self.text.replace_range(a..b, "");
            self.caret -= 1;
        }
    }
    fn delete(&mut self) {
        if self.caret < self.len() {
            let (a, b) = (self.byte(self.caret), self.byte(self.caret + 1));
            self.text.replace_range(a..b, "");
        }
    }
}

const NOTE_SIZE: f32 = 14.0;
const TODO_ROW: f32 = 30.0;
const TODO_TOP: f32 = 44.0;
const GH_ROW: f32 = 46.0;
const PHOTO_EXT: [&str; 6] = ["jpg", "jpeg", "png", "bmp", "gif", "webp"];

pub struct Widget {
    pub hwnd: HWND,
    pub index: usize,
    surf: Surface,
    kind: Kind,
    cfg: WidgetCfg,
    pub monitor: usize,
    x: i32,
    y: i32,
    w: f32,
    h: f32,
    sig: String,
    buttons: Vec<(Rect, Btn)>,
    hover: Option<Btn>,
    drag: Option<Drag>,
    art: Option<(String, Option<ID2D1Bitmap>)>,
    note: String,
    todos: Vec<Todo>,
    edit: Option<Edit>,
    /// Not: piksel kaydırma; yapılacaklar ve GitHub: satır kaydırma
    scroll: f32,
    market: Option<(Arc<Mutex<markets::Data>>, Arc<AtomicBool>)>,
    photos: Vec<PathBuf>,
    photo: Option<(usize, Option<ID2D1Bitmap>)>,
    photo_skip: usize,
    photo_scanned: Option<u64>,
}

pub const DESKTOP_LAYER: isize = 1;

fn data_path(kind: &str, index: usize, ext: &str) -> PathBuf {
    crate::config::dir().join("widgets").join(format!("{kind}-{index}.{ext}"))
}

fn save_file(p: &PathBuf, data: &str) {
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    // Önce geçici dosyaya yaz, sonra yerine taşı: yarım kalan yazım notu bozmasın
    let tmp = p.with_extension("tmp");
    if std::fs::write(&tmp, data).is_ok() {
        let _ = std::fs::rename(&tmp, p);
    }
}

fn note_colors(name: &str) -> Option<(Color, Color)> {
    Some(match name {
        "yellow" => (Color::rgba(255, 228, 140, 255), Color::rgba(58, 46, 0, 255)),
        "pink" => (Color::rgba(255, 190, 211, 255), Color::rgba(74, 18, 40, 255)),
        "green" => (Color::rgba(190, 236, 170, 255), Color::rgba(24, 58, 20, 255)),
        "blue" => (Color::rgba(176, 214, 255, 255), Color::rgba(14, 40, 72, 255)),
        "purple" => (Color::rgba(218, 196, 255, 255), Color::rgba(44, 24, 80, 255)),
        _ => return None, // "panel": temanın panel rengi
    })
}

pub const NOTE_COLORS: [&str; 6] = ["yellow", "pink", "green", "blue", "purple", "panel"];

fn pictures_dir() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join("Pictures")
}

fn scan_photos(dir: &std::path::Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if out.len() >= 2000 {
            return;
        }
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                scan_photos(&p, depth - 1, out);
            }
        } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| PHOTO_EXT.contains(&x.to_lowercase().as_str())) {
            out.push(p);
        }
    }
}

/// Fareyle tıklanan widget'a klavye odağı ver (yazı yazmak için).
fn take_focus(hwnd: HWND) {
    unsafe {
        let fg = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let me = windows::Win32::System::Threading::GetCurrentThreadId();
        let attach = fg != 0 && fg != me && windows::Win32::System::Threading::AttachThreadInput(me, fg, true).as_bool();
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);
        if attach {
            let _ = windows::Win32::System::Threading::AttachThreadInput(me, fg, false);
        }
    }
}

fn lp_xy(lp: LPARAM, scale: f32) -> (f32, f32) {
    ((lp.0 & 0xFFFF) as i16 as f32 / scale, ((lp.0 >> 16) & 0xFFFF) as i16 as f32 / scale)
}

impl Widget {
    pub fn new(index: usize, cfg: &WidgetCfg, ctx: &Ctx, mons: &[Monitor]) -> Result<Option<Widget>> {
        if !cfg.enabled || mons.is_empty() {
            return Ok(None);
        }
        let monitor = if (1..=mons.len()).contains(&cfg.monitor) { cfg.monitor - 1 } else { 0 };
        let mon = mons[monitor].rect;
        let scale = mons[monitor].scale;
        let kind = match cfg.kind.as_str() {
            "clock" => Kind::Clock,
            "weather" => Kind::Weather,
            "media" => Kind::Media,
            "sysmon" => Kind::Sysmon,
            "notes" => Kind::Notes,
            "todo" => Kind::Todo,
            "markets" => Kind::Markets,
            "photo" => Kind::Photo,
            "github" => Kind::Github,
            _ => return Ok(None),
        };
        let items: Vec<markets::Item> = if kind == Kind::Markets {
            let src: Vec<String> = if cfg.items.is_empty() { markets::DEFAULT_ITEMS.iter().map(|s| s.to_string()).collect() } else { cfg.items.clone() };
            src.iter().filter_map(|s| markets::parse(s)).take(8).collect()
        } else {
            Vec::new()
        };
        let (w, h) = match kind {
            Kind::Clock => (280.0, if cfg.calendar { 340.0 } else { 116.0 }),
            Kind::Weather => (280.0, 150.0),
            Kind::Media => (340.0, 120.0),
            Kind::Sysmon => (280.0, 236.0),
            Kind::Notes => (260.0, 240.0),
            Kind::Todo => (290.0, 320.0),
            Kind::Markets => (280.0, 48.0 + items.len().max(1) as f32 * 42.0 + 10.0),
            Kind::Photo => (320.0, 220.0),
            Kind::Github => (330.0, 48.0 + 4.0 * GH_ROW + 8.0),
        };
        let top = cfg.layer == "top";
        // Not ve yapılacaklar klavye odağı alabilmeli; diğerleri hiç etkinleşmez
        let typing = matches!(kind, Kind::Notes | Kind::Todo);
        let noact = if typing { WINDOW_EX_STYLE(0) } else { WS_EX_NOACTIVATE };
        let ex = WS_EX_TOOLWINDOW | noact | if top { WS_EX_TOPMOST } else { WINDOW_EX_STYLE(0) };
        let hwnd = create_layered(ex)?;
        let surf = Surface::new(w, h, scale, &ctx.pal.font)?;
        let px = |v: f32, lo: i32, hi: i32| if v < 0.0 { hi + (v * scale) as i32 } else { lo + (v * scale) as i32 };
        let x = px(cfg.x, mon.left, mon.right);
        let y = px(cfg.y, mon.top, mon.bottom);
        unsafe {
            if !top {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, DESKTOP_LAYER);
            }
            let z = if top { HWND_TOPMOST } else { HWND_BOTTOM };
            let _ = SetWindowPos(hwnd, z, x, y, surf.w_px, surf.h_px, SWP_NOACTIVATE);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        let note = if kind == Kind::Notes { std::fs::read_to_string(data_path("note", index, "txt")).unwrap_or_default() } else { String::new() };
        let todos = if kind == Kind::Todo {
            std::fs::read_to_string(data_path("todo", index, "json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
        } else {
            Vec::new()
        };
        let market = (kind == Kind::Markets).then(|| {
            let state = Arc::new(Mutex::new(markets::Data::default()));
            let stop = Arc::new(AtomicBool::new(false));
            markets::spawn(items, state.clone(), stop.clone(), crate::app::host());
            (state, stop)
        });
        Ok(Some(Widget {
            hwnd,
            index,
            surf,
            kind,
            cfg: cfg.clone(),
            monitor,
            x,
            y,
            w,
            h,
            sig: String::new(),
            buttons: Vec::new(),
            hover: None,
            drag: None,
            art: None,
            note,
            todos,
            edit: None,
            scroll: 0.0,
            market,
            photos: Vec::new(),
            photo: None,
            photo_skip: 0,
            photo_scanned: None,
        }))
    }

    pub fn pos(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn scale(&self) -> f32 {
        self.surf.scale
    }

    fn photo_slot(&self, ctx: &Ctx) -> u64 {
        let t = &ctx.now;
        let secs = t.hour as u64 * 3600 + t.min as u64 * 60 + t.sec as u64;
        secs / self.cfg.interval.max(5) as u64
    }

    fn signature(&self, ctx: &Ctx) -> String {
        let t = &ctx.now;
        let mut hs = std::collections::hash_map::DefaultHasher::new();
        match self.kind {
            Kind::Clock => return format!("{}:{}:{}.{}", t.hour, t.min, if self.cfg.seconds { t.sec } else { 0 }, t.day),
            Kind::Weather => return format!("{:?}", ctx.weather),
            Kind::Media => {
                return format!(
                    "{}{}{}{}{:?}",
                    ctx.media.title,
                    ctx.media.artist,
                    ctx.media.playing,
                    ctx.media.thumb.is_some(),
                    self.hover
                )
            }
            Kind::Sysmon => {
                return format!(
                    "{:.0}{:.0}{:.0}{:.0}{:.0}{}",
                    ctx.sys.cpu,
                    ctx.sys.ram,
                    ctx.sys.disk,
                    ctx.sys.net_down / 1024.0,
                    ctx.sys.net_up / 1024.0,
                    ctx.sys.cpu_hist.len()
                )
            }
            Kind::Notes => (&self.note, &self.edit, self.scroll.to_bits()).hash(&mut hs),
            Kind::Todo => (&self.todos, &self.edit, self.scroll.to_bits(), self.hover).hash(&mut hs),
            Kind::Markets => return format!("{:?}", self.market.as_ref().map(|m| m.0.lock().unwrap().clone())),
            Kind::Photo => (self.photo_slot(ctx), self.photo_skip, self.photos.len(), self.photo.as_ref().map(|p| p.0)).hash(&mut hs),
            Kind::Github => {
                let d = &ctx.feeds.data;
                return format!("{:?}{:?}{}{:?}{}", d.prs, d.gh_error, d.gh_loaded, self.hover, self.scroll);
            }
        }
        format!("{:x}", hs.finish())
    }

    /// Fotoğraf çerçevesi: sıradaki fotoğrafı (gerekirse) yükler.
    fn load_photo(&mut self, ctx: &Ctx) {
        let slot = self.photo_slot(ctx);
        // Klasörü ilk açılışta ve yaklaşık her 30 değişimde bir yeniden tara
        if self.photo_scanned.is_none_or(|s| slot.abs_diff(s) >= 30) {
            let dir = if self.cfg.path.trim().is_empty() { pictures_dir() } else { PathBuf::from(self.cfg.path.trim()) };
            let mut v = Vec::new();
            scan_photos(&dir, 2, &mut v);
            v.sort();
            self.photos = v;
            self.photo_scanned = Some(slot);
        }
        if self.photos.is_empty() {
            self.photo = None;
            return;
        }
        let n = self.photos.len();
        // Sabit ama karışık bir sıra (aynı klasör hep aynı sırayla döner)
        let idx = ((slot as usize + self.photo_skip).wrapping_mul(2654435761) >> 3) % n;
        if self.photo.as_ref().is_some_and(|p| p.0 == idx) {
            return;
        }
        let px = (self.w.max(self.h) * self.surf.scale * 1.25) as u32;
        let bmp = std::fs::read(&self.photos[idx]).ok().and_then(|b| self.surf.bitmap_from_bytes(&b, px));
        self.photo = Some((idx, bmp));
    }

    pub fn paint(&mut self, ctx: &Ctx, force: bool) {
        let sig = self.signature(ctx);
        if !force && sig == self.sig {
            return;
        }
        self.sig = sig;

        if self.kind == Kind::Media {
            let key = ctx.media.track_key();
            if self.art.as_ref().map(|a| &a.0) != Some(&key) || (ctx.media.thumb.is_some() && self.art.as_ref().is_some_and(|a| a.1.is_none())) {
                let bmp = ctx.media.thumb.as_ref().and_then(|b| self.surf.bitmap_from_bytes(b, 160));
                self.art = Some((key, bmp));
            }
        }
        if self.kind == Kind::Photo {
            self.load_photo(ctx);
        }

        let s = &self.surf;
        let full = Rect::new(0.0, 0.0, self.w, self.h);
        let radius = ctx.pal.radius.min(22.0);
        s.begin();
        skin::panel(s, full.inset(0.5), radius, &ctx.pal, false);
        self.buttons = match self.kind {
            Kind::Clock => {
                draw_clock(s, ctx, &self.cfg, self.w);
                vec![]
            }
            Kind::Weather => {
                draw_weather(s, ctx, self.w, self.h);
                vec![]
            }
            Kind::Media => draw_media(s, ctx, self.w, self.hover, self.art.as_ref().and_then(|a| a.1.as_ref())),
            Kind::Sysmon => {
                draw_sysmon(s, ctx, self.w);
                vec![]
            }
            Kind::Notes => self.draw_notes(s, ctx, radius),
            Kind::Todo => self.draw_todo(s, ctx),
            Kind::Markets => {
                let data = self.market.as_ref().map(|m| m.0.lock().unwrap().clone()).unwrap_or_default();
                draw_markets(s, ctx, self.w, &data);
                vec![]
            }
            Kind::Photo => {
                self.draw_photo(s, ctx, radius);
                vec![]
            }
            Kind::Github => draw_github(s, ctx, self.w, self.h, self.hover, self.scroll as usize),
        };
        s.end();
        s.present(self.hwnd, self.x, self.y);
    }

    fn button_at(&self, lp: LPARAM) -> Option<Btn> {
        let (x, y) = lp_xy(lp, self.surf.scale);
        self.buttons.iter().find(|(r, _)| r.contains(x, y)).map(|(_, b)| *b)
    }

    fn note_area(&self) -> Rect {
        Rect::new(16.0, 40.0, self.w - 32.0, self.h - 54.0)
    }

    fn todo_visible(&self) -> usize {
        ((self.h - TODO_TOP - 48.0) / TODO_ROW).floor().max(1.0) as usize
    }

    fn save_note(&self) {
        save_file(&data_path("note", self.index, "txt"), &self.note);
    }

    fn save_todos(&self) {
        if let Ok(t) = serde_json::to_string_pretty(&self.todos) {
            save_file(&data_path("todo", self.index, "json"), &t);
        }
    }

    pub fn next_photo(&mut self) {
        self.photo_skip += 1;
    }

    pub fn clear_done(&mut self) {
        self.todos.retain(|t| !t.done);
        self.save_todos();
        self.scroll = 0.0;
    }

    /// Düzenlemeyi bitirir. `keep_new`: yeni madde eklendiyse hemen bir sonrakine geç.
    fn commit(&mut self, keep_new: bool) {
        let Some(e) = self.edit.take() else { return };
        match e.target {
            EditTarget::Note => {
                self.note = e.text;
                self.save_note();
            }
            EditTarget::Todo(Some(i)) => {
                let t = e.text.trim().to_string();
                if i < self.todos.len() {
                    if t.is_empty() {
                        self.todos.remove(i);
                    } else {
                        self.todos[i].text = t;
                    }
                    self.save_todos();
                }
            }
            EditTarget::Todo(None) => {
                let t = e.text.trim().to_string();
                if !t.is_empty() {
                    self.todos.push(Todo { text: t, done: false });
                    self.save_todos();
                    // Yeni madde görünür kalsın
                    let vis = self.todo_visible();
                    self.scroll = self.todos.len().saturating_sub(vis) as f32;
                    if keep_new {
                        self.edit = Some(Edit { target: EditTarget::Todo(None), text: String::new(), caret: 0 });
                    }
                }
            }
        }
    }

    fn start_edit(&mut self, b: Btn, lp: LPARAM) {
        let target = match b {
            Btn::NoteArea => EditTarget::Note,
            Btn::Row(i) => EditTarget::Todo(Some(i)),
            Btn::AddRow => EditTarget::Todo(None),
            _ => return,
        };
        if self.edit.as_ref().is_some_and(|e| e.target != target) {
            self.commit(false);
        }
        if self.edit.is_none() {
            let text = match target {
                EditTarget::Note => self.note.clone(),
                EditTarget::Todo(Some(i)) => self.todos.get(i).map(|t| t.text.clone()).unwrap_or_default(),
                EditTarget::Todo(None) => String::new(),
            };
            let caret = text.chars().count();
            self.edit = Some(Edit { target, text, caret });
        }
        // Notta imleci tıklanan yere koy
        if target == EditTarget::Note {
            let a = self.note_area();
            let (x, y) = lp_xy(lp, self.surf.scale);
            if let Some(e) = self.edit.as_mut() {
                let u = self.surf.text_hit(&e.text, a.w, NOTE_SIZE, 400, x - a.x, y - a.y + self.scroll);
                e.caret = e.from_utf16(u);
            }
        }
        take_focus(self.hwnd);
    }

    /// Not imleci görünür kalsın diye kaydırmayı ayarlar.
    fn follow_caret(&mut self) {
        let Some(e) = &self.edit else { return };
        if e.target != EditTarget::Note {
            return;
        }
        let a = self.note_area();
        let (_, y, lh) = self.surf.caret_xy(&e.text, a.w, NOTE_SIZE, 400, e.utf16());
        if y + lh - self.scroll > a.h {
            self.scroll = y + lh - a.h;
        } else if y < self.scroll {
            self.scroll = y;
        }
        self.scroll = self.scroll.max(0.0);
    }

    fn edit_char(&mut self, c: char) -> bool {
        let Some(e) = self.edit.as_mut() else { return false };
        let note = e.target == EditTarget::Note;
        match c {
            '\r' if note => e.insert("\n"),
            '\r' => {
                self.commit(true);
                return true;
            }
            '\u{1b}' => {
                if note {
                    self.commit(false);
                } else {
                    self.edit = None;
                }
                return true;
            }
            '\u{8}' => e.backspace(),
            '\u{16}' => {
                if let Some(t) = crate::system::clipboard_text(self.hwnd) {
                    let t = t.replace("\r\n", "\n");
                    let t = if note { t } else { t.replace('\n', " ") };
                    e.insert(&t);
                }
            }
            '\u{1}' => e.caret = e.len(),
            c if c.is_control() => return false,
            c => e.insert(&c.to_string()),
        }
        if note {
            if let Some(e) = &self.edit {
                self.note = e.text.clone();
            }
            self.save_note();
        }
        self.follow_caret();
        true
    }

    fn edit_key(&mut self, vk: VIRTUAL_KEY) -> bool {
        let a = self.note_area();
        let surf = &self.surf;
        let Some(e) = self.edit.as_mut() else { return false };
        let note = e.target == EditTarget::Note;
        let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
        match vk {
            VK_LEFT => e.caret = e.caret.saturating_sub(1),
            VK_RIGHT => e.caret = (e.caret + 1).min(e.len()),
            VK_HOME if !note || ctrl => e.caret = 0,
            VK_END if !note || ctrl => e.caret = e.len(),
            VK_HOME | VK_END | VK_UP | VK_DOWN if note => {
                let (x, y, lh) = surf.caret_xy(&e.text, a.w, NOTE_SIZE, 400, e.utf16());
                let (tx, ty) = match vk {
                    VK_HOME => (0.0, y + lh / 2.0),
                    VK_END => (a.w + 1000.0, y + lh / 2.0),
                    VK_UP => (x, y - lh / 2.0),
                    _ => (x, y + lh * 1.5),
                };
                if ty < 0.0 {
                    e.caret = 0;
                } else {
                    let u = surf.text_hit(&e.text, a.w, NOTE_SIZE, 400, tx, ty);
                    e.caret = e.from_utf16(u);
                }
            }
            VK_DELETE => {
                e.delete();
                if note {
                    self.note = e.text.clone();
                    self.save_note();
                }
            }
            _ => return false,
        }
        self.follow_caret();
        true
    }

    pub fn handle(&mut self, msg: u32, wp: WPARAM, lp: LPARAM, locked: bool) -> Option<(Option<Action>, bool)> {
        match msg {
            WM_LBUTTONDOWN => {
                let b = self.button_at(lp);
                if let Some(b @ (Btn::NoteArea | Btn::Row(_) | Btn::AddRow)) = b {
                    self.start_edit(b, lp);
                    return Some((None, true));
                }
                let had_edit = self.edit.is_some();
                if b.is_none() {
                    self.commit(false);
                }
                if b.is_none() && !locked {
                    let mut cursor = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut cursor);
                        SetCapture(self.hwnd);
                    }
                    self.drag = Some(Drag { cursor, x: self.x, y: self.y, moved: false });
                }
                Some((None, had_edit))
            }
            WM_MOUSEMOVE => {
                if let Some(d) = &mut self.drag {
                    let mut c = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut c);
                    }
                    let (nx, ny) = (d.x + c.x - d.cursor.x, d.y + c.y - d.cursor.y);
                    if (nx - self.x).abs() + (ny - self.y).abs() > 0 {
                        d.moved = true;
                        self.x = nx;
                        self.y = ny;
                        unsafe {
                            let _ = SetWindowPos(self.hwnd, None, nx, ny, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                        }
                    }
                    return Some((None, false));
                }
                let b = self.button_at(lp);
                let repaint = b != self.hover;
                self.hover = b;
                Some((None, repaint))
            }
            WM_LBUTTONUP => {
                if let Some(d) = self.drag.take() {
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    if d.moved {
                        return Some((Some(Action::SavePos(self.index)), false));
                    }
                    // Fotoğraf çerçevesine tıklama: sıradaki fotoğraf
                    if self.kind == Kind::Photo {
                        self.photo_skip += 1;
                        return Some((None, true));
                    }
                    return Some((None, false));
                }
                let act = match self.button_at(lp) {
                    Some(Btn::Prev) => Some(Action::Media(MediaCmd::Prev)),
                    Some(Btn::Toggle) => Some(Action::Media(MediaCmd::Toggle)),
                    Some(Btn::Next) => Some(Action::Media(MediaCmd::Next)),
                    Some(Btn::Check(i)) => {
                        if let Some(t) = self.todos.get_mut(i) {
                            t.done = !t.done;
                            self.save_todos();
                        }
                        None
                    }
                    Some(Btn::Remove(i)) => {
                        if i < self.todos.len() {
                            if self.edit.as_ref().is_some_and(|e| e.target == EditTarget::Todo(Some(i))) {
                                self.edit = None;
                            }
                            self.todos.remove(i);
                            self.save_todos();
                        }
                        None
                    }
                    Some(Btn::ClearDone) => {
                        self.clear_done();
                        None
                    }
                    Some(Btn::Pr(i)) => ctx_pr_url(i).map(Action::OpenUrl),
                    _ => None,
                };
                Some((act, true))
            }
            WM_MOUSEWHEEL => {
                let delta = (wp.0 >> 16) as u16 as i16 as f32 / 120.0;
                match self.kind {
                    Kind::Notes => {
                        let a = self.note_area();
                        let text = self.edit.as_ref().map_or(&self.note, |e| &e.text);
                        let total = self.surf.text_height(text, a.w, NOTE_SIZE, 400);
                        self.scroll = (self.scroll - delta * 40.0).clamp(0.0, (total - a.h).max(0.0));
                    }
                    Kind::Todo => {
                        let max = self.todos.len().saturating_sub(self.todo_visible()) as f32;
                        self.scroll = (self.scroll - delta).round().clamp(0.0, max);
                    }
                    Kind::Github => self.scroll = (self.scroll - delta).round().max(0.0),
                    Kind::Photo => {
                        self.photo_skip = self.photo_skip.wrapping_add(if delta < 0.0 { 1 } else { usize::MAX });
                    }
                    _ => return None,
                }
                Some((None, true))
            }
            WM_CHAR if self.edit.is_some() => {
                let c = char::from_u32(wp.0 as u32).unwrap_or('\0');
                Some((None, self.edit_char(c)))
            }
            WM_KEYDOWN if self.edit.is_some() => {
                let r = self.edit_key(VIRTUAL_KEY(wp.0 as u16));
                r.then_some((None, true))
            }
            WM_KILLFOCUS if self.edit.is_some() => {
                self.commit(false);
                Some((None, true))
            }
            WM_ACTIVATE if (wp.0 & 0xFFFF) as u32 == WA_INACTIVE && self.edit.is_some() => {
                self.commit(false);
                Some((None, true))
            }
            WM_RBUTTONUP => {
                self.commit(false);
                Some((Some(Action::Menu { widget: Some(self.index) }), true))
            }
            _ => None,
        }
    }

    fn draw_notes(&self, s: &Surface, ctx: &Ctx, radius: f32) -> Vec<(Rect, Btn)> {
        let p = &ctx.pal;
        let full = Rect::new(0.0, 0.0, self.w, self.h);
        let (bg, fg) = match note_colors(&self.cfg.color) {
            Some((bg, fg)) => {
                s.fill_round(full.inset(0.5), radius, bg);
                (Some(bg), fg)
            }
            None => (None, p.text),
        };
        let dim = fg.alpha(0.55);
        s.glyph('\u{E70B}', Rect::new(14.0, 10.0, 20.0, 22.0), 13.0, dim);
        s.text(tr("Not"), Rect::new(38.0, 10.0, self.w - 60.0, 22.0), 13.0, 600, dim, Align::Left);
        if bg.is_some() {
            // Yapışkan not şeridi
            s.fill_round(Rect::new(self.w / 2.0 - 28.0, -6.0, 56.0, 14.0), 3.0, Color::rgba(255, 255, 255, 90));
        }
        let a = self.note_area();
        let editing = self.edit.as_ref().filter(|e| e.target == EditTarget::Note);
        let text = editing.map_or(self.note.as_str(), |e| e.text.as_str());
        if text.is_empty() && editing.is_none() {
            s.text(tr("Yazmak için tıkla…"), Rect::new(a.x, a.y, a.w, 22.0), NOTE_SIZE, 400, fg.alpha(0.4), Align::Left);
        } else {
            s.text_wrapped(text, a, NOTE_SIZE, 400, fg, self.scroll);
        }
        if let Some(e) = editing {
            let (cx, cy, lh) = s.caret_xy(&e.text, a.w, NOTE_SIZE, 400, e.utf16());
            let y = a.y + cy - self.scroll;
            if y >= a.y - 1.0 && y + lh <= a.y + a.h + 1.0 {
                s.fill_round(Rect::new(a.x + cx, y + 1.0, 1.5, lh - 2.0), 0.5, if bg.is_some() { fg } else { p.accent });
            }
        }
        vec![(a, Btn::NoteArea)]
    }

    fn draw_todo(&self, s: &Surface, ctx: &Ctx) -> Vec<(Rect, Btn)> {
        let p = &ctx.pal;
        let w = self.w;
        let mut out = Vec::new();
        let done = self.todos.iter().filter(|t| t.done).count();
        s.text(tr("Yapılacaklar"), Rect::new(18.0, 12.0, w - 110.0, 22.0), 13.0, 600, p.dim, Align::Left);
        if !self.todos.is_empty() {
            s.text(&format!("{done}/{}", self.todos.len()), Rect::new(w - 70.0, 12.0, 52.0, 22.0), 12.0, 600, p.dim, Align::Right);
        }
        if done > 0 {
            let r = Rect::new(w - 100.0, 9.0, 28.0, 28.0);
            if self.hover == Some(Btn::ClearDone) {
                s.fill_circle(r.x + 14.0, r.y + 14.0, 13.0, p.text.alpha(0.08));
            }
            s.glyph('\u{E74D}', r, 12.0, p.dim);
            out.push((r, Btn::ClearDone));
        }

        let vis = self.todo_visible();
        let first = (self.scroll as usize).min(self.todos.len().saturating_sub(vis));
        if self.todos.is_empty() && self.edit.is_none() {
            s.text(tr("Henüz görev yok"), Rect::new(18.0, TODO_TOP + 6.0, w - 36.0, 22.0), 13.0, 400, p.dim.alpha(0.7), Align::Left);
        }
        for (row, i) in (first..self.todos.len()).take(vis).enumerate() {
            let t = &self.todos[i];
            let y = TODO_TOP + row as f32 * TODO_ROW;
            let hov = matches!(self.hover, Some(Btn::Row(j) | Btn::Check(j) | Btn::Remove(j)) if j == i);
            if hov {
                s.fill_round(Rect::new(10.0, y + 1.0, w - 20.0, TODO_ROW - 2.0), 8.0, p.text.alpha(0.05));
            }
            let cb = Rect::new(14.0, y, 28.0, TODO_ROW);
            let (cx, cy) = (cb.x + 13.0, y + TODO_ROW / 2.0);
            if t.done {
                s.fill_circle(cx, cy, 8.0, p.accent);
                s.glyph('\u{E73E}', Rect::new(cx - 8.0, cy - 8.0, 16.0, 16.0), 9.0, p.on_accent);
            } else {
                s.fill_circle(cx, cy, 8.0, p.text.alpha(if self.hover == Some(Btn::Check(i)) { 0.35 } else { 0.22 }));
                s.fill_circle(cx, cy, 6.5, Color { a: 1.0, ..ctx.pal.bg });
            }
            out.push((cb, Btn::Check(i)));
            let tr_ = Rect::new(46.0, y, w - 46.0 - 36.0, TODO_ROW);
            let editing = self.edit.as_ref().filter(|e| e.target == EditTarget::Todo(Some(i)));
            match editing {
                Some(e) => self.draw_line_edit(s, ctx, e, tr_),
                None => {
                    let c = if t.done { p.dim.alpha(0.7) } else { p.text };
                    s.text(&t.text, tr_, 13.0, 400, c, Align::Left);
                    if t.done {
                        let tw = s.measure(&t.text, 13.0, 400).min(tr_.w);
                        s.line(tr_.x, cy, tr_.x + tw, cy, 1.0, p.dim.alpha(0.8));
                    }
                }
            }
            out.push((tr_, Btn::Row(i)));
            if hov {
                let rr = Rect::new(w - 40.0, y, 28.0, TODO_ROW);
                let c = if self.hover == Some(Btn::Remove(i)) { Color::rgba(255, 107, 107, 255) } else { p.dim };
                s.glyph('\u{E711}', rr, 10.0, c);
                out.push((rr, Btn::Remove(i)));
            }
        }
        if self.todos.len() > vis {
            // Kaydırma göstergesi
            let track = self.h - TODO_TOP - 52.0;
            let th = (track * vis as f32 / self.todos.len() as f32).max(14.0);
            let ty = TODO_TOP + (track - th) * first as f32 / (self.todos.len() - vis).max(1) as f32;
            s.fill_round(Rect::new(w - 7.0, ty, 3.0, th), 1.5, p.dim.alpha(0.4));
        }

        let add = Rect::new(12.0, self.h - 42.0, w - 24.0, 30.0);
        let adding = self.edit.as_ref().filter(|e| e.target == EditTarget::Todo(None));
        s.fill_round(add, 9.0, p.text.alpha(if adding.is_some() || self.hover == Some(Btn::AddRow) { 0.09 } else { 0.05 }));
        match adding {
            Some(e) => self.draw_line_edit(s, ctx, e, Rect::new(add.x + 12.0, add.y, add.w - 24.0, add.h)),
            None => {
                s.glyph('\u{E710}', Rect::new(add.x + 6.0, add.y, 22.0, add.h), 11.0, p.accent);
                s.text(tr("Yeni görev ekle"), Rect::new(add.x + 30.0, add.y, add.w - 40.0, add.h), 13.0, 400, p.dim, Align::Left);
            }
        }
        out.push((add, Btn::AddRow));
        out
    }

    /// Tek satırlık düzenleme alanı (yapılacaklar maddesi) ve imleci.
    fn draw_line_edit(&self, s: &Surface, ctx: &Ctx, e: &Edit, r: Rect) {
        let p = &ctx.pal;
        let before: String = e.text.chars().take(e.caret).collect();
        let cw = s.measure(&before, 13.0, 400);
        // İmleç sağa taşarsa metni sola kaydır
        let shift = (cw - (r.w - 4.0)).max(0.0);
        s.push_clip(r);
        s.text(&e.text, Rect::new(r.x - shift, r.y, r.w + shift + 2000.0, r.h), 13.0, 400, p.text, Align::Left);
        s.pop_clip();
        s.fill_round(Rect::new(r.x + cw - shift, r.y + 7.0, 1.5, r.h - 14.0), 0.5, p.accent);
    }

    fn draw_photo(&self, s: &Surface, ctx: &Ctx, radius: f32) {
        let p = &ctx.pal;
        let full = Rect::new(0.0, 0.0, self.w, self.h);
        match self.photo.as_ref().and_then(|p| p.1.as_ref()) {
            Some(bmp) => {
                s.image(bmp, full.inset(1.0), radius);
                s.stroke_round(full.inset(0.5), radius, 1.0, Color::rgba(255, 255, 255, 30));
            }
            None => {
                s.glyph('\u{E91B}', Rect::new(0.0, self.h / 2.0 - 46.0, self.w, 40.0), 26.0, p.dim);
                let msg = if self.photos.is_empty() { tr("Klasörde fotoğraf yok") } else { tr("Fotoğraf açılamadı") };
                s.text(msg, Rect::new(16.0, self.h / 2.0, self.w - 32.0, 22.0), 13.0, 600, p.text, Align::Center);
                s.text(tr("Sağ tık › Fotoğraf klasörü seç"), Rect::new(16.0, self.h / 2.0 + 22.0, self.w - 32.0, 20.0), 11.5, 400, p.dim, Align::Center);
            }
        }
    }
}

thread_local! {
    /// Son çizilen GitHub PR adresleri (tıklamada açmak için)
    static PR_URLS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn ctx_pr_url(i: usize) -> Option<String> {
    PR_URLS.with(|u| u.borrow().get(i).cloned())
}

impl Drop for Widget {
    fn drop(&mut self) {
        self.commit(false);
        if let Some((_, stop)) = &self.market {
            stop.store(true, Ordering::SeqCst);
        }
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn draw_markets(s: &Surface, ctx: &Ctx, w: f32, d: &markets::Data) {
    let p = &ctx.pal;
    let up = Color::rgba(46, 204, 113, 255);
    let down = Color::rgba(255, 107, 107, 255);
    s.text(tr("Piyasalar"), Rect::new(18.0, 12.0, w - 120.0, 22.0), 13.0, 600, p.dim, Align::Left);
    let status = match (&d.updated, d.error) {
        (Some(t), false) => t.clone(),
        (Some(t), true) => format!("⚠ {t}"),
        (None, true) => tr("Bağlantı yok").to_string(),
        (None, false) => tr("Yükleniyor…").to_string(),
    };
    s.text(&status, Rect::new(w - 118.0, 12.0, 100.0, 22.0), 11.5, 400, p.dim, Align::Right);
    for (i, q) in d.quotes.iter().enumerate() {
        let y = 44.0 + i as f32 * 42.0;
        if i > 0 {
            s.line(18.0, y - 3.0, w - 18.0, y - 3.0, 1.0, p.text.alpha(0.06));
        }
        s.text(&q.label, Rect::new(18.0, y, w - 150.0, 20.0), 13.5, 600, p.text, Align::Left);
        s.text(&q.code, Rect::new(18.0, y + 19.0, 80.0, 16.0), 11.0, 400, p.dim, Align::Left);
        s.text(&q.price, Rect::new(w - 168.0, y, 150.0, 20.0), 14.0, 600, p.text, Align::Right);
        if let Some(c) = q.change {
            let (col, arrow) = if c >= 0.0 { (up, '▲') } else { (down, '▼') };
            let txt = format!("{arrow} %{}", markets::fmt_num(c.abs(), 2));
            s.text(&txt, Rect::new(w - 168.0, y + 19.0, 150.0, 16.0), 11.0, 600, col, Align::Right);
        }
    }
}

fn draw_github(s: &Surface, ctx: &Ctx, w: f32, h: f32, hover: Option<Btn>, scroll: usize) -> Vec<(Rect, Btn)> {
    let p = &ctx.pal;
    let d = &ctx.feeds.data;
    let mut out = Vec::new();
    s.glyph('\u{E943}', Rect::new(14.0, 10.0, 22.0, 24.0), 13.0, p.dim);
    s.text("GitHub", Rect::new(38.0, 10.0, 120.0, 24.0), 13.0, 600, p.dim, Align::Left);
    let msg = match d.gh_error.as_deref() {
        Some("gh") => Some(tr("GitHub CLI (gh) bulunamadı")),
        Some(_) => Some(tr("Terminalde 'gh auth login' ile giriş yap")),
        None if !d.gh_loaded => Some(tr("Yükleniyor…")),
        None if d.prs.is_empty() => Some(tr("Açık pull request yok")),
        None => None,
    };
    if !d.prs.is_empty() {
        let n = d.prs.len();
        let label = if i18n::en() { format!("{n} open PR") } else { format!("{n} açık PR") };
        s.text(&label, Rect::new(w - 140.0, 10.0, 122.0, 24.0), 12.0, 600, p.accent, Align::Right);
    }
    if let Some(m) = msg {
        s.text(m, Rect::new(18.0, 48.0, w - 36.0, 22.0), 13.0, 400, p.dim, Align::Left);
        PR_URLS.with(|u| u.borrow_mut().clear());
        return out;
    }
    let rows = ((h - 48.0) / GH_ROW).floor().max(1.0) as usize;
    let first = scroll.min(d.prs.len().saturating_sub(rows));
    for (row, (i, pr)) in d.prs.iter().enumerate().skip(first).take(rows).enumerate() {
        let r = Rect::new(8.0, 42.0 + row as f32 * GH_ROW, w - 16.0, GH_ROW - 4.0);
        if hover == Some(Btn::Pr(i)) {
            s.fill_round(r, 9.0, p.text.alpha(0.06));
        }
        let ci = match pr.ci.as_str() {
            "SUCCESS" => Some(Color::rgba(46, 204, 113, 255)),
            "FAILURE" | "ERROR" => Some(Color::rgba(255, 107, 107, 255)),
            "PENDING" | "EXPECTED" => Some(Color::rgba(255, 193, 7, 255)),
            _ => None,
        };
        if let Some(c) = ci {
            s.fill_circle(r.x + 14.0, r.y + 13.0, 4.0, c);
        }
        s.text(&format!("{} #{}", pr.repo, pr.number), Rect::new(r.x + 26.0, r.y + 3.0, r.w - 34.0, 18.0), 11.0, 500, p.dim, Align::Left);
        s.text(&pr.title, Rect::new(r.x + 26.0, r.y + 20.0, r.w - 34.0, 20.0), 13.0, 500, p.text, Align::Left);
        out.push((r, Btn::Pr(i)));
    }
    PR_URLS.with(|u| *u.borrow_mut() = d.prs.iter().map(|p| p.url.clone()).collect());
    out
}

fn draw_clock(s: &Surface, ctx: &Ctx, cfg: &WidgetCfg, w: f32) {
    let p = &ctx.pal;
    let t = &ctx.now;
    let time = format!("{:02}:{:02}", t.hour, t.min);
    s.text(&time, Rect::new(20.0, 10.0, 220.0, 66.0), 56.0, 300, p.text, Align::Left);
    if cfg.seconds {
        let tw = s.measure(&time, 56.0, 300);
        s.text(&format!("{:02}", t.sec), Rect::new(26.0 + tw, 34.0, 48.0, 30.0), 20.0, 400, p.accent, Align::Left);
    }
    let date = format!("{} {}, {}", t.day, i18n::month(t.month), i18n::day(t.dow));
    s.text(&date, Rect::new(22.0, 74.0, w - 44.0, 24.0), 15.0, 400, p.dim, Align::Left);

    if !cfg.calendar {
        return;
    }
    s.line(20.0, 112.0, w - 20.0, 112.0, 1.0, p.dim.alpha(0.25));
    s.text(&format!("{} {}", i18n::month(t.month), t.year), Rect::new(20.0, 120.0, w - 40.0, 24.0), 14.0, 600, p.text, Align::Left);

    let cell = (w - 40.0) / 7.0;
    for (i, d) in i18n::days_short().iter().enumerate() {
        let c = if i >= 5 { p.accent.alpha(0.8) } else { p.dim };
        s.text(d, Rect::new(20.0 + i as f32 * cell, 148.0, cell, 20.0), 11.5, 600, c, Align::Center);
    }
    let first_sun = (t.dow as i32 - (t.day as i32 - 1)).rem_euclid(7);
    let first = (first_sun + 6) % 7;
    let days = days_in_month(t.year, t.month) as i32;
    for d in 1..=days {
        let idx = first + d - 1;
        let (col, row) = (idx % 7, idx / 7);
        let cx = 20.0 + col as f32 * cell;
        let cy = 172.0 + row as f32 * 27.0;
        let r = Rect::new(cx, cy, cell, 25.0);
        let today = d == t.day as i32;
        if today {
            s.fill_circle(cx + cell / 2.0, cy + 12.5, 12.5, p.accent);
        }
        let c = if today {
            p.on_accent
        } else if col >= 5 {
            p.text.alpha(0.6)
        } else {
            p.text
        };
        s.text(&d.to_string(), r, 12.5, if today { 700 } else { 400 }, c, Align::Center);
    }
}

fn draw_weather(s: &Surface, ctx: &Ctx, w: f32, h: f32) {
    let p = &ctx.pal;
    s.text(&ctx.cfg.weather.city, Rect::new(20.0, 12.0, w - 110.0, 22.0), 13.0, 600, p.dim, Align::Left);
    let Some(wt) = ctx.weather else {
        s.text(tr("Hava durumu yükleniyor…"), Rect::new(20.0, 40.0, w - 40.0, h - 60.0), 14.0, 400, p.dim, Align::Left);
        return;
    };
    s.text(&format!("{:.0}°", wt.temp), Rect::new(18.0, 32.0, 160.0, 60.0), 50.0, 300, p.text, Align::Left);
    s.text(wt.description(), Rect::new(20.0, 92.0, w - 110.0, 22.0), 14.0, 500, p.text, Align::Left);
    let mut extra = if i18n::en() { format!("Wind {:.0} km/h", wt.wind) } else { format!("Rüzgar {:.0} km/s", wt.wind) };
    if wt.tmax.is_finite() && wt.tmin.is_finite() {
        extra = format!("Y {:.0}°  D {:.0}°  ·  {}", wt.tmax, wt.tmin, extra);
    }
    s.text(&extra, Rect::new(20.0, 116.0, w - 40.0, 20.0), 12.0, 400, p.dim, Align::Left);
    draw_sky(s, wt.sky(), wt.is_day, w - 64.0, 62.0, 84.0);
}

fn draw_media(s: &Surface, ctx: &Ctx, w: f32, hover: Option<Btn>, art: Option<&ID2D1Bitmap>) -> Vec<(Rect, Btn)> {
    let p = &ctx.pal;
    let m = ctx.media;
    let ar = Rect::new(14.0, 14.0, 92.0, 92.0);
    match art {
        Some(b) => s.image(b, ar, 10.0),
        None => {
            s.fill_round(ar, 10.0, p.accent.alpha(0.18));
            s.glyph('\u{E8D6}', ar, 34.0, p.accent);
        }
    }
    let tx = 120.0;
    if !m.active {
        s.text(tr("Çalan bir şey yok"), Rect::new(tx, 30.0, w - tx - 14.0, 24.0), 15.0, 600, p.dim, Align::Left);
        s.text(tr("Spotify, YouTube vb. başlatın"), Rect::new(tx, 54.0, w - tx - 14.0, 20.0), 12.0, 400, p.dim, Align::Left);
        return vec![];
    }
    s.text(&m.title, Rect::new(tx, 16.0, w - tx - 14.0, 26.0), 16.0, 600, p.text, Align::Left);
    s.text(&m.artist, Rect::new(tx, 41.0, w - tx - 14.0, 20.0), 13.0, 400, p.dim, Align::Left);

    let mut out = Vec::new();
    let white = p.text;
    let btns = [
        (Btn::Prev, '\u{E892}'),
        (Btn::Toggle, if m.playing { '\u{E769}' } else { '\u{E768}' }),
        (Btn::Next, '\u{E893}'),
    ];
    for (i, (b, g)) in btns.iter().enumerate() {
        let r = Rect::new(tx - 4.0 + i as f32 * 42.0, 68.0, 36.0, 36.0);
        let (cx, cy) = (r.x + 18.0, r.y + 18.0);
        if *b == Btn::Toggle {
            let bg = if hover == Some(*b) { p.accent.alpha(0.8) } else { p.accent };
            s.fill_circle(cx, cy, 17.0, bg);
            s.glyph(*g, r, 14.0, p.on_accent);
        } else {
            if hover == Some(*b) {
                s.fill_circle(cx, cy, 17.0, white.alpha(0.1));
            }
            s.glyph(*g, r, 14.0, p.text);
        }
        out.push((r, *b));
    }
    out
}

fn draw_sysmon(s: &Surface, ctx: &Ctx, w: f32) {
    let p = &ctx.pal;
    let sys = ctx.sys;
    let warn = Color::rgba(255, 107, 107, 255);
    s.text(tr("Sistem"), Rect::new(20.0, 10.0, w - 40.0, 24.0), 13.0, 600, p.dim, Align::Left);

    let rows = [
        (tr("İşlemci"), format!("{:.0}%", sys.cpu), sys.cpu),
        (tr("Bellek"), format!("{:.1} / {:.1} GB", sys.ram_used_gb, sys.ram_total_gb), sys.ram),
        ("Disk C:", if i18n::en() { format!("{:.0} GB free", sys.disk_free_gb) } else { format!("{:.0} GB boş", sys.disk_free_gb) }, sys.disk),
    ];
    for (i, (label, value, pct)) in rows.iter().enumerate() {
        let y = 38.0 + i as f32 * 40.0;
        s.text(label, Rect::new(20.0, y, 120.0, 20.0), 13.0, 400, p.text, Align::Left);
        s.text(value, Rect::new(110.0, y, w - 130.0, 20.0), 13.0, 600, p.text, Align::Right);
        let track = Rect::new(20.0, y + 23.0, w - 40.0, 6.0);
        s.fill_round(track, 3.0, p.text.alpha(0.08));
        let c = if *pct > 85.0 { warn } else { p.accent };
        s.fill_round(Rect::new(track.x, track.y, (track.w * pct / 100.0).max(6.0), 6.0), 3.0, c);
    }

    s.text(
        &format!("↓ {}    ↑ {}", fmt_speed(sys.net_down), fmt_speed(sys.net_up)),
        Rect::new(20.0, 158.0, w - 40.0, 20.0),
        12.0,
        400,
        p.dim,
        Align::Left,
    );

    let g = Rect::new(20.0, 184.0, w - 40.0, 36.0);
    s.fill_round(g.inset(-2.0), 6.0, p.text.alpha(0.03));
    let pts = |hist: &std::collections::VecDeque<f32>| -> Vec<(f32, f32)> {
        let off = HISTORY - hist.len();
        hist.iter()
            .enumerate()
            .map(|(i, v)| (g.x + (off + i) as f32 * g.w / (HISTORY - 1) as f32, g.y + g.h - g.h * v / 100.0))
            .collect()
    };
    let cpu = pts(&sys.cpu_hist);
    if cpu.len() > 1 {
        let mut area = cpu.clone();
        area.push((cpu.last().unwrap().0, g.y + g.h));
        area.push((cpu[0].0, g.y + g.h));
        s.fill_poly(&area, p.accent.alpha(0.18));
        s.polyline(&cpu, 1.5, p.accent);
    }
    let ram = pts(&sys.ram_hist);
    if ram.len() > 1 {
        s.polyline(&ram, 1.5, Color::rgba(180, 140, 255, 220));
    }
}

pub fn draw_sky(s: &Surface, sky: Sky, is_day: bool, cx: f32, cy: f32, size: f32) {
    let u = size / 100.0;
    let at = |x: f32, y: f32| (cx + (x - 50.0) * u, cy + (y - 50.0) * u);
    let sun_c = Color::rgba(255, 200, 61, 255);
    let moon_c = Color::rgba(232, 230, 217, 255);
    let cloud_c = Color::rgba(230, 233, 239, 255);

    let sun = |x: f32, y: f32, r: f32| {
        let (sx, sy) = at(x, y);
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::FRAC_PI_4;
            let (c, sn) = (a.cos(), a.sin());
            s.line(sx + c * r * 1.4 * u, sy + sn * r * 1.4 * u, sx + c * r * 1.8 * u, sy + sn * r * 1.8 * u, 3.0 * u.max(0.4), sun_c);
        }
        s.fill_circle(sx, sy, r * u, sun_c);
    };
    let moon = |x: f32, y: f32, r: f32| {
        let (mx, my) = at(x, y);
        s.crescent(mx, my, r * u, moon_c);
    };
    let cloud = |x: f32, y: f32, k: f32, c: Color| {
        let p = |dx: f32, dy: f32| at(x + dx * k, y + dy * k);
        let (ax, ay) = p(-17.0, 2.0);
        s.fill_circle(ax, ay, 13.0 * k * u, c);
        let (bx, by) = p(1.0, -6.0);
        s.fill_circle(bx, by, 19.0 * k * u, c);
        let (dx, dy) = p(18.0, 3.0);
        s.fill_circle(dx, dy, 12.0 * k * u, c);
        let (rx, ry) = p(-30.0, 2.0);
        s.fill_round(Rect::new(rx, ry, 60.0 * k * u, 14.0 * k * u), 7.0 * k * u, c);
    };
    let celestial = |x: f32, y: f32, r: f32| if is_day { sun(x, y, r) } else { moon(x, y, r * 1.15) };
    let drops = |n: usize, len: f32, c: Color, dots: bool| {
        for i in 0..n {
            let x = 30.0 + i as f32 * (40.0 / (n - 1).max(1) as f32);
            let (x1, y1) = at(x, 70.0);
            if dots {
                s.fill_circle(x1, y1 + 4.0 * u * (i % 2) as f32, 3.5 * u, c);
            } else {
                let (x2, y2) = at(x - 5.0, 70.0 + len);
                s.line(x1, y1, x2, y2, 3.0 * u.max(0.4), c);
            }
        }
    };

    match sky {
        Sky::Clear => celestial(50.0, 50.0, 20.0),
        Sky::PartlyCloudy => {
            celestial(38.0, 36.0, 15.0);
            cloud(56.0, 60.0, 0.95, cloud_c);
        }
        Sky::Cloudy => {
            cloud(38.0, 42.0, 0.75, Color::rgba(154, 163, 176, 255));
            cloud(56.0, 56.0, 1.0, cloud_c);
        }
        Sky::Fog => {
            cloud(50.0, 40.0, 0.9, Color::rgba(201, 206, 214, 255));
            for (i, y) in [64.0, 74.0, 84.0].iter().enumerate() {
                let (x1, yy) = at(24.0 + i as f32 * 6.0, *y);
                let (x2, _) = at(76.0 - i as f32 * 6.0, *y);
                s.line(x1, yy, x2, yy, 3.0 * u.max(0.4), Color::rgba(201, 206, 214, 200));
            }
        }
        Sky::Drizzle => {
            cloud(50.0, 42.0, 1.0, cloud_c);
            drops(3, 8.0, Color::rgba(90, 176, 255, 255), false);
        }
        Sky::Rain => {
            cloud(50.0, 42.0, 1.0, Color::rgba(190, 198, 210, 255));
            drops(4, 16.0, Color::rgba(90, 176, 255, 255), false);
        }
        Sky::Snow => {
            cloud(50.0, 42.0, 1.0, cloud_c);
            drops(5, 0.0, Color::rgba(255, 255, 255, 255), true);
        }
        Sky::Storm => {
            cloud(50.0, 40.0, 1.0, Color::rgba(140, 149, 163, 255));
            let bolt: Vec<(f32, f32)> =
                [(52.0, 54.0), (40.0, 74.0), (49.0, 74.0), (44.0, 92.0), (62.0, 68.0), (53.0, 68.0), (58.0, 54.0)]
                    .iter()
                    .map(|&(x, y)| at(x, y))
                    .collect();
            s.fill_poly(&bolt, sun_c);
        }
    }
}
