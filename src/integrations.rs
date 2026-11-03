use std::path::{Path, PathBuf};

const EXE: &str = "taskbaroque.exe";

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

fn claude_settings() -> Option<PathBuf> {
    home().map(|h| h.join(".claude").join("settings.json"))
}

fn profiles() -> Vec<PathBuf> {
    let Some(h) = home() else { return Vec::new() };
    let mut v = Vec::new();
    for docs in [h.join("Documents"), h.join("OneDrive").join("Documents")] {
        for sub in ["WindowsPowerShell", "PowerShell"] {
            for f in ["profile.ps1", "Microsoft.PowerShell_profile.ps1"] {
                v.push(docs.join(sub).join(f));
            }
        }
    }
    v.into_iter().filter(|p| p.exists()).collect()
}

fn current() -> Option<String> {
    std::env::current_exe().ok().map(|p| p.display().to_string())
}

fn is_dev(p: &str) -> bool {
    let l = p.to_ascii_lowercase().replace('/', "\\");
    l.contains("\\target\\release\\") || l.contains("\\target\\debug\\")
}

fn same(a: &str, b: &str) -> bool {
    a.replace('/', "\\").eq_ignore_ascii_case(&b.replace('/', "\\"))
}

fn should_take(old: &str, cur: &str) -> bool {
    !same(old, cur) && (!Path::new(old).exists() || !is_dev(cur))
}

fn rewrite(text: &str, open: &str, close: &str, cur: &str) -> Option<String> {
    let needle = format!("{EXE}{close}").to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut changed = false;
    while let Some(i) = rest.to_ascii_lowercase().find(&needle) {
        let end = i + EXE.len();
        match rest[..i].rfind(open).map(|a| a + open.len()) {
            Some(a) if !rest[a..end].contains(['\n', '"', '\'']) && should_take(&rest[a..end], cur) => {
                out.push_str(&rest[..a]);
                out.push_str(cur);
                changed = true;
            }
            _ => out.push_str(&rest[..end]),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    changed.then_some(out)
}

fn update_file(path: &Path, f: impl Fn(&str) -> Option<String>) {
    let Ok(text) = std::fs::read_to_string(path) else { return };
    if let Some(new) = f(&text) {
        let _ = std::fs::write(path, new);
    }
}

pub fn heal() {
    let Some(cur) = current() else { return };
    if let Some(p) = claude_settings() {
        let fwd = cur.replace('\\', "/");
        update_file(&p, |t| rewrite(t, "\\\"", "\\\" --hook", &fwd));
    }
    for p in profiles() {
        update_file(&p, |t| rewrite(t, "'", "'", &cur));
    }
    if let Some(t) = crate::util::autostart_target() {
        if should_take(&t, &cur) {
            crate::util::set_autostart(true);
        }
    }
}

/// Claude Code'da Taskbaroque'un dinlediği olaylar.
const HOOK_EVENTS: [&str; 9] = [
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

/// Komut bizim mi? (yol ne olursa olsun `taskbaroque.exe" --hook`)
fn is_ours(cmd: &str) -> bool {
    let c = cmd.replace('\\', "/").to_ascii_lowercase();
    c.contains("taskbaroque.exe\" --hook") || c.contains("taskbaroque.exe --hook")
}

fn read_json(p: &Path) -> Result<serde_json::Value, String> {
    match std::fs::read_to_string(p) {
        Ok(t) => {
            let t = t.trim_start_matches('\u{feff}');
            if t.trim().is_empty() {
                return Ok(serde_json::json!({}));
            }
            serde_json::from_str(t).map_err(|e| format!("settings.json okunamadı: {e}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({})),
        Err(e) => Err(e.to_string()),
    }
}

/// Yedekleyip geçici dosya üzerinden yazar (yarım kalan yazım dosyayı bozmasın).
fn write_json(p: &Path, v: &serde_json::Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    if p.exists() {
        let t = crate::util::now();
        let name = format!("settings.json.bak-taskbaroque-{:04}{:02}{:02}-{:02}{:02}{:02}", t.year, t.month, t.day, t.hour, t.min, t.sec);
        std::fs::copy(p, p.with_file_name(name)).map_err(|e| e.to_string())?;
    } else if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = p.with_extension("json.taskbaroque-tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, p).map_err(|e| e.to_string())
}

/// Bizim hook kayıtlarını çıkarır; başka araçların (coucou, Orca…) kayıtlarına dokunmaz.
fn without_ours(v: &mut serde_json::Value) {
    let Some(hooks) = v.get_mut("hooks").and_then(|h| h.as_object_mut()) else { return };
    for groups in hooks.values_mut() {
        let Some(groups) = groups.as_array_mut() else { continue };
        for g in groups.iter_mut() {
            if let Some(list) = g.get_mut("hooks").and_then(|l| l.as_array_mut()) {
                list.retain(|h| !h.get("command").and_then(|c| c.as_str()).is_some_and(is_ours));
            }
        }
        groups.retain(|g| g.get("hooks").and_then(|l| l.as_array()).map_or(true, |l| !l.is_empty()));
    }
    hooks.retain(|_, g| g.as_array().map_or(true, |a| !a.is_empty()));
}

fn with_ours(v: &serde_json::Value, exe: &str) -> serde_json::Value {
    let mut v = v.clone();
    without_ours(&mut v);
    if !v.is_object() {
        v = serde_json::json!({});
    }
    let cmd = format!("\"{}\" --hook", exe.replace('\\', "/"));
    let obj = v.as_object_mut().unwrap();
    let hooks = obj.entry("hooks").or_insert_with(|| serde_json::json!({}));
    if !hooks.is_object() {
        *hooks = serde_json::json!({});
    }
    let hooks = hooks.as_object_mut().unwrap();
    for ev in HOOK_EVENTS {
        let list = hooks.entry(ev).or_insert_with(|| serde_json::json!([]));
        if let Some(a) = list.as_array_mut() {
            a.push(serde_json::json!({ "hooks": [{ "type": "command", "command": cmd, "timeout": 10 }] }));
        }
    }
    v
}

/// Claude Code hook'larını kurar ya da günceller. Değişiklik yoksa dosyaya dokunmaz.
/// Geliştirme derlemelerinden (target\release) kurmaz; kurulu sürüm kendi yolunu yazar.
pub fn ensure_hooks() -> Result<bool, String> {
    let Some(cur) = current() else { return Ok(false) };
    if is_dev(&cur) {
        return Ok(false);
    }
    let Some(p) = claude_settings() else { return Ok(false) };
    let old = read_json(&p)?;
    let new = with_ours(&old, &cur);
    if new == old {
        return Ok(false);
    }
    write_json(&p, &new)?;
    Ok(true)
}

pub fn unregister() {
    let Some(cur) = current() else { return };
    if crate::util::autostart_target().is_some_and(|t| same(&t, &cur)) {
        crate::util::set_autostart(false);
    }
    let Some(p) = claude_settings() else { return };
    let Ok(old) = read_json(&p) else { return };
    let mut v = old.clone();
    without_ours(&mut v);
    if v != old {
        let _ = write_json(&p, &v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_merge_keeps_others() {
        let old = serde_json::json!({
            "model": "x",
            "hooks": {
                "Stop": [
                    { "hooks": [{ "type": "command", "command": "\"C:/c/coucou-hook.exe\" Stop" }] },
                    { "hooks": [{ "type": "command", "command": "\"C:/old/taskbaroque.exe\" --hook" }] }
                ]
            }
        });
        let new = with_ours(&old, "C:\\P\\taskbaroque.exe");
        let stop = new["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "\"C:/c/coucou-hook.exe\" Stop");
        assert_eq!(stop[1]["hooks"][0]["command"], "\"C:/P/taskbaroque.exe\" --hook");
        assert_eq!(new["model"], "x");
        for ev in HOOK_EVENTS {
            let ours = new["hooks"][ev].as_array().unwrap().iter().filter(|g| g["hooks"][0]["command"].as_str().is_some_and(is_ours)).count();
            assert_eq!(ours, 1);
        }
        // İkinci kez uygulamak değişiklik yapmaz
        assert_eq!(with_ours(&new, "C:\\P\\taskbaroque.exe"), new);
        let mut gone = new.clone();
        without_ours(&mut gone);
        assert_eq!(gone["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert!(gone["hooks"].get("PreToolUse").is_none());
    }

    #[test]
    fn rewrites_missing_paths() {
        let t = r#"{"command": "\"C:/nope/old/taskbaroque.exe\" --hook", "x": "\"C:/other/coucou.exe\" Stop"}"#;
        let r = rewrite(t, "\\\"", "\\\" --hook", "C:/new/taskbaroque.exe").unwrap();
        assert_eq!(r, r#"{"command": "\"C:/new/taskbaroque.exe\" --hook", "x": "\"C:/other/coucou.exe\" Stop"}"#);
        assert!(rewrite(&r, "\\\"", "\\\" --hook", "C:/new/taskbaroque.exe").is_none());
        let p = "$global:__TaskbaroqueExe = 'C:\\nope\\taskbaroque.exe'\n";
        assert_eq!(rewrite(p, "'", "'", "C:\\new\\taskbaroque.exe").unwrap(), "$global:__TaskbaroqueExe = 'C:\\new\\taskbaroque.exe'\n");
    }
}
