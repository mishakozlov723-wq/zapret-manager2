// Shared GitHub HTTP helpers: quiet retries for short hiccups and errors the UI can turn into a countdown.
use serde::Serialize;
use std::{io::Read, thread, time::{Duration, SystemTime, UNIX_EPOCH}};

pub const UA: &str = "Retocker-Zapret-Manager";
/// How long the UI asks the user to wait after a network hiccup.
pub const RETRY_WAIT: u64 = 60;
const ATTEMPTS: u32 = 3;

/// kind: "retry" — GitHub or the network briefly failed (504, timeout, reset): show a countdown;
/// "rate_limit" — GitHub request limit, `wait` is the real time until it resets;
/// "fatal" — a real problem (bad checksum, missing file): plain error, no timer.
#[derive(Serialize, Debug, Clone)]
pub struct NetError { pub kind: &'static str, pub wait: u64, pub message: String }

impl NetError {
    pub fn fatal(message: impl Into<String>) -> Self { Self { kind: "fatal", wait: 0, message: message.into() } }
    fn retry(message: String) -> Self { Self { kind: "retry", wait: RETRY_WAIT, message } }
}
impl From<String> for NetError { fn from(m: String) -> Self { Self::fatal(m) } }
impl From<&str> for NetError { fn from(m: &str) -> Self { Self::fatal(m) } }

fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

/// Seconds to wait when GitHub says "too many requests"; None when the status is not a rate limit.
pub fn rate_limit_wait(code: u16, remaining: Option<&str>, reset: Option<&str>, retry_after: Option<&str>, now: u64) -> Option<u64> {
    let limited = code == 429 || (code == 403 && (remaining.map(str::trim) == Some("0") || retry_after.is_some()));
    if !limited { return None; }
    let wait = retry_after.and_then(|v| v.trim().parse::<u64>().ok())
        .or_else(|| reset.and_then(|v| v.trim().parse::<u64>().ok()).map(|r| r.saturating_sub(now) + 2))
        .unwrap_or(RETRY_WAIT);
    // Never more than a minute: after that we try again, and the install also has a route around the API limit.
    Some(wait.clamp(5, RETRY_WAIT))
}

pub fn describe(what: &str, err: ureq::Error) -> NetError {
    match err {
        ureq::Error::Status(code, resp) => {
            if let Some(wait) = rate_limit_wait(code, resp.header("x-ratelimit-remaining"), resp.header("x-ratelimit-reset"), resp.header("retry-after"), now()) {
                return NetError { kind: "rate_limit", wait, message: format!("GitHub временно ограничил число запросов с вашего адреса ({what}). Ограничение снимется само.") };
            }
            if matches!(code, 500 | 502 | 503 | 504) { return NetError::retry(format!("GitHub не ответил вовремя ({what}, ошибка {code}). Обычно это проходит за минуту.")); }
            if code == 404 { return NetError::fatal(format!("На GitHub не найдено: {what}.")); }
            NetError::fatal(format!("GitHub ответил ошибкой {code} ({what})."))
        }
        ureq::Error::Transport(t) => NetError::retry(format!("Нет связи с GitHub ({what}: {}). Обычно это проходит за минуту.", t.kind())),
    }
}

/// Runs `op` up to 3 times while it fails with a short-lived ("retry") error.
pub fn retry_loop<T>(delay: Duration, mut op: impl FnMut() -> Result<T, NetError>) -> Result<T, NetError> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match op() {
            Err(e) if e.kind == "retry" && attempt < ATTEMPTS => thread::sleep(delay * attempt),
            other => return other,
        }
    }
}

pub fn get_json(agent: &ureq::Agent, url: &str, what: &str) -> Result<serde_json::Value, NetError> {
    retry_loop(Duration::from_secs(2), || {
        let resp = agent.get(url).set("User-Agent", UA).set("Accept", "application/vnd.github+json").call().map_err(|e| describe(what, e))?;
        resp.into_json().map_err(|e| NetError::retry(format!("Ответ GitHub оборвался ({what}): {e}")))
    })
}

/// Downloads at most `limit` bytes; `progress` gets the byte count so far.
pub fn get_bytes(agent: &ureq::Agent, url: &str, what: &str, limit: u64, progress: &dyn Fn(u64)) -> Result<Vec<u8>, NetError> {
    retry_loop(Duration::from_secs(2), || {
        let resp = agent.get(url).set("User-Agent", UA).call().map_err(|e| describe(what, e))?;
        let mut reader = resp.into_reader().take(limit + 1);
        let mut out = Vec::new();
        let mut buf = vec![0u8; 64 * 1024];
        progress(0);
        loop {
            let n = reader.read(&mut buf).map_err(|e| NetError::retry(format!("Загрузка оборвалась ({what}): {e}")))?;
            if n == 0 { break; }
            out.extend_from_slice(&buf[..n]);
            progress(out.len() as u64);
        }
        Ok(out)
    })
}

/// Latest release tag via the github.com redirect. It is not the API, so the 60-requests-per-hour limit does not apply.
pub fn latest_tag_web(agent: &ureq::Agent, repo: &str, what: &str) -> Result<String, NetError> {
    let url = format!("https://github.com/{repo}/releases/latest");
    retry_loop(Duration::from_secs(2), || {
        let resp = agent.get(&url).set("User-Agent", UA).call().map_err(|e| describe(what, e))?;
        tag_from_url(resp.get_url(), repo).ok_or_else(|| NetError::fatal(format!("GitHub не показал последний релиз ({what}).")))
    })
}

pub fn tag_from_url(url: &str, repo: &str) -> Option<String> {
    let rest = url.strip_prefix(&format!("https://github.com/{repo}/releases/tag/"))?;
    let tag = rest.split(|c| c == '?' || c == '#').next().unwrap_or("");
    let ok = !tag.is_empty() && tag.len() <= 64 && tag.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) && tag != "." && tag != "..";
    if ok { Some(tag.to_string()) } else { None }
}

/// SHA-256 of a release file from the release's file list page (also not the API).
pub fn asset_digest_web(agent: &ureq::Agent, repo: &str, tag: &str, asset: &str, what: &str) -> Result<String, NetError> {
    let url = format!("https://github.com/{repo}/releases/expanded_assets/{tag}");
    let html = retry_loop(Duration::from_secs(2), || {
        let resp = agent.get(&url).set("User-Agent", UA).call().map_err(|e| describe(what, e))?;
        resp.into_string().map_err(|e| NetError::retry(format!("Ответ GitHub оборвался ({what}): {e}")))
    })?;
    digest_from_html(&html, asset).ok_or_else(|| NetError::fatal(format!("GitHub не показал контрольную сумму ({what}).")))
}

pub fn digest_from_html(html: &str, asset: &str) -> Option<String> {
    let start = html.find(&format!("/{asset}\""))?;
    let rest = &html[start..];
    let block = &rest[..rest.find("</li>").unwrap_or(rest.len())];
    block.match_indices("sha256:").find_map(|(i, _)| {
        let hex = block.get(i + 7..i + 71)?;
        if hex.bytes().all(|b| b.is_ascii_hexdigit()) { Some(hex.to_ascii_lowercase()) } else { None }
    })
}

#[cfg(test)]
mod tests {
    #[test] fn reads_tag_and_digest_from_github_pages() {
        assert_eq!(tag_from_url("https://github.com/a/b/releases/tag/1.10.4", "a/b").as_deref(), Some("1.10.4"));
        assert_eq!(tag_from_url("https://github.com/a/b/releases", "a/b"), None);
        assert_eq!(tag_from_url("https://github.com/a/b/releases/tag/x/y", "a/b"), None);
        let h = format!("<li><a href=\"/a/b/releases/download/1/f.tar.gz\">f.tar.gz</a> sha256:{}</li><li><a href=\"/a/b/releases/download/1/f.zip\">f.zip</a> <span>sha256:{}</span></li>", "1".repeat(64), "AB".repeat(32));
        assert_eq!(digest_from_html(&h, "f.zip"), Some("ab".repeat(32)));
        assert_eq!(digest_from_html(&h, "g.zip"), None);
    }

    use super::*;
    use std::{cell::Cell, io::Write, net::TcpListener};

    #[test] fn rate_limit_uses_reset_time() {
        assert_eq!(rate_limit_wait(403, Some("0"), Some("1300"), None, 1000), Some(60));
        assert_eq!(rate_limit_wait(429, None, None, Some("30"), 0), Some(30));
        assert_eq!(rate_limit_wait(403, Some("12"), Some("1300"), None, 1000), None);
        assert_eq!(rate_limit_wait(404, Some("0"), None, None, 0), None);
        assert_eq!(rate_limit_wait(403, Some("0"), Some("999999"), None, 0), Some(60));
    }

    #[test] fn retries_only_short_failures() {
        let calls = Cell::new(0);
        let r: Result<(), _> = retry_loop(Duration::ZERO, || { calls.set(calls.get() + 1); Err(NetError::retry("x".into())) });
        assert_eq!(calls.get(), 3);
        assert_eq!(r.unwrap_err().kind, "retry");
        let calls = Cell::new(0);
        let _: Result<(), _> = retry_loop(Duration::ZERO, || { calls.set(calls.get() + 1); Err(NetError::fatal("x")) });
        assert_eq!(calls.get(), 1);
        let calls = Cell::new(0);
        let ok = retry_loop(Duration::ZERO, || { calls.set(calls.get() + 1); if calls.get() < 2 { Err(NetError::retry("x".into())) } else { Ok(7) } });
        assert_eq!(ok.unwrap(), 7);
    }

    fn mock(head: &'static str) -> NetError {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || { let (mut s, _) = listener.accept().unwrap(); let mut b = [0; 1024]; let _ = s.read(&mut b); let _ = s.write_all(head.as_bytes()); });
        describe("тест", ureq::get(&format!("http://{addr}/")).call().unwrap_err())
    }

    #[test] fn classifies_github_errors() {
        assert_eq!(mock("HTTP/1.1 504 Gateway Timeout\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").kind, "retry");
        let limited = mock("HTTP/1.1 403 Forbidden\r\nx-ratelimit-remaining: 0\r\nretry-after: 40\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        assert_eq!((limited.kind, limited.wait), ("rate_limit", 40));
        assert_eq!(mock("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").kind, "fatal");
    }
}
