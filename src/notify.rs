use std::collections::HashMap;
use std::io::Read;

use serde::{Deserialize, Serialize};

use crate::i18n::{self, tr};
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const COPYDATA_NOTIFY: usize = 0x5754_4E31;
pub const COPYDATA_AGENT: usize = 0x5754_4E32;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentEvent {
    pub session: String,
    pub project: String,
    pub state: String,
    pub detail: String,
    #[serde(default)]
    pub target: isize,
    /// `target` kullanıcının o an yazdığı pencere mi (kesin), yoksa tahmin mi
    #[serde(default)]
    pub strong: bool,
    /// Oturumun Claude süreci; kapanınca oturum listeden düşer
    #[serde(default)]
    pub pid: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Notice {
    pub title: String,
    pub message: String,
    pub sound: bool,
    #[serde(default)]
    pub target: isize,
    /// Ses türü: "done" | "attention" | "error" | "" (genel)
    #[serde(default)]
    pub kind: String,
}

pub fn from_args(args: &[String]) -> Option<Notice> {
    let i = args.iter().position(|a| a == "--notify")?;
    let message = args.get(i + 1).filter(|m| !m.starts_with("--")).cloned().unwrap_or_default();
    let title = args
        .iter()
        .position(|a| a == "--title")
        .and_then(|t| args.get(t + 1).cloned())
        .unwrap_or_else(|| tr("Bildirim").into());
    Some(Notice { title, message, sound: args.iter().any(|a| a == "--sound"), target: 0, kind: String::new() })
}

fn project_of(v: &serde_json::Value) -> String {
    v["cwd"]
        .as_str()
        .and_then(|c| c.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next())
        .unwrap_or("")
        .to_string()
}

fn short(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s
    }
}

/// Bildirimin türü: Claude Code `notification_type` gönderir; eski sürümler için mesajdan tahmin.
fn notification_kind(v: &serde_json::Value) -> &'static str {
    match v["notification_type"].as_str() {
        Some("permission_prompt") => "permission",
        Some("elicitation_dialog") => "question",
        Some("idle_prompt") => "idle",
        Some("auth_success") => "auth",
        Some(_) => "other",
        None => {
            let m = v["message"].as_str().unwrap_or("").to_lowercase();
            if m.contains("waiting for your input") {
                "idle"
            } else if m.contains("permission") {
                "permission"
            } else {
                "other"
            }
        }
    }
}

/// Aracı ve neyle çalıştığını kısaca anlatır: "Komut: git push", "Düzenliyor: app.rs"
fn tool_summary(v: &serde_json::Value) -> String {
    let file = |key: &str| {
        let p = v["tool_input"][key].as_str().unwrap_or("");
        p.rsplit(['\\', '/']).next().unwrap_or(p).to_string()
    };
    let tool = v["tool_name"].as_str().unwrap_or(tr("Araç"));
    match tool {
        "Bash" | "PowerShell" => format!("{}: {}", if i18n::en() { "Running" } else { "Komut" }, short(v["tool_input"]["command"].as_str().unwrap_or(""), 40)),
        "Edit" | "MultiEdit" => format!("{}: {}", if i18n::en() { "Editing" } else { "Düzenliyor" }, file("file_path")),
        "Write" => format!("{}: {}", if i18n::en() { "Writing" } else { "Yazıyor" }, file("file_path")),
        "Read" => format!("{}: {}", if i18n::en() { "Reading" } else { "Okuyor" }, file("file_path")),
        "Grep" | "Glob" => format!("{}: {}", if i18n::en() { "Searching" } else { "Arıyor" }, short(v["tool_input"]["pattern"].as_str().unwrap_or(""), 30)),
        "WebFetch" | "WebSearch" => tr("Web'e bakıyor").into(),
        "Agent" | "Task" => tr("Alt agent çalıştırıyor").into(),
        other => other.to_string(),
    }
}

/// Açılır bildirim + ses gerektiren olaylar. "Girdi bekliyorum" hatırlatması bilerek susturulur.
pub fn from_hook(json: &str) -> Option<Notice> {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let event = v["hook_event_name"].as_str().unwrap_or("");
    let msg = v["message"].as_str().unwrap_or("").to_string();
    let project = project_of(&v);
    let title = if project.is_empty() { "Claude Code".to_string() } else { format!("Claude Code · {project}") };
    let (title, message, kind) = match event {
        "Notification" => match notification_kind(&v) {
            "permission" => (title, if msg.is_empty() { tr("Onayını bekliyor").into() } else { msg }, "attention"),
            "question" => (title, if msg.is_empty() { tr("Senden yanıt bekliyor").into() } else { msg }, "attention"),
            "other" if !msg.is_empty() => (title, msg, "attention"),
            _ => return None,
        },
        "Stop" => (title, tr("Görev bitti").into(), "done"),
        "StopFailure" => (title, tr("Bir hatayla durdu").into(), "error"),
        "" if !msg.is_empty() => ("Agent".into(), msg, ""),
        _ => return None,
    };
    Some(Notice { title, message, sound: true, target: 0, kind: kind.into() })
}

pub fn agent_from_hook(json: &str) -> Option<AgentEvent> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let session = v["session_id"].as_str()?.to_string();
    let project = project_of(&v);
    let (state, detail) = match v["hook_event_name"].as_str()? {
        "UserPromptSubmit" => ("working", tr("Düşünüyor").to_string()),
        "PreToolUse" => ("working", tool_summary(&v)),
        // Onaydan sonra (ya da araç bitince) bekleme durumu kalkar
        "PostToolUse" | "PostToolUseFailure" => ("working", String::new()),
        "PermissionRequest" => ("waiting", format!("{} · {}", tr("Onay"), tool_summary(&v))),
        "Notification" => match notification_kind(&v) {
            "permission" | "question" => ("waiting", short(v["message"].as_str().unwrap_or(tr("Senden yanıt bekliyor")), 60)),
            _ => return None,
        },
        "Stop" => ("done", tr("Görev bitti").to_string()),
        "StopFailure" => ("done", tr("Bir hatayla durdu").to_string()),
        "SessionEnd" => ("end", String::new()),
        _ => return None,
    };
    Some(AgentEvent { session, project, state: state.into(), detail, target: 0, strong: false, pid: 0 })
}

pub fn run_hook() {
    let mut s = String::new();
    if std::io::stdin().read_to_string(&mut s).is_err() {
        return;
    }
    let s = s.trim_start_matches('\u{feff}');
    let event = serde_json::from_str::<serde_json::Value>(s).ok().and_then(|v| v["hook_event_name"].as_str().map(String::from)).unwrap_or_default();
    let agent = agent_from_hook(s);
    let notice = from_hook(s);
    if agent.is_none() && notice.is_none() {
        return;
    }
    let chain = ancestors();
    // Kullanıcı Claude'a yazdığı anda öndeki pencere, o oturumun terminalidir (Warp gibi tek
    // süreçte çok pencere tutan terminallerde doğru pencereyi ancak böyle buluruz).
    let fg = (event == "UserPromptSubmit").then(|| foreground_in(&chain)).flatten();
    let target = fg.unwrap_or_else(|| first_window(&chain));
    let pid = chain.iter().find(|(_, name)| name == "claude.exe" || name == "node.exe").map_or(0, |(p, _)| *p);
    if let Some(mut ev) = agent {
        ev.target = target;
        ev.strong = fg.is_some();
        ev.pid = pid;
        if let Ok(data) = serde_json::to_vec(&ev) {
            send_raw(COPYDATA_AGENT, &data, 500);
        }
    }
    if let Some(mut n) = notice {
        n.target = target;
        if let Ok(data) = serde_json::to_vec(&n) {
            send_raw(COPYDATA_NOTIFY, &data, 1500);
        }
    }
}

pub fn send(n: &Notice) -> bool {
    let mut n = n.clone();
    if n.target == 0 {
        n.target = first_window(&ancestors());
    }
    let Ok(data) = serde_json::to_vec(&n) else { return false };
    send_raw(COPYDATA_NOTIFY, &data, 3000)
}

fn send_raw(kind: usize, data: &[u8], timeout_ms: u32) -> bool {
    unsafe {
        let Ok(host) = FindWindowW(w!("TaskbaroqueWnd"), w!("TaskbaroqueHost")) else { return false };
        let cds = COPYDATASTRUCT { dwData: kind, cbData: data.len() as u32, lpData: data.as_ptr() as _ };
        let mut res = 0usize;
        let ok = SendMessageTimeoutW(host, WM_COPYDATA, WPARAM(0), LPARAM(&cds as *const _ as isize), SMTO_ABORTIFHUNG, timeout_ms, Some(&mut res));
        ok.0 != 0
    }
}

/// Bu sürecin üst süreçleri (en yakından uzağa): (pid, küçük harf exe adı).
fn ancestors() -> Vec<(u32, String)> {
    let procs = process_map();
    let mut out = Vec::new();
    let mut pid = unsafe { GetCurrentProcessId() };
    for _ in 0..16 {
        let Some((ppid, _)) = procs.get(&pid) else { break };
        let ppid = *ppid;
        if ppid == 0 || ppid == pid || out.iter().any(|(p, _)| *p == ppid) {
            break;
        }
        let Some((_, name)) = procs.get(&ppid) else { break };
        out.push((ppid, name.clone()));
        pid = ppid;
    }
    out
}

fn process_map() -> HashMap<u32, (u32, String)> {
    let mut map = HashMap::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return map };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let n = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
            map.insert(e.th32ProcessID, (e.th32ParentProcessID, String::from_utf16_lossy(&e.szExeFile[..n]).to_lowercase()));
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    map
}

/// Öndeki pencere üst süreçlerden birine aitse onu döndürür.
fn foreground_in(chain: &[(u32, String)]) -> Option<isize> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return None;
        }
        let mut p = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut p));
        chain.iter().any(|(pid, _)| *pid == p).then_some(fg.0 as isize)
    }
}

/// Üst süreçlerden ilk görünür ana pencere (tahmin).
fn first_window(chain: &[(u32, String)]) -> isize {
    chain.iter().map(|(pid, _)| main_window(*pid)).find(|h| *h != 0).unwrap_or(0)
}

fn main_window(pid: u32) -> isize {
    unsafe extern "system" fn each(h: HWND, lp: LPARAM) -> BOOL {
        let st = &mut *(lp.0 as *mut (u32, isize));
        let mut p = 0u32;
        GetWindowThreadProcessId(h, Some(&mut p));
        let owned = GetWindow(h, GW_OWNER).map(|o| !o.is_invalid()).unwrap_or(false);
        let tool = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0;
        if p == st.0 && IsWindowVisible(h).as_bool() && !owned && !tool && GetWindowTextLengthW(h) > 0 {
            st.1 = h.0 as isize;
            return FALSE;
        }
        TRUE
    }
    let mut st = (pid, 0isize);
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut st as *mut _ as isize));
    }
    st.1
}

pub enum Incoming {
    Notice(Notice),
    Agent(AgentEvent),
}

pub fn parse(lp: LPARAM) -> Option<Incoming> {
    if lp.0 == 0 {
        return None;
    }
    unsafe {
        let cds = &*(lp.0 as *const COPYDATASTRUCT);
        if cds.lpData.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize);
        match cds.dwData {
            COPYDATA_NOTIFY => serde_json::from_slice(bytes).ok().map(Incoming::Notice),
            COPYDATA_AGENT => serde_json::from_slice(bytes).ok().map(Incoming::Agent),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_events() {
        let n = from_hook(r#"{"hook_event_name":"Notification","notification_type":"permission_prompt","message":"Claude needs your permission to use Bash","cwd":"D:\\Repos\\app"}"#).unwrap();
        assert_eq!(n.message, "Claude needs your permission to use Bash");
        assert_eq!((n.title.as_str(), n.kind.as_str()), ("Claude Code · app", "attention"));
        assert_eq!(from_hook(r#"{"hook_event_name":"Stop"}"#).unwrap().message, "Görev bitti");
        assert_eq!(from_hook(r#"{"hook_event_name":"Stop"}"#).unwrap().kind, "done");
        // "Girdi bekliyorum" hatırlatması susturulur
        assert!(from_hook(r#"{"hook_event_name":"Notification","notification_type":"idle_prompt","message":"Claude is waiting for your input"}"#).is_none());
        assert!(from_hook(r#"{"hook_event_name":"Notification","message":"Claude is waiting for your input"}"#).is_none());
        assert!(from_hook("").is_none());
        assert!(from_hook(r#"{"hook_event_name":"PreToolUse","session_id":"s"}"#).is_none());
    }

    #[test]
    fn agent_events() {
        let e = agent_from_hook(r#"{"hook_event_name":"PreToolUse","session_id":"s1","cwd":"D:\\Repos\\win-theme","tool_name":"Edit","tool_input":{"file_path":"D:\\Repos\\win-theme\\src\\app.rs"}}"#).unwrap();
        assert_eq!((e.project.as_str(), e.state.as_str(), e.detail.as_str()), ("win-theme", "working", "Düzenliyor: app.rs"));
        let e = agent_from_hook(r#"{"hook_event_name":"Stop","session_id":"s1","cwd":"C:/x/proj/"}"#).unwrap();
        assert_eq!((e.project.as_str(), e.state.as_str()), ("proj", "done"));
        let e = agent_from_hook(r#"{"hook_event_name":"PermissionRequest","session_id":"s1","tool_name":"Bash","tool_input":{"command":"git push"}}"#).unwrap();
        assert_eq!((e.state.as_str(), e.detail.as_str()), ("waiting", "Onay · Komut: git push"));
        assert_eq!(agent_from_hook(r#"{"hook_event_name":"PostToolUse","session_id":"s1"}"#).unwrap().state, "working");
        assert!(agent_from_hook(r#"{"hook_event_name":"Notification","session_id":"s1","notification_type":"idle_prompt"}"#).is_none());
    }

    #[test]
    fn args() {
        let a: Vec<String> = ["x", "--notify", "Derleme bitti", "--title", "Warp", "--sound"].iter().map(|s| s.to_string()).collect();
        assert_eq!(from_args(&a), Some(Notice { title: "Warp".into(), message: "Derleme bitti".into(), sound: true, target: 0, kind: String::new() }));
    }
}
