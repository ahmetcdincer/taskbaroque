use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Once;
use std::time::{Duration, Instant};

use windows::core::{w, Result, HSTRING};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{eRender, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::DataExchange::AddClipboardFormatListener;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_LBUTTON, VK_N};
use windows::Win32::UI::Shell::{DragAcceptFiles, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::{create_layered, Action, Ctx};
use crate::bar;
use crate::gfx::{Align, Color, Rect, Surface};
use crate::media::MediaCmd;
use crate::monitors::Monitor;
use crate::notify::AgentEvent;
use crate::sysinfo::fmt_speed;
use crate::system::{self, Audio};
use crate::i18n::{self, tr};
use crate::util::{self, days_in_month};

const WM_MOUSELEAVE_: u32 = 0x02A3;
const WM_CLIPBOARDUPDATE_: u32 = 0x031D;
const TIMER: usize = 1;
pub const TIMER_POLL: usize = 2;
const W: f32 = 520.0;
const H: f32 = 236.0;
const OPEN: (f32, f32) = (500.0, 218.0);
const ALERT: (f32, f32) = (390.0, 60.0);
const ALERT_SECS: f32 = 9.0;
const BARS: usize = 5;
const TABS_Y: f32 = 12.0;
const CONTENT_Y: f32 = 50.0;
const NET_SHOW: f64 = 300.0 * 1024.0;
pub const WM_APP_FRAME: u32 = WM_APP + 9;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Tab {
    Media,
    Mixer,
    Screen,
    Agenda,
    Agents,
    Github,
    Timer,
    Clip,
    Shelf,
    Quick,
    History,
}

const TABS: [(Tab, char, &str); 11] = [
    (Tab::Media, '\u{E8D6}', "Medya"),
    (Tab::Mixer, '\u{E995}', "Ses"),
    (Tab::Agenda, '\u{E787}', "Ajanda"),
    (Tab::Agents, '\u{E99A}', "Agent"),
    (Tab::Github, '\u{E943}', "GitHub"),
    (Tab::Timer, '\u{E916}', "Süre"),
    (Tab::Clip, '\u{E77F}', "Pano"),
    (Tab::Shelf, '\u{E7B8}', "Raf"),
    (Tab::Quick, '\u{E945}', "Hızlı"),
    (Tab::Screen, '\u{E823}', "Ekran süresi"),
    (Tab::History, '\u{E81C}', "Geçmiş"),
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Quick {
    Dark,
    Mute,
    Mic,
    Bluetooth,
    Wifi,
    Shot,
    Lock,
    Focus,
    Picker,
    Desktop,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Btn {
    Tab(Tab),
    Prev,
    Toggle,
    Next,
    Calendar,
    Agent(usize),
    TimerGo,
    TimerReset,
    TimerAdd,
    Preset(u32),
    Clip(usize),
    ClipClear,
    ShelfItem(usize),
    ShelfRemove(usize),
    ShelfClear,
    Quick(Quick),
    Pr(usize),
    Hist(usize),
    HistClear,
    MixSlider(usize),
    MixMute(usize),
}

struct AgentSession {
    id: String,
    project: String,
    state: String,
    detail: String,
    since: Instant,
    started: Instant,
    target: isize,
}

#[derive(Default)]
struct Countdown {
    total: f32,
    left: f32,
    running: bool,
}

struct ShelfItem {
    path: String,
    name: String,
    icon: Option<ID2D1Bitmap>,
}

pub struct Notch {
    pub hwnd: HWND,
    surf: Surface,
    mon: Monitor,
    mode: String,
    x: i32,
    y: i32,
    open: f32,
    vel: f32,
    target: f32,
    hover: bool,
    hover_for: f32,
    away_for: f32,
    tracking: bool,
    btn_hover: Option<Btn>,
    buttons: Vec<(Rect, Btn)>,
    levels: [f32; BARS],
    meters: Vec<IAudioMeterInformation>,
    gain: f32,
    meter_retry: f32,
    art: Option<(String, Option<ID2D1Bitmap>)>,
    last: Instant,
    t: f32,
    interval: u32,
    alert: Option<(String, String)>,
    alert_target: isize,
    alert_left: f32,
    al: f32,
    al_vel: f32,
    slim: f32,
    covered: bool,
    shape: Rect,
    visible: bool,

    tab: Tab,
    agents: Vec<AgentSession>,
    countdown: Countdown,
    timer_done: bool,
    clips: Vec<String>,
    clip_flash: Option<(usize, f32)>,
    shelf: Vec<ShelfItem>,
    press: Option<(Btn, POINT)>,
    audio: Audio,
    audio_age: f32,
    volume: Option<(f32, bool)>,
    osd: f32,
    mic_users: Vec<String>,
    cam_users: Vec<String>,
    privacy_age: f32,
    poll_last: Instant,
    pr_urls: Vec<String>,
    scroll: usize,
    hist_targets: Vec<isize>,
    mixer: Vec<system::AppVolume>,
    mixer_age: f32,
    mix_drag: Option<usize>,
    app_icons: std::collections::HashMap<String, Option<ID2D1Bitmap>>,
}

impl Notch {
    pub fn new(ctx: &Ctx, mon: Monitor, mode: &str) -> Result<Notch> {
        let hwnd = create_layered(WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE)?;
        let surf = Surface::new(W, H, mon.scale, &ctx.pal.font)?;
        let x = mon.rect.left + ((mon.rect.right - mon.rect.left) - surf.w_px) / 2;
        let y = mon.rect.top;
        let mut audio = Audio::default();
        audio.refresh();
        let mut n = Notch {
            hwnd,
            surf,
            mon,
            mode: mode.to_string(),
            x,
            y,
            open: 0.0,
            vel: 0.0,
            target: 0.0,
            hover: false,
            hover_for: 0.0,
            away_for: 1.0,
            tracking: false,
            btn_hover: None,
            buttons: Vec::new(),
            levels: [0.0; BARS],
            meters: audio_meters(),
            gain: 0.05,
            meter_retry: 0.0,
            art: None,
            last: Instant::now(),
            t: 0.0,
            interval: 0,
            alert: None,
            alert_target: 0,
            alert_left: 0.0,
            al: 0.0,
            al_vel: 0.0,
            slim: 0.0,
            covered: false,
            shape: Rect::default(),
            visible: false,
            tab: Tab::Media,
            agents: Vec::new(),
            countdown: Countdown::default(),
            timer_done: false,
            clips: Vec::new(),
            clip_flash: None,
            shelf: Vec::new(),
            press: None,
            volume: audio.volume(),
            audio,
            audio_age: 0.0,
            osd: 0.0,
            mic_users: Vec::new(),
            cam_users: Vec::new(),
            privacy_age: 10.0,
            poll_last: Instant::now(),
            pr_urls: Vec::new(),
            scroll: 0,
            hist_targets: Vec::new(),
            mixer: Vec::new(),
            mixer_age: 0.0,
            mix_drag: None,
            app_icons: std::collections::HashMap::new(),
        };
        unsafe {
            DragAcceptFiles(hwnd, true);
            let _ = AddClipboardFormatListener(hwnd);
            SetTimer(hwnd, TIMER_POLL, 150, None);
        }
        n.step(ctx);
        Ok(n)
    }

    fn set_interval(&mut self, ms: u32) {
        if self.interval != ms {
            self.interval = ms;
            unsafe {
                SetTimer(self.hwnd, TIMER, ms, None);
            }
        }
    }

    fn closed(&self) -> (f32, f32) {
        let k = self.slim.clamp(0.0, 1.0);
        (300.0 + (120.0 - 300.0) * k, 34.0 + (7.0 - 34.0) * k)
    }

    fn slim_target(&self) -> f32 {
        if self.mode == "hover" || self.covered {
            1.0
        } else {
            0.0
        }
    }

    fn top_covered(&self) -> bool {
        unsafe {
            let fg = GetForegroundWindow();
            let mut r = RECT::default();
            if fg.is_invalid() || fg == self.hwnd || MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST) != self.mon.hmon || GetWindowRect(fg, &mut r).is_err() {
                return false;
            }
            let mut cls = [0u16; 32];
            let n = GetClassNameW(fg, &mut cls);
            let cls = String::from_utf16_lossy(&cls[..n.max(0) as usize]);
            if cls == "Progman" || cls == "WorkerW" || IsIconic(fg).as_bool() {
                return false;
            }
            let cx = (self.mon.rect.left + self.mon.rect.right) / 2;
            IsZoomed(fg).as_bool() || (r.top <= self.mon.rect.top + 2 && r.left < cx && r.right > cx)
        }
    }

    fn fullscreen(&self) -> bool {
        unsafe {
            let fg = GetForegroundWindow();
            let mut r = RECT::default();
            if fg.is_invalid() || MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST) != self.mon.hmon || GetWindowRect(fg, &mut r).is_err() {
                return false;
            }
            let mut cls = [0u16; 32];
            let n = GetClassNameW(fg, &mut cls);
            let cls = String::from_utf16_lossy(&cls[..n.max(0) as usize]);
            r.left <= self.mon.rect.left
                && r.top <= self.mon.rect.top
                && r.right >= self.mon.rect.right
                && r.bottom >= self.mon.rect.bottom
                && cls != "Progman"
                && cls != "WorkerW"
        }
    }

    pub fn on_timer(&mut self, id: usize, ctx: &Ctx) {
        if id == TIMER_POLL {
            if self.poll() {
                self.step(ctx);
            }
        } else {
            self.step(ctx);
        }
    }

    fn poll(&mut self) -> bool {
        let now = Instant::now();
        let dt = (now - self.poll_last).as_secs_f32();
        self.poll_last = now;
        let mut changed = false;
        let covered = self.top_covered();
        if covered != self.covered {
            self.covered = covered;
            changed = true;
        }
        self.audio_age += dt;
        if self.audio_age > 5.0 {
            self.audio_age = 0.0;
            self.audio.refresh();
        }
        let v = self.audio.volume();
        if let (Some(old), Some(new)) = (self.volume, v) {
            if (old.0 - new.0).abs() > 0.001 || old.1 != new.1 {
                self.osd = 1.8;
                changed = true;
            }
        }
        self.volume = v;
        if self.tab == Tab::Mixer && self.open > 0.5 && self.mix_drag.is_none() {
            self.mixer_age += dt;
            if self.mixer_age > 1.0 {
                self.mixer_age = 0.0;
                let m = system::app_volumes();
                let sig = |v: &[system::AppVolume]| v.iter().map(|a| format!("{}{:.2}{}", a.key, a.vol, a.muted)).collect::<String>();
                if sig(&m) != sig(&self.mixer) {
                    self.mixer = m;
                    changed = true;
                }
            }
        }
        self.privacy_age += dt;
        if self.privacy_age > 2.0 {
            self.privacy_age = 0.0;
            let (mic, cam) = system::privacy_users();
            if mic != self.mic_users || cam != self.cam_users {
                self.mic_users = mic;
                self.cam_users = cam;
                changed = true;
            }
        }
        if changed {
            self.request_frame();
        }
        changed || self.osd > 0.0
    }

    pub fn step(&mut self, ctx: &Ctx) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        let real_dt = (now - self.last).as_secs_f32().min(5.0);
        self.last = now;
        self.t += dt;
        self.osd = (self.osd - dt).max(0.0);
        if let Some((i, f)) = self.clip_flash {
            self.clip_flash = (f - dt > 0.0).then_some((i, f - dt));
        }
        if self.countdown.running {
            self.countdown.left -= real_dt;
            if self.countdown.left <= 0.0 {
                self.countdown = Countdown::default();
                self.timer_done = true;
            }
        }
        self.agents.retain(|a| match a.state.as_str() {
            "done" => a.since.elapsed() < Duration::from_secs(15 * 60),
            _ => a.since.elapsed() < Duration::from_secs(45 * 60),
        });

        self.alert_left -= dt;
        if self.alert_left <= 0.0 {
            self.alert = None;
        }
        let want = (match self.mode.as_str() {
            "media" => ctx.media.active || self.hover,
            "off" => false,
            _ => true,
        } || self.alert.is_some()
            || self.al > 0.01)
            && !self.fullscreen();
        if want != self.visible {
            self.visible = want;
            unsafe {
                let _ = ShowWindow(self.hwnd, if want { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
        }
        if !want {
            self.set_interval(1000);
            return;
        }

        self.recheck_hover();
        if self.hover {
            self.hover_for += dt;
            self.away_for = 0.0;
        } else {
            self.away_for += dt;
            self.hover_for = 0.0;
        }
        let slim_t = self.slim_target();
        self.slim += (slim_t - self.slim) * (1.0 - (-dt * 14.0).exp());
        if (slim_t - self.slim).abs() < 0.005 {
            self.slim = slim_t;
        }
        let delay = if self.covered && self.mode != "hover" { 0.45 } else { 0.12 };
        if self.hover && self.hover_for >= delay {
            self.target = 1.0;
        } else if !self.hover && self.away_for >= 0.22 {
            self.target = 0.0;
        }
        spring(&mut self.open, &mut self.vel, self.target, dt);
        let target = self.target;
        let al_target = if self.alert.is_some() { 1.0 } else { 0.0 };
        spring(&mut self.al, &mut self.al_vel, al_target, dt);

        let playing = ctx.media.playing;
        let peak = if playing { self.peak(dt) } else { 0.0 };
        self.gain = (self.gain * (-dt / 4.0).exp()).max(peak).max(0.01);
        let level = (peak / self.gain).sqrt().min(1.0);
        for (i, l) in self.levels.iter_mut().enumerate() {
            let wobble = ((self.t * (5.0 + i as f32 * 1.7) + i as f32 * 1.3).sin() * 0.5 + 0.5) * 0.6 + 0.4;
            let goal = if playing { (0.18 + 0.82 * level) * wobble } else { 0.0 };
            *l += (goal - *l) * (dt * 14.0).min(1.0);
        }

        if ctx.media.active {
            let key = ctx.media.track_key();
            let stale = self.art.as_ref().map(|a| &a.0) != Some(&key)
                || (ctx.media.thumb.is_some() && self.art.as_ref().is_some_and(|a| a.1.is_none()));
            if stale {
                let bmp = ctx.media.thumb.as_ref().and_then(|b| self.surf.bitmap_from_bytes(b, 128));
                self.art = Some((key, bmp));
            }
        } else {
            self.art = None;
        }

        let animating = self.open != target
            || self.slim != slim_t
            || self.vel != 0.0
            || self.al != al_target
            || (self.hover && target == 0.0)
            || (!self.hover && target == 1.0);
        let busy_agent = self.agents.iter().any(|a| a.state != "done");
        let fast = self.alert.is_some()
            || self.osd > 0.0
            || self.clip_flash.is_some()
            || busy_agent
            || ctx.media.playing && (self.slim < 0.5 || self.open > 0.0);
        self.set_interval(if fast {
            50
        } else if self.countdown.running {
            250
        } else {
            1000
        });
        self.draw(ctx);
        vsync(self.hwnd, animating);
    }

    pub fn wake(&mut self) {
        self.request_frame();
    }

    pub fn take_timer_done(&mut self) -> bool {
        std::mem::take(&mut self.timer_done)
    }

    fn request_frame(&mut self) {
        vsync(self.hwnd, true);
    }

    pub fn recheck_hover(&mut self) {
        if !self.hover {
            return;
        }
        let mut c = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut c);
        }
        let sc = self.surf.scale;
        let r = self.shape;
        let inside = c.x >= self.x + (r.x * sc) as i32
            && c.x < self.x + ((r.x + r.w) * sc) as i32
            && c.y >= self.y
            && c.y < self.y + ((r.y + r.h) * sc) as i32;
        if !inside {
            self.hover = false;
            self.tracking = false;
            self.btn_hover = None;
            self.request_frame();
        }
    }

    pub fn alert(&mut self, title: &str, message: &str, target: isize) {
        self.alert = Some((title.to_string(), message.to_string()));
        self.alert_target = target;
        self.alert_left = ALERT_SECS;
        self.request_frame();
    }

    pub fn agent_event(&mut self, ev: AgentEvent) {
        if ev.state == "end" {
            self.agents.retain(|a| a.id != ev.session);
        } else {
            let now = Instant::now();
            match self.agents.iter_mut().find(|a| a.id == ev.session) {
                Some(a) => {
                    if a.state != "working" && ev.state == "working" {
                        a.started = now;
                    }
                    if a.state != ev.state {
                        a.since = now;
                    }
                    a.state = ev.state;
                    if !ev.detail.is_empty() {
                        a.detail = ev.detail;
                    }
                    if !ev.project.is_empty() {
                        a.project = ev.project;
                    }
                    if ev.target != 0 {
                        a.target = ev.target;
                    }
                }
                None => self.agents.push(AgentSession {
                    id: ev.session,
                    project: ev.project,
                    state: ev.state,
                    detail: ev.detail,
                    since: now,
                    started: now,
                    target: ev.target,
                }),
            }
        }
        self.request_frame();
    }

    fn peak(&mut self, dt: f32) -> f32 {
        self.meter_retry -= dt;
        if self.meter_retry <= 0.0 {
            self.meters = audio_meters();
            self.meter_retry = 10.0;
        }
        let mut peak = 0.0f32;
        for m in &self.meters {
            match unsafe { m.GetPeakValue() } {
                Ok(v) => peak = peak.max(v),
                Err(_) => self.meter_retry = 0.0,
            }
        }
        peak
    }

    fn draw(&mut self, ctx: &Ctx) {
        let p = &ctx.pal;
        let e = self.open.max(0.0);
        let (cw, ch) = self.closed();
        let a = self.al.max(0.0);
        let (cw, ch) = (cw + (ALERT.0 - cw) * a, ch + (ALERT.1 - ch) * a);
        let (w, h) = (cw + (OPEN.0 - cw) * e, ch + (OPEN.1 - ch) * e);
        let x0 = (W - w) / 2.0;
        let r = (h / 2.0).min(14.0 + 8.0 * e.min(1.0));
        let white = Color::rgba(255, 255, 255, 255);
        let dim = Color::rgba(255, 255, 255, 150);
        let accent = Color { a: 1.0, ..p.accent };
        let mut buttons = Vec::new();
        if self.tab == Tab::Mixer {
            self.load_mixer_icons();
        }

        let s = &self.surf;
        s.begin();
        s.fill_round(Rect::new(x0, -r, w, h + r), r, Color::rgba(8, 8, 11, 255));
        s.stroke_round(Rect::new(x0 + 0.5, -r, w - 1.0, h + r - 0.5), r, 1.0, white.alpha(0.06));

        let ca = (1.0 - e * 3.0).clamp(0.0, 1.0) * (1.0 - a * 2.0).clamp(0.0, 1.0);
        let aa = ((a - 0.5) / 0.5).clamp(0.0, 1.0) * (1.0 - e * 3.0).clamp(0.0, 1.0);
        if let (true, Some((title, msg))) = (aa > 0.0, &self.alert) {
            let cy = h / 2.0;
            let pulse = 1.0 + (self.t * 7.0).sin() * 0.06;
            let ix = x0 + 18.0 + 14.0;
            s.fill_circle(ix, cy, 15.0 * pulse, accent.alpha(aa));
            s.glyph('\u{EA8F}', Rect::new(ix - 15.0, cy - 15.0, 30.0, 30.0), 14.0, p.on_accent.alpha(aa));
            let tx = ix + 24.0;
            let tw = x0 + w - 18.0 - tx;
            s.text(title, Rect::new(tx, cy - 19.0, tw, 19.0), 13.0, 700, white.alpha(aa), Align::Left);
            s.text(msg, Rect::new(tx, cy, tw, 19.0), 12.0, 400, dim.alpha(aa), Align::Left);
        }
        let ca = ca * (1.0 - self.slim * 2.5).clamp(0.0, 1.0);
        if ca > 0.0 && ch > 10.0 {
            self.draw_closed(s, ctx, x0, w, ch, ca, accent);
        }

        let ea = ((e - 0.6) / 0.4).clamp(0.0, 1.0);
        let ea = ea * ea * (3.0 - 2.0 * ea);
        if ea > 0.0 {
            s.push_clip(Rect::new(x0, 0.0, w, h));
            let slide = (1.0 - ea) * -8.0;
            let widths: Vec<f32> = TABS.iter().map(|(t, _, l)| if self.tab == *t { 40.0 + s.measure(tr(l), 11.5, 600) } else { 35.0 }).collect();
            let mut tx = x0 + (w - widths.iter().sum::<f32>()) / 2.0;
            for (i, (tab, g, label)) in TABS.iter().enumerate() {
                let r = Rect::new(tx + 2.0, TABS_Y + slide, widths[i] - 4.0, 28.0);
                tx += widths[i];
                let sel = self.tab == *tab;
                let hov = self.btn_hover == Some(Btn::Tab(*tab));
                if sel {
                    s.fill_round(r, 14.0, accent.alpha(0.22 * ea));
                } else if hov {
                    s.fill_round(r, 14.0, white.alpha(0.07 * ea));
                }
                let c = if sel { white } else { dim };
                if sel {
                    s.glyph(*g, Rect::new(r.x + 8.0, r.y, 18.0, r.h), 12.0, c.alpha(ea));
                    s.text(tr(label), Rect::new(r.x + 27.0, r.y, r.w - 30.0, r.h), 11.5, 600, c.alpha(ea), Align::Left);
                } else {
                    s.glyph(*g, r, 12.5, c.alpha(ea));
                }
                if *tab == Tab::Agents && self.agents.iter().any(|a| a.state == "waiting") {
                    s.fill_circle(r.x + r.w - 8.0, r.y + 7.0, 3.0, Color::rgba(255, 159, 67, (255.0 * ea) as u8));
                }
                buttons.push((r, Btn::Tab(*tab)));
            }
            let area = Rect::new(x0 + 18.0, CONTENT_Y + slide, w - 36.0, OPEN.1 - CONTENT_Y - 14.0);
            match self.tab {
                Tab::Media => self.draw_media(s, ctx, area, ea, accent, &mut buttons),
                Tab::Agents => self.draw_agents(s, area, ea, accent, &mut buttons),
                Tab::Timer => self.draw_timer(s, ctx, area, ea, accent, &mut buttons),
                Tab::Clip => self.draw_clips(s, area, ea, accent, &mut buttons),
                Tab::Shelf => self.draw_shelf(s, area, ea, accent, &mut buttons),
                Tab::Quick => self.draw_quick(s, ctx, area, ea, accent, &mut buttons),
                Tab::Agenda => draw_agenda(s, ctx, area, ea, accent),
                Tab::Mixer => {
                    self.scroll = self.scroll.min(self.mixer.len().saturating_sub(LIST_ROWS));
                    self.draw_mixer(s, area, ea, accent, &mut buttons);
                }
                Tab::Screen => draw_screen(s, ctx, area, ea, accent),
                Tab::Github => {
                    self.pr_urls = ctx.feeds.data.prs.iter().map(|p| p.url.clone()).collect();
                    self.scroll = self.scroll.min(self.pr_urls.len().saturating_sub(LIST_ROWS));
                    draw_github(s, ctx, area, ea, self.btn_hover, self.scroll, &mut buttons);
                }
                Tab::History => {
                    self.hist_targets = ctx.feeds.history.iter().map(|h| h.target).collect();
                    self.scroll = self.scroll.min(self.hist_targets.len().saturating_sub(LIST_ROWS));
                    draw_history(s, ctx, area, ea, self.btn_hover, self.scroll, &mut buttons);
                }
            }
            s.pop_clip();
        }
        s.end();
        s.present(self.hwnd, self.x, self.y);
        self.shape = Rect::new(x0, 0.0, w, h);
        self.buttons = buttons;
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_closed(&self, s: &Surface, ctx: &Ctx, x0: f32, w: f32, ch: f32, ca: f32, accent: Color) {
        let white = Color::rgba(255, 255, 255, 255);
        let dim = Color::rgba(255, 255, 255, 150);
        let orange = Color::rgba(255, 159, 67, 255);
        let t = &ctx.now;
        let cy = ch / 2.0;

        if let (true, Some((v, muted))) = (self.osd > 0.0, self.volume) {
            let a = ca * (self.osd * 4.0).min(1.0);
            let g = if muted || v <= 0.0 { '\u{E74F}' } else if v < 0.34 { '\u{E993}' } else if v < 0.67 { '\u{E994}' } else { '\u{E995}' };
            s.glyph(g, Rect::new(x0 + 12.0, 0.0, 24.0, ch), 13.0, white.alpha(a));
            let track = Rect::new(x0 + 44.0, cy - 3.0, w - 100.0, 6.0);
            s.fill_round(track, 3.0, white.alpha(0.15 * a));
            let lv = if muted { 0.0 } else { v };
            s.fill_round(Rect::new(track.x, track.y, (track.w * lv).max(6.0), 6.0), 3.0, accent.alpha(a));
            s.text(&format!("{:.0}", v * 100.0), Rect::new(x0 + w - 52.0, 0.0, 38.0, ch), 12.5, 600, white.alpha(a), Align::Right);
            return;
        }

        let waiting = self.agents.iter().find(|a| a.state == "waiting");
        let working = self.agents.iter().filter(|a| a.state == "working").max_by_key(|a| a.started.elapsed());
        let lx = x0 + 16.0;
        if let Some(_ag) = waiting {
            let pulse = 0.55 + 0.45 * (self.t * 5.0).sin().abs();
            s.fill_circle(lx + 5.0, cy, 4.5, orange.alpha(ca * pulse));
            s.text(tr("Onay bekliyor"), Rect::new(lx + 15.0, 0.0, 100.0, ch), 11.5, 600, orange.alpha(ca), Align::Left);
        } else if let Some(ag) = working {
            spinner(s, lx + 6.0, cy, 6.0, self.t, accent.alpha(ca));
            let el = ag.started.elapsed().as_secs();
            s.text(&format!("Claude {}:{:02}", el / 60, el % 60), Rect::new(lx + 17.0, 0.0, 100.0, ch), 11.5, 600, white.alpha(ca), Align::Left);
        } else if ctx.feeds.focus {
            s.crescent(lx + 6.0, cy, 6.0, accent.alpha(ca));
            let label = match ctx.feeds.focus_until {
                Some(t) => {
                    let left = t.saturating_duration_since(Instant::now()).as_secs();
                    format!("{} {}:{:02}", tr("Odak"), left / 60, left % 60)
                }
                None => tr("Odak").to_string(),
            };
            s.text(&label, Rect::new(lx + 17.0, 0.0, 110.0, ch), 11.5, 600, white.alpha(ca), Align::Left);
        } else if let Some(ev) = upcoming(ctx) {
            s.fill_circle(lx + 5.0, cy, 4.0, accent.alpha(ca));
            s.text(&ev, Rect::new(lx + 15.0, 0.0, w / 2.0 - 60.0, ch), 11.5, 600, white.alpha(ca), Align::Left);
        } else if ctx.media.active {
            draw_bars(s, lx, cy, &self.levels, 14.0, accent.alpha(ca));
        } else if ctx.sys.net_down > NET_SHOW {
            s.text(&format!("↓ {}", fmt_speed(ctx.sys.net_down)), Rect::new(lx, 0.0, 100.0, ch), 11.5, 600, dim.alpha(ca), Align::Left);
        } else {
            let d = format!("{} {}", t.day, i18n::month_short(t.month));
            s.text(&d, Rect::new(lx, 0.0, 80.0, ch), 12.0, 500, dim.alpha(ca), Align::Left);
        }
        s.text(&format!("{:02}:{:02}", t.hour, t.min), Rect::new(x0, 0.0, w, ch), 13.5, 600, white.alpha(ca), Align::Center);

        let mut rx = x0 + w - 14.0;
        if !self.mic_users.is_empty() {
            s.fill_circle(rx - 3.0, cy, 3.5, orange.alpha(ca));
            rx -= 11.0;
        }
        if !self.cam_users.is_empty() {
            s.fill_circle(rx - 3.0, cy, 3.5, Color::rgba(46, 204, 113, (255.0 * ca) as u8));
            rx -= 11.0;
        }
        if self.countdown.total > 0.0 {
            let left = self.countdown.left.max(0.0) as u32;
            let txt = format!("{}:{:02}", left / 60, left % 60);
            let tw = 44.0;
            s.text(&txt, Rect::new(rx - tw, 0.0, tw, ch), 12.0, 600, white.alpha(ca * if self.countdown.running { 1.0 } else { 0.6 }), Align::Right);
            let frac = self.countdown.left / self.countdown.total;
            let rcx = rx - tw - 12.0;
            ring(s, rcx, cy, 7.0, frac, 2.5, accent.alpha(ca), white.alpha(0.15 * ca));
        } else {
            let right = Rect::new(rx - 22.0, cy - 11.0, 22.0, 22.0);
            match self.art.as_ref().and_then(|a| a.1.as_ref()) {
                Some(bmp) if ca > 0.5 => s.image(bmp, right, 6.0),
                _ => {
                    if let Some(wt) = ctx.weather {
                        let tr = Rect::new(rx - 60.0, 0.0, 60.0, ch);
                        s.text(&format!("{:.0}°", wt.temp), tr, 12.5, 600, dim.alpha(ca), Align::Right);
                    }
                }
            }
        }
    }

    fn draw_media(&self, s: &Surface, ctx: &Ctx, area: Rect, ea: f32, accent: Color, buttons: &mut Vec<(Rect, Btn)>) {
        let p = &ctx.pal;
        let white = Color::rgba(255, 255, 255, 255);
        let dim = Color::rgba(255, 255, 255, 150);
        let t = &ctx.now;
        let left = Rect::new(area.x, area.y, 286.0, area.h);
        if ctx.media.active {
            let art = Rect::new(left.x, left.y + 4.0, 72.0, 72.0);
            match self.art.as_ref().and_then(|a| a.1.as_ref()) {
                Some(bmp) if ea > 0.5 => s.image(bmp, art, 12.0),
                _ => {
                    s.fill_round(art, 12.0, white.alpha(0.08 * ea));
                    s.glyph('\u{E8D6}', art, 22.0, dim.alpha(ea));
                }
            }
            let tx = art.x + art.w + 14.0;
            let tw = left.x + left.w - tx;
            s.text(&ctx.media.title, Rect::new(tx, art.y + 8.0, tw, 22.0), 14.0, 600, white.alpha(ea), Align::Left);
            s.text(&ctx.media.artist, Rect::new(tx, art.y + 30.0, tw, 20.0), 12.0, 400, dim.alpha(ea), Align::Left);
            draw_bars(s, tx, art.y + 60.0, &self.levels, 10.0, accent.alpha(ea));
            let by = art.y + art.h + 34.0;
            let cx = left.x + 130.0;
            for (dx, rad, g, b) in [
                (-54.0, 16.0, '\u{E892}', Btn::Prev),
                (0.0, 21.0, if ctx.media.playing { '\u{E769}' } else { '\u{E768}' }, Btn::Toggle),
                (54.0, 16.0, '\u{E893}', Btn::Next),
            ] {
                let rr = Rect::new(cx + dx - rad, by - rad, 2.0 * rad, 2.0 * rad);
                let hov = self.btn_hover == Some(b);
                if b == Btn::Toggle {
                    s.fill_circle(cx + dx, by, rad, accent.alpha(ea * if hov { 0.85 } else { 1.0 }));
                    s.glyph(g, rr, 15.0, p.on_accent.alpha(ea));
                } else {
                    if hov {
                        s.fill_circle(cx + dx, by, rad, white.alpha(0.1 * ea));
                    }
                    s.glyph(g, rr, 13.0, white.alpha(ea));
                }
                buttons.push((rr, b));
            }
        } else {
            s.text(&format!("{:02}:{:02}", t.hour, t.min), Rect::new(left.x, left.y, left.w, 50.0), 40.0, 600, white.alpha(ea), Align::Left);
            let date = format!("{}, {} {} {}", i18n::day(t.dow), t.day, i18n::month(t.month), t.year);
            s.text(&date, Rect::new(left.x, left.y + 56.0, left.w, 20.0), 13.0, 500, dim.alpha(ea), Align::Left);
            if let Some(wt) = ctx.weather {
                let line = format!("{} · {:.0}° {}", ctx.cfg.weather.city, wt.temp, wt.description());
                s.text(&line, Rect::new(left.x, left.y + 80.0, left.w, 20.0), 12.5, 400, dim.alpha(ea), Align::Left);
            }
        }
        let net = format!("↓ {}   ↑ {}", fmt_speed(ctx.sys.net_down), fmt_speed(ctx.sys.net_up));
        s.text(&net, Rect::new(left.x, area.y + area.h - 18.0, left.w, 18.0), 11.0, 500, dim.alpha(ea * 0.8), Align::Left);
        let cal = Rect::new(left.x + left.w + 16.0, area.y, area.x + area.w - (left.x + left.w + 16.0), area.h);
        draw_calendar(s, cal, t, accent, p.on_accent, ea);
        buttons.push((cal, Btn::Calendar));
    }

    fn draw_agents(&self, s: &Surface, area: Rect, ea: f32, accent: Color, buttons: &mut Vec<(Rect, Btn)>) {
        let white = Color::rgba(255, 255, 255, 255);
        let dim = Color::rgba(255, 255, 255, 150);
        let orange = Color::rgba(255, 159, 67, 255);
        let green = Color::rgba(46, 204, 113, 255);
        if self.agents.is_empty() {
            s.glyph('\u{E99A}', Rect::new(area.x, area.y + 30.0, area.w, 40.0), 26.0, dim.alpha(0.6 * ea));
            s.text(tr("Çalışan agent yok"), Rect::new(area.x, area.y + 78.0, area.w, 22.0), 13.0, 600, dim.alpha(ea), Align::Center);
            s.text(tr("Claude Code çalışınca burada ne yaptığı görünür"), Rect::new(area.x, area.y + 100.0, area.w, 20.0), 11.5, 400, dim.alpha(0.7 * ea), Align::Center);
            return;
        }
        let row = 38.0;
        for (i, ag) in self.agents.iter().take(4).enumerate() {
            let r = Rect::new(area.x, area.y + i as f32 * (row + 2.0), area.w, row);
            if self.btn_hover == Some(Btn::Agent(i)) {
                s.fill_round(r, 10.0, white.alpha(0.07 * ea));
            }
            let cy = r.y + row / 2.0;
            match ag.state.as_str() {
                "working" => spinner(s, r.x + 16.0, cy, 7.0, self.t, accent.alpha(ea)),
                "waiting" => {
                    let pulse = 0.55 + 0.45 * (self.t * 5.0).sin().abs();
                    s.fill_circle(r.x + 16.0, cy, 6.0, orange.alpha(ea * pulse));
                }
                _ => {
                    s.fill_circle(r.x + 16.0, cy, 8.0, green.alpha(0.2 * ea));
                    s.glyph('\u{E73E}', Rect::new(r.x + 8.0, cy - 8.0, 16.0, 16.0), 10.0, green.alpha(ea));
                }
            }
            let name = if ag.project.is_empty() { "Claude Code" } else { ag.project.as_str() };
            s.text(name, Rect::new(r.x + 34.0, r.y + 2.0, r.w - 110.0, 18.0), 13.0, 600, white.alpha(ea), Align::Left);
            let detail_c = if ag.state == "waiting" { orange } else { dim };
            s.text(&ag.detail, Rect::new(r.x + 34.0, r.y + 19.0, r.w - 110.0, 17.0), 11.5, 400, detail_c.alpha(ea), Align::Left);
            let el = if ag.state == "working" { ag.started.elapsed() } else { ag.since.elapsed() };
            let secs = el.as_secs();
            let right = if ag.state == "working" {
                format!("{}:{:02}", secs / 60, secs % 60)
            } else if secs < 60 {
                tr("şimdi").into()
            } else {
                if i18n::en() { format!("{} min ago", secs / 60) } else { format!("{} dk önce", secs / 60) }
            };
            s.text(&right, Rect::new(r.x + r.w - 80.0, r.y, 70.0, row), 11.5, 500, dim.alpha(ea), Align::Right);
            buttons.push((r, Btn::Agent(i)));
        }
    }

    fn draw_timer(&self, s: &Surface, ctx: &Ctx, area: Rect, ea: f32, accent: Color, buttons: &mut Vec<(Rect, Btn)>) {
        let white = Color::rgba(255, 255, 255, 255);
        let dim = Color::rgba(255, 255, 255, 150);
        let c = &self.countdown;
        let (cx, cy, rad) = (area.x + 78.0, area.y + area.h / 2.0, 62.0);
        let frac = if c.total > 0.0 { c.left / c.total } else { 0.0 };
        ring(s, cx, cy, rad, frac, 6.0, accent.alpha(ea), white.alpha(0.1 * ea));
        let left = c.left.max(0.0).ceil() as u32;
        let big = if c.total > 0.0 { format!("{}:{:02}", left / 60, left % 60) } else { "0:00".into() };
        s.text(&big, Rect::new(cx - rad, cy - 22.0, 2.0 * rad, 34.0), 26.0, 600, white.alpha(ea), Align::Center);
        let state = if c.running {
            tr("Çalışıyor")
        } else if c.total > 0.0 {
            tr("Duraklatıldı")
        } else {
            tr("Süre seç")
        };
        s.text(state, Rect::new(cx - rad, cy + 12.0, 2.0 * rad, 18.0), 11.5, 500, dim.alpha(ea), Align::Center);

        let px = area.x + 180.0;
        s.text(tr("Hazır süreler"), Rect::new(px, area.y, 200.0, 18.0), 11.5, 600, dim.alpha(ea), Align::Left);
        for (i, m) in [5u32, 15, 25, 50].iter().enumerate() {
            let r = Rect::new(px + (i % 2) as f32 * 108.0, area.y + 24.0 + (i / 2) as f32 * 38.0, 100.0, 32.0);
            let hov = self.btn_hover == Some(Btn::Preset(*m));
            s.fill_round(r, 16.0, white.alpha(if hov { 0.14 } else { 0.08 } * ea));
            let unit = if i18n::en() { "min" } else { "dk" };
            let label = if *m == 25 { format!("25 {unit} · Pomodoro") } else { format!("{m} {unit}") };
            s.text(&label, r, 11.5, 600, white.alpha(ea), Align::Center);
            buttons.push((r, Btn::Preset(*m)));
        }
        let by = area.y + 108.0;
        let go = Rect::new(px, by, 100.0, 34.0);
        let go_label = if c.running { tr("Duraklat") } else { tr("Başlat") };
        let hov = self.btn_hover == Some(Btn::TimerGo);
        s.fill_round(go, 17.0, accent.alpha(ea * if hov { 0.85 } else { 1.0 }));
        s.text(go_label, go, 12.5, 700, ctx.pal.on_accent.alpha(ea), Align::Center);
        buttons.push((go, Btn::TimerGo));
        for (i, (b, label)) in [(Btn::TimerAdd, tr("+1 dk")), (Btn::TimerReset, tr("Sıfırla"))].iter().enumerate() {
            let r = Rect::new(px + 108.0 + i as f32 * 54.0, by, 50.0, 34.0);
            let hov = self.btn_hover == Some(*b);
            s.fill_round(r, 17.0, white.alpha(if hov { 0.14 } else { 0.08 } * ea));
            s.text(label, r, 11.0, 600, white.alpha(ea), Align::Center);
            buttons.push((r, *b));
        }
    }

    fn draw_clips(&self, s: &Surface, area: Rect, ea: f32, accent: Color, buttons: &mut Vec<(Rect, Btn)>) {
        let white = Color::rgba(255, 255, 255, 255);
        let dim = Color::rgba(255, 255, 255, 150);
        if self.clips.is_empty() {
            s.glyph('\u{E77F}', Rect::new(area.x, area.y + 30.0, area.w, 40.0), 26.0, dim.alpha(0.6 * ea));
            s.text(tr("Kopyaladığın metinler burada görünür"), Rect::new(area.x, area.y + 78.0, area.w, 22.0), 12.5, 500, dim.alpha(ea), Align::Center);
            s.text(tr("Birine tıklayınca yeniden kopyalanır"), Rect::new(area.x, area.y + 100.0, area.w, 20.0), 11.5, 400, dim.alpha(0.7 * ea), Align::Center);
            return;
        }
        let clear = Rect::new(area.x + area.w - 70.0, area.y - 4.0, 70.0, 22.0);
        let hov = self.btn_hover == Some(Btn::ClipClear);
        s.text(tr("Temizle"), clear, 11.0, 600, if hov { white } else { dim }.alpha(ea), Align::Right);
        buttons.push((clear, Btn::ClipClear));
        s.text(tr("Son kopyalananlar"), Rect::new(area.x, area.y - 4.0, 200.0, 22.0), 11.5, 600, dim.alpha(ea), Align::Left);
        let row = 27.0;
        for (i, text) in self.clips.iter().take(5).enumerate() {
            let r = Rect::new(area.x, area.y + 22.0 + i as f32 * (row + 2.0), area.w, row);
            let hov = self.btn_hover == Some(Btn::Clip(i));
            s.fill_round(r, 8.0, white.alpha(if hov { 0.1 } else { 0.05 } * ea));
            let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let flash = self.clip_flash.is_some_and(|(j, _)| j == i);
            if flash {
                s.glyph('\u{E73E}', Rect::new(r.x + r.w - 28.0, r.y, 22.0, r.h), 11.0, accent.alpha(ea));
                s.text(tr("Kopyalandı"), Rect::new(r.x + r.w - 110.0, r.y, 80.0, r.h), 11.0, 600, accent.alpha(ea), Align::Right);
            }
            let tw = if flash { r.w - 130.0 } else { r.w - 20.0 };
            s.text(&one_line, Rect::new(r.x + 10.0, r.y, tw, r.h), 12.0, 400, white.alpha(ea), Align::Left);
            buttons.push((r, Btn::Clip(i)));
        }
    }

    fn draw_shelf(&self, s: &Surface, area: Rect, ea: f32, accent: Color, buttons: &mut Vec<(Rect, Btn)>) {
        let white = Color::rgba(255, 255, 255, 255);
        let dim = Color::rgba(255, 255, 255, 150);
        if self.shelf.is_empty() {
            let drop = Rect::new(area.x, area.y, area.w, area.h - 4.0);
            s.stroke_round(drop.inset(1.0), 14.0, 1.5, white.alpha(0.18 * ea));
            s.glyph('\u{E896}', Rect::new(area.x, area.y + 36.0, area.w, 36.0), 24.0, dim.alpha(0.7 * ea));
            s.text(tr("Dosyaları çentiğe sürükleyip bırak"), Rect::new(area.x, area.y + 78.0, area.w, 22.0), 12.5, 600, dim.alpha(ea), Align::Center);
            s.text(tr("Burada bekler; sonra istediğin yere sürükle"), Rect::new(area.x, area.y + 100.0, area.w, 20.0), 11.5, 400, dim.alpha(0.7 * ea), Align::Center);
            return;
        }
        let clear = Rect::new(area.x + area.w - 70.0, area.y - 4.0, 70.0, 22.0);
        let hov = self.btn_hover == Some(Btn::ShelfClear);
        s.text(tr("Temizle"), clear, 11.0, 600, if hov { white } else { dim }.alpha(ea), Align::Right);
        buttons.push((clear, Btn::ShelfClear));
        s.text(tr("Sürükleyerek istediğin yere bırak"), Rect::new(area.x, area.y - 4.0, 260.0, 22.0), 11.5, 600, dim.alpha(ea), Align::Left);
        let (cols, gap) = (5usize, 8.0);
        let tw = (area.w - gap * (cols as f32 - 1.0)) / cols as f32;
        let th = 64.0;
        for (i, it) in self.shelf.iter().take(10).enumerate() {
            let r = Rect::new(area.x + (i % cols) as f32 * (tw + gap), area.y + 22.0 + (i / cols) as f32 * (th + gap), tw, th);
            let hov = self.btn_hover == Some(Btn::ShelfItem(i)) || self.btn_hover == Some(Btn::ShelfRemove(i));
            s.fill_round(r, 10.0, white.alpha(if hov { 0.12 } else { 0.06 } * ea));
            if let Some(bmp) = &it.icon {
                s.image(bmp, Rect::new(r.x + (r.w - 30.0) / 2.0, r.y + 7.0, 30.0, 30.0), 0.0);
            } else {
                s.glyph('\u{E8A5}', Rect::new(r.x, r.y + 7.0, r.w, 30.0), 20.0, dim.alpha(ea));
            }
            s.text(&it.name, Rect::new(r.x + 4.0, r.y + 40.0, r.w - 8.0, 18.0), 10.5, 500, white.alpha(ea), Align::Center);
            buttons.push((r, Btn::ShelfItem(i)));
            if hov {
                let x = Rect::new(r.x + r.w - 20.0, r.y + 3.0, 17.0, 17.0);
                s.fill_circle(x.x + 8.5, x.y + 8.5, 8.5, Color::rgba(196, 43, 28, (230.0 * ea) as u8));
                s.glyph('\u{E711}', x, 8.0, white.alpha(ea));
                buttons.insert(0, (x, Btn::ShelfRemove(i)));
            }
        }
        let _ = accent;
    }

    fn draw_quick(&self, s: &Surface, ctx: &Ctx, area: Rect, ea: f32, accent: Color, buttons: &mut Vec<(Rect, Btn)>) {
        let white = Color::rgba(255, 255, 255, 255);
        let dim = Color::rgba(255, 255, 255, 150);
        let radios = system::radios();
        let (vol, muted) = self.volume.unwrap_or((0.0, false));
        let mic_muted = self.audio.mic_muted();
        let items: [(Quick, char, String, Option<bool>, bool); 10] = [
            (Quick::Focus, '\u{E708}', tr("Odak").into(), Some(ctx.feeds.focus), true),
            (Quick::Dark, '\u{E793}', tr("Koyu tema").into(), Some(system::dark_mode()), true),
            (Quick::Mute, if muted { '\u{E74F}' } else { '\u{E767}' }, if muted { tr("Ses kapalı").into() } else { if i18n::en() { format!("Volume {:.0}%", vol * 100.0) } else { format!("Ses %{:.0}", vol * 100.0) } }, Some(muted), self.volume.is_some()),
            (Quick::Mic, if mic_muted == Some(true) { '\u{F781}' } else { '\u{E720}' }, if mic_muted == Some(true) { tr("Mikrofon kapalı").into() } else { tr("Mikrofon").into() }, mic_muted, mic_muted.is_some()),
            (Quick::Bluetooth, '\u{E702}', "Bluetooth".into(), radios.bluetooth, radios.bluetooth.is_some()),
            (Quick::Wifi, '\u{E701}', "Wi-Fi".into(), radios.wifi, radios.wifi.is_some()),
            (Quick::Shot, '\u{E924}', tr("Ekran alıntısı").into(), None, true),
            (Quick::Picker, '\u{E790}', tr("Renk seç").into(), None, true),
            (Quick::Desktop, '\u{E7C4}', tr("Yeni masaüstü").into(), None, true),
            (Quick::Lock, '\u{E72E}', tr("Kilitle").into(), None, true),
        ];
        let (cols, gap) = (5usize, 8.0);
        let tw = (area.w - gap * (cols as f32 - 1.0)) / cols as f32;
        let th = 60.0;
        let mut cx = area.x;
        for d in &ctx.feeds.bt {
            let label = format!("{} %{}", d.name, d.percent);
            let label = if i18n::en() { format!("{} {}%", d.name, d.percent) } else { label };
            let w = s.measure(&label, 10.5, 600) + 30.0;
            if cx + w > area.x + area.w {
                break;
            }
            let r = Rect::new(cx, area.y + 2.0 * (th + gap) + 2.0, w, 18.0);
            let low = d.percent <= 15;
            s.fill_round(r, 9.0, white.alpha(0.07 * ea));
            let c = if low { Color::rgba(255, 99, 71, 255) } else { dim };
            s.glyph('\u{E702}', Rect::new(r.x + 4.0, r.y, 16.0, r.h), 10.0, c.alpha(ea));
            s.text(&label, Rect::new(r.x + 20.0, r.y, w - 22.0, r.h), 10.5, 600, if low { c } else { white }.alpha(ea), Align::Left);
            cx += w + 6.0;
        }
        for (i, (q, g, label, on, ok)) in items.iter().enumerate() {
            let r = Rect::new(area.x + (i % cols) as f32 * (tw + gap), area.y + (i / cols) as f32 * (th + gap), tw, th);
            let hov = self.btn_hover == Some(Btn::Quick(*q));
            let active = *on == Some(true);
            let bg = if active { accent.alpha(0.9 * ea) } else { white.alpha(if hov { 0.13 } else { 0.07 } * ea) };
            s.fill_round(r, 12.0, bg);
            let fg = if active { ctx.pal.on_accent } else if *ok { white } else { dim.alpha(0.5) };
            s.glyph(*g, Rect::new(r.x, r.y + 8.0, r.w, 24.0), 15.0, fg.alpha(ea));
            s.text(label, Rect::new(r.x + 4.0, r.y + 36.0, r.w - 8.0, 18.0), 11.0, 600, fg.alpha(ea), Align::Center);
            if *ok {
                buttons.push((r, Btn::Quick(*q)));
            }
        }
    }

    fn draw_mixer(&self, s: &Surface, area: Rect, ea: f32, accent: Color, buttons: &mut Vec<(Rect, Btn)>) {
        let white = Color::rgba(255, 255, 255, 255);
        let dim = Color::rgba(255, 255, 255, 150);
        if self.mixer.is_empty() {
            empty_state(s, area, ea, '\u{E995}', tr("Ses çalan uygulama yok"), tr("Ses çıkaran uygulamalar burada listelenir"));
            return;
        }
        s.text(tr("Uygulama sesleri"), Rect::new(area.x, area.y - 4.0, area.w, 20.0), 11.5, 600, dim.alpha(ea), Align::Left);
        let row = 31.0;
        let top = area.y + 20.0;
        scrollbar(s, area, top, LIST_ROWS as f32 * (row + 2.0) - 2.0, self.mixer.len(), self.scroll, ea);
        for i in self.scroll..(self.scroll + LIST_ROWS).min(self.mixer.len()) {
            let a = &self.mixer[i];
            let r = Rect::new(area.x, top + (i - self.scroll) as f32 * (row + 2.0), area.w, row);
            let ir = Rect::new(r.x + 4.0, r.y + (row - 18.0) / 2.0, 18.0, 18.0);
            match self.app_icons.get(&a.key).and_then(|b| b.as_ref()) {
                Some(b) => s.image(b, ir, 0.0),
                None => s.glyph('\u{E7F5}', ir, 13.0, dim.alpha(ea)),
            }
            s.text(&a.name, Rect::new(r.x + 30.0, r.y, 130.0, row), 12.0, 500, white.alpha(ea), Align::Left);
            let track = Rect::new(r.x + 170.0, r.y + row / 2.0 - 2.0, r.w - 170.0 - 84.0, 4.0);
            let v = if a.muted { 0.0 } else { a.vol };
            s.fill_round(track, 2.0, white.alpha(0.15 * ea));
            s.fill_round(Rect::new(track.x, track.y, track.w * v, 4.0), 2.0, if a.muted { dim } else { accent }.alpha(ea));
            s.fill_circle(track.x + track.w * v, track.y + 2.0, 6.0, white.alpha(ea));
            buttons.push((Rect::new(track.x - 6.0, r.y, track.w + 12.0, row), Btn::MixSlider(i)));
            let pct = format!("{:.0}", a.vol * 100.0);
            s.text(&pct, Rect::new(track.x + track.w + 10.0, r.y, 34.0, row), 11.5, 600, dim.alpha(ea), Align::Right);
            let mr = Rect::new(r.x + r.w - 32.0, r.y + 2.0, 28.0, row - 4.0);
            if self.btn_hover == Some(Btn::MixMute(i)) {
                s.fill_round(mr, 8.0, white.alpha(0.1 * ea));
            }
            s.glyph(if a.muted { '\u{E74F}' } else { '\u{E767}' }, mr, 12.5, if a.muted { Color::rgba(255, 99, 71, 255) } else { white }.alpha(ea));
            buttons.push((mr, Btn::MixMute(i)));
        }
    }

    fn load_mixer_icons(&mut self) {
        for a in &self.mixer {
            if !self.app_icons.contains_key(&a.key) {
                let bmp = (!a.path.is_empty()).then(|| system::file_icon(&a.path)).flatten().and_then(|h| {
                    let b = self.surf.bitmap_from_icon(h);
                    unsafe {
                        let _ = DestroyIcon(h);
                    }
                    b
                });
                self.app_icons.insert(a.key.clone(), bmp);
            }
        }
    }

    fn mix_set(&mut self, lp: LPARAM) {
        let Some(i) = self.mix_drag else { return };
        let x = (lp.0 & 0xFFFF) as i16 as f32 / self.surf.scale;
        let Some((r, _)) = self.buttons.iter().find(|(_, b)| *b == Btn::MixSlider(i)) else { return };
        let v = ((x - r.x - 6.0) / (r.w - 12.0)).clamp(0.0, 1.0);
        if let Some(a) = self.mixer.get_mut(i) {
            a.vol = v;
            a.muted = false;
            system::set_app_volume(&a.key, Some(v), None);
        }
        self.request_frame();
    }

    fn button_at(&self, lp: LPARAM) -> Option<Btn> {
        let x = (lp.0 & 0xFFFF) as i16 as f32 / self.surf.scale;
        let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32 / self.surf.scale;
        self.buttons.iter().find(|(r, _)| r.contains(x, y)).map(|(_, b)| *b)
    }

    fn click(&mut self, b: Btn) -> Option<Action> {
        match b {
            Btn::Tab(t) => {
                if self.tab != t {
                    self.scroll = 0;
                }
                if t == Tab::Mixer {
                    self.mixer = system::app_volumes();
                    self.mixer_age = 0.0;
                }
                self.tab = t;
            }
            Btn::Prev => return Some(Action::Media(MediaCmd::Prev)),
            Btn::Toggle => return Some(Action::Media(MediaCmd::Toggle)),
            Btn::Next => return Some(Action::Media(MediaCmd::Next)),
            Btn::Calendar => util::send_win(Some(VK_N)),
            Btn::Agent(i) => {
                if let Some(a) = self.agents.get(i).filter(|a| a.target != 0) {
                    bar::bring(util::hwnd(a.target));
                }
            }
            Btn::Preset(m) => {
                self.countdown = Countdown { total: m as f32 * 60.0, left: m as f32 * 60.0, running: true };
                if m == 25 {
                    return Some(Action::Focus(Some(25)));
                }
            }
            Btn::Pr(i) => {
                if let Some(u) = self.pr_urls.get(i) {
                    return Some(Action::OpenUrl(u.clone()));
                }
            }
            Btn::Hist(i) => {
                if let Some(&t) = self.hist_targets.get(i).filter(|t| **t != 0) {
                    bar::bring(util::hwnd(t));
                }
            }
            Btn::HistClear => return Some(Action::ClearHistory),
            Btn::TimerGo => {
                if self.countdown.total <= 0.0 {
                    self.countdown = Countdown { total: 25.0 * 60.0, left: 25.0 * 60.0, running: true };
                } else {
                    self.countdown.running = !self.countdown.running;
                }
            }
            Btn::TimerAdd => {
                let c = &mut self.countdown;
                c.left += 60.0;
                c.total = c.total.max(c.left);
            }
            Btn::TimerReset => self.countdown = Countdown::default(),
            Btn::Clip(i) => {
                if let Some(text) = self.clips.get(i).cloned() {
                    system::set_clipboard_text(self.hwnd, &text);
                    self.clips.retain(|c| *c != text);
                    self.clips.insert(0, text);
                    self.clip_flash = Some((0, 1.2));
                }
            }
            Btn::ClipClear => self.clips.clear(),
            Btn::ShelfItem(i) => {
                if let Some(it) = self.shelf.get(i) {
                    unsafe {
                        ShellExecuteW(None, w!("open"), &HSTRING::from(it.path.as_str()), None, None, SW_SHOWNORMAL);
                    }
                }
            }
            Btn::ShelfRemove(i) => {
                if i < self.shelf.len() {
                    self.shelf.remove(i);
                }
            }
            Btn::ShelfClear => self.shelf.clear(),
            Btn::Quick(q) => match q {
                Quick::Dark => system::set_dark_mode(!system::dark_mode()),
                Quick::Mute => self.audio.toggle_mute(),
                Quick::Mic => self.audio.toggle_mic(),
                Quick::Bluetooth => system::toggle_radio(true),
                Quick::Wifi => system::toggle_radio(false),
                Quick::Shot => system::screenshot(),
                Quick::Lock => system::lock(),
                Quick::Focus => return Some(Action::Focus(None)),
                Quick::Picker => return Some(Action::PickColor),
                Quick::Desktop => system::new_desktop(),
            },
            Btn::MixSlider(_) => {}
            Btn::MixMute(i) => {
                if let Some(a) = self.mixer.get_mut(i) {
                    a.muted = !a.muted;
                    system::set_app_volume(&a.key, None, Some(a.muted));
                }
            }
        }
        None
    }

    fn add_files(&mut self, paths: Vec<String>) {
        let n = paths.len();
        for path in paths {
            if self.shelf.iter().any(|s| s.path == path) {
                continue;
            }
            let name = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string();
            let icon = system::file_icon(&path).and_then(|h| {
                let b = self.surf.bitmap_from_icon(h);
                unsafe {
                    let _ = DestroyIcon(h);
                }
                b
            });
            self.shelf.push(ShelfItem { path, name, icon });
        }
        self.tab = Tab::Shelf;
        self.alert(tr("Raf"), &if i18n::en() { format!("{n} file(s) added to the shelf") } else { format!("{n} dosya rafa eklendi") }, 0);
    }

    pub fn handle(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<Option<Action>> {
        match msg {
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
                self.hover = true;
                self.btn_hover = self.button_at(lp);
                if self.mix_drag.is_some() {
                    if unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } as u16 & 0x8000 != 0 {
                        self.mix_set(lp);
                    } else {
                        self.mix_drag = None;
                    }
                }
                if let Some((Btn::ShelfItem(i), start)) = self.press {
                    let mut c = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut c);
                    }
                    let down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } as u16 & 0x8000 != 0;
                    if !down {
                        self.press = None;
                    } else if (c.x - start.x).abs() + (c.y - start.y).abs() > 6 {
                        self.press = None;
                        if let Some(path) = self.shelf.get(i).map(|s| s.path.clone()) {
                            if system::drag_file(self.hwnd, &path) {
                                self.shelf.retain(|s| s.path != path);
                            }
                        }
                    }
                }
                self.request_frame();
            }
            WM_MOUSELEAVE_ => {
                self.tracking = false;
                self.hover = false;
                self.btn_hover = None;
                self.request_frame();
            }
            WM_LBUTTONDOWN => {
                let mut c = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut c);
                }
                self.press = self.button_at(lp).map(|b| (b, c));
                if let Some(Btn::MixSlider(i)) = self.press.map(|p| p.0) {
                    self.mix_drag = Some(i);
                    self.mix_set(lp);
                }
            }
            WM_LBUTTONUP => {
                self.mix_drag = None;
                let pressed = self.press.take().map(|p| p.0);
                let b = self.button_at(lp);
                let act = match b {
                    Some(b) if pressed.is_none() || pressed == Some(b) => self.click(b),
                    Some(_) => None,
                    None => {
                        if self.alert.take().is_some() && self.alert_target != 0 {
                            bar::bring(util::hwnd(self.alert_target));
                        }
                        None
                    }
                };
                self.request_frame();
                return Some(act);
            }
            WM_MOUSEWHEEL if self.open > 0.5 && self.tab == Tab::Mixer && matches!(self.btn_hover, Some(Btn::MixSlider(_) | Btn::MixMute(_))) => {
                let delta = (wp.0 >> 16) as u16 as i16 as f32 / 120.0;
                if let Some(Btn::MixSlider(i) | Btn::MixMute(i)) = self.btn_hover {
                    if let Some(a) = self.mixer.get_mut(i) {
                        a.vol = (a.vol + delta * 0.05).clamp(0.0, 1.0);
                        a.muted = false;
                        system::set_app_volume(&a.key, Some(a.vol), None);
                    }
                }
                self.request_frame();
            }
            WM_MOUSEWHEEL if self.open > 0.5 && matches!(self.tab, Tab::Github | Tab::History | Tab::Mixer) => {
                let delta = (wp.0 >> 16) as u16 as i16;
                let len = match self.tab {
                    Tab::Github => self.pr_urls.len(),
                    Tab::Mixer => self.mixer.len(),
                    _ => self.hist_targets.len(),
                };
                let max = len.saturating_sub(LIST_ROWS);
                self.scroll = if delta > 0 { self.scroll.saturating_sub(1) } else { (self.scroll + 1).min(max) };
                self.request_frame();
            }
            WM_MOUSEWHEEL => {
                let delta = (wp.0 >> 16) as u16 as i16 as f32 / 120.0;
                if let Some((v, _)) = self.audio.volume() {
                    self.audio.set_volume(v + delta * 0.02);
                    self.volume = self.audio.volume();
                    self.osd = 1.8;
                    self.request_frame();
                }
            }
            WM_DROPFILES => {
                let files = system::dropped_files(wp);
                if !files.is_empty() {
                    self.add_files(files);
                }
            }
            WM_CLIPBOARDUPDATE_ => {
                if let Some(text) = system::clipboard_text(self.hwnd) {
                    let text: String = text.trim().chars().take(2000).collect();
                    if !text.is_empty() && self.clips.first() != Some(&text) {
                        self.clips.retain(|c| *c != text);
                        self.clips.insert(0, text);
                        self.clips.truncate(10);
                    }
                }
            }
            WM_RBUTTONUP => return Some(Some(Action::Menu { widget: None })),
            _ => return None,
        }
        Some(None)
    }
}

impl Drop for Notch {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn audio_meters() -> Vec<IAudioMeterInformation> {
    unsafe {
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return Vec::new() };
        let Ok(list) = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return Vec::new() };
        let n = list.GetCount().unwrap_or(0);
        (0..n).filter_map(|i| list.Item(i).ok()?.Activate(CLSCTX_ALL, None).ok()).collect()
    }
}

fn draw_bars(s: &Surface, x: f32, cy: f32, levels: &[f32], max_h: f32, c: Color) {
    for (i, l) in levels.iter().enumerate() {
        let h = 3.0 + l * max_h;
        s.fill_round(Rect::new(x + i as f32 * 5.5, cy - h / 2.0, 3.0, h), 1.5, c);
    }
}

fn arc(s: &Surface, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, width: f32, c: Color) {
    let n = ((a1 - a0).abs() * 12.0).ceil().max(2.0) as usize;
    let pts: Vec<(f32, f32)> = (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            (cx + a.sin() * r, cy - a.cos() * r)
        })
        .collect();
    s.polyline(&pts, width, c);
}

fn spinner(s: &Surface, cx: f32, cy: f32, r: f32, t: f32, c: Color) {
    let a0 = t * 7.0;
    arc(s, cx, cy, r, a0, a0 + 4.2, 2.2, c);
}

#[allow(clippy::too_many_arguments)]
fn ring(s: &Surface, cx: f32, cy: f32, r: f32, frac: f32, width: f32, fg: Color, bg: Color) {
    s.stroke_round(Rect::new(cx - r, cy - r, 2.0 * r, 2.0 * r), r, width, bg);
    if frac > 0.001 {
        arc(s, cx, cy, r, 0.0, frac.min(1.0) * std::f32::consts::TAU, width, fg);
    }
}

fn draw_calendar(s: &Surface, r: Rect, t: &util::Time, accent: Color, on_accent: Color, a: f32) {
    let white = Color::rgba(255, 255, 255, 255);
    let title = format!("{} {}", i18n::month(t.month), t.year);
    s.text(&title, Rect::new(r.x, r.y, r.w, 18.0), 12.0, 600, white.alpha(a), Align::Left);
    let cw = r.w / 7.0;
    let row = ((r.h - 34.0) / 6.0).min(17.0);
    for (i, d) in i18n::days_short().iter().enumerate() {
        s.text(d, Rect::new(r.x + i as f32 * cw, r.y + 18.0, cw, 14.0), 9.5, 500, white.alpha(0.45 * a), Align::Center);
    }
    let first = ((t.dow as i32 - (t.day as i32 - 1)).rem_euclid(7) + 6) % 7;
    for d in 1..=days_in_month(t.year, t.month) as i32 {
        let k = first + d - 1;
        let (col, line) = (k % 7, k / 7);
        let cell = Rect::new(r.x + col as f32 * cw, r.y + 34.0 + line as f32 * row, cw, row);
        let today = d == t.day as i32;
        if today {
            s.fill_circle(cell.x + cw / 2.0, cell.y + row / 2.0, row / 2.0 + 0.5, accent.alpha(a));
        }
        let c = if today { on_accent.alpha(a) } else { white.alpha(0.85 * a) };
        s.text(&d.to_string(), cell, 9.5, if today { 700 } else { 400 }, c, Align::Center);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "ses aygıtı gerektirir"]
    fn audio_peak_live() {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
        }
        let ms = super::audio_meters();
        println!("aygıt sayısı: {}", ms.len());
        for _ in 0..10 {
            let v: Vec<_> = ms.iter().map(|m| unsafe { m.GetPeakValue() }).collect();
            println!("peak = {v:?}");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

fn spring(pos: &mut f32, vel: &mut f32, target: f32, dt: f32) {
    let k = if target > 0.5 { 150.0 } else { 190.0 };
    let damp = 2.0 * f32::sqrt(k) * if target > 0.5 { 0.74 } else { 1.0 };
    let n = (dt / 0.004).ceil().max(1.0);
    let h = dt / n;
    for _ in 0..n as usize {
        let acc = k * (target - *pos) - damp * *vel;
        *vel += acc * h;
        *pos += *vel * h;
    }
    if (target - *pos).abs() < 0.0015 && vel.abs() < 0.01 {
        *pos = target;
        *vel = 0.0;
    }
}

static VSYNC_ON: AtomicBool = AtomicBool::new(false);
static VSYNC_HWND: AtomicIsize = AtomicIsize::new(0);
static VSYNC_PENDING: AtomicBool = AtomicBool::new(false);
static VSYNC_THREAD: Once = Once::new();

fn vsync(hwnd: HWND, on: bool) {
    VSYNC_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    VSYNC_ON.store(on, Ordering::SeqCst);
    VSYNC_THREAD.call_once(|| {
        std::thread::spawn(|| loop {
            if !VSYNC_ON.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(15));
                continue;
            }
            if unsafe { DwmFlush() }.is_err() {
                std::thread::sleep(Duration::from_millis(16));
            }
            if !VSYNC_PENDING.swap(true, Ordering::SeqCst) {
                unsafe {
                    let h = HWND(VSYNC_HWND.load(Ordering::SeqCst) as _);
                    if PostMessageW(h, WM_APP_FRAME, WPARAM(0), LPARAM(0)).is_err() {
                        VSYNC_PENDING.store(false, Ordering::SeqCst);
                    }
                }
            }
        });
    });
}

pub fn frame_ack() {
    VSYNC_PENDING.store(false, Ordering::SeqCst);
}

fn fmt_hm(secs: i64) -> String {
    let t = secs.rem_euclid(86400);
    format!("{:02}:{:02}", t / 3600, (t % 3600) / 60)
}

fn upcoming(ctx: &Ctx) -> Option<String> {
    let now = crate::feeds::local_now();
    let ev = ctx.feeds.next_event(now)?;
    if ev.start <= now {
        return Some(format!("{} · {}", tr("Şimdi"), ev.title));
    }
    let mins = (ev.start - now + 59) / 60;
    (mins <= 15).then(|| if i18n::en() { format!("{mins} min · {}", ev.title) } else { format!("{mins} dk · {}", ev.title) })
}

fn empty_state(s: &Surface, area: Rect, ea: f32, glyph: char, title: &str, sub: &str) {
    let dim = Color::rgba(255, 255, 255, 150);
    s.glyph(glyph, Rect::new(area.x, area.y + 30.0, area.w, 40.0), 26.0, dim.alpha(0.6 * ea));
    s.text(title, Rect::new(area.x, area.y + 78.0, area.w, 22.0), 13.0, 600, dim.alpha(ea), Align::Center);
    s.text(sub, Rect::new(area.x, area.y + 100.0, area.w, 20.0), 11.5, 400, dim.alpha(0.7 * ea), Align::Center);
}

fn draw_agenda(s: &Surface, ctx: &Ctx, area: Rect, ea: f32, accent: Color) {
    let white = Color::rgba(255, 255, 255, 255);
    let dim = Color::rgba(255, 255, 255, 150);
    if ctx.cfg.calendar.ics.is_empty() {
        empty_state(s, area, ea, '\u{E787}', tr("Takvim bağlı değil"), tr("Ayarlar > Çentik'ten ICS bağlantısı ekle"));
        return;
    }
    let now = crate::feeds::local_now();
    let today = now.div_euclid(86400);
    let evs: Vec<_> = ctx.feeds.data.events.iter().filter(|e| e.end > now).take(12).collect();
    if evs.is_empty() {
        empty_state(s, area, ea, '\u{E787}', tr("Yaklaşan etkinlik yok"), tr("Önümüzdeki 7 gün boş görünüyor"));
        return;
    }
    let mut y = area.y - 4.0;
    let mut last_day = i64::MIN;
    for e in evs {
        if y > area.y + area.h - 26.0 {
            break;
        }
        let d = e.start.div_euclid(86400).max(today);
        if d != last_day {
            last_day = d;
            let label = match d - today {
                0 => tr("Bugün").to_string(),
                1 => tr("Yarın").to_string(),
                _ => {
                    let (_, m, dd) = crate::feeds::civil_from_days(d);
                    let dow = ((d + 4).rem_euclid(7)) as u16;
                    format!("{}, {} {}", i18n::day(dow), dd, i18n::month(m as u16))
                }
            };
            s.text(&label, Rect::new(area.x, y, area.w, 20.0), 11.0, 700, dim.alpha(ea), Align::Left);
            y += 20.0;
        }
        let r = Rect::new(area.x, y, area.w, 26.0);
        let live = e.start <= now && e.end > now;
        if live {
            s.fill_round(r, 8.0, accent.alpha(0.18 * ea));
        }
        s.fill_round(Rect::new(r.x + 4.0, r.y + 5.0, 3.0, 16.0), 1.5, accent.alpha(ea));
        let time = if e.all_day { tr("Tüm gün").to_string() } else { format!("{}–{}", fmt_hm(e.start), fmt_hm(e.end)) };
        s.text(&time, Rect::new(r.x + 14.0, r.y, 92.0, r.h), 11.5, 600, if live { white } else { dim }.alpha(ea), Align::Left);
        s.text(&e.title, Rect::new(r.x + 108.0, r.y, r.w - 112.0, r.h), 12.0, 500, white.alpha(ea), Align::Left);
        y += 28.0;
    }
}

const LIST_ROWS: usize = 4;

fn scrollbar(s: &Surface, area: Rect, top: f32, h: f32, len: usize, off: usize, ea: f32) {
    if len <= LIST_ROWS {
        return;
    }
    let th = (h * LIST_ROWS as f32 / len as f32).max(14.0);
    let y = top + (h - th) * off as f32 / (len - LIST_ROWS) as f32;
    s.fill_round(Rect::new(area.x + area.w + 4.0, top, 3.0, h), 1.5, Color::rgba(255, 255, 255, 20).alpha(ea));
    s.fill_round(Rect::new(area.x + area.w + 4.0, y, 3.0, th), 1.5, Color::rgba(255, 255, 255, 110).alpha(ea));
}

fn draw_github(s: &Surface, ctx: &Ctx, area: Rect, ea: f32, hover: Option<Btn>, off: usize, buttons: &mut Vec<(Rect, Btn)>) {
    let white = Color::rgba(255, 255, 255, 255);
    let dim = Color::rgba(255, 255, 255, 150);
    let d = &ctx.feeds.data;
    if !ctx.cfg.github.enabled {
        empty_state(s, area, ea, '\u{E943}', tr("GitHub kapalı"), tr("Ayarlar > Çentik'ten açabilirsin"));
        return;
    }
    match d.gh_error.as_deref() {
        Some("gh") => return empty_state(s, area, ea, '\u{E943}', tr("GitHub CLI bulunamadı"), tr("gh kurulunca PR ve CI durumları burada görünür")),
        Some(_) if d.prs.is_empty() => return empty_state(s, area, ea, '\u{E943}', tr("GitHub'a bağlanılamadı"), tr("Terminalde 'gh auth login' çalıştır")),
        _ => {}
    }
    if !d.gh_loaded {
        return empty_state(s, area, ea, '\u{E943}', tr("Yükleniyor…"), "");
    }
    if d.prs.is_empty() {
        return empty_state(s, area, ea, '\u{E943}', tr("Açık PR yok"), tr("Açtığın pull request'ler burada görünür"));
    }
    s.text(tr("Açık pull request'lerin"), Rect::new(area.x, area.y - 4.0, area.w, 20.0), 11.5, 600, dim.alpha(ea), Align::Left);
    let row = 31.0;
    scrollbar(s, area, area.y + 20.0, LIST_ROWS as f32 * (row + 2.0) - 2.0, d.prs.len(), off, ea);
    for (i, pr) in d.prs.iter().enumerate().skip(off).take(LIST_ROWS) {
        let r = Rect::new(area.x, area.y + 20.0 + (i - off) as f32 * (row + 2.0), area.w, row);
        if hover == Some(Btn::Pr(i)) {
            s.fill_round(r, 8.0, white.alpha(0.08 * ea));
        }
        let (c, label) = match pr.ci.as_str() {
            "SUCCESS" => (Color::rgba(46, 204, 113, 255), tr("geçti")),
            "FAILURE" | "ERROR" => (Color::rgba(255, 99, 71, 255), tr("başarısız")),
            "PENDING" | "EXPECTED" => (Color::rgba(255, 159, 67, 255), tr("çalışıyor")),
            _ => (Color::rgba(255, 255, 255, 90), "—"),
        };
        s.fill_circle(r.x + 12.0, r.y + row / 2.0, 4.5, c.alpha(ea));
        let head = format!("{} #{}", pr.repo.rsplit('/').next().unwrap_or(&pr.repo), pr.number);
        let hw = s.measure(&head, 11.5, 600).min(150.0);
        s.text(&head, Rect::new(r.x + 24.0, r.y, hw + 2.0, row), 11.5, 600, dim.alpha(ea), Align::Left);
        s.text(&pr.title, Rect::new(r.x + 32.0 + hw, r.y, r.w - hw - 110.0, row), 12.0, 500, white.alpha(ea), Align::Left);
        s.text(label, Rect::new(r.x + r.w - 76.0, r.y, 70.0, row), 11.0, 600, c.alpha(ea), Align::Right);
        buttons.push((r, Btn::Pr(i)));
    }
}

fn draw_screen(s: &Surface, ctx: &Ctx, area: Rect, ea: f32, accent: Color) {
    let white = Color::rgba(255, 255, 255, 255);
    let dim = Color::rgba(255, 255, 255, 150);
    let total = ctx.feeds.screen_total;
    if total < 60 {
        empty_state(s, area, ea, '\u{E823}', tr("Henüz veri yok"), tr("Bugün kullandığın uygulamalar burada birikir"));
        return;
    }
    s.text(tr("Bugün ekran başında"), Rect::new(area.x, area.y - 4.0, 200.0, 20.0), 11.5, 600, dim.alpha(ea), Align::Left);
    s.text(&crate::screentime::fmt_dur(total), Rect::new(area.x + area.w - 200.0, area.y - 6.0, 200.0, 24.0), 16.0, 700, white.alpha(ea), Align::Right);
    let max = ctx.feeds.screen_top.first().map_or(1, |u| u.secs).max(1) as f32;
    let row = 24.0;
    for (i, u) in ctx.feeds.screen_top.iter().take(5).enumerate() {
        let y = area.y + 24.0 + i as f32 * (row + 1.0);
        s.text(&u.name, Rect::new(area.x, y, 150.0, row), 12.0, 500, white.alpha(ea), Align::Left);
        let track = Rect::new(area.x + 158.0, y + row / 2.0 - 3.0, area.w - 158.0 - 86.0, 6.0);
        s.fill_round(track, 3.0, white.alpha(0.08 * ea));
        s.fill_round(Rect::new(track.x, track.y, (track.w * u.secs as f32 / max).max(6.0), 6.0), 3.0, accent.alpha((0.95 - i as f32 * 0.13) * ea));
        s.text(&crate::screentime::fmt_dur(u.secs), Rect::new(area.x + area.w - 80.0, y, 80.0, row), 11.5, 600, dim.alpha(ea), Align::Right);
    }
}

fn draw_history(s: &Surface, ctx: &Ctx, area: Rect, ea: f32, hover: Option<Btn>, off: usize, buttons: &mut Vec<(Rect, Btn)>) {
    let white = Color::rgba(255, 255, 255, 255);
    let dim = Color::rgba(255, 255, 255, 150);
    let h = &ctx.feeds.history;
    if h.is_empty() {
        empty_state(s, area, ea, '\u{E81C}', tr("Bildirim yok"), tr("Gelen bildirimler burada birikir"));
        return;
    }
    let clear = Rect::new(area.x + area.w - 70.0, area.y - 4.0, 70.0, 22.0);
    s.text(tr("Temizle"), clear, 11.0, 600, if hover == Some(Btn::HistClear) { white } else { dim }.alpha(ea), Align::Right);
    buttons.push((clear, Btn::HistClear));
    let label = if ctx.feeds.focus { tr("Odak modunda gelenler dahil") } else { tr("Son bildirimler") };
    s.text(label, Rect::new(area.x, area.y - 4.0, 260.0, 22.0), 11.5, 600, dim.alpha(ea), Align::Left);
    let now = crate::feeds::local_now();
    let row = 33.0;
    scrollbar(s, area, area.y + 22.0, LIST_ROWS as f32 * (row + 2.0) - 2.0, h.len(), off, ea);
    for (i, it) in h.iter().enumerate().skip(off).take(LIST_ROWS) {
        let r = Rect::new(area.x, area.y + 22.0 + (i - off) as f32 * (row + 2.0), area.w, row);
        if hover == Some(Btn::Hist(i)) {
            s.fill_round(r, 8.0, white.alpha(0.08 * ea));
        }
        s.text(&it.title, Rect::new(r.x + 8.0, r.y + 1.0, r.w * 0.4, 16.0), 12.0, 600, white.alpha(ea), Align::Left);
        s.text(&it.message, Rect::new(r.x + 8.0, r.y + 16.0, r.w - 90.0, 16.0), 11.0, 400, dim.alpha(ea), Align::Left);
        let ago = (now - it.at).max(0) / 60;
        let t = if ago == 0 { tr("şimdi").to_string() } else if i18n::en() { format!("{ago} min ago") } else { format!("{ago} dk önce") };
        s.text(&t, Rect::new(r.x + r.w - 84.0, r.y, 78.0, row), 11.0, 500, dim.alpha(ea), Align::Right);
        buttons.push((r, Btn::Hist(i)));
    }
}
