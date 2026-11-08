// In-app updates: GitHub "latest" release of this app, verified by size and SHA-256 before launch.
use crate::net::{self, NetError};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

const REPO: &str = "mishakozlov723-wq/zapret-manager2";
const ASSET: &str = "Zapret-Manager-Setup.exe";
const MAX_SETUP: u64 = 400 * 1024 * 1024;

static DONE: AtomicU64 = AtomicU64::new(0);
static TOTAL: AtomicU64 = AtomicU64::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);
static READY: Mutex<Option<PathBuf>> = Mutex::new(None);

#[derive(Serialize, Clone, Debug)]
pub struct AppUpdate {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub notes: String,
    pub page: String,
    pub size: u64,
    #[serde(skip)] url: String,
    #[serde(skip)] digest: String,
}

#[derive(Serialize)]
pub struct Progress { pub done: u64, pub total: u64, pub busy: bool }

pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim();
    let v = v.strip_prefix('v').unwrap_or(v);
    let mut it = v.split('.');
    let a = it.next()?.parse().ok()?;
    let b = it.next()?.parse().ok()?;
    let c = it.next()?.parse().ok()?;
    if it.next().is_some() { return None; }
    Some((a, b, c))
}
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) { (Some(l), Some(c)) => l > c, _ => false }
}
fn valid_setup_url(value: &str, tag: &str) -> bool {
    let Ok(u) = url::Url::parse(value) else { return false };
    let path = format!("/{}/releases/download/{}/{}", REPO, tag, ASSET);
    u.scheme() == "https" && u.host_str() == Some("github.com") && u.port().is_none() && u.username().is_empty() && u.password().is_none() && u.query().is_none() && u.fragment().is_none() && u.path() == path
}

pub fn read_update(json: &serde_json::Value, current: &str) -> Result<AppUpdate, NetError> {
    let tag = json["tag_name"].as_str().ok_or("В релизе нет номера версии")?;
    if parse_version(tag).is_none() { return Err("Некорректный номер версии".into()); }
    let latest = tag.trim_start_matches('v').to_string();
    let page = format!("https://github.com/{}/releases/tag/{}", REPO, tag);
    let notes: String = json["body"].as_str().unwrap_or("").chars().take(2000).collect();
    let available = is_newer(tag, current);
    let mut up = AppUpdate { current: current.into(), latest, available, notes, page, size: 0, url: String::new(), digest: String::new() };
    if !available { return Ok(up); }
    let asset = json["assets"].as_array().and_then(|a| a.iter().find(|x| x["name"].as_str() == Some(ASSET) && x["state"].as_str() == Some("uploaded"))).ok_or("В новой версии нет установщика")?;
    let href = asset["browser_download_url"].as_str().ok_or("Нет ссылки на установщик")?;
    if !valid_setup_url(href, tag) { return Err("Неожиданный адрес установщика".into()); }
    let digest = asset["digest"].as_str().and_then(|s| s.strip_prefix("sha256:")).ok_or("У установщика нет контрольной суммы")?.to_ascii_lowercase();
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("Неверная контрольная сумма".into()); }
    let size = asset["size"].as_u64().ok_or("Неизвестный размер установщика")?;
    if size == 0 || size > MAX_SETUP { return Err("Недопустимый размер установщика".into()); }
    up.size = size;
    up.url = href.into();
    up.digest = digest;
    Ok(up)
}

fn fetch() -> Result<AppUpdate, NetError> {
    let agent = super::https_agent(Duration::from_secs(30))?;
    let json = net::get_json(&agent, &format!("https://api.github.com/repos/{}/releases/latest", REPO))?;
    read_update(&json, env!("CARGO_PKG_VERSION"))
}

fn download() -> Result<String, NetError> {
    let up = fetch()?;
    if !up.available { return Err("У тебя уже последняя версия".into()); }
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(up.size, Ordering::Relaxed);
    let agent = super::https_agent(Duration::from_secs(120))?;
    let bytes = net::get_bytes_progress(&agent, &up.url, MAX_SETUP, &|n| DONE.store(n, Ordering::Relaxed))?;
    if bytes.len() as u64 != up.size || format!("{:x}", Sha256::digest(&bytes)) != up.digest {
        return Err("Установщик не прошёл проверку SHA-256".into());
    }
    let dir = std::env::temp_dir().join("ZapretManagerUpdate");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Не удалось создать папку: {e}"))?;
    let path = dir.join(format!("Zapret-Manager-Setup-{}.exe", up.latest));
    std::fs::write(&path, &bytes).map_err(|e| format!("Не удалось сохранить установщик: {e}"))?;
    *READY.lock().map_err(|_| "Ошибка состояния обновления")? = Some(path);
    Ok(up.latest)
}

#[tauri::command]
pub fn app_version() -> String { env!("CARGO_PKG_VERSION").into() }

#[tauri::command]
pub async fn check_app_update() -> Result<AppUpdate, NetError> {
    tauri::async_runtime::spawn_blocking(fetch).await.map_err(|e| NetError::fatal(e.to_string()))?
}

#[tauri::command]
pub fn app_update_progress() -> Progress {
    Progress { done: DONE.load(Ordering::Relaxed), total: TOTAL.load(Ordering::Relaxed), busy: BUSY.load(Ordering::Relaxed) }
}

#[tauri::command]
pub async fn download_app_update() -> Result<String, NetError> {
    if BUSY.swap(true, Ordering::SeqCst) { return Err("Загрузка уже идёт".into()); }
    let result = tauri::async_runtime::spawn_blocking(download).await.map_err(|e| NetError::fatal(e.to_string()));
    BUSY.store(false, Ordering::SeqCst);
    result?
}

// Runs only the installer this app downloaded and verified, then closes the app so files can be replaced.
#[tauri::command]
pub fn launch_app_update(app: tauri::AppHandle) -> Result<(), String> {
    let path = READY.lock().map_err(|_| "Ошибка состояния обновления".to_string())?.clone().ok_or("Сначала скачай обновление")?;
    if !path.is_file() { return Err("Файл установщика не найден, скачай ещё раз".into()); }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", "Start-Sleep -Seconds 2; Start-Process -FilePath $env:ZM_SETUP"])
            .env("ZM_SETUP", &path)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Не удалось запустить установщик: {e}"))?;
        app.exit(0);
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Err("Обновление доступно только в Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn versions() {
        assert_eq!(parse_version("v0.3.0"), Some((0, 3, 0)));
        assert!(parse_version("v1.2").is_none());
        assert!(parse_version("test-5").is_none());
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(!is_newer("v0.3.0", "0.3.0"));
        assert!(!is_newer("junk", "0.1.0"));
    }
    #[test] fn reads_release() {
        let tag = "v9.9.9";
        let href = format!("https://github.com/{}/releases/download/{}/{}", REPO, tag, ASSET);
        let digest = format!("sha256:{}", "a".repeat(64));
        let json = serde_json::json!({"tag_name": tag, "body": "notes", "assets": [{"name": ASSET, "state": "uploaded", "size": 1000, "digest": digest, "browser_download_url": href}]});
        let up = read_update(&json, "0.3.0").unwrap();
        assert!(up.available);
        assert_eq!(up.size, 1000);
        let mut bad = json.clone();
        bad["assets"][0]["browser_download_url"] = serde_json::json!("https://evil.com/x.exe");
        assert!(read_update(&bad, "0.3.0").is_err());
        let same = read_update(&serde_json::json!({"tag_name": "v0.3.0", "assets": []}), "0.3.0").unwrap();
        assert!(!same.available);
    }
}
