// Old Zapret versions: downloaded archives ({tag}-{stamp} folders) and unpacked service copies ({sha} folders).
// One entry per release (same SHA-256), so the user deletes a version, not separate folders.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}, time::UNIX_EPOCH};

#[derive(Serialize, Clone, Default)]
pub struct Version {
    pub id: String,
    pub tag: String,
    pub date: u64,
    pub size: u64,
    pub used: bool,
    pub note: String,
    #[serde(skip)]
    archives: Vec<PathBuf>,
    #[serde(skip)]
    installed: bool,
}

fn dir_size(p: &Path) -> u64 {
    let Ok(items) = fs::read_dir(p) else { return 0 };
    items.flatten().map(|e| match e.file_type() {
        Ok(t) if t.is_dir() => dir_size(&e.path()),
        Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
        _ => 0,
    }).sum()
}
fn modified_ms(p: &Path) -> u64 {
    fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64)
}
fn is_sha(s: &str) -> bool { s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) }

/// SHA-256 of a downloaded archive: the one saved at download time, otherwise computed.
pub fn archive_sha(dir: &Path) -> Option<String> {
    if let Ok(s) = fs::read_to_string(dir.join("release.sha256")) {
        let s = s.trim().to_ascii_lowercase();
        if is_sha(&s) { return Some(s); }
    }
    let data = fs::read(dir.join("release.zip")).ok()?;
    Some(format!("{:x}", Sha256::digest(&data)))
}

/// `current` = SHA of the archive used on connect, `running` = SHA the service points to. Both can't be deleted.
pub fn collect(archive_dirs: &[PathBuf], install: Option<&Path>, current: Option<&str>, running: Option<&str>) -> Vec<Version> {
    let mut map: BTreeMap<String, Version> = BTreeMap::new();
    for base in archive_dirs {
        let Ok(items) = fs::read_dir(base) else { continue };
        for e in items.flatten() {
            let path = e.path();
            if !path.is_dir() { continue; }
            let name = e.file_name().to_string_lossy().to_string();
            let Some((tag, stamp)) = name.rsplit_once('-') else { continue };
            let Ok(stamp) = stamp.parse::<u64>() else { continue };
            if tag.is_empty() || !path.join("release.zip").is_file() { continue; }
            let Some(sha) = archive_sha(&path) else { continue };
            let v = map.entry(sha.clone()).or_insert_with(|| Version { id: sha.clone(), ..Default::default() });
            v.tag = tag.to_string();
            v.date = v.date.max(stamp);
            v.size += dir_size(&path);
            v.archives.push(path);
        }
    }
    if let Some(base) = install {
        if let Ok(items) = fs::read_dir(base) {
            for e in items.flatten() {
                let path = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if !is_sha(&name) || !path.is_dir() { continue; }
                let v = map.entry(name.clone()).or_insert_with(|| Version { id: name.clone(), ..Default::default() });
                if v.date == 0 { v.date = modified_ms(&path); }
                v.size += dir_size(&path);
                v.installed = true;
            }
        }
    }
    let mut out: Vec<Version> = map.into_values().map(|mut v| {
        if v.tag.is_empty() { v.tag = "Неизвестная версия".into(); }
        let is_running = running == Some(v.id.as_str());
        let is_current = current == Some(v.id.as_str());
        v.used = is_running || is_current;
        v.note = if is_running { "у службы".into() } else if is_current { "текущая".into() } else { String::new() };
        v
    }).collect();
    out.sort_by(|a, b| b.date.cmp(&a.date));
    out
}

fn service_sha() -> Option<String> {
    #[cfg(windows)]
    {
        let text = super::control::powershell(r#"$p=Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\ZapretManagerWinws' -ErrorAction SilentlyContinue;if($p){[string]$p.ImagePath}"#, false).ok()?;
        text.split(|c: char| c == '\\' || c == '"').map(|s| s.to_ascii_lowercase()).find(|s| is_sha(s))
    }
    #[cfg(not(windows))]
    { None }
}

fn snapshot() -> Result<Vec<Version>, String> {
    let downloads = super::settings::downloads_dir()?;
    let archive = super::settings::archive_dir()?;
    let install = super::settings::install_base().ok();
    let current = super::control::latest_archive(&downloads).ok().and_then(|(zip, _)| zip.parent().and_then(archive_sha));
    let running = service_sha();
    Ok(collect(&[downloads, archive], install.as_deref(), current.as_deref(), running.as_deref()))
}

fn remove_installed(shas: &[String]) -> Result<(), String> {
    #[cfg(windows)]
    { super::control::cleanup(shas) }
    #[cfg(not(windows))]
    { let _ = shas; Err("Доступно только в Windows".into()) }
}

#[tauri::command]
pub async fn list_versions() -> Result<Vec<Version>, String> {
    tauri::async_runtime::spawn_blocking(snapshot).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_versions(ids: Vec<String>) -> Result<Vec<Version>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let all = snapshot()?;
        let picked: Vec<&Version> = all.iter().filter(|v| ids.contains(&v.id)).collect();
        if picked.is_empty() { return Err("Выберите версии для удаления.".into()); }
        if picked.iter().any(|v| v.used) { return Err("Текущую версию удалить нельзя.".into()); }
        // Service copies first: if the admin prompt is cancelled, nothing is deleted at all.
        let shas: Vec<String> = picked.iter().filter(|v| v.installed).map(|v| v.id.clone()).collect();
        if !shas.is_empty() { remove_installed(&shas)?; }
        for v in &picked {
            for dir in &v.archives {
                fs::remove_dir_all(dir).map_err(|e| format!("Не удалось удалить {}: {e}", dir.display()))?;
            }
        }
        snapshot()
    }).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_by_hash_and_marks_used() {
        let root = std::env::temp_dir().join(format!("zm-versions-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dl = root.join("dl");
        let old = root.join("old");
        let inst = root.join("inst");
        let a = dl.join("1.10.4-2000");
        let b = old.join("1.10.3-1000");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("release.zip"), b"new").unwrap();
        fs::write(b.join("release.zip"), b"old").unwrap();
        let sha_a = format!("{:x}", Sha256::digest(b"new"));
        let sha_b = format!("{:x}", Sha256::digest(b"old"));
        fs::write(a.join("release.sha256"), &sha_a).unwrap();
        fs::create_dir_all(inst.join(&sha_b).join("x")).unwrap();
        fs::write(inst.join(&sha_b).join("x").join("f"), b"12345").unwrap();
        let orphan_id = "f".repeat(64);
        fs::create_dir_all(inst.join(&orphan_id)).unwrap();
        fs::create_dir_all(inst.join("not-a-version")).unwrap();
        let list = collect(&[dl, old], Some(inst.as_path()), Some(sha_a.as_str()), None);
        assert_eq!(list.len(), 3);
        let new = list.iter().find(|v| v.id == sha_a).unwrap();
        assert!(new.used);
        assert_eq!(new.tag, "1.10.4");
        let prev = list.iter().find(|v| v.id == sha_b).unwrap();
        assert!(!prev.used && prev.installed);
        assert_eq!(prev.archives.len(), 1);
        assert_eq!(prev.size, 3 + 5);
        let orphan = list.iter().find(|v| v.id == orphan_id).unwrap();
        assert_eq!(orphan.tag, "Неизвестная версия");
        assert!(!orphan.used);
        let _ = fs::remove_dir_all(&root);
    }
}
