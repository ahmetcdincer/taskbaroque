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

pub fn unregister() {
    let Some(cur) = current() else { return };
    if crate::util::autostart_target().is_some_and(|t| same(&t, &cur)) {
        crate::util::set_autostart(false);
    }
    let Some(p) = claude_settings() else { return };
    let Ok(text) = std::fs::read_to_string(&p) else { return };
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&text) else { return };
    let ours = |h: &serde_json::Value| {
        h.get("command").and_then(|c| c.as_str()).is_some_and(|c| {
            let c = c.replace('\\', "/").to_ascii_lowercase();
            c.contains(&cur.replace('\\', "/").to_ascii_lowercase())
        })
    };
    let Some(hooks) = v.get_mut("hooks").and_then(|h| h.as_object_mut()) else { return };
    let mut changed = false;
    for groups in hooks.values_mut() {
        let Some(groups) = groups.as_array_mut() else { continue };
        for g in groups.iter_mut() {
            if let Some(list) = g.get_mut("hooks").and_then(|l| l.as_array_mut()) {
                let n = list.len();
                list.retain(|h| !ours(h));
                changed |= list.len() != n;
            }
        }
        groups.retain(|g| g.get("hooks").and_then(|l| l.as_array()).map_or(true, |l| !l.is_empty()));
    }
    hooks.retain(|_, g| g.as_array().map_or(true, |a| !a.is_empty()));
    if changed {
        if let Ok(s) = serde_json::to_string_pretty(&v) {
            let _ = std::fs::write(&p, s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
