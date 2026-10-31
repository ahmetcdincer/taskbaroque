use std::collections::HashMap;

use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use crate::feeds::local_now;
use crate::util;

const IDLE_MS: u32 = 120_000;

#[derive(Clone, Debug, Default)]
pub struct Usage {
    pub name: String,
    pub secs: u32,
}

#[derive(Default)]
pub struct Tracker {
    day: i64,
    secs: HashMap<String, u32>,
    names: HashMap<String, String>,
    pub summary_day: i64,
    unsaved: u32,
}

fn path() -> std::path::PathBuf {
    crate::config::dir().join("screentime.txt")
}

fn idle_ms() -> u32 {
    let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        if !GetLastInputInfo(&mut li).as_bool() {
            return 0;
        }
        windows::Win32::System::SystemInformation::GetTickCount().wrapping_sub(li.dwTime)
    }
}

impl Tracker {
    pub fn load() -> Tracker {
        let mut t = Tracker { day: local_now().div_euclid(86400), ..Default::default() };
        let Ok(text) = std::fs::read_to_string(path()) else { return t };
        let mut lines = text.lines();
        let head: Vec<i64> = lines.next().unwrap_or("").split('\t').filter_map(|v| v.parse().ok()).collect();
        t.summary_day = head.get(1).copied().unwrap_or(0);
        if head.first() == Some(&t.day) {
            for l in lines {
                if let Some((k, v)) = l.rsplit_once('\t') {
                    if let Ok(v) = v.parse() {
                        t.secs.insert(k.to_string(), v);
                    }
                }
            }
        }
        t
    }

    pub fn save(&mut self) {
        self.unsaved = 0;
        let mut out = format!("{}\t{}\n", self.day, self.summary_day);
        for (k, v) in &self.secs {
            out.push_str(&format!("{k}\t{v}\n"));
        }
        let _ = std::fs::write(path(), out);
    }

    pub fn tick(&mut self, playing: bool) {
        let day = local_now().div_euclid(86400);
        if day != self.day {
            self.save();
            self.day = day;
            self.secs.clear();
        }
        if idle_ms() > IDLE_MS && !playing {
            return;
        }
        let fg = unsafe { GetForegroundWindow() };
        if fg.is_invalid() {
            return;
        }
        let Some(p) = util::process_path(fg) else { return };
        let key = crate::bar::app_key(&p);
        if key.ends_with("\\taskbaroque.exe") || key.ends_with("\\lockapp.exe") {
            return;
        }
        if !self.names.contains_key(&key) {
            let name = util::file_description(&p)
                .or_else(|| std::path::Path::new(&p).file_stem().map(|s| s.to_string_lossy().into_owned()))
                .unwrap_or_default();
            self.names.insert(key.clone(), name);
        }
        *self.secs.entry(key).or_insert(0) += 1;
        self.unsaved += 1;
        if self.unsaved >= 60 {
            self.save();
        }
    }

    pub fn total(&self) -> u32 {
        self.secs.values().sum()
    }

    pub fn top(&self, n: usize) -> Vec<Usage> {
        let mut v: Vec<Usage> = self
            .secs
            .iter()
            .map(|(k, s)| Usage {
                name: self.names.get(k).cloned().unwrap_or_else(|| std::path::Path::new(k).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()),
                secs: *s,
            })
            .collect();
        v.sort_by(|a, b| b.secs.cmp(&a.secs));
        v.truncate(n);
        v
    }

    pub fn day(&self) -> i64 {
        self.day
    }
}

pub fn fmt_dur(secs: u32) -> String {
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    let en = crate::i18n::en();
    match (h, en) {
        (0, true) => format!("{m} min"),
        (0, false) => format!("{m} dk"),
        (_, true) => format!("{h} h {m} min"),
        (_, false) => format!("{h} sa {m} dk"),
    }
}
