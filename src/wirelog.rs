//! Ordered, redacted JSONL recording for LSP wire messages.

use serde_json::Value;
use std::{
    collections::HashSet,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{mpsc, Mutex},
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};

/// Recorder for redacted JSONL wire messages. Directions are `in` and `out`.
pub struct WireLog {
    sender: Mutex<Option<mpsc::Sender<(String, u128, Value)>>>,
    worker: Mutex<Option<JoinHandle<io::Result<()>>>>,
    state: Mutex<RedactionState>,
}

struct RedactionState {
    identities: Vec<String>,
    secret_requests: HashSet<(String, String)>,
}

impl WireLog {
    /// Create recorder at explicit path, replacing `identity` occurrences with `[REDACTED]`.
    pub fn new(path: impl AsRef<Path>, identity: impl Into<String>) -> io::Result<Self> {
        let path: PathBuf = path.as_ref().into();
        let identity = identity.into();
        let identities = expand_identity(&identity);
        let (tx, rx) = mpsc::channel::<(String, u128, Value)>();
        let worker = thread::spawn(move || {
            #[cfg(unix)]
            let mut file = {
                use std::os::unix::fs::OpenOptionsExt;
                std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .mode(0o600)
                    .open(path)?
            };
            #[cfg(not(unix))]
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)?;
            for (direction, time, raw) in rx {
                let row = serde_json::json!({"direction":direction,"time":time,"pid":std::process::id(),"raw":raw});
                let mut line = serde_json::to_vec(&row)?;
                line.push(b'\n');
                #[cfg(unix)]
                {
                    use std::os::fd::AsRawFd;
                    let lock = FileLock::lock(file.as_raw_fd())?;
                    file.write_all(&line)?;
                    drop(lock);
                }
                #[cfg(not(unix))]
                file.write_all(&line)?;
            }
            file.flush()
        });
        Ok(Self {
            sender: Mutex::new(Some(tx)),
            worker: Mutex::new(Some(worker)),
            state: Mutex::new(RedactionState {
                identities,
                secret_requests: HashSet::new(),
            }),
        })
    }

    /// Enqueue a message (`in` or `out`) in shared serialization order.
    pub fn record(&self, direction: &str, message: &lsp_server::Message) -> io::Result<()> {
        let mut raw = serde_json::to_value(message).map_err(io::Error::other)?;
        let mut state = self.state.lock().unwrap();
        let method = raw.get("method").and_then(Value::as_str).unwrap_or("");
        let id = raw.get("id").map(Value::to_string);
        let prompt_secret = method.ends_with("/prompt")
            && raw.pointer("/params/secret").and_then(Value::as_bool) == Some(true);
        let correlated =
            if method.is_empty() && (raw.get("result").is_some() || raw.get("error").is_some()) {
                id.as_ref().is_some_and(|id| {
                    state
                        .secret_requests
                        .remove(&(id.clone(), opposite(direction).to_owned()))
                })
            } else {
                false
            };
        if prompt_secret {
            if let Some(id) = id {
                state.secret_requests.insert((id, direction.to_owned()));
            }
        }
        if correlated || prompt_secret {
            redact_secret_values(&mut raw);
        }
        redact(&mut raw, &state.identities, false, false);
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        self.sender
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "wirelog finished"))?
            .send((direction.to_owned(), time, raw))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "wirelog worker stopped"))
    }

    /// Close enqueue channel and wait until all queued records reach disk.
    pub fn finish(&self) -> io::Result<()> {
        self.sender.lock().unwrap().take();
        if let Some(worker) = self.worker.lock().unwrap().take() {
            worker
                .join()
                .map_err(|_| io::Error::other("wirelog worker panicked"))??;
        }
        Ok(())
    }

    /// Create recorder when `MAGIT_LSP_WIRELOG` is set; identity uses `$HOME`.
    pub fn from_env() -> io::Result<Option<Self>> {
        match std::env::var_os("MAGIT_LSP_WIRELOG") {
            Some(path) => {
                let home = std::env::var("HOME").unwrap_or_default();
                let user = std::env::var("USER")
                    .or_else(|_| std::env::var("USERNAME"))
                    .unwrap_or_default();
                let log = Self::new(path, home)?;
                log.state
                    .lock()
                    .unwrap()
                    .identities
                    .extend(expand_identity(&user));
                Ok(Some(log))
            }
            None => Ok(None),
        }
    }
}

#[cfg(unix)]
struct FileLock(i32);

#[cfg(unix)]
impl FileLock {
    fn lock(fd: i32) -> io::Result<Self> {
        unsafe extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }
        if unsafe { flock(fd, 2) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(fd))
    }
}

#[cfg(unix)]
impl Drop for FileLock {
    fn drop(&mut self) {
        unsafe extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }
        let _ = unsafe { flock(self.0, 8) };
    }
}

fn expand_identity(identity: &str) -> Vec<String> {
    let mut identities = Vec::new();
    let mut add_forms = |value: &str| {
        if value.is_empty() {
            return;
        }
        identities.push(value.to_owned());
        identities.push(percent_encode(value, true));
        identities.push(percent_encode(value, false));
    };
    add_forms(identity);
    if let Some(name) = Path::new(identity).file_name().and_then(|s| s.to_str()) {
        add_forms(name);
    }
    identities.sort();
    identities.dedup();
    identities
}

fn percent_encode(value: &str, preserve_slash: bool) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) || preserve_slash && byte == b'/'
        {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

impl Drop for WireLog {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn redact(value: &mut Value, identities: &[String], secret_prompt: bool, prompt_answer: bool) {
    match value {
        Value::Object(map) => {
            let secret = secret_prompt
                || map.get("method").and_then(Value::as_str) == Some("view/prompt")
                    && map
                        .get("params")
                        .and_then(|p| p.get("secret"))
                        .and_then(Value::as_bool)
                        == Some(true);
            let prompt_answer = prompt_answer
                || map.get("method").and_then(Value::as_str) == Some("view/promptRespond");
            for (key, child) in map.iter_mut() {
                let lower = key.to_ascii_lowercase();
                if (secret && (key == "default" || key == "value"))
                    || (prompt_answer && key == "value")
                    || [
                        "password",
                        "credential",
                        "askpass",
                        "token",
                        "access_token",
                        "refresh_token",
                        "auth_token",
                    ]
                    .iter()
                    .any(|s| lower == *s)
                {
                    *child = Value::String("[REDACTED]".into());
                } else {
                    redact(child, identities, secret, prompt_answer);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact(item, identities, secret_prompt, prompt_answer);
            }
        }
        Value::String(text) => {
            for identity in identities {
                *text = replace_case_insensitive(text, identity);
            }
        }
        _ => {}
    }
}

fn redact_secret_values(value: &mut Value) {
    if let Some(params) = value.get_mut("params") {
        if let Some(map) = params.as_object_mut() {
            for k in ["value", "default"] {
                if map.contains_key(k) {
                    map.insert(k.into(), Value::String("[REDACTED]".into()));
                }
            }
        }
    }
    if let Some(result) = value.get_mut("result") {
        *result = Value::String("[REDACTED]".into());
    }
    if let Some(error) = value.get_mut("error").and_then(Value::as_object_mut) {
        if let Some(message) = error.get_mut("message") {
            *message = Value::String("[REDACTED]".into());
        }
        if let Some(data) = error.get_mut("data") {
            *data = Value::String("[REDACTED]".into());
        }
    }
}

fn opposite(direction: &str) -> &'static str {
    if direction == "in" {
        "out"
    } else {
        "in"
    }
}

fn replace_case_insensitive(text: &str, needle: &str) -> String {
    if needle.is_empty() {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut rest = text;
    while let Some(index) = rest.to_ascii_lowercase().find(&needle.to_ascii_lowercase()) {
        out.push_str(&rest[..index]);
        out.push_str("[REDACTED]");
        rest = &rest[index + needle.len()..];
    }
    out.push_str(rest);
    out
}
