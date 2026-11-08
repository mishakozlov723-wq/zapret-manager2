use serde::Serialize;
use sha2::{Digest,Sha256};
use std::{fs,io::{Read,Cursor},path::{Path,PathBuf},time::Duration};

#[derive(Serialize)]
pub struct StrategyList { version:String, strategies:Vec<String> }
fn versions_path()->Result<PathBuf,String> {super::settings::downloads_dir()}
fn latest_archive(base:&Path)->Result<(PathBuf,String),String> {
 let mut found=Vec::new();
 for entry in fs::read_dir(base).map_err(|_|"Сначала скачай свежий Zapret на главной странице".to_string())? {
  let e=entry.map_err(|e|e.to_string())?; if !e.file_type().map_err(|e|e.to_string())?.is_dir(){continue}
  let n=e.file_name().to_string_lossy().to_string();let Some((tag,time))=n.rsplit_once('-') else {continue};
  if tag.is_empty()||!tag.bytes().all(|b|b.is_ascii_alphanumeric()||b"._-".contains(&b)){continue}
  if let Ok(t)=time.parse::<u64>() {let p=e.path().join("release.zip");if p.is_file(){found.push((t,p,tag.to_string()));}}
 }
 found.sort_by_key(|x|x.0);let (_,path,tag)=found.pop().ok_or("Сначала скачай свежий Zapret на главной странице")?;Ok((path,tag))
}
fn archive_scripts(data:&[u8])->Result<(String,Vec<String>),String>{
 let mut z=zip::ZipArchive::new(Cursor::new(data)).map_err(|_|"Не удалось прочитать архив")?;
 if z.len()>5000{return Err("Слишком много файлов в архиве".into())}
 let mut top=String::new();let mut names=Vec::new();let mut total=0u64;
 for i in 0..z.len(){let f=z.by_index(i).map_err(|e|e.to_string())?;let name=f.name();
  if name.contains('\\')||name.contains(':')||name.starts_with('/')||name.split('/').filter(|x|!x.is_empty()).any(|x|x=="."||x==".."||x.ends_with('.')||x.ends_with(' '))||f.enclosed_name().is_none()||f.unix_mode().map_or(false,|m|m&0o170000==0o120000){return Err("Небезопасный архив".into())}
  total=total.checked_add(f.size()).ok_or("Архив слишком велик")?;if total>200*1024*1024{return Err("Архив слишком велик".into())}
  let mut parts=name.split('/');let first=parts.next().unwrap_or("");if first.is_empty(){return Err("Неверная структура архива".into())}if top.is_empty(){top=first.into()}else if top!=first{return Err("Неожиданная структура архива".into())}
  if let (Some(file),None)=(parts.next(),parts.next()){if file.to_lowercase().starts_with("general")&&file.to_lowercase().ends_with(".bat")&& !file.contains(['\r','\n','\"']){names.push(file.to_string());}}
 }
 names.sort();if names.is_empty(){return Err("Стратегии в архиве не найдены".into())}Ok((top,names))
}
fn load_archive()->Result<(PathBuf,String,Vec<u8>),String>{let (p,tag)=latest_archive(&versions_path()?)?;let data=fs::read(&p).map_err(|e|e.to_string())?;if data.len()>40*1024*1024{return Err("Архив слишком велик".into())}Ok((p,tag,data))}
fn strategy_arguments(data:&[u8],root:&str,name:&str,dest:&Path)->Result<String,String>{
 let mut z=zip::ZipArchive::new(Cursor::new(data)).map_err(|e|e.to_string())?;let mut f=z.by_name(&format!("{root}/{name}")).map_err(|_|"Стратегия не найдена")?;
 if f.size()>100_000{return Err("Стратегия слишком велика".into())}let mut s=String::new();f.read_to_string(&mut s).map_err(|e|e.to_string())?;
 let marker="%BIN%winws.exe\""; let pos=s.find(marker).ok_or("Неизвестный формат стратегии")?;
 let tail=&s[pos+marker.len()..];let mut lines=Vec::new();for l in tail.lines(){let t=l.trim();if t.is_empty(){break}if !t.starts_with("--")&&!lines.is_empty(){return Err("Неизвестный формат команды стратегии".into())}let continued=t.ends_with('^');lines.push(t.trim_end_matches('^').trim());if !continued{break}}
 if lines.is_empty() {return Err("Пустая стратегия".into())}
 let raw=lines.join(" ");if raw.contains(['\r','\n','&','|','<','>'])||raw.matches('"').count()%2!=0{return Err("Неподдерживаемые параметры стратегии".into())}
 let root=dest.to_string_lossy().to_string();if root.contains(['"','\r','\n','%']){return Err("Недопустимый путь установки".into())}
 let bin=format!("{}\\bin\\",root);let lists=format!("{}\\lists\\",root);
 let args=raw.replace("%BIN%",&bin).replace("%LISTS%",&lists).replace("%GameFilterTCP%","12").replace("%GameFilterUDP%","12");
 if args.contains('%'){return Err("Эта стратегия использует неподдерживаемые настройки".into())}Ok(args)
}
#[tauri::command]
pub async fn strategies()->Result<StrategyList,String>{tauri::async_runtime::spawn_blocking(||{let (_,tag,data)=load_archive()?;let (_,names)=archive_scripts(&data)?;Ok(StrategyList{version:tag,strategies:names})}).await.map_err(|e|e.to_string())?}
#[cfg(windows)]
fn official_hash(tag:&str,data:&[u8])->Result<String,String>{
 let mut url=url::Url::parse("https://api.github.com/repos/Flowseal/zapret-discord-youtube/releases/tags/").unwrap();url.path_segments_mut().map_err(|_|"URL")?.pop_if_empty().push(tag);
 let json:serde_json::Value=super::https_agent(Duration::from_secs(20))?.get(url.as_str()).set("User-Agent","Retocker-Zapret-Manager").call().map_err(|e|format!("Не удалось проверить релиз на GitHub: {e}"))?.into_json().map_err(|e|e.to_string())?;
 let asset=json["assets"].as_array().ok_or("Нет файлов релиза")?.iter().find(|a|a["name"].as_str().map_or(false,|n|n.starts_with("zapret-discord-youtube-")&&n.ends_with(".zip"))&&a["state"]=="uploaded").ok_or("Нет ZIP релиза")?;
 let hash=asset["digest"].as_str().and_then(|d|d.strip_prefix("sha256:")).ok_or("Нет SHA-256 релиза")?.to_ascii_lowercase();
 if hash.len()!=64||format!("{:x}",Sha256::digest(data))!=hash{return Err("Архив не совпадает с официальным релизом".into())}Ok(hash)
}
#[cfg(windows)]
fn powershell(script:&str,elevated:bool)->Result<String,String>{
 use base64::{engine::general_purpose::STANDARD,Engine as _};use std::{process::Command,os::windows::process::CommandExt};
 let encode=|s:&str|STANDARD.encode(s.encode_utf16().flat_map(|v|v.to_le_bytes()).collect::<Vec<u8>>());
 let mut gz=flate2::write::GzEncoder::new(Vec::new(),flate2::Compression::default());
 std::io::Write::write_all(&mut gz,script.as_bytes()).map_err(|e|e.to_string())?;
 let compressed=STANDARD.encode(gz.finish().map_err(|e|e.to_string())?);
 let unpacker=format!("$b=[Convert]::FromBase64String('{compressed}');$m=New-Object IO.MemoryStream(,$b);$g=New-Object IO.Compression.GZipStream($m,[IO.Compression.CompressionMode]::Decompress);$r=New-Object IO.StreamReader($g,[Text.Encoding]::UTF8);. ([ScriptBlock]::Create($r.ReadToEnd()))");
 let encoded=encode(&unpacker);
 let mut cmd=Command::new("powershell.exe");cmd.args(["-NoProfile","-NonInteractive"]);
 if elevated {
  let launcher=format!("try {{$p=Start-Process powershell.exe -ArgumentList '-NoProfile','-NonInteractive','-WindowStyle','Hidden','-EncodedCommand','{encoded}' -WindowStyle Hidden -Verb RunAs -Wait -PassThru; exit $p.ExitCode}} catch {{exit 90}}");
  cmd.args(["-Command",&launcher]);
 } else {cmd.args(["-EncodedCommand",&encoded]);}
 let result=cmd.creation_flags(0x08000000).output().map_err(|e|e.to_string())?;
 if result.status.success(){return Ok(String::from_utf8_lossy(&result.stdout).trim().to_string())}
 if std::env::var("ZAPRET_SERVICE_TEST").as_deref()==Ok("1") { eprintln!("PowerShell status {:?}; stderr: {}; stdout: {}",result.status.code(),String::from_utf8_lossy(&result.stderr),String::from_utf8_lossy(&result.stdout)); }
 Err(match result.status.code(){Some(10)=>"Старый Zapret ещё запущен или включён его автозапуск. Сначала отключи старую службу на главной.",Some(11)=>"Обнаружен другой работающий winws. Останови его вручную, затем повтори.",Some(12)=>"Найдена чужая служба с именем менеджера. Ничего не изменено.",Some(13)=>"Ошибка проверки официального архива или защищённой папки.",Some(14)=>"Служба не запустилась. Проверь, не мешают ли другие программы или антивирус.",Some(15)=>"Сначала выбери стратегию и подключи её.",Some(90)=>"Подтверждение администратора отменено или не получено.",_=>"Windows не смогла выполнить операцию. Проверь состояние службы."}.into())
}
#[cfg(windows)]
fn run_action(action:&str,name:Option<String>)->Result<String,String>{
 use base64::{engine::general_purpose::STANDARD,Engine as _};
 let mut payload=serde_json::json!({"action":action});
 let mut domains:Vec<String>=Vec::new();
 if action=="connect" {
  let (zip,tag,data)=load_archive()?;let hash=official_hash(&tag,&data)?;let (root,names)=archive_scripts(&data)?;let name=name.ok_or("Выбери стратегию")?;if !names.contains(&name){return Err("Стратегия не найдена".into())}
  let s=super::settings::load();domains=super::settings::clean_domains(&s.domains)?;
  let base=super::settings::install_base()?;let dest=base.join(&hash).join(&root);let args=strategy_arguments(&data,&root,&name,&dest)?;
  // Default base is handled inside the script; only a custom folder is passed (and re-validated there).
  let custom=s.install.as_ref().map(|_|base.to_string_lossy().to_string());
  payload=serde_json::json!({"action":action,"source":zip.to_string_lossy(),"sha":hash,"root":root,"strategy":name,"args":args,"base":custom,"domains":domains});
 }
 let payload=STANDARD.encode(serde_json::to_vec(&payload).map_err(|e|e.to_string())?);
 let script=include_str!("control.ps1").replace("__PAYLOAD__",&payload);
 powershell(&script,true)?;
 if action=="connect" {super::settings::mark_applied(&domains);}
 Ok("Готово".into())
}
#[tauri::command]
pub async fn strategy_action(action:String,name:Option<String>)->Result<String,String>{
 if !["connect","disconnect","auto_on","auto_off","kill_foreign"].contains(&action.as_str()){return Err("Неизвестное действие".into())}
 tauri::async_runtime::spawn_blocking(move||{
  #[cfg(windows)] {run_action(&action,name)}
  #[cfg(not(windows))] {let _=(action,name);Err("Доступно только в Windows".into())}
 }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn strategy_status()->Result<serde_json::Value,String>{tauri::async_runtime::spawn_blocking(||{
 #[cfg(windows)] {
 let script=r#"$s=Get-CimInstance Win32_Service -Filter "Name='ZapretManagerWinws'";if(!$s){'{"present":false}';exit};$p=Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Services\ZapretManagerWinws' -ErrorAction SilentlyContinue;@{present=$true;running=($s.State -eq 'Running');automatic=($p.Start -eq 2);strategy=$p.ZapretManagerStrategy;owned=($p.ZapretManagerOwner -eq 'RetockerZapretManager')}|ConvertTo-Json -Compress"#;
 let text=powershell(script,false)?;serde_json::from_str(&text).map_err(|e|format!("Не удалось прочитать состояние службы: {e}"))
 }
 #[cfg(not(windows))] {Err("Доступно только в Windows".into())}
 }).await.map_err(|e|e.to_string())?}

// DNS that returns nothing or a local/private address for a public site means the provider tampers with it;
// no winws strategy can fix that, so the UI must say so instead of letting the user cycle strategies.
fn classify_dns(result:std::io::Result<Vec<std::net::IpAddr>>)->&'static str{
 match result {
  Err(_)=>"blocked",
  Ok(v) if v.is_empty()=>"blocked",
  Ok(v) if v.iter().all(|ip|match ip{std::net::IpAddr::V4(a)=>a.is_loopback()||a.is_private()||a.is_unspecified()||a.is_link_local(),std::net::IpAddr::V6(a)=>a.is_loopback()||a.is_unspecified()})=>"spoofed",
  Ok(_)=>"ok",
 }
}
#[cfg_attr(not(windows),allow(dead_code))]
fn dns_state(host:&str)->&'static str{use std::net::ToSocketAddrs;classify_dns((host,443).to_socket_addrs().map(|a|a.map(|s|s.ip()).collect()))}

/// One read-only snapshot for the whole UI: old service, manager service, stray winws, conflicting apps,
/// downloaded version and DNS health. Nothing is changed.
#[tauri::command]
pub async fn overview()->Result<serde_json::Value,String>{tauri::async_runtime::spawn_blocking(||{
 #[cfg(windows)] {
  let text=powershell(include_str!("overview.ps1"),false)?;
  let mut value:serde_json::Value=serde_json::from_str(&text).map_err(|e|format!("Не удалось прочитать состояние: {e}"))?;
  let obj=value.as_object_mut().ok_or("Неверный ответ проверки")?;
  obj.insert("downloaded".into(),match versions_path().and_then(|p|latest_archive(&p)){Ok((_,tag))=>tag.into(),Err(_)=>serde_json::Value::Null});
  obj.insert("dns".into(),serde_json::json!({"youtube":dns_state("www.youtube.com"),"discord":dns_state("discord.com")}));
  Ok(value)
 }
 #[cfg(not(windows))] {Err("Доступно только в Windows".into())}
}).await.map_err(|e|e.to_string())?}

#[cfg(test)] mod dns_tests {
 use super::classify_dns;use std::net::IpAddr;
 fn ips(v:&[&str])->std::io::Result<Vec<IpAddr>>{Ok(v.iter().map(|s|s.parse().unwrap()).collect())}
 #[test] fn detects_tampered_dns(){
  assert_eq!(classify_dns(Err(std::io::Error::new(std::io::ErrorKind::NotFound,"nx"))),"blocked");
  assert_eq!(classify_dns(ips(&[])),"blocked");
  assert_eq!(classify_dns(ips(&["127.0.0.1"])),"spoofed");
  assert_eq!(classify_dns(ips(&["10.10.34.35","0.0.0.0"])),"spoofed");
  assert_eq!(classify_dns(ips(&["142.250.74.110"])),"ok");
  assert_eq!(classify_dns(ips(&["127.0.0.1","142.250.74.110"])),"ok");
 }
}
#[cfg(test)] mod tests {use super::*;#[test] fn parse_all_real_strategies(){let data=std::fs::read(std::env::var("ZAPRET_TEST_ZIP").unwrap()).unwrap();let (root,names)=archive_scripts(&data).unwrap();assert!(names.len()>5);for name in names {strategy_arguments(&data,&root,&name,Path::new("C:\\Program Files\\Zapret Manager\\version")).unwrap_or_else(|e|panic!("{name}: {e}"));}}}

#[cfg(all(test,windows))]
mod windows_integration {
 use super::*;
 use base64::{engine::general_purpose::STANDARD,Engine as _};
 fn invoke(payload:serde_json::Value)->Result<String,String>{let p=STANDARD.encode(serde_json::to_vec(&payload).unwrap());powershell(&include_str!("control.ps1").replace("__PAYLOAD__",&p),false)}
 #[test] fn isolated_runner_service_lifecycle(){
  if std::env::var("ZAPRET_SERVICE_TEST").as_deref()!=Ok("1") || std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true") {return}
  let exists=powershell("if (Get-Service -Name ZapretManagerWinws -ErrorAction SilentlyContinue) {exit 1} else {exit 0}",false);assert!(exists.is_ok(),"test requires no pre-existing manager service");
  struct Guard;impl Drop for Guard{fn drop(&mut self){let _=powershell("Stop-Service -Name ZapretManagerWinws -ErrorAction SilentlyContinue; sc.exe delete ZapretManagerWinws | Out-Null; exit 0",false);}}
  let _guard=Guard;
  let path=PathBuf::from(std::env::var("ZAPRET_TEST_ZIP").unwrap());let data=fs::read(&path).unwrap();let hash=format!("{:x}",Sha256::digest(&data));
  let (root,names)=archive_scripts(&data).unwrap();let name=names.iter().find(|s|s.as_str()=="general.bat").unwrap();let dest=PathBuf::from(std::env::var_os("ProgramFiles").unwrap()).join("Zapret Manager").join("versions").join(&hash).join(&root);let args=strategy_arguments(&data,&root,name,&dest).unwrap();
  invoke(serde_json::json!({"action":"connect","source":path,"sha":hash,"root":root,"strategy":name,"args":args})).expect("real service connect");
  let status=powershell("$s=Get-CimInstance Win32_Service -Filter \"Name='ZapretManagerWinws'\"; $s | Select-Object State,StartMode,ExitCode,ServiceSpecificExitCode,ProcessId,PathName | ConvertTo-Json -Compress",false).unwrap(); eprintln!("Service details: {}",status);
  powershell("$s=Get-CimInstance Win32_Service -Filter \"Name='ZapretManagerWinws'\"; $p=Get-ItemProperty 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\ZapretManagerWinws'; if ($s.State -ne 'Running' -or $p.Start -ne 3) { exit 1 }",false).expect("running manual service");
  invoke(serde_json::json!({"action":"auto_on"})).expect("enable autostart");
  powershell("$s=Get-CimInstance Win32_Service -Filter \"Name='ZapretManagerWinws'\"; $p=Get-ItemProperty 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\ZapretManagerWinws'; if ($p.Start -ne 2) {exit 1}",false).expect("autostart is automatic");
  let alt=names.iter().find(|s|s.as_str()=="general (ALT).bat").unwrap();let alt_args=strategy_arguments(&data,&root,alt,&dest).unwrap();
  invoke(serde_json::json!({"action":"connect","source":path,"sha":hash,"root":root,"strategy":alt,"args":alt_args})).expect("switch strategy");
  powershell("$p=Get-ItemProperty 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\ZapretManagerWinws'; if ($p.Start -ne 2 -or $p.ZapretManagerStrategy -ne 'general (ALT).bat') {exit 1}",false).expect("switch preserved autostart");
  invoke(serde_json::json!({"action":"auto_off"})).expect("disable autostart");
  invoke(serde_json::json!({"action":"disconnect"})).expect("disconnect");
  powershell("$s=Get-CimInstance Win32_Service -Filter \"Name='ZapretManagerWinws'\"; $p=Get-ItemProperty 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\ZapretManagerWinws'; if ($s.State -ne 'Stopped' -or $p.Start -ne 3) {exit 1}",false).expect("stopped and manual");
 }
}
