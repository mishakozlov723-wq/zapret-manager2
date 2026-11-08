// User settings: storage folders and the user's own domains for list-general-user.txt.
// Stored in %LOCALAPPDATA%\ZapretManager\settings.json, separately from any downloaded Zapret,
// so a new release always brings a fresh list-general.txt and the user's additions are re-applied on top.
use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};

pub const MAX_DOMAINS: usize = 500;

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct Settings {
    pub downloads: Option<String>,
    pub archive: Option<String>,
    pub install: Option<String>,
    pub domains: Vec<String>,
    /// Domains written into the running service's list on the last successful connect.
    pub applied: Option<Vec<String>>,
    /// Start with Windows: the user's choice, kept while disconnected and applied on connect.
    pub autostart: Option<bool>,
    /// Sites winws must never touch (list-exclude-user.txt).
    pub exclude: Vec<String>,
    /// Game mode: "all" | "tcp" | "udp"; None = off.
    pub game: Option<String>,
    /// Exclusions, game mode and IP list as they were on the last successful connect.
    pub applied_extra: Option<String>,
}

fn data_dir() -> Result<PathBuf, String> {
    Ok(PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("Не найдена папка LOCALAPPDATA")?).join("ZapretManager"))
}
fn settings_file() -> Result<PathBuf, String> { Ok(data_dir()?.join("settings.json")) }

pub fn load() -> Settings {
    settings_file().ok().and_then(|p| fs::read(p).ok()).and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
}
pub fn save(s: &Settings) -> Result<(), String> {
    let path = settings_file()?;
    fs::create_dir_all(data_dir()?).map_err(|e| format!("Не удалось сохранить настройки: {e}"))?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?).map_err(|e| format!("Не удалось сохранить настройки: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Не удалось сохранить настройки: {e}"))
}

pub fn default_downloads() -> Result<PathBuf, String> { Ok(data_dir()?.join("versions")) }
pub fn default_archive() -> Result<PathBuf, String> { Ok(data_dir()?.join("old")) }
pub fn default_install() -> Result<PathBuf, String> {
    Ok(PathBuf::from(std::env::var_os("ProgramFiles").ok_or("Нет папки Program Files")?).join("Zapret Manager").join("versions"))
}
pub fn downloads_dir() -> Result<PathBuf, String> { load().downloads.map(PathBuf::from).map_or_else(default_downloads, Ok) }
pub fn archive_dir() -> Result<PathBuf, String> { load().archive.map(PathBuf::from).map_or_else(default_archive, Ok) }
/// A custom install folder gets its own ZapretManager subfolder: its permissions are locked to administrators,
/// and we must never change the permissions of a folder the user picked (it may hold their other files).
pub fn install_base() -> Result<PathBuf, String> { load().install.map(|p| PathBuf::from(p).join("ZapretManager")).map_or_else(default_install, Ok) }

/// Local absolute path like D:\Zapret: no UNC/network paths, no characters that would break the service command.
pub fn checked_folder(input: &str) -> Result<PathBuf, String> {
    let p = checked_browse_folder(input)?;
    if p.parent().is_none() { return Err("Откройте или создайте папку внутри диска".into()); }
    Ok(p)
}

// Browsing and creating a child must accept drive roots; storing releases there must not.
fn checked_browse_folder(input: &str) -> Result<PathBuf, String> {
    let raw = input.trim();
    let bytes = raw.as_bytes();
    if bytes.len() == 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/') {
        return Ok(PathBuf::from(format!("{}\\", &raw[..2])));
    }
    let s = raw.trim_end_matches(['\\', '/']);
    let b = s.as_bytes();
    // A bare drive root (C:\) is not accepted: versions must live in their own folder.
    if s.len() < 4 || s.len() > 180 || !b[0].is_ascii_alphabetic() || b[1] != b':' { return Err("Выбери папку на локальном диске".into()); }
    if s.len() > 2 && b[2] != b'\\' && b[2] != b'/' { return Err("Выбери папку на локальном диске".into()); }
    if s.chars().any(|c| c.is_control() || "\"%<>|?*".contains(c)) || s[2..].contains(':') { return Err("В пути есть недопустимые символы".into()); }
    let p = PathBuf::from(format!("{}\\", &s[..2])).join(s[2..].trim_start_matches(['\\', '/']));
    if p.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::CurDir)) { return Err("Недопустимый путь".into()); }
    if let Some(win) = std::env::var_os("SystemRoot") {
        if p.to_string_lossy().to_lowercase().starts_with(&win.to_string_lossy().to_lowercase()) { return Err("Нельзя использовать системную папку Windows".into()); }
    }
    Ok(p)
}

/// "https://SoundCloud.com/x" -> "soundcloud.com". Only plain hostnames go into the list file.
pub fn checked_domain(input: &str) -> Result<String, String> {
    let mut s = input.trim().to_lowercase();
    for p in ["https://", "http://"] { if let Some(r) = s.strip_prefix(p) { s = r.to_string(); } }
    if let Some(i) = s.find('/') { s.truncate(i); }
    let s = s.trim_end_matches('.').to_string();
    let ok = s.len() <= 253 && s.contains('.') && s.split('.').all(|l| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'));
    if !ok { return Err(format!("«{}» не похоже на домен", input.trim())); }
    Ok(s)
}
pub fn clean_domains(list: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for d in list { let d = checked_domain(d)?; if !out.contains(&d) { out.push(d); } }
    if out.len() > MAX_DOMAINS { return Err(format!("Не больше {MAX_DOMAINS} доменов")); }
    Ok(out)
}

/// Move release folders (tag-timestamp) from one folder to another; keep `keep` in place.
pub fn move_versions(from: &Path, to: &Path, keep: Option<&str>) -> Result<usize, String> {
    let Ok(items) = fs::read_dir(from) else { return Ok(0) };
    let mut moved = 0;
    for e in items.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some((tag, time)) = name.rsplit_once('-') else { continue };
        if tag.is_empty() || time.parse::<u64>().is_err() || !e.path().join("release.zip").is_file() || Some(name.as_str()) == keep { continue; }
        fs::create_dir_all(to).map_err(|e| format!("Не удалось создать папку: {e}"))?;
        let dest = to.join(&name);
        if dest.exists() { continue; }
        if fs::rename(e.path(), &dest).is_err() {
            copy_dir(&e.path(), &dest).map_err(|e| format!("Не удалось перенести {name}: {e}"))?;
            fs::remove_dir_all(e.path()).map_err(|e| format!("Не удалось убрать старую копию {name}: {e}"))?;
        }
        moved += 1;
    }
    Ok(moved)
}
fn view() -> Result<serde_json::Value, String> {
    let s = load();
    let extra_pending = s.applied_extra.clone().unwrap_or_else(|| "|off|off".into()) != extra_key(&s);
    let ipset = ipset_info().map(|(n, t)| serde_json::json!({"count": n, "time": t}));
    let game = s.game.clone().unwrap_or_else(|| "off".into());
    Ok(serde_json::json!({
        "exclude": s.exclude, "game": game, "ipset": ipset, "extra_pending": extra_pending,
        "downloads": downloads_dir()?.display().to_string(), "downloads_custom": s.downloads.is_some(),
        "archive": archive_dir()?.display().to_string(), "archive_custom": s.archive.is_some(),
        "install": install_base()?.display().to_string(), "install_custom": s.install.is_some(),
        "domains": s.domains, "applied": s.applied,
    }))
}
#[tauri::command]
pub async fn get_settings() -> Result<serde_json::Value, String> { tauri::async_runtime::spawn_blocking(view).await.map_err(|e| e.to_string())? }

/// kind: downloads | archive | install. path None = back to default. Downloaded versions follow the downloads folder.
#[tauri::command]
pub async fn set_folder(kind: String, path: Option<String>) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let chosen = match path.as_deref() { Some(p) => Some(checked_folder(p)?), None => None };
        if let Some(p) = &chosen { fs::create_dir_all(p).map_err(|e| format!("Не удалось открыть папку: {e}"))?; }
        let mut s = load();
        let value = chosen.map(|p| p.display().to_string());
        match kind.as_str() {
            "downloads" => {
                let old = downloads_dir()?;
                let new = match &value { Some(v) => PathBuf::from(v), None => default_downloads()? };
                if old != new { move_versions(&old, &new, None)?; }
                s.downloads = value;
            }
            "archive" => s.archive = value,
            "install" => s.install = value,
            _ => return Err("Неизвестная папка".into()),
        }
        save(&s)?;
        view()
    }).await.map_err(|e| e.to_string())?
}

/// Folder listing for the in-app picker. Empty path = list of drives.
#[tauri::command]
pub async fn browse(path: Option<String>) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let drives: Vec<String> = (b'A'..=b'Z').map(|c| format!("{}:\\", c as char)).filter(|d| Path::new(d).is_dir()).collect();
        let Some(p) = path.filter(|p| !p.trim().is_empty()) else { return Ok(serde_json::json!({"path": null, "parent": null, "dirs": [], "drives": drives})) };
        let dir = checked_browse_folder(&p)?;
        let mut dirs: Vec<String> = fs::read_dir(&dir).map_err(|_| "Папку не удалось открыть".to_string())?.flatten()
            .filter(|e| e.file_type().map_or(false, |t| t.is_dir()))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| !n.starts_with('$') && !n.starts_with('.') && n != "System Volume Information")
            .collect();
        dirs.sort_by_key(|n| n.to_lowercase()); dirs.truncate(500);
        let parent = dir.parent().map(|p| p.display().to_string());
        Ok(serde_json::json!({"path": dir.display().to_string(), "parent": parent, "dirs": dirs, "drives": drives}))
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn make_folder(parent: String, name: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let n = name.trim();
        if n.is_empty() || n.len() > 80 || n == "." || n == ".." || n.ends_with('.') || n.chars().any(|c| c.is_control() || "\\/:*?\"<>|%".contains(c)) { return Err("Недопустимое имя папки".to_string()); }
        let p = checked_folder(&checked_browse_folder(&parent)?.join(n).display().to_string())?;
        fs::create_dir(&p).map_err(|e| if e.kind() == std::io::ErrorKind::AlreadyExists { "Такая папка уже есть".to_string() } else { format!("Не удалось создать папку: {e}") })?;
        Ok(p.display().to_string())
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn save_domains(domains: Vec<String>) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = load(); s.domains = clean_domains(&domains)?; save(&s)?; view()
    }).await.map_err(|e| e.to_string())?
}
pub fn mark_applied(domains: &[String], extra: &str) { let mut s = load(); s.applied = Some(domains.to_vec()); s.applied_extra = Some(extra.to_string()); let _ = save(&s); }

pub fn data_path(name: &str) -> Result<PathBuf, String> {
    let dir = data_dir()?;
    fs::create_dir_all(&dir).map_err(|e| format!("Не удалось создать папку данных: {e}"))?;
    Ok(dir.join(name))
}
pub fn ipset_file() -> Result<PathBuf, String> { Ok(data_dir()?.join("ipset-all.txt")) }
/// (addresses, unix time) of the downloaded IP list, if there is one.
fn ipset_info() -> Option<(usize, u64)> {
    let p = ipset_file().ok()?;
    let meta = fs::metadata(&p).ok()?;
    let text = fs::read_to_string(&p).ok()?;
    let n = text.lines().filter(|l| !l.trim().is_empty()).count();
    let t = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some((n, t))
}
/// Ports for %GameFilterTCP% / %GameFilterUDP%: all high ports in game mode, otherwise the official "off" value 12.
pub fn game_ports(s: &Settings) -> (&'static str, &'static str) {
    const ALL: &str = "1024-65535";
    match s.game.as_deref() { Some("all") => (ALL, ALL), Some("tcp") => (ALL, "12"), Some("udp") => ("12", ALL), _ => ("12", "12") }
}
/// Fingerprint of everything besides own sites that the running service was started with.
pub fn extra_key(s: &Settings) -> String {
    let mut ex = s.exclude.clone(); ex.sort();
    let ip = ipset_info().map(|(n, t)| format!("{n}@{t}")).unwrap_or_else(|| "off".into());
    format!("{}|{}|{}", ex.join(","), s.game.as_deref().unwrap_or("off"), ip)
}
#[tauri::command]
pub async fn save_exclude(domains: Vec<String>) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = load(); s.exclude = clean_domains(&domains)?; save(&s)?; view()
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn set_game(mode: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = load();
        s.game = match mode.as_str() { "off" => None, "all" | "tcp" | "udp" => Some(mode.clone()), _ => return Err("Неизвестный режим".to_string()) };
        save(&s)?; view()
    }).await.map_err(|e| e.to_string())?
}
const IPSET_URL: &str = "https://raw.githubusercontent.com/Flowseal/zapret-discord-youtube/refs/heads/main/.service/ipset-service.txt";
/// One IPv4/IPv6 address or network per line. Returns the cleaned list (CRLF) and its size.
pub fn checked_ipset(text: &str) -> Result<(String, usize), String> {
    let mut out: Vec<&str> = Vec::new();
    for l in text.lines() {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') { continue; }
        let bad = || format!("Неверная строка в списке IP: {l}");
        let (ip, pre) = match l.split_once('/') { Some((a, b)) => (a, Some(b)), None => (l, None) };
        let addr: std::net::IpAddr = ip.parse().map_err(|_| bad())?;
        if let Some(p) = pre {
            let p: u8 = p.parse().map_err(|_| bad())?;
            let max = if addr.is_ipv4() { 32 } else { 128 };
            if p > max { return Err(bad()); }
        }
        out.push(l);
    }
    if out.len() < 10 { return Err("Список IP пустой или повреждён".into()); }
    let n = out.len();
    Ok((out.join("\r\n") + "\r\n", n))
}
#[tauri::command]
pub async fn update_ipset() -> Result<serde_json::Value, super::net::NetError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, super::net::NetError> {
        let agent = super::https_agent(std::time::Duration::from_secs(30))?;
        let data = super::net::get_bytes(&agent, IPSET_URL, 5 * 1024 * 1024)?;
        let text = String::from_utf8(data).map_err(|_| "Список IP повреждён".to_string())?;
        let (clean, _) = checked_ipset(&text)?;
        let p = data_path("ipset-all.txt")?;
        let tmp = p.with_extension("tmp");
        fs::write(&tmp, clean).map_err(|e| format!("Не удалось сохранить список IP: {e}"))?;
        fs::rename(&tmp, &p).map_err(|e| format!("Не удалось сохранить список IP: {e}"))?;
        Ok(view()?)
    }).await.map_err(|e| super::net::NetError::fatal(e.to_string()))?
}
#[tauri::command]
pub async fn clear_ipset() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = ipset_file()?;
        if p.exists() { fs::remove_file(&p).map_err(|e| format!("Не удалось выключить список IP: {e}"))?; }
        view()
    }).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod extra_tests {
    use super::*;
    #[test] fn ipset_validation() {
        let good: String = (1..=12).map(|i| format!("10.0.{i}.0/24\n")).collect::<String>() + "# comment\n2606:4700::/32\n1.1.1.1\n";
        let (clean, n) = checked_ipset(&good).unwrap();
        assert_eq!(n, 14);
        assert!(!clean.contains('#'));
        assert!(checked_ipset("<html>").is_err());
        assert!(checked_ipset(&(good.clone() + "10.0.0.0/33\n")).is_err());
        assert!(checked_ipset("1.1.1.1\n").is_err());
    }
    #[test] fn game_modes() {
        let mut s = Settings::default();
        assert_eq!(game_ports(&s), ("12", "12"));
        s.game = Some("all".into()); assert_eq!(game_ports(&s), ("1024-65535", "1024-65535"));
        s.game = Some("udp".into()); assert_eq!(game_ports(&s), ("12", "1024-65535"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn settings_validation() {
        assert_eq!(checked_domain("https://SoundCloud.com/discover").unwrap(), "soundcloud.com");
        assert_eq!(checked_domain("sndcdn.com.").unwrap(), "sndcdn.com");
        for bad in ["", "localhost", "a b.com", "-x.com", "x..com", "x.com\"&calc", "%TEMP%.com"] { assert!(checked_domain(bad).is_err(), "{bad}"); }
        assert_eq!(clean_domains(&["a.com".into(), "A.com".into(), "b.com".into()]).unwrap(), vec!["a.com", "b.com"]);
        assert_eq!(checked_folder("D:\\Zapret\\").unwrap(), PathBuf::from("D:\\Zapret"));
        assert_eq!(checked_folder("d:/games/zap").unwrap(), PathBuf::from("d:\\games\\zap"));
        for bad in ["\\\\server\\share", "relative\\x", "C:", "D:\\a\"b", "D:\\%TEMP%", "D:\\a\\..\\b", "D:x", "C:\\Windows\\System32"] { assert!(checked_folder(bad).is_err(), "{bad}"); }
    }
    #[test] fn drive_roots_are_browsable_not_storage() {
        for root in ["C:\\", "D:/", "F:\\"] {
            assert!(checked_browse_folder(root).is_ok(), "{root}");
            assert!(checked_folder(root).is_err(), "{root}");
            let child = checked_browse_folder(root).unwrap().join("Zapret");
            assert!(checked_folder(&child.display().to_string()).is_ok());
        }
        for bad in ["C:", "D:relative", "\\\\server\\share", "D:\\a\\..\\b"] {
            assert!(checked_browse_folder(bad).is_err(), "{bad}");
        }
    }
    #[test] fn browse_real_windows_drive() {
        let cwd = std::env::current_dir().unwrap();
        let root = cwd.ancestors().last().unwrap().display().to_string();
        let listing = tauri::async_runtime::block_on(browse(Some(root.clone()))).unwrap();
        assert_eq!(listing["path"], root);
        assert!(listing["parent"].is_null());
        assert!(listing["dirs"].as_array().unwrap().len() > 0);
        let listing = tauri::async_runtime::block_on(browse(Some(cwd.display().to_string()))).unwrap();
        assert!(listing["dirs"].as_array().unwrap().iter().any(|n| n == "src"));
    }
    #[test] fn create_and_browse_real_folder() {
        let parent = std::env::current_dir().unwrap().join("target");
        let name = format!("picker-regression-{}", std::process::id());
        let created = tauri::async_runtime::block_on(make_folder(parent.display().to_string(), name)).unwrap();
        let listing = tauri::async_runtime::block_on(browse(Some(created.clone()))).unwrap();
        assert_eq!(listing["path"], created);
        assert_eq!(checked_folder(&created).unwrap(), PathBuf::from(&created));
        fs::remove_dir(created).unwrap();
    }
    #[test] fn moves_old_versions() {
        let root = std::env::temp_dir().join(format!("zm-move-{}", std::process::id()));
        let (a, b) = (root.join("a"), root.join("b"));
        for v in ["1.0-1", "1.1-2"] { fs::create_dir_all(a.join(v).join("files")).unwrap(); fs::write(a.join(v).join("release.zip"), b"x").unwrap(); }
        fs::create_dir_all(a.join("unrelated")).unwrap();
        assert_eq!(move_versions(&a, &b, Some("1.1-2")).unwrap(), 1);
        assert!(b.join("1.0-1").join("release.zip").is_file() && a.join("1.1-2").is_dir() && a.join("unrelated").is_dir());
        fs::remove_dir_all(root).unwrap();
    }
}
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        if e.file_type()?.is_dir() { copy_dir(&e.path(), &to.join(e.file_name()))?; } else { fs::copy(e.path(), to.join(e.file_name()))?; }
    }
    Ok(())
}
