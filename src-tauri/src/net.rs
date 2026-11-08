// Shared GitHub HTTP helpers: quiet retries for flaky networks, and a clear "wait N seconds" for limits.
use serde::Serialize;
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// kind: "retry" (network/GitHub hiccup, try again after `wait`), "rate_limit" (GitHub limit, wait until reset), "fatal".
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct NetError { pub kind: String, pub wait: u64, pub message: String }

impl NetError {
    pub fn fatal(message: impl Into<String>) -> Self { NetError { kind: "fatal".into(), wait: 0, message: message.into() } }
    pub fn retry(wait: u64, message: impl Into<String>) -> Self { NetError { kind: "retry".into(), wait, message: message.into() } }
    pub fn rate_limit(wait: u64) -> Self { NetError { kind: "rate_limit".into(), wait, message: "GitHub временно ограничил запросы".into() } }
}
impl From<String> for NetError { fn from(s: String) -> Self { NetError::fatal(s) } }
impl From<&str> for NetError { fn from(s: &str) -> Self { NetError::fatal(s) } }
impl std::fmt::Display for NetError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) } }

const UA: &str = "Retocker-Zapret-Manager";
const ATTEMPTS: u32 = 3;
pub const COOLDOWN: u64 = 60;

fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

// Turn a failed request into a decision: retry quietly, wait for the limit, or stop.
pub fn classify(err: ureq::Error) -> NetError {
    match err {
        ureq::Error::Status(code, resp) => {
            let remaining = resp.header("x-ratelimit-remaining").map(|v| v.trim().to_owned());
            let reset = resp.header("x-ratelimit-reset").and_then(|v| v.trim().parse::<u64>().ok());
            let after = resp.header("retry-after").and_then(|v| v.trim().parse::<u64>().ok());
            classify_status(code, remaining.as_deref(), reset, after, now())
        }
        ureq::Error::Transport(t) => NetError::retry(COOLDOWN, format!("Нет связи с GitHub: {t}")),
    }
}
pub fn classify_status(code: u16, remaining: Option<&str>, reset: Option<u64>, after: Option<u64>, now: u64) -> NetError {
    if code == 429 || (code == 403 && (remaining == Some("0") || after.is_some())) {
        let wait = after.or_else(|| reset.map(|r| r.saturating_sub(now) + 1)).unwrap_or(COOLDOWN).clamp(5, 3600);
        return NetError::rate_limit(wait);
    }
    if code >= 500 || code == 408 { return NetError::retry(COOLDOWN, format!("GitHub не отвечает (HTTP {code})")); }
    NetError::fatal(format!("GitHub вернул ошибку HTTP {code}"))
}

// A few quiet attempts (pause 2 s, then 4 s) before the user sees anything.
pub fn retry_loop<T>(mut f: impl FnMut() -> Result<T, NetError>) -> Result<T, NetError> {
    let mut last = NetError::fatal("Неизвестная ошибка сети");
    for i in 0..ATTEMPTS {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if e.kind == "retry" => {
                last = e;
                if i + 1 < ATTEMPTS { std::thread::sleep(Duration::from_secs(2u64 << i)); }
            }
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

pub fn get_json(agent: &ureq::Agent, url: &str) -> Result<serde_json::Value, NetError> {
    retry_loop(|| {
        let r = agent.get(url).set("User-Agent", UA).set("Accept", "application/vnd.github+json").call().map_err(classify)?;
        r.into_json::<serde_json::Value>().map_err(|e| NetError::retry(COOLDOWN, format!("Ответ GitHub оборвался: {e}")))
    })
}

pub fn get_bytes(agent: &ureq::Agent, url: &str, max: u64) -> Result<Vec<u8>, NetError> {
    get_bytes_progress(agent, url, max, &|_| {})
}

// Same as get_bytes, but reports how many bytes have arrived so far.
pub fn get_bytes_progress(agent: &ureq::Agent, url: &str, max: u64, progress: &dyn Fn(u64)) -> Result<Vec<u8>, NetError> {
    retry_loop(|| {
        let r = agent.get(url).set("User-Agent", UA).call().map_err(classify)?;
        let mut reader = r.into_reader().take(max + 1);
        let mut bytes = Vec::new();
        let mut buf = vec![0u8; 64 * 1024];
        progress(0);
        loop {
            let n = reader.read(&mut buf).map_err(|e| NetError::retry(COOLDOWN, format!("Загрузка оборвалась: {e}")))?;
            if n == 0 { break; }
            bytes.extend_from_slice(&buf[..n]);
            progress(bytes.len() as u64);
        }
        if bytes.len() as u64 > max { return Err(NetError::fatal("Файл слишком большой")); }
        Ok(bytes)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn classifies_statuses() {
        assert_eq!(classify_status(504, None, None, None, 0).kind, "retry");
        assert_eq!(classify_status(502, None, None, None, 0).wait, COOLDOWN);
        let limit = classify_status(403, Some("0"), Some(1300), None, 1000);
        assert_eq!(limit.kind, "rate_limit");
        assert_eq!(limit.wait, 301);
        assert_eq!(classify_status(429, None, None, Some(30), 0).wait, 30);
        assert_eq!(classify_status(403, Some("12"), None, None, 0).kind, "fatal");
        assert_eq!(classify_status(404, None, None, None, 0).kind, "fatal");
    }
    #[test] fn retries_then_succeeds() {
        let mut calls = 0;
        let r = retry_loop(|| { calls += 1; if calls < 2 { Err(NetError::retry(1, "x")) } else { Ok(7) } });
        assert_eq!(r, Ok(7));
        assert_eq!(calls, 2);
    }
    #[test] fn fatal_stops_at_once() {
        let mut calls = 0;
        let r: Result<(), NetError> = retry_loop(|| { calls += 1; Err(NetError::fatal("no")) });
        assert!(r.is_err());
        assert_eq!(calls, 1);
    }
}
