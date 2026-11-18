//! Engines outside this computer, Grok by xAI first.
//!
//! It answers this ask: "user can add their outside inference, start with
//! Grok and Grok ai. user adds it manually, they will need to provide or
//! specify a path to a key, now, the Rusty Business™ can query Grok for the
//! same question. ... you can use grok by it's self too."
//!
//! The rules, each with a test:
//!
//! - The person gives the path to a file that holds their xAI key. The app
//!   keeps the path, never the key. Rust reads the key from the file each time
//!   it asks xAI, and the key never reaches the page, the app's own files, or
//!   a message.
//! - xAI is asked only while the outbound gate is on, over https, at its API
//!   alone, an address no setting changes. The gate is looked at before the
//!   key is read, and turning it off stops an answer under way.
//! - Only Grok models for text, as xAI lists them, are offered, and only
//!   those are accepted. Grok answers alone or as one of the models asked, and
//!   waits for no turn: it is not on this computer.
//! - Removing Grok forgets the path. The key file is never touched.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::watch;

use crate::engine::{self, ChatDone, ChatEvent, ChatRequest, ModelInfo};
use crate::gate::Gate;

/// The one host asked, and its API.
pub const XAI_HOST: &str = "api.x.ai";
const XAI_BASE: &str = "https://api.x.ai/v1";

/// What the page's ids for Grok's models start with.
pub const PREFIX: &str = "xai:";

const NAME: &str = "Grok";
const MAKER: &str = "xAI";

/// What Grok says while the outbound gate is off.
const GATE_OFF: &str = "The gate is off. Turn it on to ask Grok.";

/// What adding Grok says while the outbound gate is off.
const GATE_OFF_FIND: &str = "Grok is added. Turn on the gate to find its models.";

/// What an answer says when the gate is turned off part way through.
const GATE_WENT_OFF: &str = "The gate went off, so Grok stopped.";

const NOT_ADDED: &str = "Grok is not added. Add it in Settings with the path to your xAI key.";
const NO_PATH: &str = "Give the path to the file that holds your xAI key.";
const WHOLE_PATH: &str = "Give the whole path to the file that holds your xAI key, starting with / or ~/, not the key itself.";
const UNREACHABLE: &str = "Grok could not be reached.";
const UNREADABLE: &str = "xAI's list of models could not be read.";

/// How long the app waits to reach xAI. A reasoning model can think for
/// minutes, so an answer has no overall limit; Stop and the gate end it.
const CONNECT_WAIT: Duration = Duration::from_secs(10);

/// How long a list of models may take, whole.
const LIST_WAIT: Duration = Duration::from_secs(30);

/// The most of a list the app reads.
const MOST_BYTES: usize = 1_000_000;

/// The most models kept from xAI's list.
const MOST_MODELS: usize = 30;

/// The longest path taken, and the biggest key file read.
const PATH_MOST: usize = 4096;
const KEY_FILE_MOST: u64 = 1024;

/// A key's length, from the shortest to the longest taken.
const KEY_SHORTEST: usize = 20;
const KEY_LONGEST: usize = 200;

// ---------------------------------------------------------------------------
// The key
// ---------------------------------------------------------------------------

/// The person's xAI key, read from their file for one call and dropped after
/// it. It has no `Debug` and no `Display`, so nothing can print it.
struct Key(String);

/// The key in the file at `path`. A file that holds anything but one key is
/// refused, and no message says what the file holds.
fn read_key(path: &Path) -> Result<Key, String> {
    let about = fs::metadata(path).map_err(|_| "There is no key file at that path, or the app cannot read it.".to_string())?;
    if about.is_dir() {
        return Err("That path is a folder. Give the file that holds the key.".to_string());
    }
    if about.len() > KEY_FILE_MOST {
        return Err("That file is too big to be a key.".to_string());
    }
    let mut text = String::new();
    fs::File::open(path)
        .and_then(|file| file.take(KEY_FILE_MOST + 1).read_to_string(&mut text))
        .map_err(|_| "The app could not read the key file.".to_string())?;
    let key = text.trim();
    if key.len() < KEY_SHORTEST || key.len() > KEY_LONGEST || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("That file does not hold just a key. Put the key alone in it, on one line.".to_string());
    }
    Ok(Key(key.to_string()))
}

/// A warning when other accounts on this computer can read the key file.
fn warning_for(path: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path).ok()?.permissions().mode();
        if mode & 0o077 != 0 {
            return Some("Other accounts on this computer can read the key file. Run chmod 600 on it to keep it to yours.".to_string());
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    None
}

/// This computer's home folder, for a path that starts with `~/`.
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).filter(|home| home.is_absolute())
}

/// The path the person gave, whole. Anything else, a key pasted in its place
/// most of all, is refused, and the message never repeats it.
fn checked_path(given: &str) -> Result<PathBuf, String> {
    let given = given.trim();
    if given.is_empty() {
        return Err(NO_PATH.to_string());
    }
    if given.chars().count() > PATH_MOST {
        return Err(WHOLE_PATH.to_string());
    }
    let path = match given.strip_prefix("~/").or_else(|| given.strip_prefix("~\\")) {
        Some(rest) => home().ok_or_else(|| WHOLE_PATH.to_string())?.join(rest),
        None => PathBuf::from(given),
    };
    if !path.is_absolute() {
        return Err(WHOLE_PATH.to_string());
    }
    Ok(path)
}

// ---------------------------------------------------------------------------
// Grok's models
// ---------------------------------------------------------------------------

/// True for an id that reads as one of Grok's models and nothing more.
fn fits(id: &str) -> bool {
    id.starts_with("grok-")
        && id.len() <= 100
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// "grok-4.7" reads "Grok 4.7".
fn name_of(id: &str) -> String {
    id.split('-')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut letters = word.chars();
            match letters.next() {
                Some(first) if first.is_ascii_alphabetic() => first.to_ascii_uppercase().to_string() + letters.as_str(),
                _ => word.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One of Grok's models as kept: its id at xAI, and its window, 0 when xAI
/// does not say.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
struct Found {
    id: String,
    #[serde(default)]
    context: u32,
}

impl Found {
    fn info(&self) -> ModelInfo {
        ModelInfo { id: format!("{PREFIX}{}", self.id), name: name_of(&self.id), maker: MAKER.to_string(), context: self.context }
    }
}

/// The Grok models for text in xAI's list, each once, in its order.
fn text_models(answer: &[u8]) -> Result<Vec<String>, String> {
    let value: Value = serde_json::from_slice(answer).map_err(|_| UNREADABLE.to_string())?;
    let listed = value.get("models").and_then(Value::as_array).ok_or_else(|| UNREADABLE.to_string())?;
    let mut ids: Vec<String> = Vec::new();
    for entry in listed {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        let text = match entry.get("output_modalities") {
            None | Some(Value::Null) => true,
            Some(Value::Array(kinds)) => kinds.iter().any(|kind| kind.as_str() == Some("text")),
            Some(_) => false,
        };
        if text && fits(id) && !ids.iter().any(|kept| kept == id) {
            ids.push(id.to_string());
        }
        if ids.len() == MOST_MODELS {
            break;
        }
    }
    if ids.is_empty() {
        return Err("xAI listed no Grok models for text.".to_string());
    }
    Ok(ids)
}

/// Each model's window in xAI's other list, where it says.
fn windows(answer: &[u8]) -> HashMap<String, u32> {
    let Ok(value) = serde_json::from_slice::<Value>(answer) else {
        return HashMap::new();
    };
    let mut found = HashMap::new();
    for entry in value.get("data").and_then(Value::as_array).into_iter().flatten() {
        if let (Some(id), Some(context)) = (entry.get("id").and_then(Value::as_str), entry.get("context_length").and_then(Value::as_u64)) {
            found.insert(id.to_string(), u32::try_from(context).unwrap_or(u32::MAX));
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Asking xAI
// ---------------------------------------------------------------------------

/// The one client for xAI, through the outbound gate. It follows no redirect
/// and sends no cookie; the key goes only in the call it is read for.
fn client(gate: &Gate) -> Result<reqwest::Client, String> {
    gate.client(GATE_OFF, |builder| {
        builder
            .no_proxy()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .referer(false)
            .user_agent(concat!("rusty-business/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(CONNECT_WAIT)
    })
}

/// True for an https address on xAI's API, with no account in it and no port
/// of its own.
fn from_xai(url: &reqwest::Url) -> bool {
    url.scheme() == "https" && url.username().is_empty() && url.password().is_none() && url.port().is_none() && url.domain() == Some(XAI_HOST)
}

/// Why xAI turned a call down, from its status alone. What xAI writes back is
/// never shown: it could repeat what was sent.
fn status_error(code: u16) -> String {
    match code {
        401 | 403 => format!("xAI did not take the key ({code}). Check the key file."),
        404 => "xAI answered 404. It may no longer offer that model: refresh Grok's models in Settings.".to_string(),
        429 => "xAI says too many requests, or no credits left (429). Wait a moment, or check your xAI account.".to_string(),
        500..=599 => format!("xAI is not answering right now ({code})."),
        _ => format!("xAI answered {code}."),
    }
}

/// An answer, read up to `most` bytes and no further.
async fn read_capped<B: AsRef<[u8]>, E>(chunks: impl Stream<Item = Result<B, E>>, most: usize) -> Result<Vec<u8>, String> {
    let mut chunks = std::pin::pin!(chunks);
    let mut answer = Vec::new();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|_| "xAI stopped answering part way through.".to_string())?;
        if answer.len() + chunk.as_ref().len() > most {
            return Err("xAI's answer was too large.".to_string());
        }
        answer.extend_from_slice(chunk.as_ref());
    }
    Ok(answer)
}

/// One of xAI's lists, `models` or `language-models`.
async fn list(client: &reqwest::Client, key: &Key, which: &str) -> Result<Vec<u8>, String> {
    let response = client
        .get(format!("{XAI_BASE}/{which}"))
        .bearer_auth(&key.0)
        .timeout(LIST_WAIT)
        .send()
        .await
        .map_err(|_| UNREACHABLE.to_string())?;
    if !from_xai(response.url()) {
        return Err("xAI answered from somewhere else.".to_string());
    }
    if !response.status().is_success() {
        return Err(status_error(response.status().as_u16()));
    }
    read_capped(response.bytes_stream(), MOST_BYTES).await
}

/// Grok's models for text, with their windows where xAI gives them.
async fn find_models(client: &reqwest::Client, key: &Key) -> Result<Vec<Found>, String> {
    let ids = text_models(&list(client, key, "language-models").await?)?;
    // The windows are a nicety: without them the models still answer.
    let windows = list(client, key, "models").await.map(|answer| windows(&answer)).unwrap_or_default();
    Ok(ids.into_iter().map(|id| Found { context: windows.get(&id).copied().unwrap_or(0), id }).collect())
}

// ---------------------------------------------------------------------------
// What the app keeps
// ---------------------------------------------------------------------------

/// What is kept for xAI: the path to the key, never the key, and the models
/// found. The models stay after Grok is removed, so saved chats still name
/// them.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Kept {
    #[serde(default)]
    key_path: Option<PathBuf>,
    #[serde(default)]
    models: Vec<Found>,
}

impl Kept {
    /// Only what reads right is kept: a whole path, and Grok's ids, each
    /// once.
    fn cleaned(mut self) -> Self {
        self.key_path = self.key_path.filter(|path| path.is_absolute());
        let mut models: Vec<Found> = Vec::new();
        for found in self.models {
            if fits(&found.id) && !models.iter().any(|kept| kept.id == found.id) {
                models.push(found);
            }
        }
        models.truncate(MOST_MODELS);
        self.models = models;
        self
    }
}

#[derive(Default, Deserialize, Serialize)]
struct KeptFile {
    #[serde(default)]
    xai: Kept,
}

/// The outside engines added, kept in the app's own folder.
pub struct Outside {
    file: PathBuf,
    kept: Mutex<Kept>,
}

/// An outside engine as the page shows it. It holds the key's path, never
/// the key.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutsideStatus {
    name: &'static str,
    maker: &'static str,
    host: &'static str,
    added: bool,
    key_path: Option<String>,
    models: Vec<ModelInfo>,
    warning: Option<String>,
    error: Option<String>,
}

impl Outside {
    /// What was kept the last time. A file that cannot be read keeps nothing.
    pub fn new(file: PathBuf) -> Self {
        let kept = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<KeptFile>(&bytes).ok())
            .map(|kept| kept.xai.cleaned())
            .unwrap_or_default();
        Self { file, kept: Mutex::new(kept) }
    }

    fn lock(&self) -> MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Changes what is kept, and keeps it for the next time the app opens.
    /// Nothing changes when it cannot be kept.
    fn change(&self, how: impl FnOnce(&mut Kept)) -> Result<(), String> {
        let mut kept = self.lock();
        let mut next = kept.clone();
        how(&mut next);
        let text = serde_json::to_string_pretty(&KeptFile { xai: next.clone() }).map_err(|_| "The app could not keep Grok's settings.".to_string())?;
        match self.file.parent() {
            Some(folder) => fs::create_dir_all(folder),
            None => Ok(()),
        }
        .and_then(|()| fs::write(&self.file, text + "\n"))
        .map_err(|error| format!("The app could not keep Grok's settings: {error}."))?;
        *kept = next;
        Ok(())
    }

    fn key_path(&self) -> Option<PathBuf> {
        self.lock().key_path.clone()
    }

    /// True when `id`, an id at xAI, is one of the models found.
    fn knows(&self, id: &str) -> bool {
        self.lock().models.iter().any(|found| found.id == id)
    }

    /// Grok as the page shows it. Nothing is asked of xAI.
    pub fn status(&self) -> OutsideStatus {
        let kept = self.lock().clone();
        OutsideStatus {
            name: NAME,
            maker: MAKER,
            host: XAI_HOST,
            added: kept.key_path.is_some(),
            key_path: kept.key_path.as_ref().map(|path| path.display().to_string()),
            models: kept.models.iter().map(Found::info).collect(),
            warning: kept.key_path.as_deref().and_then(warning_for),
            error: None,
        }
    }

    /// Adds Grok with the path to the person's key. The path and the key in
    /// it are checked first, and nothing is kept when either is refused.
    /// With the gate on, Grok's models are found at once.
    async fn add(&self, gate: &Gate, given: &str) -> Result<OutsideStatus, String> {
        let path = checked_path(given)?;
        drop(read_key(&path)?);
        self.change(|kept| kept.key_path = Some(path))?;
        if !gate.is_open() {
            return Ok(OutsideStatus { error: Some(GATE_OFF_FIND.to_string()), ..self.status() });
        }
        // The path is kept even when xAI cannot be asked yet.
        Ok(self.refresh(gate).await.unwrap_or_else(|error| OutsideStatus { error: Some(error), ..self.status() }))
    }

    /// Asks xAI for Grok's models, unless the outbound gate is off. A refresh
    /// that fails keeps the models found before.
    async fn refresh(&self, gate: &Gate) -> Result<OutsideStatus, String> {
        let client = client(gate)?;
        let path = self.key_path().ok_or_else(|| NOT_ADDED.to_string())?;
        let key = read_key(&path)?;
        let found = tokio::select! {
            found = find_models(&client, &key) => found?,
            () = gate.closed() => return Err(GATE_OFF.to_string()),
        };
        drop(key);
        self.change(|kept| kept.models = found)?;
        Ok(self.status())
    }

    /// Forgets the path to the key. The key file is never touched, and the
    /// models found stay, so saved chats still name them.
    fn remove(&self) -> Result<OutsideStatus, String> {
        self.change(|kept| kept.key_path = None)?;
        Ok(self.status())
    }
}

/// True for a page id that names one of Grok's models.
pub fn is_outside(id: &str) -> bool {
    id.starts_with(PREFIX)
}

/// Asks Grok, unless the outbound gate is off: the same question the models
/// on this computer get, with the same events back.
pub async fn run_chat(
    gate: &Gate,
    outside: &Outside,
    request: &ChatRequest,
    cancel: &mut watch::Receiver<bool>,
    emit: impl FnMut(ChatEvent),
) -> Result<ChatDone, String> {
    let model = request
        .model
        .strip_prefix(PREFIX)
        .filter(|id| outside.knows(id))
        .ok_or_else(|| "That Grok model is not on the list. Refresh Grok's models in Settings.".to_string())?;
    engine::checked_messages(&request.messages)?;
    // The gate first: with it off, the key is not even read.
    let client = client(gate)?;
    let path = outside.key_path().ok_or_else(|| NOT_ADDED.to_string())?;
    let key = read_key(&path)?;
    let body = serde_json::json!({
        "model": model,
        "messages": request.messages,
        "stream": true,
        "stream_options": { "include_usage": true },
        "max_completion_tokens": request.max_tokens.unwrap_or(engine::DEFAULT_MAX_TOKENS),
    });
    let sending = client.post(format!("{XAI_BASE}/chat/completions")).bearer_auth(&key.0).json(&body).send();
    drop(key);
    let response = tokio::select! {
        _ = cancel.changed() => return Ok(ChatDone { stopped: true, ..ChatDone::default() }),
        () = gate.closed() => return Err(GATE_WENT_OFF.to_string()),
        response = sending => response.map_err(|_| UNREACHABLE.to_string())?,
    };
    if !from_xai(response.url()) {
        return Err("xAI answered from somewhere else.".to_string());
    }
    if !response.status().is_success() {
        return Err(status_error(response.status().as_u16()));
    }
    read_answer(gate, response.bytes_stream(), cancel, emit).await
}

/// Reads Grok's answer as it comes, until it ends, Stop is pressed, or the
/// gate is turned off.
async fn read_answer<B: AsRef<[u8]>, E>(
    gate: &Gate,
    reply: impl Stream<Item = Result<B, E>>,
    cancel: &mut watch::Receiver<bool>,
    emit: impl FnMut(ChatEvent),
) -> Result<ChatDone, String> {
    let gate_off = async {
        gate.closed().await;
        GATE_WENT_OFF.to_string()
    };
    engine::read_reply(reply, cancel, gate_off, emit, "Grok stopped answering part way through.").await
}

// ---------------------------------------------------------------------------
// What the page asks for
// ---------------------------------------------------------------------------

/// Grok as kept. Nothing is asked of xAI.
#[tauri::command]
pub fn outside_status(outside: tauri::State<'_, Outside>) -> OutsideStatus {
    outside.status()
}

/// Adds Grok with the path to the person's key. The page sends the path,
/// never the key.
#[tauri::command]
pub async fn add_outside(key_path: String, outside: tauri::State<'_, Outside>, gate: tauri::State<'_, Gate>) -> Result<OutsideStatus, String> {
    outside.add(&gate, &key_path).await
}

/// Asks xAI for Grok's models again.
#[tauri::command]
pub async fn refresh_outside(outside: tauri::State<'_, Outside>, gate: tauri::State<'_, Gate>) -> Result<OutsideStatus, String> {
    outside.refresh(&gate).await
}

/// Forgets the path to the key.
#[tauri::command]
pub fn remove_outside(outside: tauri::State<'_, Outside>) -> Result<OutsideStatus, String> {
    outside.remove()
}

#[cfg(test)]
mod tests {
    // Brad, 2026-11-17: "user can add their outside inference, start with
    // Grok and Grok ai. user adds it manually, they will need to provide or
    // specify a path to a key, now, the Rusty Business™ can query Grok for the
    // same question." And: "you can use grok by it's self too."

    use super::*;
    use tauri::async_runtime::block_on;

    /// Plainly not a key anyone holds.
    const FAKE_KEY: &str = "xai-not-a-real-key-only-for-these-tests-0123456789";

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("rusty-outside-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    /// A key file holding `text`, kept to this account.
    fn key_file(place: &Path, text: &str) -> PathBuf {
        let path = place.join("xai-key");
        fs::write(&path, text).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        path
    }

    fn request(model: &str) -> ChatRequest {
        ChatRequest {
            model: model.to_string(),
            messages: vec![engine::ChatMessage { role: "user".into(), content: "What is 2+2?".into() }],
            max_tokens: None,
        }
    }

    fn chat(gate: &Gate, outside: &Outside, request: &ChatRequest) -> Result<ChatDone, String> {
        let (_stop, mut stopped) = watch::channel(false);
        block_on(run_chat(gate, outside, request, &mut stopped, |_| {}))
    }

    #[test]
    fn the_key_never_leaves_rust() {
        let place = folder("key");
        let path = key_file(&place, &format!("{FAKE_KEY}\n"));
        let outside = Outside::new(place.join("outside.json"));
        let status = block_on(outside.add(&Gate::at(false), path.to_str().unwrap())).unwrap();
        assert!(status.added);
        assert_eq!(status.key_path.as_deref(), path.to_str());
        assert_eq!(status.error.as_deref(), Some(GATE_OFF_FIND));
        assert_eq!(read_key(&path).unwrap().0, FAKE_KEY);

        // Not in what the page is told, not in the app's own file.
        let told = serde_json::to_string(&outside.status()).unwrap();
        assert!(!told.contains(FAKE_KEY) && !told.contains("not-a-real-key"), "{told}");
        let kept = fs::read_to_string(place.join("outside.json")).unwrap();
        assert!(!kept.contains(FAKE_KEY) && kept.contains("keyPath"), "{kept}");
        // The page is told these names, and no other.
        let told: Value = serde_json::from_str(&told).unwrap();
        let mut names: Vec<&str> = told.as_object().unwrap().keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, ["added", "error", "host", "keyPath", "maker", "models", "name", "warning"]);
        assert_eq!((told["name"].as_str(), told["maker"].as_str(), told["host"].as_str()), (Some("Grok"), Some("xAI"), Some("api.x.ai")));

        // Not in any message, whatever goes wrong.
        let mut errors = vec![
            chat(&Gate::at(false), &outside, &request("xai:grok-4.7")).unwrap_err(),
            block_on(outside.refresh(&Gate::at(false))).unwrap_err(),
            checked_path(FAKE_KEY).unwrap_err(),
        ];
        for (name, text) in [("spaced", format!("{FAKE_KEY} {FAKE_KEY}")), ("short", "xai-short".to_string())] {
            errors.push(read_key(&key_file(&folder(name), &text)).err().unwrap());
        }
        for error in errors {
            assert!(!error.contains(FAKE_KEY) && !error.contains("xai-short"), "{error}");
        }
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn a_key_pasted_where_the_path_goes_is_refused_and_not_kept() {
        let place = folder("pasted");
        let outside = Outside::new(place.join("outside.json"));
        for pasted in [FAKE_KEY, &format!("  {FAKE_KEY}\n"), "xai-abc/def"] {
            let error = block_on(outside.add(&Gate::at(true), pasted)).unwrap_err();
            assert_eq!(error, WHOLE_PATH);
            assert!(!error.contains(pasted.trim()));
        }
        assert_eq!(block_on(outside.add(&Gate::at(true), "   ")).unwrap_err(), NO_PATH);
        assert_eq!(block_on(outside.add(&Gate::at(true), &"/a".repeat(PATH_MOST))).unwrap_err(), WHOLE_PATH);
        assert!(!place.join("outside.json").exists(), "a refused path was kept");
        assert!(!outside.status().added);
        // A whole path, or one from the home folder, is taken.
        assert_eq!(checked_path(" /Users/me/.config/xai-key ").unwrap(), PathBuf::from("/Users/me/.config/xai-key"));
        if let Some(home) = home() {
            assert_eq!(checked_path("~/.config/xai-key").unwrap(), home.join(".config/xai-key"));
        }
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn a_file_that_is_not_just_a_key_is_refused() {
        let place = folder("files");
        assert!(read_key(&place.join("missing")).err().unwrap().starts_with("There is no key file"));
        assert_eq!(read_key(&place).err().unwrap(), "That path is a folder. Give the file that holds the key.");
        let big = key_file(&place, &"x".repeat(KEY_FILE_MOST as usize + 1));
        assert_eq!(read_key(&big).err().unwrap(), "That file is too big to be a key.");
        let not_just = "That file does not hold just a key. Put the key alone in it, on one line.";
        for text in [
            "".to_string(),
            "short".to_string(),
            format!("{FAKE_KEY}\n{FAKE_KEY}"),
            format!("XAI_API_KEY={FAKE_KEY} # mine"),
            format!("{FAKE_KEY}é"),
            "k".repeat(KEY_LONGEST + 1),
        ] {
            assert_eq!(read_key(&key_file(&place, &text)).err().unwrap(), not_just, "{text:?}");
        }
        // Space around the key is fine.
        assert_eq!(read_key(&key_file(&place, &format!("\n  {FAKE_KEY}  \n"))).unwrap().0, FAKE_KEY);
        // A path that is not whole is refused before any file is looked at.
        for relative in ["xai-key", "./xai-key", "config/xai-key", "~xai-key"] {
            assert_eq!(checked_path(relative).unwrap_err(), WHOLE_PATH, "{relative}");
        }
        // Adding Grok reads the file first, and keeps no path it refused.
        let outside = Outside::new(place.join("outside.json"));
        let missing = place.join("missing");
        assert!(block_on(outside.add(&Gate::at(false), missing.to_str().unwrap())).unwrap_err().starts_with("There is no key file"));
        let short = key_file(&place, "short");
        assert_eq!(block_on(outside.add(&Gate::at(false), short.to_str().unwrap())).unwrap_err(), not_just);
        assert!(!place.join("outside.json").exists(), "a path to no key was kept");
        assert!(!outside.status().added);
        fs::remove_dir_all(&place).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_key_file_others_can_read_is_warned_of() {
        use std::os::unix::fs::PermissionsExt;
        let place = folder("warn");
        let path = key_file(&place, FAKE_KEY);
        assert_eq!(warning_for(&path), None);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let warning = warning_for(&path).unwrap();
        assert!(warning.contains("chmod 600"), "{warning}");
        // It is still read: the warning is advice.
        assert!(read_key(&path).is_ok());
        let outside = Outside::new(place.join("outside.json"));
        let status = block_on(outside.add(&Gate::at(false), path.to_str().unwrap())).unwrap();
        assert_eq!(status.warning, Some(warning));
        fs::remove_dir_all(&place).unwrap();
    }

    // Brad, 2026-11-17: "off by default, everything stops".
    #[test]
    fn with_the_gate_off_grok_is_not_asked_and_the_key_is_not_read() {
        let place = folder("gate");
        let outside = Outside::new(place.join("outside.json"));
        // A path to no file at all: reading it would say so.
        outside.change(|kept| kept.key_path = Some(place.join("no-such-key"))).unwrap();
        outside.change(|kept| kept.models = vec![Found { id: "grok-4.7".into(), context: 0 }]).unwrap();
        assert_eq!(chat(&Gate::at(false), &outside, &request("xai:grok-4.7")).unwrap_err(), GATE_OFF);
        assert_eq!(block_on(outside.refresh(&Gate::at(false))).unwrap_err(), GATE_OFF);
        assert_eq!(client(&Gate::at(false)).unwrap_err(), GATE_OFF);
        // With it on, the key is read, and its absence said, before anything
        // is sent.
        assert!(chat(&Gate::at(true), &outside, &request("xai:grok-4.7")).unwrap_err().starts_with("There is no key file"));
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn only_grok_models_found_are_asked() {
        let place = folder("models");
        let outside = Outside::new(place.join("outside.json"));
        outside.change(|kept| kept.models = vec![Found { id: "grok-4.7".into(), context: 0 }]).unwrap();
        let refused = "That Grok model is not on the list. Refresh Grok's models in Settings.";
        for model in ["xai:grok-9", "grok-4.7", "xai:", "xai:../grok-4.7", "phi-4"] {
            assert_eq!(chat(&Gate::at(true), &outside, &request(model)).unwrap_err(), refused, "{model}");
        }
        let mut empty = request("xai:grok-4.7");
        empty.messages.clear();
        assert_eq!(chat(&Gate::at(true), &outside, &empty).unwrap_err(), "There is nothing to send.");
        assert!(is_outside("xai:grok-4.7") && !is_outside("mlx-community/phi-4-4bit") && !is_outside("grok-4.7"));
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn grok_is_asked_at_xais_api_alone_over_https() {
        assert_eq!(XAI_BASE, "https://api.x.ai/v1");
        assert_eq!(XAI_BASE, format!("https://{XAI_HOST}/v1"));
        assert!(from_xai(&reqwest::Url::parse("https://api.x.ai/v1/chat/completions").unwrap()));
        for away in [
            "http://api.x.ai/v1/chat/completions",
            "https://api.x.ai:8443/v1",
            "https://someone@api.x.ai/v1",
            "https://api.x.ai.example.com/v1",
            "https://x.ai/v1",
            "https://evilapi.x.ai/v1",
            "https://127.0.0.1/v1",
        ] {
            assert!(!from_xai(&reqwest::Url::parse(away).unwrap()), "{away}");
        }
        // The client goes over https only: a stand-in on plain http is never
        // reached.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let sent = block_on(client(&Gate::at(true)).unwrap().get(format!("http://127.0.0.1:{port}/v1/models")).send());
        assert!(sent.is_err());
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err(), "the client reached the stand-in over plain http");
    }

    #[test]
    fn xais_list_gives_grok_models_for_text_with_their_names() {
        let listed = serde_json::json!({ "models": [
            { "id": "grok-4.7", "output_modalities": ["text"] },
            { "id": "grok-420-reasoning", "output_modalities": ["text"], "aliases": ["latest"] },
            { "id": "grok-imagine-image", "output_modalities": ["image"] },
            { "id": "grok-imagine-video", "output_modalities": ["video"] },
            { "id": "grok-imagine-odd", "output_modalities": "image" },
            { "id": "grok-4.3" },
            { "id": "grok-4.7", "output_modalities": ["text"] },
            { "id": "latest", "output_modalities": ["text"] },
            { "id": "grok-4.7/../x", "output_modalities": ["text"] },
            { "id": "grok-<b>", "output_modalities": ["text"] },
            { "id": format!("grok-{}", "x".repeat(100)) },
            { "id": 7 },
            { "name": "no id" },
        ]});
        let ids = text_models(listed.to_string().as_bytes()).unwrap();
        assert_eq!(ids, ["grok-4.7", "grok-420-reasoning", "grok-4.3"]);
        assert_eq!(text_models(b"not json").unwrap_err(), UNREADABLE);
        assert_eq!(text_models(b"{}").unwrap_err(), UNREADABLE);
        assert_eq!(text_models(br#"{"models":[{"id":"grok-imagine-image","output_modalities":["image"]}]}"#).unwrap_err(), "xAI listed no Grok models for text.");
        let many: Vec<Value> = (0..50).map(|n| serde_json::json!({ "id": format!("grok-{n}") })).collect();
        assert_eq!(text_models(serde_json::json!({ "models": many }).to_string().as_bytes()).unwrap().len(), MOST_MODELS);

        let windows = windows(br#"{"data":[{"id":"grok-4.7","context_length":256000},{"id":"grok-4.3","context_length":null},{"id":"big","context_length":99999999999}]}"#);
        assert_eq!(windows.get("grok-4.7"), Some(&256_000));
        assert_eq!(windows.get("grok-4.3"), None);
        assert_eq!(windows.get("big"), Some(&u32::MAX));

        assert_eq!(name_of("grok-4.7"), "Grok 4.7");
        assert_eq!(name_of("grok-420-reasoning"), "Grok 420 Reasoning");
        assert_eq!(
            Found { id: "grok-4.7".into(), context: 256_000 }.info(),
            ModelInfo { id: "xai:grok-4.7".into(), name: "Grok 4.7".into(), maker: "xAI".into(), context: 256_000 }
        );
    }

    #[test]
    fn xais_refusals_are_said_plainly() {
        assert_eq!(status_error(401), "xAI did not take the key (401). Check the key file.");
        assert_eq!(status_error(403), "xAI did not take the key (403). Check the key file.");
        assert!(status_error(404).contains("refresh Grok's models"));
        assert!(status_error(429).contains("too many requests"));
        assert_eq!(status_error(503), "xAI is not answering right now (503).");
        assert_eq!(status_error(418), "xAI answered 418.");
        let chunks = |sizes: &[usize]| futures_util::stream::iter(sizes.iter().map(|size| Ok::<_, ()>(vec![b'x'; *size])).collect::<Vec<_>>());
        assert_eq!(block_on(read_capped(chunks(&[400, 600]), 1000)).unwrap().len(), 1000);
        assert_eq!(block_on(read_capped(chunks(&[400, 601]), 1000)).unwrap_err(), "xAI's answer was too large.");
    }

    #[test]
    fn turning_the_gate_off_stops_grok_part_way_through() {
        let piece = |text: &str| Ok::<_, std::io::Error>(format!("data: {}\n\n", serde_json::json!({ "choices": [{ "delta": { "content": text } }] })).into_bytes());
        // A stand-in for xAI that sends two pieces, then nothing more.
        let reply = || futures_util::stream::iter(vec![piece("2+2 "), piece("is")]).chain(futures_util::stream::pending());
        let gate = Gate::at(true);
        let switch = gate.clone();
        let turned_off = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            let _ = switch.set(false);
        });
        let (_stop, mut stopped) = watch::channel(false);
        let mut heard = String::new();
        // Were the gate not heard, the stand-in would never end, so the wait
        // is short and the test fails rather than hangs.
        let reading = read_answer(&gate, reply(), &mut stopped, |event| heard.push_str(&format!("{event:?}")));
        let ended = block_on(async { tokio::time::timeout(Duration::from_secs(10), reading).await }).expect("turning the gate off did not stop Grok");
        assert_eq!(ended.unwrap_err(), GATE_WENT_OFF);
        assert!(heard.contains("2+2") && heard.contains("is"), "{heard}");
        turned_off.join().unwrap();

        // Stop keeps what came.
        let (stop, mut stopped) = watch::channel(false);
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            let _ = stop.send(true);
            stop
        });
        let done = block_on(read_answer(&Gate::at(true), reply(), &mut stopped, |_| {})).unwrap();
        assert!(done.stopped);
        assert_eq!(done.answer, "2+2 is");
        drop(stopper.join().unwrap());
    }

    #[test]
    fn removing_grok_forgets_the_path_and_keeps_the_models() {
        let place = folder("remove");
        let path = key_file(&place, FAKE_KEY);
        let outside = Outside::new(place.join("outside.json"));
        block_on(outside.add(&Gate::at(false), path.to_str().unwrap())).unwrap();
        outside.change(|kept| kept.models = vec![Found { id: "grok-4.7".into(), context: 256_000 }]).unwrap();
        let status = outside.remove().unwrap();
        assert!(!status.added && status.key_path.is_none());
        assert_eq!(status.models.len(), 1, "saved chats would lose the model's name");
        assert!(path.exists(), "the key file was touched");
        assert_eq!(fs::read_to_string(&path).unwrap(), FAKE_KEY);
        // Removed stays removed.
        let again = Outside::new(place.join("outside.json"));
        assert!(!again.status().added);
        assert_eq!(again.status().models.len(), 1);
        assert_eq!(chat(&Gate::at(true), &again, &request("xai:grok-4.7")).unwrap_err(), NOT_ADDED);
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn a_kept_file_is_read_with_care() {
        let place = folder("kept");
        let file = place.join("outside.json");
        for text in ["", "not json", "[]", r#"{"xai":7}"#] {
            fs::write(&file, text).unwrap();
            assert_eq!(*Outside::new(file.clone()).lock(), Kept::default(), "{text:?}");
        }
        fs::write(
            &file,
            r#"{"xai":{"keyPath":"relative/key","models":[{"id":"grok-4.7","context":1},{"id":"grok-4.7"},{"id":"../x"},{"id":"phi-4"}]}}"#,
        )
        .unwrap();
        let kept = Outside::new(file.clone()).lock().clone();
        assert_eq!(kept, Kept { key_path: None, models: vec![Found { id: "grok-4.7".into(), context: 1 }] });
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    #[ignore = "asks xAI itself with the key at RUSTY_XAI_KEY_FILE; run with --ignored"]
    fn grok_answers_two_plus_two() {
        let path = PathBuf::from(std::env::var("RUSTY_XAI_KEY_FILE").expect("RUSTY_XAI_KEY_FILE names the key file"));
        let place = folder("live");
        let outside = Outside::new(place.join("outside.json"));
        let gate = Gate::at(true);
        let status = block_on(outside.add(&gate, path.to_str().unwrap())).unwrap();
        assert_eq!(status.error, None);
        let model = status.models.first().expect("xAI listed a model").id.clone();
        let done = chat(&gate, &outside, &request(&model)).unwrap();
        assert!(done.answer.contains('4'), "{}", done.answer);
        fs::remove_dir_all(&place).unwrap();
    }
}
