//! Other Rusty Business™ installs on your network, asked through their doors.
//!
//! Brad, 2026-11-25: "one [Rusty Business™] can talk to another [Rusty
//! Business™] on the same LAN, it has it's own connection handshake", and
//! "anything bubbled back ... goes to rb to rb back to me". Then, 2026-11-27:
//! "The handshake".
//!
//! This install pairs with another's courier door using the six-digit code
//! the other window shows, as a courier app would, and says it is an
//! install. The key the door gives is turned at once into the secret both
//! sides hold (`handshake.rs`) and forgotten. From then on each request is
//! signed, and each answer is shown only once it proves it came from the
//! install paired.
//!
//! - Only while the circuit breaker and the LAN gate are on here, and only to
//!   an address on the private network, given as numbers: no name is looked
//!   up, no proxy is used, and no redirect is followed.
//! - Words only: a picture never leaves this computer.
//! - One hop: the other install offers its chat engine's models and nothing
//!   further, never an install of its own, so a question cannot go round.
//! - Unpairing here forgets the secret. The other install's owner unpairs it
//!   there, in its courier list.

use std::fs;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::courier::{self, COURIER_PORT, Courier};
use crate::engine::{self, ChatDone, ChatEvent, ChatRequest};
use crate::gate::{Gate, LanGate};
use crate::handshake;
use crate::vision;

/// What an install's model's chat id starts with: `install:<id>/<model>`.
const PREFIX: &str = "install:";
/// The most installs this one pairs with.
const MOST_INSTALLS: usize = 8;
/// The longest name for an install.
const NAME_MOST: usize = 40;
/// The most models one install may offer here.
const MOST_MODELS: usize = 64;
/// The longest model id taken from another install.
const MODEL_ID_MOST: usize = 200;
/// The most earlier turns sent with a question, as the door takes them.
const MOST_BEFORE: usize = 200;
/// The most an answer may come to.
const ANSWER_MOST: usize = 4 * 1024 * 1024;
/// How long pairing, the handshake, and the list of models may take.
const QUICK: Duration = Duration::from_secs(10);
/// How long telling the other install to stop may take.
const CANCEL_TIME: Duration = Duration::from_secs(2);

const GONE: &str = "That install is no longer paired here.";
const BREAKER_OFF: &str =
    "The circuit breaker is off. Turn it on to ask another Rusty Business™ install.";
const BREAKER_CLOSED: &str =
    "The circuit breaker was turned off. The question to the other install stopped.";
const NOT_PROVEN: &str =
    "The answer did not prove it came from the install paired, so it was not shown.";
const REFUSED: &str =
    "The other install turned the handshake away. Pair with it again, from its window.";

// ---------------------------------------------------------------------------
// What is kept
// ---------------------------------------------------------------------------

/// One install paired: where it is, and the secret, never the key.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Install {
    /// The other install's id for this one.
    id: String,
    /// What the person calls it.
    name: String,
    /// Its door, as numbers and a port, such as 192.168.1.20:4710.
    address: String,
    /// The secret both hold, as lowercase hex.
    secret: String,
    /// Seconds since 1970 when it paired.
    since: u64,
}

/// One install, as the window shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallView {
    pub id: String,
    pub name: String,
    pub address: String,
    pub since: u64,
}

/// The installs paired, and whether they can be asked now.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallsView {
    pub installs: Vec<InstallView>,
    /// True while the circuit breaker and the LAN gate are both on.
    pub ready: bool,
}

/// The models one install offers, or why none came.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallModels {
    pub id: String,
    pub name: String,
    pub models: Vec<String>,
    pub error: Option<String>,
}

/// The installs paired, kept in the app's own folder.
pub struct Installs {
    file: PathBuf,
    kept: Mutex<Vec<Install>>,
}

impl Installs {
    /// The installs as last kept. A file that cannot be read is none.
    pub fn new(file: PathBuf) -> Self {
        let kept = fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<Install>>(&text).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|install| {
                checked_address(&install.address).as_deref() == Ok(install.address.as_str())
                    && is_secret(&install.secret)
            })
            .take(MOST_INSTALLS)
            .collect();
        Self {
            file,
            kept: Mutex::new(kept),
        }
    }

    fn all(&self) -> Vec<Install> {
        self.kept
            .lock()
            .map(|kept| kept.clone())
            .unwrap_or_default()
    }

    fn view(&self, ready: bool) -> InstallsView {
        InstallsView {
            installs: self
                .all()
                .into_iter()
                .map(|install| InstallView {
                    id: install.id,
                    name: install.name,
                    address: install.address,
                    since: install.since,
                })
                .collect(),
            ready,
        }
    }

    fn find(&self, id: &str) -> Option<Install> {
        self.all().into_iter().find(|install| install.id == id)
    }

    /// Keeps one more, readable by this account alone.
    fn add(&self, install: Install) -> Result<(), String> {
        let mut kept = self
            .kept
            .lock()
            .map_err(|_| "The app could not take the installs.".to_string())?;
        let mut next: Vec<Install> = kept
            .iter()
            .filter(|old| old.address != install.address)
            .cloned()
            .collect();
        if next.len() >= MOST_INSTALLS {
            return Err(format!(
                "At most {MOST_INSTALLS} installs can be paired. Unpair one first."
            ));
        }
        next.push(install);
        keep(&self.file, &next)?;
        *kept = next;
        Ok(())
    }

    /// Forgets one, and its secret.
    fn remove(&self, id: &str) -> Result<(), String> {
        let mut kept = self
            .kept
            .lock()
            .map_err(|_| "The app could not take the installs.".to_string())?;
        let next: Vec<Install> = kept.iter().filter(|old| old.id != id).cloned().collect();
        keep(&self.file, &next)?;
        *kept = next;
        Ok(())
    }
}

/// Writes the installs readable by this account alone, whole or not at all.
fn keep(file: &Path, installs: &[Install]) -> Result<(), String> {
    let unkept = |error: std::io::Error| format!("The app could not keep the installs: {error}.");
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder).map_err(unkept)?;
    }
    let text = serde_json::to_string_pretty(installs).map_err(|error| error.to_string())?;
    let part = file.with_extension("part");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(&part).map_err(unkept)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&part, fs::Permissions::from_mode(0o600)).map_err(unkept)?;
    }
    out.write_all(text.as_bytes())
        .and_then(|()| out.sync_all())
        .map_err(unkept)?;
    drop(out);
    fs::rename(&part, file).map_err(unkept)
}

fn is_secret(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// An install's door, as numbers and a port: 4710 when none is given. It
/// must be on this computer or the private network. A name is never taken,
/// so nothing is looked up.
pub(crate) fn checked_address(raw: &str) -> Result<String, String> {
    let words = "Give the other install's address on your private network as numbers, such as 192.168.1.20.";
    let raw = raw.trim();
    let raw = raw.strip_prefix("http://").unwrap_or(raw);
    let raw = raw.strip_suffix("/mcp").unwrap_or(raw);
    let address = match raw.parse::<SocketAddr>() {
        Ok(address) => address,
        Err(_) => {
            let ip = raw
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<IpAddr>()
                .map_err(|_| words.to_string())?;
            SocketAddr::new(ip, COURIER_PORT)
        }
    };
    if address.port() == 0 || address.ip().is_unspecified() || !courier::private_peer(address.ip())
    {
        return Err(words.to_string());
    }
    Ok(address.to_string())
}

fn checked_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the other install a name.".to_string());
    }
    if name.chars().count() > NAME_MOST || name.chars().any(char::is_control) {
        return Err(format!(
            "Keep the install's name to {NAME_MOST} letters, on one line."
        ));
    }
    Ok(name.to_string())
}

/// Words from another install, made safe to show: one line, and short.
fn their_words(words: &str) -> String {
    words
        .chars()
        .filter(|letter| !letter.is_control())
        .take(200)
        .collect()
}

// ---------------------------------------------------------------------------
// Signed requests
// ---------------------------------------------------------------------------

fn fresh() -> Result<String, String> {
    Ok(handshake::hex(&courier::random::<32>()?))
}

fn rpc(id: &str, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// Sends one signed message to an install's door, and checks the answer's
/// proof before anything in it is read.
async fn signed(install: &Install, message: &Value) -> Result<reqwest::Response, String> {
    let body = serde_json::to_vec(message).map_err(|error| error.to_string())?;
    let nonce = fresh()?;
    let said = handshake::sign(
        &install.secret,
        &install.id,
        &nonce,
        courier::seconds_now(),
        &body,
    );
    let response = engine::client()?
        .post(format!("http://{}/mcp", install.address))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header(handshake::SIGNED, said)
        .body(body)
        .send()
        .await
        .map_err(|_| format!("{} is not answering.", install.name))?;
    let proof = response
        .headers()
        .get(handshake::PROOF)
        .and_then(|value| value.to_str().ok());
    if !handshake::proven(&install.secret, &nonce, proof) {
        return Err(if response.status().as_u16() == 401 {
            REFUSED.to_string()
        } else {
            NOT_PROVEN.to_string()
        });
    }
    if !response.status().is_success() {
        return Err(format!(
            "{} answered {}.",
            install.name,
            response.status().as_u16()
        ));
    }
    Ok(response)
}

/// A signed message whose answer is one JSON-RPC result.
async fn signed_result(install: &Install, message: &Value) -> Result<Value, String> {
    let response = signed(install, message).await?;
    let reply: Value = response
        .json()
        .await
        .map_err(|_| format!("{}'s answer could not be read.", install.name))?;
    if let Some(error) = reply.get("error") {
        let words = error.get("message").and_then(Value::as_str).unwrap_or("");
        return Err(format!("{} said: {}", install.name, their_words(words)));
    }
    reply
        .get("result")
        .cloned()
        .ok_or_else(|| format!("{}'s answer could not be read.", install.name))
}

/// A tool's text, and whether it is an error.
fn text_of(result: &Value) -> (String, bool) {
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<String>()
        })
        .unwrap_or_default();
    let error = result.get("isError").and_then(Value::as_bool) == Some(true);
    (text, error)
}

/// The models an install offers, as ids this install can chat with.
async fn models_of(install: &Install) -> Result<Vec<String>, String> {
    let call = rpc(
        &fresh()?,
        "tools/call",
        json!({ "name": "models", "arguments": {} }),
    );
    let result = signed_result(install, &call).await?;
    let (text, error) = text_of(&result);
    if error {
        return Err(their_words(&text));
    }
    Ok(result
        .pointer("/structuredContent/models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(Value::as_str)
                .filter(|model| {
                    !model.is_empty()
                        && model.chars().count() <= MODEL_ID_MOST
                        && !model.chars().any(char::is_control)
                })
                .take(MOST_MODELS)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Pairing
// ---------------------------------------------------------------------------

/// What this install calls itself to the other.
fn own_name() -> String {
    courier::this_address().map_or_else(
        || "Another Rusty Business™ install".to_string(),
        |ip| format!("Rusty Business™ at {ip}"),
    )
}

/// Pairs with another install's door, proves the handshake holds both ways,
/// and only then keeps it.
pub(crate) async fn pair(
    installs: &Installs,
    address: &str,
    code: &str,
    name: &str,
) -> Result<InstallView, String> {
    let address = checked_address(address)?;
    let name = checked_name(name)?;
    let code = code.trim();
    if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Give the six-digit code the other install's window shows.".to_string());
    }
    let asked = json!({ "code": code, "name": own_name(), "kind": "install" });
    let response = engine::client()?
        .post(format!("http://{address}/pair"))
        .json(&asked)
        .send()
        .await
        .map_err(|_| format!("{name} is not answering at {address}."))?;
    let status = response.status().as_u16();
    let reply: Value = response.json().await.unwrap_or(Value::Null);
    if status != 200 {
        let words = reply.get("error").and_then(Value::as_str).unwrap_or("");
        return Err(format!("{name} did not pair: {}", their_words(words)));
    }
    let id = reply.get("id").and_then(Value::as_str).unwrap_or("");
    let key = reply.get("key").and_then(Value::as_str).unwrap_or("");
    let well_formed = !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if !well_formed || key.is_empty() {
        return Err(format!("{name}'s answer to pairing could not be read."));
    }
    let install = Install {
        id: id.to_string(),
        name,
        address,
        // The key is turned into the secret here and never kept.
        secret: handshake::secret_of(key),
        since: courier::seconds_now(),
    };
    let hello = rpc(
        &fresh()?,
        "initialize",
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "rusty-business", "version": env!("CARGO_PKG_VERSION") }
        }),
    );
    let result = signed_result(&install, &hello).await.map_err(|error| {
        format!(
            "{error} It may still be paired there; its owner can unpair it in its courier list."
        )
    })?;
    if result.pointer("/serverInfo/name").and_then(Value::as_str) != Some("rusty-business") {
        return Err(format!(
            "{} is not a Rusty Business™ install.",
            install.name
        ));
    }
    let view = InstallView {
        id: install.id.clone(),
        name: install.name.clone(),
        address: install.address.clone(),
        since: install.since,
    };
    installs.add(install)?;
    Ok(view)
}

// ---------------------------------------------------------------------------
// Asking
// ---------------------------------------------------------------------------

/// True for a chat id of a model on another install.
pub fn is_install(id: &str) -> bool {
    id.starts_with(PREFIX)
}

/// The install's id and the model's, from a chat id. The model may hold a
/// '/'; the install's id never does.
fn split_id(chat_id: &str) -> Option<(&str, &str)> {
    let (install, model) = chat_id.strip_prefix(PREFIX)?.split_once('/')?;
    (!install.is_empty() && !model.is_empty()).then_some((install, model))
}

/// Runs `asking` only while the LAN gate and the breaker are on, and stops
/// it the moment either goes off.
async fn guarded<T>(
    lan: &LanGate,
    breaker: &Gate,
    asking: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    if !breaker.is_open() {
        return Err(BREAKER_OFF.to_string());
    }
    lan.run(async {
        tokio::select! {
            () = breaker.closed() => Err(BREAKER_CLOSED.to_string()),
            done = asking => done,
        }
    })
    .await
}

/// One chat with a model on another install: the last question in words,
/// with the turns before it, asked through a signed request, and its answer
/// shown as it comes once it has proven itself.
pub(crate) async fn run_chat(
    lan: &LanGate,
    breaker: &Gate,
    installs: &Installs,
    request: &ChatRequest,
    stopped: &mut watch::Receiver<bool>,
    emit: impl FnMut(ChatEvent),
) -> Result<ChatDone, String> {
    if vision::has_pictures(&request.messages) {
        return Err(vision::STAYS_HERE.to_string());
    }
    let Some((id, model)) = split_id(&request.model) else {
        return Err(GONE.to_string());
    };
    let Some(install) = installs.find(id) else {
        return Err(GONE.to_string());
    };
    let Some((last, earlier)) = request.messages.split_last() else {
        return Err("There is no question to ask.".to_string());
    };
    if last.role != "user" || last.content.trim().is_empty() {
        return Err("There is no question to ask.".to_string());
    }
    // The newest turns, when there are more than the door takes.
    let before: Vec<Value> = earlier
        .iter()
        .skip(earlier.len().saturating_sub(MOST_BEFORE))
        .map(|turn| json!({ "role": turn.role, "content": turn.content }))
        .collect();
    let call_id = fresh()?;
    let call = json!({
        "jsonrpc": "2.0",
        "id": call_id,
        "method": "tools/call",
        "params": {
            "name": "ask",
            "arguments": { "model": model, "question": last.content, "before": before },
            "_meta": { "progressToken": call_id }
        }
    });
    guarded(lan, breaker, ask(&install, &call_id, &call, stopped, emit)).await
}

/// Tells the other install to stop, briefly, and keeps what came.
async fn cancel(install: &Install, call_id: &str) {
    let note = json!({
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "params": { "requestId": call_id, "reason": "Stopped here." }
    });
    let _ = tokio::time::timeout(CANCEL_TIME, signed(install, &note)).await;
}

fn stopped_with(answer: String) -> ChatDone {
    ChatDone {
        answer,
        stopped: true,
        ..ChatDone::default()
    }
}

/// What one server-sent event says, when it is a JSON-RPC message.
fn message_of(event: &str) -> Option<Value> {
    let data: Vec<&str> = event
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect();
    serde_json::from_str(&data.join("\n")).ok()
}

async fn ask(
    install: &Install,
    call_id: &str,
    call: &Value,
    stopped: &mut watch::Receiver<bool>,
    mut emit: impl FnMut(ChatEvent),
) -> Result<ChatDone, String> {
    if *stopped.borrow() {
        return Ok(stopped_with(String::new()));
    }
    let response = tokio::select! {
        _ = stopped.changed() => return Ok(stopped_with(String::new())),
        response = signed(install, call) => response?,
    };
    let streamed = response
        .headers()
        .get("content-type")
        .and_then(|kind| kind.to_str().ok())
        .is_some_and(|kind| kind.starts_with("text/event-stream"));
    let mut shown = String::new();
    let result = if streamed {
        let mut pieces = response.bytes_stream();
        let mut waiting = String::new();
        let mut size = 0usize;
        loop {
            let piece = tokio::select! {
                _ = stopped.changed() => {
                    cancel(install, call_id).await;
                    return Ok(stopped_with(shown));
                }
                piece = pieces.next() => piece,
            };
            let Some(Ok(bytes)) = piece else {
                return Err(format!(
                    "{} stopped answering part way through.",
                    install.name
                ));
            };
            size += bytes.len();
            if size > ANSWER_MOST {
                cancel(install, call_id).await;
                return Err(format!("{}'s answer ran past 4 MiB.", install.name));
            }
            waiting.push_str(&String::from_utf8_lossy(&bytes).replace("\r\n", "\n"));
            let mut done = None;
            while let Some(end) = waiting.find("\n\n") {
                let event: String = waiting.drain(..end + 2).collect();
                let Some(message) = message_of(&event) else {
                    continue;
                };
                if message.get("method").and_then(Value::as_str) == Some("notifications/progress") {
                    if let Some(text) = message.pointer("/params/message").and_then(Value::as_str) {
                        shown.push_str(text);
                        emit(ChatEvent::Answer {
                            text: text.to_string(),
                        });
                    }
                } else if message.get("id").and_then(Value::as_str) == Some(call_id) {
                    done = Some(message);
                    break;
                }
            }
            if let Some(message) = done {
                break message;
            }
        }
    } else {
        response
            .json::<Value>()
            .await
            .map_err(|_| format!("{}'s answer could not be read.", install.name))?
    };
    let Some(result) = result.get("result") else {
        let words = result
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("");
        return Err(format!("{} said: {}", install.name, their_words(words)));
    };
    let (text, error) = text_of(result);
    if error {
        return Err(format!("{} said: {}", install.name, their_words(&text)));
    }
    if text != shown {
        // The whole answer is the word: what was shown gives way to it.
        emit(ChatEvent::Reset {
            thinking: String::new(),
            answer: text.clone(),
        });
    }
    Ok(ChatDone {
        answer: text,
        ..ChatDone::default()
    })
}

// ---------------------------------------------------------------------------
// What the page asks
// ---------------------------------------------------------------------------

fn ready(lan: &LanGate, breaker: &Gate) -> bool {
    lan.gate().is_open() && breaker.is_open()
}

#[tauri::command]
pub fn installs_status(
    installs: tauri::State<'_, Installs>,
    lan: tauri::State<'_, LanGate>,
    courier: tauri::State<'_, Courier>,
) -> InstallsView {
    installs.view(ready(&lan, &courier.breaker()))
}

#[tauri::command]
pub async fn pair_install(
    installs: tauri::State<'_, Installs>,
    lan: tauri::State<'_, LanGate>,
    courier: tauri::State<'_, Courier>,
    address: String,
    code: String,
    name: String,
) -> Result<InstallsView, String> {
    let breaker = courier.breaker();
    let pairing = async {
        tokio::time::timeout(QUICK, pair(&installs, &address, &code, &name))
            .await
            .map_err(|_| "The other install took too long to pair.".to_string())?
    };
    guarded(&lan, &breaker, pairing).await?;
    Ok(installs.view(ready(&lan, &breaker)))
}

/// Forgets an install here. Its owner unpairs this one there.
#[tauri::command]
pub fn unpair_install(
    installs: tauri::State<'_, Installs>,
    lan: tauri::State<'_, LanGate>,
    courier: tauri::State<'_, Courier>,
    id: String,
) -> Result<InstallsView, String> {
    installs.remove(&id)?;
    Ok(installs.view(ready(&lan, &courier.breaker())))
}

/// The models each install offers, asked now.
pub(crate) async fn all_models(
    lan: &LanGate,
    breaker: &Gate,
    installs: &Installs,
) -> Vec<InstallModels> {
    let mut found = Vec::new();
    for install in installs.all() {
        let asking = async {
            tokio::time::timeout(QUICK, models_of(&install))
                .await
                .map_err(|_| format!("{} took too long to answer.", install.name))?
        };
        let (models, error) = match guarded(lan, breaker, asking).await {
            Ok(models) => (models, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        found.push(InstallModels {
            id: install.id,
            name: install.name,
            models,
            error,
        });
    }
    found
}

#[tauri::command]
pub async fn install_models(
    installs: tauri::State<'_, Installs>,
    lan: tauri::State<'_, LanGate>,
    courier: tauri::State<'_, Courier>,
) -> Result<Vec<InstallModels>, String> {
    Ok(all_models(&lan, &courier.breaker(), &installs).await)
}

#[cfg(test)]
mod tests {
    // Brad, 2026-11-25: "one [Rusty Business™] can talk to another [Rusty
    // Business™] on the same LAN, it has it's own connection handshake", and
    // "anything bubbled back ... goes to rb to rb back to me". Then, 2026-11-27:
    // "The handshake".
    use super::*;
    use crate::courier::Answers;
    use crate::engine::ChatMessage;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;
    use tauri::async_runtime::block_on;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("rusty-installs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    /// The other install's chat engine: it answers in pieces, with what it
    /// was told before, or waits until it is stopped.
    #[derive(Clone, Default)]
    struct Other {
        waits: bool,
        stopped: Arc<AtomicBool>,
    }

    impl Answers for Other {
        async fn models(&self) -> Result<Vec<String>, String> {
            Ok(vec!["phi-4".to_string(), "maker/model-2".to_string()])
        }

        async fn ask(
            &self,
            model: &str,
            before: &[(String, String)],
            question: &str,
            stopped: &mut watch::Receiver<bool>,
            mut emit: impl FnMut(ChatEvent) + Send,
        ) -> Result<ChatDone, String> {
            if self.waits {
                emit(ChatEvent::Answer {
                    text: "Thinking it over".to_string(),
                });
                let _ = stopped.wait_for(|stop| *stop).await;
                self.stopped.store(true, Ordering::SeqCst);
                return Ok(ChatDone {
                    stopped: true,
                    ..ChatDone::default()
                });
            }
            let roles: Vec<&str> = before.iter().map(|(role, _)| role.as_str()).collect();
            let answer = format!("{model} heard {question:?} after [{}]", roles.join(" "));
            let (one, two) = answer.split_at(answer.len() / 2);
            for text in [one, two] {
                emit(ChatEvent::Answer {
                    text: text.to_string(),
                });
            }
            Ok(ChatDone {
                answer,
                ..ChatDone::default()
            })
        }
    }

    /// Both sides: the other install's door, listening on this computer,
    /// and this install's installs, LAN gate, and breaker, all on.
    struct Pair {
        door: Courier,
        door_lan: Gate,
        listener: TcpListener,
        address: String,
        lan: LanGate,
        breaker: Courier,
        installs: Installs,
        place: PathBuf,
    }

    async fn both(name: &str) -> Pair {
        let place = folder(name);
        let door = Courier::new(place.join("door-breaker"), place.join("couriers.json"));
        door.set(true).unwrap();
        let door_lan = Gate::new(place.join("door-lan"));
        door_lan.set(true).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let lan = LanGate::new(place.join("lan-gate"));
        lan.gate().set(true).unwrap();
        let breaker = Courier::new(place.join("breaker"), place.join("mine.json"));
        breaker.set(true).unwrap();
        let installs = Installs::new(place.join("installs.json"));
        Pair {
            door,
            door_lan,
            listener,
            address,
            lan,
            breaker,
            installs,
            place,
        }
    }

    /// Runs `body` while the door listens.
    async fn with_door<T>(pair: &Pair, engine: &Other, body: impl Future<Output = T>) -> T {
        tokio::select! {
            () = courier::listen(&pair.listener, &pair.door, &pair.door_lan, engine) => {
                panic!("the door closed")
            }
            done = body => done,
        }
    }

    async fn paired(pair: &Pair) -> InstallView {
        let code = pair.door.start_pairing(Instant::now()).unwrap();
        super::pair(&pair.installs, &pair.address, &code, "Upstairs")
            .await
            .unwrap()
    }

    fn question(model: &str, before: &[(&str, &str)], last: &str) -> ChatRequest {
        let mut messages: Vec<ChatMessage> = before
            .iter()
            .map(|(role, content)| ChatMessage {
                role: role.to_string(),
                content: content.to_string(),
                images: Vec::new(),
            })
            .collect();
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: last.to_string(),
            images: Vec::new(),
        });
        ChatRequest {
            model: model.to_string(),
            messages,
            max_tokens: None,
        }
    }

    async fn chat(pair: &Pair, request: &ChatRequest) -> (Result<ChatDone, String>, String) {
        let (_stop, mut stopped) = watch::channel(false);
        let mut shown = String::new();
        let done = run_chat(
            &pair.lan,
            &pair.breaker.breaker(),
            &pair.installs,
            request,
            &mut stopped,
            |event| match event {
                ChatEvent::Answer { text } => shown.push_str(&text),
                ChatEvent::Reset { answer, .. } => shown = answer,
                ChatEvent::Thinking { .. } => {}
            },
        )
        .await;
        (done, shown)
    }

    /// One raw request to the door, and all that comes back.
    async fn raw(address: &str, request: String) -> String {
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        socket.write_all(request.as_bytes()).await.unwrap();
        let mut reply = String::new();
        socket.read_to_string(&mut reply).await.unwrap();
        reply
    }

    fn signed_post(install: &Install, body: &str, nonce: &str, time: u64) -> String {
        let said = handshake::sign(&install.secret, &install.id, nonce, time, body.as_bytes());
        format!(
            "POST /mcp HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\n{}: {said}\r\nContent-Length: {}\r\n\r\n{body}",
            handshake::SIGNED,
            body.len()
        )
    }

    #[test]
    fn an_install_pairs_with_the_code_keeps_only_the_secret_and_asks_through_the_door() {
        block_on(async {
            let pair = both("asks").await;
            let engine = Other::default();
            with_door(&pair, &engine, async {
                let view = paired(&pair).await;
                assert_eq!(view.name, "Upstairs");
                // The door knows it as an install.
                let listed = pair.door.status(Instant::now()).paired;
                assert_eq!(listed.len(), 1);
                assert_eq!(listed[0].kind, courier::Kind::Install);
                // The file holds the secret, never a key, readable by its
                // owner alone.
                let file = pair.place.join("installs.json");
                let kept: Vec<Install> =
                    serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
                assert!(is_secret(&kept[0].secret));
                assert!(!fs::read_to_string(&file).unwrap().contains("\"key\""));
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
                    assert_eq!(mode, 0o600);
                }

                let models = all_models(&pair.lan, &pair.breaker.breaker(), &pair.installs).await;
                assert_eq!(models[0].models, vec!["phi-4", "maker/model-2"]);
                assert_eq!(models[0].error, None);

                // The question, and the turns before it, go across; the
                // answer comes back as it is written.
                let chat_id = format!("install:{}/maker/model-2", view.id);
                let request = question(
                    &chat_id,
                    &[
                        ("system", "Be brief."),
                        ("user", "Hi"),
                        ("assistant", "Hello"),
                    ],
                    "And now?",
                );
                let (done, shown) = chat(&pair, &request).await;
                let done = done.unwrap();
                assert_eq!(
                    done.answer,
                    "maker/model-2 heard \"And now?\" after [system user assistant]"
                );
                assert_eq!(shown, done.answer);
            })
            .await;
            fs::remove_dir_all(&pair.place).ok();
        });
    }

    #[test]
    fn a_wrong_code_pairs_nothing_and_nothing_is_kept() {
        block_on(async {
            let pair = both("wrong").await;
            let engine = Other::default();
            with_door(&pair, &engine, async {
                let code = pair.door.start_pairing(Instant::now()).unwrap();
                let wrong = if code == "000000" { "111111" } else { "000000" };
                let error = super::pair(&pair.installs, &pair.address, wrong, "Upstairs")
                    .await
                    .unwrap_err();
                assert!(error.starts_with("Upstairs did not pair"), "{error}");
                assert!(pair.installs.all().is_empty());
                assert!(!pair.place.join("installs.json").exists());
            })
            .await;
            fs::remove_dir_all(&pair.place).ok();
        });
    }

    #[test]
    fn an_installs_key_is_never_taken_as_a_bearer_token() {
        block_on(async {
            let pair = both("bearer").await;
            let engine = Other::default();
            with_door(&pair, &engine, async {
                // Pair by hand, as an install, to hold the key once.
                let code = pair.door.start_pairing(Instant::now()).unwrap();
                let body = json!({ "code": code, "name": "Kitchen", "kind": "install" }).to_string();
                let reply = raw(
                    &pair.address,
                    format!(
                        "POST /pair HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    ),
                )
                .await;
                let paired: Value =
                    serde_json::from_str(&reply[reply.find("\r\n\r\n").unwrap() + 4..]).unwrap();
                let key = paired["key"].as_str().unwrap();
                let ping = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }).to_string();
                let reply = raw(
                    &pair.address,
                    format!(
                        "POST /mcp HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nAuthorization: Bearer {key}\r\nContent-Length: {}\r\n\r\n{ping}",
                        ping.len()
                    ),
                )
                .await;
                assert!(reply.starts_with("HTTP/1.1 401"), "{reply}");
            })
            .await;
            fs::remove_dir_all(&pair.place).ok();
        });
    }

    #[test]
    fn a_request_copied_off_the_network_changed_late_or_unsigned_is_turned_away() {
        block_on(async {
            let pair = both("replay").await;
            let engine = Other::default();
            with_door(&pair, &engine, async {
                paired(&pair).await;
                let install = pair.installs.all().remove(0);
                let ping = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }).to_string();
                let nonce = "a".repeat(64);
                let now = courier::seconds_now();
                let first = raw(&pair.address, signed_post(&install, &ping, &nonce, now)).await;
                assert!(first.starts_with("HTTP/1.1 200"), "{first}");
                let proof = handshake::proof(&install.secret, &nonce);
                assert!(first.contains(&format!("{}: {proof}\r\n", handshake::PROOF)));
                // The same request again: its number was seen.
                let again = raw(&pair.address, signed_post(&install, &ping, &nonce, now)).await;
                assert!(again.starts_with("HTTP/1.1 401"), "{again}");
                // Signed for one body, sent with another.
                let other = json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" }).to_string();
                let swapped =
                    signed_post(&install, &ping, &"b".repeat(64), now).replace(&ping, &other);
                assert!(
                    raw(&pair.address, swapped)
                        .await
                        .starts_with("HTTP/1.1 401")
                );
                // A clock three minutes off.
                let late = signed_post(&install, &ping, &"c".repeat(64), now - 180);
                assert!(raw(&pair.address, late).await.starts_with("HTTP/1.1 401"));
                // Another secret.
                let impostor = Install {
                    secret: handshake::secret_of("a guess"),
                    ..install.clone()
                };
                let guessed = signed_post(&impostor, &ping, &"d".repeat(64), now);
                assert!(
                    raw(&pair.address, guessed)
                        .await
                        .starts_with("HTTP/1.1 401")
                );
            })
            .await;
            fs::remove_dir_all(&pair.place).ok();
        });
    }

    #[test]
    fn an_answer_that_does_not_prove_itself_is_never_shown() {
        block_on(async {
            let place = folder("unproven");
            // A stand-in that answers like a door but holds no secret.
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let served = tauri::async_runtime::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buffer = vec![0u8; 64 * 1024];
                let _ = socket.read(&mut buffer).await;
                let body = json!({ "jsonrpc": "2.0", "id": "x",
                    "result": { "content": [{ "type": "text", "text": "Trust me." }], "isError": false } })
                .to_string();
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    handshake::PROOF,
                    "e".repeat(64),
                    body.len()
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
            });
            let installs = Installs::new(place.join("installs.json"));
            installs
                .add(Install {
                    id: "door-1".to_string(),
                    name: "Upstairs".to_string(),
                    address,
                    secret: handshake::secret_of("k"),
                    since: 0,
                })
                .unwrap();
            let lan = LanGate::new(place.join("lan-gate"));
            lan.gate().set(true).unwrap();
            let breaker = Courier::new(place.join("breaker"), place.join("mine.json"));
            breaker.set(true).unwrap();
            let (_stop, mut stopped) = watch::channel(false);
            let mut shown = String::new();
            let error = run_chat(
                &lan,
                &breaker.breaker(),
                &installs,
                &question("install:door-1/phi-4", &[], "Hello?"),
                &mut stopped,
                |event| {
                    if let ChatEvent::Answer { text } = event {
                        shown.push_str(&text);
                    }
                },
            )
            .await
            .unwrap_err();
            assert_eq!(error, NOT_PROVEN);
            assert!(shown.is_empty());
            served.await.unwrap();
            fs::remove_dir_all(&place).ok();
        });
    }

    #[test]
    fn stop_reaches_the_other_install_and_keeps_what_came() {
        block_on(async {
            let pair = both("stop").await;
            let engine = Other {
                waits: true,
                ..Other::default()
            };
            with_door(&pair, &engine, async {
                let view = paired(&pair).await;
                let (stop, mut stopped) = watch::channel(false);
                let request = question(&format!("install:{}/phi-4", view.id), &[], "Long one?");
                let shown = std::cell::RefCell::new(String::new());
                let breaker = pair.breaker.breaker();
                let asking = run_chat(
                    &pair.lan,
                    &breaker,
                    &pair.installs,
                    &request,
                    &mut stopped,
                    |event| {
                        if let ChatEvent::Answer { text } = event {
                            shown.borrow_mut().push_str(&text);
                        }
                    },
                );
                tokio::pin!(asking);
                let started = Instant::now();
                while !engine.stopped.load(Ordering::SeqCst) {
                    tokio::select! {
                        done = &mut asking => {
                            let done = done.unwrap();
                            assert!(done.stopped);
                            assert_eq!(done.answer, "Thinking it over");
                            break;
                        }
                        () = tokio::time::sleep(Duration::from_millis(200)) => {
                            let _ = stop.send(true);
                        }
                    }
                    assert!(started.elapsed() < Duration::from_secs(5), "never stopped");
                }
                // The other install's question stopped too.
                for _ in 0..50 {
                    if engine.stopped.load(Ordering::SeqCst) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                assert!(engine.stopped.load(Ordering::SeqCst));
                assert_eq!(*shown.borrow(), "Thinking it over");
            })
            .await;
            fs::remove_dir_all(&pair.place).ok();
        });
    }

    #[test]
    fn nothing_goes_with_the_breaker_or_the_lan_gate_off_or_with_a_picture() {
        block_on(async {
            let pair = both("off").await;
            let engine = Other::default();
            with_door(&pair, &engine, async {
                let view = paired(&pair).await;
                let chat_id = format!("install:{}/phi-4", view.id);
                pair.breaker.set(false).unwrap();
                let (done, _) = chat(&pair, &question(&chat_id, &[], "Hello?")).await;
                assert_eq!(done.unwrap_err(), BREAKER_OFF);
                let code = pair.door.start_pairing(Instant::now()).unwrap();
                let lan = &pair.lan;
                let refused = guarded(lan, &pair.breaker.breaker(), async {
                    super::pair(&pair.installs, &pair.address, &code, "Again").await
                })
                .await;
                assert_eq!(refused.unwrap_err(), BREAKER_OFF);
                pair.breaker.set(true).unwrap();
                pair.lan.gate().set(false).unwrap();
                let (done, _) = chat(&pair, &question(&chat_id, &[], "Hello?")).await;
                assert!(done.unwrap_err().starts_with("The LAN gate is off"));
                pair.lan.gate().set(true).unwrap();
                let mut picture = question(&chat_id, &[], "What is this?");
                picture.messages[0].images = vec!["data:image/png;base64,AAAA".to_string()];
                let (done, _) = chat(&pair, &picture).await;
                assert_eq!(done.unwrap_err(), vision::STAYS_HERE);
                // An install not paired here.
                let (done, _) = chat(&pair, &question("install:nobody/phi-4", &[], "Hi")).await;
                assert_eq!(done.unwrap_err(), GONE);
                // Unpaired here, it is gone.
                pair.installs.remove(&view.id).unwrap();
                let (done, _) = chat(&pair, &question(&chat_id, &[], "Hi")).await;
                assert_eq!(done.unwrap_err(), GONE);
            })
            .await;
            fs::remove_dir_all(&pair.place).ok();
        });
    }

    #[test]
    fn an_address_is_numbers_on_the_private_network() {
        assert_eq!(
            checked_address("192.168.1.20").unwrap(),
            "192.168.1.20:4710"
        );
        assert_eq!(
            checked_address("http://10.0.0.5:4800/mcp").unwrap(),
            "10.0.0.5:4800"
        );
        assert_eq!(checked_address("[fd00::5]").unwrap(), "[fd00::5]:4710");
        for bad in [
            "8.8.8.8",
            "upstairs.local",
            "example.com:4710",
            "0.0.0.0",
            "192.168.1.20:0",
            "",
            "https://192.168.1.20",
        ] {
            assert!(checked_address(bad).is_err(), "{bad}");
        }
        assert_eq!(
            split_id("install:ab-1/maker/model"),
            Some(("ab-1", "maker/model"))
        );
        assert_eq!(split_id("install:/model"), None);
        assert_eq!(split_id("rig:m1"), None);
        assert!(is_install("install:x/y") && !is_install("rig:m1"));
    }
}
