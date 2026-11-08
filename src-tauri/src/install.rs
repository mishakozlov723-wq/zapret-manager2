use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs::{self, File}, io::{self, Read, Write, Cursor}, path::{Path, PathBuf}, time::{Duration, SystemTime, UNIX_EPOCH}};

const MAX_ZIP: u64 = 40 * 1024 * 1024;
const MAX_UNPACKED: u64 = 200 * 1024 * 1024;
#[derive(Serialize)]
pub struct Installed { pub tag: String, pub folder: String, pub reused: bool }

fn safe_tag(tag: &str) -> bool {
    !tag.is_empty() && tag.len() <= 64 && tag.bytes().all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)) && tag != "." && tag != ".."
}
fn valid_asset_url(value: &str, tag: &str) -> bool {
    if !safe_tag(tag) { return false; }
    let Ok(u) = url::Url::parse(value) else { return false };
    let prefix = format!("/Flowseal/zapret-discord-youtube/releases/download/{tag}/");
    u.scheme() == "https" && u.host_str() == Some("github.com") && u.port().is_none() && u.username().is_empty() && u.password().is_none() && u.query().is_none() && u.fragment().is_none() && u.path().starts_with(&prefix) && u.path().ends_with(".zip") && !u.path()[prefix.len()..].contains('/')
}
fn safe_zip_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('\\') && !name.contains(':') && !name.starts_with('/') && name.split('/').filter(|p| !p.is_empty()).all(|p| p != "." && p != ".." && !p.ends_with(' ') && !p.ends_with('.'))
}
fn unpack(data: &[u8], target: &Path) -> Result<PathBuf, String> {
    let mut z = zip::ZipArchive::new(Cursor::new(data)).map_err(|e| format!("Неверный ZIP: {e}"))?;
    if z.len() > 5000 { return Err("Слишком много файлов в архиве".into()); }
    let mut total = 0u64;
    for i in 0..z.len() {
        let f = z.by_index(i).map_err(|e| e.to_string())?;
        if !safe_zip_name(f.name()) || f.enclosed_name().is_none() || f.unix_mode().map_or(false, |m| (m & 0o170000) == 0o120000) { return Err("Архив содержит небезопасный путь или ссылку".into()); }
        total = total.checked_add(f.size()).ok_or("Слишком большой архив")?;
        if total > MAX_UNPACKED { return Err("Архив слишком велик".into()); }
    }
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        let expected_size = f.size();
        let dest = target.join(f.enclosed_name().ok_or("Небезопасный путь")?);
        if f.is_dir() { fs::create_dir_all(&dest).map_err(|e| e.to_string())?; continue; }
        if let Some(p) = dest.parent() { fs::create_dir_all(p).map_err(|e| e.to_string())?; }
        let mut out = fs::OpenOptions::new().write(true).create_new(true).open(dest).map_err(|e| format!("Не удалось распаковать: {e}"))?;
        let written=io::copy(&mut (&mut f).take(expected_size + 1), &mut out).map_err(|e| format!("Ошибка распаковки: {e}"))?;
        if written != expected_size { return Err("Размер файла в архиве не совпадает".into()); }
    }
    let entries: Vec<_> = fs::read_dir(target).map_err(|e| e.to_string())?.collect::<Result<_,_>>().map_err(|e| e.to_string())?;
    let root = if target.join("service.bat").is_file() { target.to_owned() } else if entries.len() == 1 && entries[0].path().is_dir() { entries[0].path() } else { return Err("Неожиданная структура архива".into()); };
    if !root.join("service.bat").is_file() || !root.join("bin").join("winws.exe").is_file() || !root.join("lists").is_dir() { return Err("В архиве отсутствуют файлы Zapret".into()); }
    Ok(root)
}
// A verified copy of this exact release already on disk: reuse it instead of piling up duplicate folders.
fn existing_release(base: &Path, tag: &str, digest: &str) -> Option<Installed> {
    let prefix = format!("{tag}-");
    for entry in fs::read_dir(base).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(&prefix) || name[prefix.len()..].parse::<u64>().is_err() { continue; }
        let Ok(data) = fs::read(entry.path().join("release.zip")) else { continue };
        if format!("{:x}", Sha256::digest(&data)) != digest { continue; }
        let files = entry.path().join("files");
        let Ok(items) = fs::read_dir(&files) else { continue };
        let dirs: Vec<_> = items.flatten().filter(|x| x.path().is_dir()).collect();
        let root = if files.join("service.bat").is_file() { files.clone() } else if dirs.len() == 1 { dirs[0].path() } else { continue };
        if root.join("service.bat").is_file() && root.join("bin").join("winws.exe").is_file() {
            return Some(Installed { tag: tag.to_owned(), folder: root.display().to_string(), reused: true });
        }
    }
    None
}
fn download_install(base: &Path) -> Result<Installed, String> {
    let agent = super::https_agent(Duration::from_secs(90))?;
    let json: serde_json::Value = agent.get("https://api.github.com/repos/Flowseal/zapret-discord-youtube/releases/latest").set("User-Agent", "Retocker-Zapret-Manager").call().map_err(|e| format!("Не удалось получить релиз: {e}"))?.into_json().map_err(|e| e.to_string())?;
    let tag = json["tag_name"].as_str().ok_or("Нет версии релиза")?;
    if !safe_tag(tag) { return Err("Некорректный номер версии".into()); }
    let asset = json["assets"].as_array().ok_or("В релизе нет файлов")?.iter().find(|a| a["name"].as_str().map_or(false, |n| n.starts_with("zapret-discord-youtube-") && n.ends_with(".zip")) && a["state"].as_str() == Some("uploaded")).ok_or("В релизе нет официального ZIP")?;
    let href = asset["browser_download_url"].as_str().ok_or("Нет ссылки на архив")?;
    if !valid_asset_url(href, tag) || href.rsplit('/').next() != asset["name"].as_str() { return Err("Неожиданный адрес архива".into()); }
    let digest = asset["digest"].as_str().and_then(|s| s.strip_prefix("sha256:")).ok_or("В релизе нет контрольной суммы SHA-256")?;
    if digest.len()!=64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("Неверная контрольная сумма".into()); }
    let size=asset["size"].as_u64().ok_or("Неизвестный размер архива")?;
    if size == 0 || size > MAX_ZIP { return Err("Недопустимый размер архива".into()); }
    if let Some(found) = existing_release(base, tag, &digest.to_ascii_lowercase()) { return Ok(found); }
    let response = agent.get(href).set("User-Agent", "Retocker-Zapret-Manager").call().map_err(|e| format!("Не удалось скачать архив: {e}"))?;
    let mut bytes=Vec::new(); response.into_reader().take(MAX_ZIP+1).read_to_end(&mut bytes).map_err(|e| format!("Ошибка загрузки: {e}"))?;
    if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(&bytes)) != digest.to_ascii_lowercase() { return Err("Архив не прошёл проверку размера или SHA-256".into()); }
    fs::create_dir_all(base).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_millis();
    let dir=base.join(format!("{tag}-{stamp}")); fs::create_dir(&dir).map_err(|e|format!("Не удалось создать папку: {e}"))?;
    let zip_path=dir.join("release.zip"); let mut file=File::create(zip_path).map_err(|e|e.to_string())?; file.write_all(&bytes).map_err(|e|e.to_string())?;
    let content=dir.join("files");fs::create_dir(&content).map_err(|e|e.to_string())?;
    let root=unpack(&bytes, &content)?;
    Ok(Installed { tag:tag.to_owned(), folder:root.display().to_string(), reused:false })
}
#[tauri::command]
pub async fn install_release() -> Result<Installed, String> {
    tauri::async_runtime::spawn_blocking(|| {
        #[cfg(target_os="windows")]
        {
            let base = super::settings::downloads_dir()?;
            let got = download_install(&base)?;
            // Older downloads go to the archive folder; the fresh one stays where strategies are read from.
            let keep = Path::new(&got.folder).ancestors().find(|a| a.parent() == Some(base.as_path())).and_then(|a| a.file_name()).map(|n| n.to_string_lossy().to_string());
            if keep.is_some() { super::settings::move_versions(&base, &super::settings::archive_dir()?, keep.as_deref())?; }
            Ok(got)
        }
        #[cfg(not(target_os="windows"))]
        { Err("Установка доступна только в Windows".into()) }
    }).await.map_err(|e| e.to_string())?
}

// Fixed script, no user-supplied commands or paths. Refuse an unrecognised service.
#[cfg(target_os="windows")]
fn service_action(restore: bool) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const SCRIPT: &str = r#"$ErrorActionPreference='Stop'
try {
 $key='HKLM:\SYSTEM\CurrentControlSet\Services\zapret'
 if (-not (Test-Path $key)) { exit 10 }
 $props=Get-ItemProperty -LiteralPath $key
 if (-not $props.'zapret-discord-youtube' -or $props.ImagePath -notmatch '(?i)winws\.exe') { exit 11 }
 if ('__ACTION__' -eq 'stop') {
   if (-not (Get-ItemProperty -LiteralPath $key -Name 'ZapretManagerPreviousStart' -ErrorAction SilentlyContinue)) {
     New-ItemProperty -LiteralPath $key -Name 'ZapretManagerPreviousStart' -PropertyType DWord -Value $props.Start -Force | Out-Null
   }
   $svc=Get-Service -Name 'zapret'
   if ($svc.Status -ne 'Stopped') { Stop-Service -Name 'zapret' -ErrorAction Stop; $svc.WaitForStatus('Stopped', [TimeSpan]::FromSeconds(20)) }
   Set-Service -Name 'zapret' -StartupType Disabled
 } else {
   $saved=(Get-ItemProperty -LiteralPath $key -Name 'ZapretManagerPreviousStart').ZapretManagerPreviousStart
   if ($saved -eq 2) { Set-Service -Name 'zapret' -StartupType Automatic }
   elseif ($saved -eq 3) { Set-Service -Name 'zapret' -StartupType Manual }
   elseif ($saved -eq 4) { Set-Service -Name 'zapret' -StartupType Disabled }
   else { exit 12 }
   Remove-ItemProperty -LiteralPath $key -Name 'ZapretManagerPreviousStart'
 }
 exit 0
} catch { exit 1 }"#;
    let code=SCRIPT.replace("__ACTION__", if restore {"restore"} else {"stop"});
    let utf16: Vec<u8> = code.encode_utf16().flat_map(|v| v.to_le_bytes()).collect();
    let encoded=STANDARD.encode(utf16);
    let launcher=format!("try {{ $p=Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{encoded}' -Verb RunAs -Wait -PassThru; exit $p.ExitCode }} catch {{ exit 13 }}");
    let result=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command", &launcher]).creation_flags(0x08000000).status().map_err(|e|format!("Не удалось запросить права: {e}"))?;
    match result.code() { Some(0)=>Ok(if restore {"Тип запуска восстановлен. Служба не запущена автоматически."} else {"Служба остановлена, автозапуск отключён. Её файлы сохранены."}.into()), Some(10)=>Err("Служба zapret не найдена; ничего не изменено.".into()), Some(11)=>Err("Службу не удалось однозначно определить как Flowseal Zapret; ничего не изменено.".into()), Some(12)=>Err("Нет сохранённого типа запуска службы.".into()), Some(13)=>Err("Права администратора не получены или запрос отменён.".into()), _=>Err("Не удалось завершить операцию. Проверь состояние службы: она могла остановиться, но автозапуск остаться включённым.".into()) }
}
#[tauri::command]
pub async fn disable_old_service() -> Result<String,String> {
    tauri::async_runtime::spawn_blocking(|| {
        #[cfg(target_os="windows")] {service_action(false)}
        #[cfg(not(target_os="windows"))] {Err("Доступно только в Windows".into())}
    }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn restore_old_service() -> Result<String,String> {
    tauri::async_runtime::spawn_blocking(|| {
        #[cfg(target_os="windows")] {service_action(true)}
        #[cfg(not(target_os="windows"))] {Err("Доступно только в Windows".into())}
    }).await.map_err(|e|e.to_string())?
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn download_and_unpack_official_release() {
  let path=std::env::temp_dir().join(format!("zapret-manager-test-{}", std::process::id()));
  let result=download_install(&path).expect("official download, sha256 check, extraction");
  assert!(Path::new(&result.folder).join("service.bat").is_file());
  assert!(Path::new(&result.folder).join("bin/winws.exe").is_file());
  assert!(!result.reused);
  // Second call must reuse the verified copy, not create another folder.
  let again=download_install(&path).expect("second call");
  assert!(again.reused);
  assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
  std::fs::remove_dir_all(path).unwrap();
 }
 #[test] fn rejects_paths_and_urls() {
  for n in ["../bad", "/absolute", "folder/../bad", "C:/bad", "a\\b", "foo./evil"] {assert!(!safe_zip_name(n),"{n}");}
  assert!(safe_zip_name("release/bin/winws.exe"));
  assert!(valid_asset_url("https://github.com/Flowseal/zapret-discord-youtube/releases/download/1.10.4/zapret.zip", "1.10.4"));
  assert!(!valid_asset_url("https://evil.com/Flowseal/zapret-discord-youtube/releases/download/1.10.4/x.zip", "1.10.4"));
  assert!(!valid_asset_url("https://github.com/Flowseal/zapret-discord-youtube/releases/download/1.10.4/x.zip?x=1", "1.10.4"));
 }
}
