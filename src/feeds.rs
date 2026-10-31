use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Win32::Networking::NetworkListManager::{INetworkListManager, NetworkListManager};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::*;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use crate::util;

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub start: i64,
    pub end: i64,
    pub title: String,
    pub all_day: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pr {
    pub repo: String,
    pub number: i64,
    pub title: String,
    pub url: String,
    pub ci: String,
}

#[derive(Clone, Debug, Default)]
pub struct FeedData {
    pub events: Vec<Event>,
    pub prs: Vec<Pr>,
    pub gh_error: Option<String>,
    pub gh_loaded: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct FeedCfg {
    pub ics: Vec<String>,
    pub github: bool,
}

#[derive(Clone, Debug)]
pub struct HistItem {
    pub title: String,
    pub message: String,
    pub at: i64,
    pub target: isize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
    pub secs_left: Option<u32>,
}

#[derive(Default)]
pub struct Feeds {
    pub data: FeedData,
    pub history: Vec<HistItem>,
    pub battery: Option<Battery>,
    pub online: bool,
    pub focus: bool,
    pub focus_until: Option<Instant>,
    pub quiet: bool,
    pub bt: Vec<crate::devices::BtDevice>,
    pub desktops: (usize, usize),
    pub screen_total: u32,
    pub screen_top: Vec<crate::screentime::Usage>,
}

impl Feeds {
    pub fn next_event(&self, now: i64) -> Option<&Event> {
        self.data.events.iter().filter(|e| !e.all_day && e.end > now).min_by_key(|e| e.start)
    }

    pub fn push_history(&mut self, title: &str, message: &str, target: isize) {
        self.history.insert(0, HistItem { title: title.into(), message: message.into(), at: local_now(), target });
        self.history.truncate(40);
    }
}

pub fn spawn(state: Arc<Mutex<FeedData>>, cfg: FeedCfg, notify: isize) -> Sender<FeedCfg> {
    let (tx, rx) = channel::<FeedCfg>();
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let mut cfg = cfg;
        let mut round = 0u32;
        let mut changed = true;
        loop {
            let mut data = state.lock().unwrap().clone();
            if changed || round % 5 == 0 {
                let now = local_now();
                let mut events = Vec::new();
                for url in &cfg.ics {
                    if let Some(text) = http_get(url) {
                        events.extend(parse_ics(&text, now - 86400, now + 8 * 86400));
                    }
                }
                events.sort_by_key(|e| e.start);
                events.dedup();
                data.events = events;
            }
            if cfg.github {
                match github_prs() {
                    Ok(prs) => {
                        data.prs = prs;
                        data.gh_error = None;
                    }
                    Err(e) => data.gh_error = Some(e),
                }
                data.gh_loaded = true;
            } else {
                data.prs.clear();
                data.gh_error = None;
                data.gh_loaded = false;
            }
            *state.lock().unwrap() = data;
            util::post_data_changed(notify);
            round += 1;
            changed = false;
            match rx.recv_timeout(Duration::from_secs(120)) {
                Ok(c) => {
                    cfg = c;
                    changed = true;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    tx
}

fn http_get(url: &str) -> Option<String> {
    crate::http::get(url)
}

const GH_QUERY: &str = "query { viewer { pullRequests(first: 8, states: OPEN, orderBy: {field: UPDATED_AT, direction: DESC}) { nodes { number title url repository { nameWithOwner } commits(last: 1) { nodes { commit { statusCheckRollup { state } } } } } } } }";

fn github_prs() -> std::result::Result<Vec<Pr>, String> {
    let out = Command::new("gh")
        .args(["api", "graphql", "-f", &format!("query={GH_QUERY}")])
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|_| "gh".to_string())?;
    if !out.status.success() {
        return Err("auth".into());
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|_| "json".to_string())?;
    let nodes = v["data"]["viewer"]["pullRequests"]["nodes"].as_array().cloned().unwrap_or_default();
    Ok(nodes
        .iter()
        .map(|n| Pr {
            repo: n["repository"]["nameWithOwner"].as_str().unwrap_or("").to_string(),
            number: n["number"].as_i64().unwrap_or(0),
            title: n["title"].as_str().unwrap_or("").to_string(),
            url: n["url"].as_str().unwrap_or("").to_string(),
            ci: n["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["state"].as_str().unwrap_or("").to_string(),
        })
        .collect())
}

pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn local_now() -> i64 {
    let t = unsafe { GetLocalTime() };
    days_from_civil(t.wYear as i64, t.wMonth as i64, t.wDay as i64) * 86400 + t.wHour as i64 * 3600 + t.wMinute as i64 * 60 + t.wSecond as i64
}

fn utc_offset() -> i64 {
    let mut tz = TIME_ZONE_INFORMATION::default();
    let r = unsafe { GetTimeZoneInformation(&mut tz) };
    let bias = tz.Bias + if r == 2 { tz.DaylightBias } else { tz.StandardBias };
    -(bias as i64) * 60
}

fn parse_dt(value: &str, params: &str) -> Option<(i64, bool)> {
    let v = value.trim();
    let num = |s: &str| s.parse::<i64>().ok();
    if v.len() == 8 || params.contains("VALUE=DATE") && !v.contains('T') {
        let d = days_from_civil(num(&v[0..4])?, num(&v[4..6])?, num(&v[6..8])?);
        return Some((d * 86400, true));
    }
    if v.len() < 15 {
        return None;
    }
    let d = days_from_civil(num(&v[0..4])?, num(&v[4..6])?, num(&v[6..8])?);
    let secs = d * 86400 + num(&v[9..11])? * 3600 + num(&v[11..13])? * 60 + num(&v[13..15])?;
    Some((if v.ends_with('Z') { secs + utc_offset() } else { secs }, false))
}

fn unescape(s: &str) -> String {
    s.replace("\\n", " ").replace("\\N", " ").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

#[derive(Default)]
struct Raw {
    uid: String,
    start: Option<(i64, bool)>,
    end: Option<(i64, bool)>,
    title: String,
    rrule: String,
    exdates: Vec<i64>,
    recurrence_id: Option<i64>,
    cancelled: bool,
}

pub fn parse_ics(text: &str, from: i64, to: i64) -> Vec<Event> {
    let unfolded = text.replace("\r\n ", "").replace("\r\n\t", "").replace("\n ", "").replace("\n\t", "");
    let mut raws = Vec::new();
    let mut cur: Option<Raw> = None;
    for line in unfolded.lines() {
        let line = line.trim_end_matches('\r');
        if line == "BEGIN:VEVENT" {
            cur = Some(Raw::default());
            continue;
        }
        if line == "END:VEVENT" {
            if let Some(r) = cur.take() {
                raws.push(r);
            }
            continue;
        }
        let Some(r) = cur.as_mut() else { continue };
        let Some((head, value)) = line.split_once(':') else { continue };
        let (key, params) = head.split_once(';').unwrap_or((head, ""));
        match key {
            "UID" => r.uid = value.to_string(),
            "DTSTART" => r.start = parse_dt(value, params),
            "DTEND" => r.end = parse_dt(value, params),
            "SUMMARY" => r.title = unescape(value),
            "RRULE" => r.rrule = value.to_string(),
            "STATUS" => r.cancelled = value.eq_ignore_ascii_case("CANCELLED"),
            "EXDATE" => r.exdates.extend(value.split(',').filter_map(|v| parse_dt(v, params)).map(|x| x.0)),
            "RECURRENCE-ID" => r.recurrence_id = parse_dt(value, params).map(|x| x.0),
            _ => {}
        }
    }
    let overridden: Vec<(String, i64)> = raws.iter().filter_map(|r| r.recurrence_id.map(|t| (r.uid.clone(), t))).collect();
    let mut out = Vec::new();
    for r in raws.iter().filter(|r| !r.cancelled) {
        let Some((start, all_day)) = r.start else { continue };
        let len = r.end.map(|e| e.0 - start).filter(|l| *l > 0).unwrap_or(if all_day { 86400 } else { 3600 });
        let title = if r.title.is_empty() { "(—)".to_string() } else { r.title.clone() };
        let mut push = |s: i64| {
            if s + len > from && s < to && !r.exdates.contains(&s) && !(r.recurrence_id.is_none() && overridden.iter().any(|(u, t)| *u == r.uid && *t == s)) {
                out.push(Event { start: s, end: s + len, title: title.clone(), all_day });
            }
        };
        if r.rrule.is_empty() || r.recurrence_id.is_some() {
            push(start);
            continue;
        }
        for s in expand(&r.rrule, start, to) {
            push(s);
        }
    }
    out
}

fn expand(rule: &str, start: i64, to: i64) -> Vec<i64> {
    let mut freq = "";
    let mut interval = 1i64;
    let mut count: Option<usize> = None;
    let mut until = i64::MAX;
    let mut byday: Vec<i64> = Vec::new();
    for part in rule.split(';') {
        let Some((k, v)) = part.split_once('=') else { continue };
        match k {
            "FREQ" => freq = v,
            "INTERVAL" => interval = v.parse().unwrap_or(1).max(1),
            "COUNT" => count = v.parse().ok(),
            "UNTIL" => until = parse_dt(v, "").map(|x| x.0 + if v.len() == 8 { 86399 } else { 0 }).unwrap_or(i64::MAX),
            "BYDAY" => {
                byday = v
                    .split(',')
                    .filter_map(|d| ["MO", "TU", "WE", "TH", "FR", "SA", "SU"].iter().position(|x| d.ends_with(x)).map(|i| i as i64))
                    .collect()
            }
            _ => {}
        }
    }
    let end = to.min(until);
    let max = count.unwrap_or(usize::MAX);
    let mut v = Vec::new();
    let day = 86400;
    let start_day = start.div_euclid(day);
    let tod = start.rem_euclid(day);
    match freq {
        "DAILY" => {
            let mut s = start;
            while s <= end && v.len() < max && v.len() < 4000 {
                v.push(s);
                s += interval * day;
            }
        }
        "WEEKLY" => {
            let wd = |d: i64| (d + 3).rem_euclid(7);
            let days = if byday.is_empty() { vec![wd(start_day)] } else { byday };
            let week0 = start_day - wd(start_day);
            let mut w = 0i64;
            'outer: loop {
                for &dd in &days {
                    let d = week0 + w * 7 * interval + dd;
                    if d < start_day {
                        continue;
                    }
                    let s = d * day + tod;
                    if s > end || v.len() >= max || v.len() >= 4000 {
                        break 'outer;
                    }
                    v.push(s);
                }
                w += 1;
                if week0 + w * 7 * interval > end / day + 7 {
                    break;
                }
            }
        }
        "MONTHLY" | "YEARLY" => {
            let (y, m, d) = civil_from_days(start_day);
            let mut i = 0i64;
            while v.len() < max && v.len() < 600 {
                let (yy, mm) = if freq == "MONTHLY" {
                    let t = (y * 12 + m - 1) + i * interval;
                    (t.div_euclid(12), t.rem_euclid(12) + 1)
                } else {
                    (y + i * interval, m)
                };
                let s = days_from_civil(yy, mm, d) * day + tod;
                if s > end {
                    break;
                }
                if civil_from_days(s.div_euclid(day)).2 == d {
                    v.push(s);
                }
                i += 1;
            }
        }
        _ => v.push(start),
    }
    v
}

pub fn battery() -> Option<Battery> {
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s).ok()? };
    if s.BatteryFlag & 128 != 0 || s.BatteryLifePercent == 255 {
        return None;
    }
    Some(Battery {
        percent: s.BatteryLifePercent.min(100),
        charging: s.ACLineStatus == 1,
        secs_left: (s.BatteryLifeTime != u32::MAX).then_some(s.BatteryLifeTime),
    })
}

thread_local! {
    static NLM: Option<INetworkListManager> = unsafe { CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL).ok() };
}

pub fn online() -> bool {
    NLM.with(|n| n.as_ref().map_or(true, |m| unsafe { m.IsConnectedToInternet().map_or(true, |b| b.as_bool()) }))
}

const PUSH: windows::core::PCWSTR = windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\PushNotifications");

fn marker() -> std::path::PathBuf {
    crate::config::dir().join("focus_toasts.txt")
}

fn toasts_enabled() -> u32 {
    let mut v = 1u32;
    let mut n = 4u32;
    unsafe {
        let _ = RegGetValueW(HKEY_CURRENT_USER, PUSH, windows::core::w!("ToastEnabled"), RRF_RT_REG_DWORD, None, Some(&mut v as *mut _ as _), Some(&mut n));
    }
    v
}

fn set_toasts(v: u32) {
    unsafe {
        let mut k = HKEY::default();
        if RegCreateKeyExW(HKEY_CURRENT_USER, PUSH, 0, None, REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut k, None).is_ok() {
            let _ = RegSetValueExW(k, windows::core::w!("ToastEnabled"), 0, REG_DWORD, Some(&v.to_le_bytes()));
            let _ = RegCloseKey(k);
        }
    }
}

pub fn set_windows_dnd(on: bool) {
    if on {
        if !marker().exists() {
            let _ = std::fs::write(marker(), toasts_enabled().to_string());
        }
        set_toasts(0);
    } else {
        restore_windows_dnd();
    }
}

pub fn restore_windows_dnd() {
    if let Ok(prev) = std::fs::read_to_string(marker()) {
        set_toasts(prev.trim().parse().unwrap_or(1));
        let _ = std::fs::remove_file(marker());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_roundtrip() {
        for d in [-1000, 0, 19000, 20391] {
            let (y, m, dd) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, dd), d);
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn ics_weekly_and_single() {
        let base = days_from_civil(2026, 10, 26) * 86400;
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nDTSTART:20261026T090000\r\nDTEND:20261026T093000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE\r\nSUMMARY:Daily\\, sync\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b\r\nDTSTART;VALUE=DATE:20261030\r\nSUMMARY:Tatil\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let ev = parse_ics(ics, base, base + 14 * 86400);
        let daily: Vec<_> = ev.iter().filter(|e| e.title == "Daily, sync").collect();
        assert_eq!(daily.len(), 4);
        assert_eq!(daily[1].start, base + 2 * 86400 + 9 * 3600);
        assert!(ev.iter().any(|e| e.title == "Tatil" && e.all_day));
    }
}
