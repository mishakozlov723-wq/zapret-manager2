#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod install;
mod net;
mod update;
mod control;
mod settings;
use serde::Serialize;
use std::time::Duration;
use std::sync::Arc;

#[derive(Serialize)]
struct Detection { service: String, process: String, detail: String }
#[derive(Serialize)]
struct Release { tag: String, url: String }
#[derive(Serialize)]
struct Check { status: String, detail: String }

// Use Windows' native certificate store, and always verify server certificates.
fn https_agent(timeout: Duration) -> Result<ureq::Agent, String> {
    let connector = native_tls::TlsConnector::new().map_err(|e| format!("Не удалось настроить HTTPS: {e}"))?;
    Ok(ureq::AgentBuilder::new().tls_connector(Arc::new(connector)).timeout(timeout).build())
}
// For large downloads: no overall deadline, but give up when the connection stalls.
fn stream_agent() -> Result<ureq::Agent, String> {
    let connector = native_tls::TlsConnector::new().map_err(|e| format!("Не удалось настроить HTTPS: {e}"))?;
    Ok(ureq::AgentBuilder::new().tls_connector(Arc::new(connector)).timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(30)).build())
}

// Detection is deliberately read-only: no process is stopped, no service is changed.
#[tauri::command]
async fn detect() -> Result<Detection, String> {
    tauri::async_runtime::spawn_blocking(|| {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            use std::process::Command;
            const NO_WINDOW: u32 = 0x08000000;
            let s = Command::new("sc.exe").args(["query", "zapret"]).creation_flags(NO_WINDOW).output().map_err(|e| e.to_string())?;
            let text = String::from_utf8_lossy(&s.stdout);
            let service = if !s.status.success() && text.contains("1060") { "Не найдена" } else if !s.status.success() { return Err("Не удалось проверить службу zapret".into()); } else if text.contains("RUNNING") { "Запущена" } else if text.contains("STOPPED") { "Остановлена" } else { "Обнаружена" };
            let p = Command::new("tasklist.exe").args(["/FI", "IMAGENAME eq winws.exe", "/FO", "CSV", "/NH"]).creation_flags(NO_WINDOW).output().map_err(|e| e.to_string())?;
            if !p.status.success() { return Err("Не удалось проверить процессы Windows".into()); }
            let process = if String::from_utf8_lossy(&p.stdout).to_lowercase().contains("winws.exe") { "Найден" } else { "Не найден" };
            Ok(Detection {service: service.into(), process: process.into(), detail: "Проверены служба zapret и процесс winws.exe. Другие сборки могут не обнаруживаться.".into()})
        }
        #[cfg(not(target_os = "windows"))]
        { Err("Проверка доступна только в Windows".into()) }
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn latest_release() -> Result<Release, net::NetError> {
    tauri::async_runtime::spawn_blocking(|| -> Result<Release, net::NetError> {
        let agent = https_agent(Duration::from_secs(15))?;
        let tag = match net::get_json(&agent, "https://api.github.com/repos/Flowseal/zapret-discord-youtube/releases/latest", "проверка версии Zapret") {
            Ok(json) => json["tag_name"].as_str().ok_or("Нет номера версии")?.to_string(),
            // API limit hit or API down: read the tag from the github.com redirect instead.
            Err(e) if e.kind != "fatal" => net::latest_tag_web(&agent, "Flowseal/zapret-discord-youtube", "проверка версии Zapret").map_err(|w| if w.kind == "fatal" { e } else { w })?,
            Err(e) => return Err(e),
        };
        Ok(Release { tag, url: "https://github.com/Flowseal/zapret-discord-youtube/releases/latest".into() })
    }).await.map_err(|e| net::NetError::fatal(e.to_string()))?
}

fn checked_domain(input: &str) -> Result<String, String> {
    let url = url::Url::parse(&format!("https://{input}/")).map_err(|_| "Некорректный домен".to_string())?;
    let host = url.host_str().ok_or("Нет домена")?;
    if input.is_empty() || input.len() > 253 || input != host || !host.contains('.') || host.ends_with('.') || url.port().is_some() || !url.username().is_empty() || url.password().is_some() || url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err("Укажи только домен сайта".into());
    }
    Ok(format!("https://{host}/"))
}
fn request_site(target: &str) -> Check {
    let agent = match https_agent(Duration::from_secs(10)) { Ok(agent) => agent, Err(err) => return Check { status: "Ошибка HTTPS".into(), detail: err } };
    let (status, detail) = match agent.get(target).set("User-Agent", "Mozilla/5.0").call() {
        Ok(r) => ("Ответ получен".to_string(), format!("HTTP {}", r.status())),
        Err(ureq::Error::Status(code, _)) => ("Сайт ответил с ошибкой".to_string(), format!("HTTP {code}")),
        Err(ureq::Error::Transport(t)) if t.kind() == ureq::ErrorKind::Dns => ("Нет ответа".to_string(), "DNS не нашёл адрес сайта".to_string()),
        Err(_) => ("Нет ответа".to_string(), "Соединение не удалось".to_string()),
    };
    Check {status, detail}
}
#[tauri::command]
async fn check_site(domain: String) -> Result<Check, String> {
    let target = checked_domain(&domain)?;
    tauri::async_runtime::spawn_blocking(move || request_site(&target)).await.map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    #[test] fn validate_domains() {
        assert_eq!(checked_domain("twitch.tv").unwrap(), "https://twitch.tv/");
        for value in ["", "localhost", "example.com/path", "a@b.com", "example.com:443", "evil.com?x", "site.com#x"] { assert!(checked_domain(value).is_err(), "{value}"); }
    }
    fn mock_status(code: u16) -> Check {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let thread = std::thread::spawn(move || {let (mut socket, _) = listener.accept().unwrap();let mut buf=[0;2048];let _ = socket.read(&mut buf);write!(socket, "HTTP/1.1 {code} test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();});
        let check = request_site(&format!("http://{addr}/"));thread.join().unwrap();check
    }
    #[test] fn reports_http_results() { assert_eq!(mock_status(200).status, "Ответ получен");assert_eq!(mock_status(503).status, "Сайт ответил с ошибкой"); }
    // Exercise real HTTPS with the same production agent. The existing mock test only covered HTTP.
    #[test] fn real_https_backend_is_usable() {
        let result = request_site("https://api.github.com/");
        assert_ne!(result.status, "Нет ответа", "HTTPS connection failed: {}", result.detail);
        assert_ne!(result.status, "Ошибка HTTPS", "TLS initialization failed: {}", result.detail);
    }
}
mod window_chrome;
fn main() { tauri::Builder::default().setup(|app| window_chrome::setup(app)).invoke_handler(tauri::generate_handler![detect, latest_release, check_site, install::install_release, install::disable_old_service, install::restore_old_service, control::strategies, control::strategy_action, control::strategy_status, control::overview, settings::get_settings, settings::set_folder, settings::browse, settings::make_folder, settings::save_domains, update::app_version, update::check_app_update, update::download_app_update, update::app_update_progress, update::launch_app_update]).run(tauri::generate_context!()).expect("failed to run app"); }
