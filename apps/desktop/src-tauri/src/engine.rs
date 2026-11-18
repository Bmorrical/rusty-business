//! The engine adapter.
//!
//! The app speaks one contract: an OpenAI-style server on this computer
//! (`GET /models`, `POST /chat/completions` with a streamed reply). Which
//! program serves it is the only thing that changes per system: on a Mac it
//! is `mlx_lm.server`; on Windows it will be another local server behind the
//! same two calls. Nothing above this file knows the difference.
//!
//! Three rules live here:
//! - a prompt to the engine never leaves the computer (loopback addresses
//!   only, no proxy, no redirects), so a chat needs no outbound gate;
//! - only models on the list below are offered or accepted;
//! - a model's thinking is separated from its answer, so the window never
//!   shows raw markers.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tokio::sync::watch;

use crate::makers::Makers;

pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8080/v1";
const DEFAULT_MAX_TOKENS: u32 = 4096;

// ---------------------------------------------------------------------------
// Where the engine is
// ---------------------------------------------------------------------------

/// The engine's address: `RUSTY_ENGINE_URL` when set, the default otherwise.
pub fn base_url() -> Result<String, String> {
    let raw = std::env::var("RUSTY_ENGINE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    checked_base_url(&raw)
}

/// Accepts only a plain-http address on this computer.
pub fn checked_base_url(raw: &str) -> Result<String, String> {
    let refusal = || format!("The engine address must be on this computer (for example {DEFAULT_BASE_URL}).");
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| refusal())?;
    if url.scheme() != "http" || !url.username().is_empty() || url.password().is_some() {
        return Err(refusal());
    }
    let host = url.host_str().unwrap_or_default();
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let local = host == "localhost" || bare.parse::<IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false);
    if !local {
        return Err(refusal());
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

pub fn engine_name() -> &'static str {
    engine_name_on(std::env::consts::OS)
}

/// The engine's name on `os`, one of Rust's names for a system ("macos",
/// "windows", "linux"). Taking the system lets the Mac's tests check the
/// words every other system gets.
fn engine_name_on(os: &str) -> &'static str {
    if os == "macos" {
        "MLX"
    } else {
        "Local engine"
    }
}

/// How to start the engine by hand on `os`. Windows and Linux have no
/// engine chosen yet, so they get no hint, and the window shows none.
fn start_hint_on(os: &str) -> &'static str {
    match os {
        "macos" => "mlx_lm.server --port 8080",
        "windows" | "linux" => "",
        _ => "Start an OpenAI-style server on port 8080 of this computer.",
    }
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|error| format!("Could not set up the connection: {error}"))
}

// ---------------------------------------------------------------------------
// Which models are offered
// ---------------------------------------------------------------------------

struct CatalogEntry {
    /// Lowercase text the engine's model id must contain.
    needle: &'static str,
    name: &'static str,
    maker: &'static str,
    license: &'static str,
    context: u32,
}

/// The allow-list. A model the engine holds that is not here is never shown
/// and never accepted. The first one the engine has is the default. The
/// person picks which makers' models they see (`makers.rs`).
const CATALOG: &[CatalogEntry] = &[
    CatalogEntry { needle: "phi-4", name: "Phi-4", maker: "Microsoft", license: "MIT", context: 16_384 },
    CatalogEntry { needle: "gpt-oss-20b", name: "gpt-oss 20B", maker: "OpenAI", license: "Apache 2.0", context: 131_072 },
    CatalogEntry { needle: "nemotron-3.5-lightning", name: "Nemotron 3.5 Lightning", maker: "NVIDIA", license: "OpenMDW 1.1", context: 262_144 },
    CatalogEntry { needle: "gemma-4-26b-a4b", name: "Gemma 4 26B", maker: "Google", license: "Apache 2.0", context: 262_144 },
    CatalogEntry { needle: "llama-3.1-8b-instruct", name: "Llama 3.1 8B", maker: "Meta", license: "Llama 3.1 Community License", context: 131_072 },
];

/// The makers on the list, in the list's order, each once.
pub fn makers() -> Vec<&'static str> {
    let mut makers: Vec<&'static str> = Vec::new();
    for entry in CATALOG {
        if !makers.contains(&entry.maker) {
            makers.push(entry.maker);
        }
    }
    makers
}

/// A maker's models on the list, each with its license.
pub fn models_by(maker: &str) -> Vec<(&'static str, &'static str)> {
    CATALOG.iter().filter(|entry| entry.maker == maker).map(|entry| (entry.name, entry.license)).collect()
}

/// The license of a model on the list, by its name.
pub fn license_of(name: &str) -> Option<&'static str> {
    CATALOG.iter().find(|entry| entry.name == name).map(|entry| entry.license)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub maker: String,
    pub context: u32,
}

fn catalog_entry(id: &str) -> Option<&'static CatalogEntry> {
    let lower = id.to_lowercase();
    CATALOG.iter().find(|entry| lower.contains(entry.needle))
}

/// The engine's models that are on the list, in the list's order.
pub fn allowed_models(ids: &[String]) -> Vec<ModelInfo> {
    let mut found = Vec::new();
    for entry in CATALOG {
        for id in ids {
            if id.to_lowercase().contains(entry.needle) && !found.iter().any(|model: &ModelInfo| &model.id == id) {
                found.push(ModelInfo {
                    id: id.clone(),
                    name: entry.name.to_string(),
                    maker: entry.maker.to_string(),
                    context: entry.context,
                });
            }
        }
    }
    found
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub name: String,
    pub url: String,
    pub reachable: bool,
    pub models: Vec<ModelInfo>,
    /// Models the engine holds from makers the person did not pick. The page
    /// never offers them, but a saved chat still names the model that wrote it.
    pub set_aside: Vec<ModelInfo>,
    /// Models the engine holds that are not on the list.
    pub hidden: usize,
    /// Models on the list that are on this computer's disk, whether or not
    /// the engine is answering.
    pub on_disk: Vec<ModelInfo>,
    /// Models the app can download, with their sizes. The page shows them
    /// only while none of the app's models is on the disk.
    pub offers: Vec<crate::downloads::Offer>,
    pub error: Option<String>,
    pub start_hint: String,
}

#[derive(Deserialize)]
struct ModelList {
    #[serde(default)]
    data: Vec<ModelRow>,
}

#[derive(Deserialize)]
struct ModelRow {
    id: String,
}

pub(crate) async fn fetch_model_ids(base: &str) -> Result<Vec<String>, String> {
    let response = client()?
        .get(format!("{base}/models"))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|_| "The engine is not answering.".to_string())?;
    if !response.status().is_success() {
        return Err(format!("The engine answered {}.", response.status().as_u16()));
    }
    let list: ModelList = response
        .json()
        .await
        .map_err(|_| "The engine's model list could not be read.".to_string())?;
    Ok(list.data.into_iter().map(|row| row.id).collect())
}

/// What the page is told before the engine is asked: the app's models on
/// this computer's disk, and the ones it can download.
fn first_status() -> EngineStatus {
    EngineStatus {
        name: engine_name().to_string(),
        url: DEFAULT_BASE_URL.to_string(),
        reachable: false,
        models: Vec::new(),
        set_aside: Vec::new(),
        hidden: 0,
        on_disk: crate::models::on_disk(),
        offers: crate::downloads::offers(),
        error: None,
        start_hint: start_hint_on(std::env::consts::OS).to_string(),
    }
}

/// Leaves in only the models of the makers the person picked: on the
/// engine, on the disk, and offered. A model of a maker not picked is still
/// on the list, so it is not counted as hidden; the engine's are set aside.
fn for_makers(mut status: EngineStatus, makers: &Makers) -> EngineStatus {
    let served = status.models.clone();
    makers.keep_picked(&mut status.models, |model| &model.maker);
    status.set_aside = served.into_iter().filter(|model| !status.models.contains(model)).collect();
    makers.keep_picked(&mut status.on_disk, |model| &model.maker);
    makers.keep_picked(&mut status.offers, |offer| &offer.maker);
    status
}

#[tauri::command]
pub async fn engine_status(makers: tauri::State<'_, Makers>) -> Result<EngineStatus, String> {
    Ok(for_makers(status_now().await, &makers))
}

async fn status_now() -> EngineStatus {
    let mut status = first_status();
    let base = match base_url() {
        Ok(base) => base,
        Err(error) => {
            status.error = Some(error);
            return status;
        }
    };
    status.url = base.clone();
    match fetch_model_ids(&base).await {
        Ok(ids) => {
            status.reachable = true;
            status.models = allowed_models(&ids);
            status.hidden = ids.len().saturating_sub(status.models.len());
        }
        Err(error) => status.error = Some(error),
    }
    status
}

// ---------------------------------------------------------------------------
// Thinking and answer, kept apart
// ---------------------------------------------------------------------------

const CHANNEL: &str = "<|channel|>";
const MESSAGE: &str = "<|message|>";
const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Split {
    pub thinking: String,
    pub answer: String,
}

/// While a reply is still arriving, a marker can be cut in half at the end
/// of what has come so far. Leave that half out until the rest arrives.
fn hold_back(raw: &str) -> &str {
    let Some(position) = raw.rfind('<') else {
        return raw;
    };
    let tail = &raw[position..];
    if tail.len() > 24 || tail.contains('>') {
        return raw;
    }
    let half_think = THINK_OPEN.starts_with(tail) || THINK_CLOSE.starts_with(tail);
    if half_think || tail.starts_with("<|") {
        &raw[..position]
    } else {
        raw
    }
}

/// Channel markers (gpt-oss): `<|channel|>NAME<|message|>TEXT<|end|>`, where
/// the channel named `final` is the answer and every other one is thinking.
fn split_channels(raw: &str) -> Split {
    let mut split = Split::default();
    let first = raw.find(CHANNEL).unwrap_or(raw.len());
    let before = &raw[..first];
    if !before.contains("<|") {
        split.answer.push_str(before.trim_start());
    }
    let mut rest = &raw[first..];
    while let Some(position) = rest.find(CHANNEL) {
        let after = &rest[position + CHANNEL.len()..];
        let Some(message_at) = after.find(MESSAGE) else {
            break;
        };
        let name = after[..message_at].trim();
        let body_onward = &after[message_at + MESSAGE.len()..];
        let end = body_onward.find("<|").unwrap_or(body_onward.len());
        let body = &body_onward[..end];
        if name.starts_with("final") {
            split.answer.push_str(body);
        } else if !body.is_empty() {
            if !split.thinking.is_empty() {
                split.thinking.push_str("\n\n");
            }
            split.thinking.push_str(body);
        }
        rest = &body_onward[end..];
    }
    split
}

/// Think tags: `<think>…</think>` before the answer. Some models leave the
/// opening tag out, so a closing tag alone still marks the thinking's end.
fn split_think_tags(raw: &str) -> Split {
    if let Some(close) = raw.find(THINK_CLOSE) {
        let before = raw[..close].trim_start();
        let thinking = before.strip_prefix(THINK_OPEN).unwrap_or(before);
        return Split {
            thinking: thinking.trim().to_string(),
            answer: raw[close + THINK_CLOSE.len()..].trim_start().to_string(),
        };
    }
    let trimmed = raw.trim_start();
    if let Some(thinking) = trimmed.strip_prefix(THINK_OPEN) {
        return Split { thinking: thinking.trim_start().to_string(), answer: String::new() };
    }
    Split { thinking: String::new(), answer: trimmed.to_string() }
}

/// Splits everything a model has written so far. `done` means no more is
/// coming, so nothing is held back.
pub fn split_reply(raw: &str, done: bool) -> Split {
    let raw = if done { raw } else { hold_back(raw) };
    if raw.contains(CHANNEL) {
        split_channels(raw)
    } else {
        split_think_tags(raw)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatEvent {
    /// More thinking text.
    Thinking { text: String },
    /// More answer text.
    Answer { text: String },
    /// What was sent so far was wrong (a late marker changed the split):
    /// replace it with this.
    Reset { thinking: String, answer: String },
}

/// Turns the engine's raw pieces into thinking and answer pieces.
#[derive(Default)]
pub struct ReplyStream {
    raw: String,
    reasoning: String,
    sent: Split,
}

impl ReplyStream {
    pub fn push_content(&mut self, text: &str) -> Vec<ChatEvent> {
        self.raw.push_str(text);
        self.changes(false)
    }

    /// Engines that separate thinking themselves send it in its own field.
    pub fn push_reasoning(&mut self, text: &str) -> Vec<ChatEvent> {
        self.reasoning.push_str(text);
        self.changes(false)
    }

    pub fn finish(&mut self) -> Vec<ChatEvent> {
        self.changes(true)
    }

    pub fn split(&self) -> &Split {
        &self.sent
    }

    fn changes(&mut self, done: bool) -> Vec<ChatEvent> {
        let mut now = split_reply(&self.raw, done);
        if !self.reasoning.is_empty() {
            now.thinking = if now.thinking.is_empty() {
                self.reasoning.clone()
            } else {
                format!("{}\n\n{}", self.reasoning, now.thinking)
            };
        }
        let mut events = Vec::new();
        match (now.thinking.strip_prefix(self.sent.thinking.as_str()), now.answer.strip_prefix(self.sent.answer.as_str())) {
            (Some(more_thinking), Some(more_answer)) => {
                if !more_thinking.is_empty() {
                    events.push(ChatEvent::Thinking { text: more_thinking.to_string() });
                }
                if !more_answer.is_empty() {
                    events.push(ChatEvent::Answer { text: more_answer.to_string() });
                }
            }
            _ => events.push(ChatEvent::Reset { thinking: now.thinking.clone(), answer: now.answer.clone() }),
        }
        self.sent = now;
        events
    }
}

// ---------------------------------------------------------------------------
// Reading the stream
// ---------------------------------------------------------------------------

/// Collects bytes and hands back each complete `data:` line's payload. Bytes
/// are kept until a whole line is in, so a character cut across two network
/// pieces is never decoded in halves.
#[derive(Default)]
pub struct SseLines {
    buffer: Vec<u8>,
}

impl SseLines {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(bytes);
        let mut payloads = Vec::new();
        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=newline).collect();
            let line = String::from_utf8_lossy(&line);
            if let Some(payload) = line.trim().strip_prefix("data:") {
                payloads.push(payload.trim().to_string());
            }
        }
        payloads
    }
}

#[derive(Deserialize)]
struct Chunk {
    #[serde(default)]
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    delta: Delta,
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct Delta {
    content: Option<String>,
    reasoning: Option<String>,
    reasoning_content: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
}

// ---------------------------------------------------------------------------
// One chat turn
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatDone {
    pub thinking: String,
    pub answer: String,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub finish_reason: Option<String>,
    /// True when the person pressed Stop.
    pub stopped: bool,
    /// Seconds from the first piece of the reply to the last.
    pub seconds: f64,
}

/// Chats in flight, so one can be stopped by its id.
#[derive(Default)]
pub struct Chats(Mutex<HashMap<String, watch::Sender<bool>>>);

fn checked_request(request: &ChatRequest) -> Result<(), String> {
    if catalog_entry(&request.model).is_none() {
        return Err("That model is not on this app's list.".to_string());
    }
    if request.messages.is_empty() {
        return Err("There is nothing to send.".to_string());
    }
    for message in &request.messages {
        if !matches!(message.role.as_str(), "system" | "user" | "assistant") {
            return Err("A message has a role the engine does not take.".to_string());
        }
    }
    Ok(())
}

pub async fn run_chat(
    request: &ChatRequest,
    cancel: &mut watch::Receiver<bool>,
    mut emit: impl FnMut(ChatEvent),
) -> Result<ChatDone, String> {
    checked_request(request)?;
    let base = base_url()?;
    let body = serde_json::json!({
        "model": request.model,
        "messages": request.messages,
        "stream": true,
        "stream_options": { "include_usage": true },
        "max_tokens": request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
    });
    let sending = client()?.post(format!("{base}/chat/completions")).json(&body).send();
    let response = tokio::select! {
        _ = cancel.changed() => return Ok(ChatDone { stopped: true, ..ChatDone::default() }),
        response = sending => response.map_err(|_| "The engine is not answering.".to_string())?,
    };
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let detail: String = response.text().await.unwrap_or_default().chars().take(300).collect();
        return Err(format!("The engine answered {status}. {detail}").trim().to_string());
    }

    let mut bytes = response.bytes_stream();
    let mut lines = SseLines::default();
    let mut reply = ReplyStream::default();
    let mut done = ChatDone::default();
    let mut first_piece: Option<Instant> = None;
    let mut last_piece = Instant::now();

    'stream: loop {
        let next = tokio::select! {
            _ = cancel.changed() => {
                done.stopped = true;
                break 'stream;
            }
            next = bytes.next() => next,
        };
        let piece = match next {
            None => break 'stream,
            Some(Err(_)) => return Err("The engine stopped answering part way through.".to_string()),
            Some(Ok(piece)) => piece,
        };
        for payload in lines.push(&piece) {
            if payload == "[DONE]" {
                break 'stream;
            }
            let Ok(chunk) = serde_json::from_str::<Chunk>(&payload) else {
                continue;
            };
            if let Some(usage) = chunk.usage {
                done.prompt_tokens = usage.prompt_tokens.or(done.prompt_tokens);
                done.completion_tokens = usage.completion_tokens.or(done.completion_tokens);
            }
            for choice in chunk.choices {
                if choice.finish_reason.is_some() {
                    done.finish_reason = choice.finish_reason;
                }
                let mut events = Vec::new();
                if let Some(text) = choice.delta.reasoning.or(choice.delta.reasoning_content) {
                    events.extend(reply.push_reasoning(&text));
                }
                if let Some(text) = choice.delta.content {
                    events.extend(reply.push_content(&text));
                }
                if !events.is_empty() {
                    first_piece.get_or_insert_with(Instant::now);
                    last_piece = Instant::now();
                }
                events.into_iter().for_each(&mut emit);
            }
        }
    }

    reply.finish().into_iter().for_each(&mut emit);
    let split = reply.split();
    done.thinking = split.thinking.trim().to_string();
    done.answer = split.answer.trim().to_string();
    done.seconds = first_piece.map(|first| last_piece.duration_since(first).as_secs_f64()).unwrap_or(0.0);
    Ok(done)
}

#[tauri::command]
pub async fn chat_stream(
    chats: tauri::State<'_, Chats>,
    chat_id: String,
    request: ChatRequest,
    on_event: Channel<ChatEvent>,
) -> Result<ChatDone, String> {
    let (stop, mut stopped) = watch::channel(false);
    chats.0.lock().map_err(|_| "Could not start the chat.".to_string())?.insert(chat_id.clone(), stop);
    let result = run_chat(&request, &mut stopped, |event| {
        let _ = on_event.send(event);
    })
    .await;
    if let Ok(mut map) = chats.0.lock() {
        map.remove(&chat_id);
    }
    result
}

#[tauri::command]
pub fn stop_chat(chats: tauri::State<'_, Chats>, chat_id: String) {
    if let Ok(map) = chats.0.lock() {
        if let Some(stop) = map.get(&chat_id) {
            let _ = stop.send(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // What gpt-oss really sent for "What is 2+2? One short sentence."
    const GPT_OSS_REPLY: &str = "<|channel|>analysis<|message|>The user asks: \"What is 2+2?\" So answer: \"2+2 equals 4.\"<|end|><|start|>assistant<|channel|>final<|message|>2\u{202f}+\u{202f}2 equals 4.";

    fn replay(pieces: impl IntoIterator<Item = String>) -> (Vec<ChatEvent>, Split) {
        let mut reply = ReplyStream::default();
        let mut events = Vec::new();
        for piece in pieces {
            events.extend(reply.push_content(&piece));
        }
        events.extend(reply.finish());
        (events, reply.split().clone())
    }

    fn rebuilt(events: &[ChatEvent]) -> Split {
        let mut split = Split::default();
        for event in events {
            match event {
                ChatEvent::Thinking { text } => split.thinking.push_str(text),
                ChatEvent::Answer { text } => split.answer.push_str(text),
                ChatEvent::Reset { thinking, answer } => {
                    split.thinking = thinking.clone();
                    split.answer = answer.clone();
                }
            }
        }
        split
    }

    #[test]
    fn channel_markers_split_thinking_from_the_answer() {
        let split = split_reply(GPT_OSS_REPLY, true);
        assert_eq!(split.answer, "2\u{202f}+\u{202f}2 equals 4.");
        assert_eq!(split.thinking, "The user asks: \"What is 2+2?\" So answer: \"2+2 equals 4.\"");
    }

    #[test]
    fn a_reply_arriving_one_character_at_a_time_never_shows_a_marker() {
        let (events, split) = replay(GPT_OSS_REPLY.chars().map(String::from));
        for event in &events {
            let shown = match event {
                ChatEvent::Thinking { text } | ChatEvent::Answer { text } => text.clone(),
                ChatEvent::Reset { thinking, answer } => format!("{thinking}{answer}"),
            };
            assert!(!shown.contains("<|") && !shown.contains("|>"), "a marker reached the window: {shown:?}");
        }
        assert_eq!(rebuilt(&events), split);
        assert_eq!(split.answer, "2\u{202f}+\u{202f}2 equals 4.");
    }

    #[test]
    fn think_tags_split_thinking_from_the_answer() {
        let (events, split) = replay("<think>\nAdd them.\n</think>\n\n4.".chars().map(String::from));
        assert_eq!(split, Split { thinking: "Add them.".to_string(), answer: "4.".to_string() });
        assert_eq!(rebuilt(&events), split);
    }

    #[test]
    fn a_closing_think_tag_alone_corrects_what_was_shown() {
        let (events, split) = replay(["Add them.".to_string(), "</think>4.".to_string()]);
        assert!(events.iter().any(|event| matches!(event, ChatEvent::Reset { .. })));
        assert_eq!(split, Split { thinking: "Add them.".to_string(), answer: "4.".to_string() });
        assert_eq!(rebuilt(&events), split);
    }

    #[test]
    fn the_reasoning_field_is_thinking() {
        let mut reply = ReplyStream::default();
        let mut events = reply.push_reasoning("Add ");
        events.extend(reply.push_reasoning("them."));
        events.extend(reply.push_content("4."));
        events.extend(reply.finish());
        assert_eq!(reply.split(), &Split { thinking: "Add them.".to_string(), answer: "4.".to_string() });
        assert_eq!(rebuilt(&events), *reply.split());
    }

    #[test]
    fn plain_text_with_angle_brackets_passes_through() {
        let text = "1 < 2, 3 > 2, and a <div> is a tag.";
        let (events, split) = replay(text.chars().map(String::from));
        assert_eq!(split.answer, text);
        assert_eq!(split.thinking, "");
        assert_eq!(rebuilt(&events), split);
    }

    #[test]
    fn only_listed_models_are_offered() {
        let ids: Vec<String> = [
            "mlx-community/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-4bit",
            "mlx-community/phi-4-4bit",
            "mlx-community/gemma-3-27b-it-qat-4bit",
            "mlx-community/gpt-oss-20b-MXFP4-Q8",
            "mlx-community/Llama-3.1-8B-Instruct-4bit",
            "mlx-community/Qwen3-30B-A3B-4bit",
            "mlx-community/gemma-4-26b-a4b-it-4bit",
        ]
        .iter()
        .map(|id| id.to_string())
        .collect();
        let models = allowed_models(&ids);
        let makers: Vec<&str> = models.iter().map(|model| model.maker.as_str()).collect();
        assert_eq!(makers, ["Microsoft", "OpenAI", "NVIDIA", "Google", "Meta"]);
        let names: Vec<&str> = models.iter().map(|model| model.name.as_str()).collect();
        assert_eq!(names, ["Phi-4", "gpt-oss 20B", "Nemotron 3.5 Lightning", "Gemma 4 26B", "Llama 3.1 8B"]);
        // Only the model named on the list: not an older Gemma, and no model
        // from a maker that is not on it.
        assert!(models.iter().all(|model| !model.id.contains("gemma-3") && !model.id.contains("Qwen")));
    }

    // Brad, 2026-11-17: "user can pick google, gemini, meta from the app if
    // they want those instead." Gemma is Google's model that runs here; its
    // hosted one comes with the outside engines.
    #[test]
    fn the_list_names_each_maker_once_with_each_models_license() {
        assert_eq!(makers(), ["Microsoft", "OpenAI", "NVIDIA", "Google", "Meta"]);
        assert_eq!(models_by("Microsoft"), [("Phi-4", "MIT")]);
        assert_eq!(models_by("OpenAI"), [("gpt-oss 20B", "Apache 2.0")]);
        assert_eq!(models_by("NVIDIA"), [("Nemotron 3.5 Lightning", "OpenMDW 1.1")]);
        assert_eq!(models_by("Google"), [("Gemma 4 26B", "Apache 2.0")]);
        assert_eq!(models_by("Meta"), [("Llama 3.1 8B", "Llama 3.1 Community License")]);
        assert_eq!(models_by("Acme"), []);
        assert_eq!(license_of("Gemma 4 26B"), Some("Apache 2.0"));
        assert_eq!(license_of("Gemma 3 27B"), None);
    }

    #[test]
    fn a_model_off_the_list_is_refused() {
        let request = ChatRequest {
            model: "mlx-community/gemma-3-27b-it-qat-4bit".to_string(),
            messages: vec![ChatMessage { role: "user".to_string(), content: "hi".to_string() }],
            max_tokens: None,
        };
        assert!(checked_request(&request).is_err());
    }

    #[test]
    fn windows_and_linux_get_no_command_to_start_an_engine_they_do_not_have() {
        // Brad, 2026-11-16: "I should be able to just pull the .dmg or .exe
        // from read me and install and go, one click ready." The roadmap, of
        // the .exe: "Until Windows has an engine, the window opens and says
        // so." Brad, 2026-11-17, to a request for Linux downloads: "I got
        // you in coming build". Windows and Linux get no command to run; the
        // Mac keeps its own.
        assert_eq!(start_hint_on("windows"), "");
        assert_eq!(start_hint_on("linux"), "");
        assert_eq!(start_hint_on("macos"), "mlx_lm.server --port 8080");
        assert_eq!(start_hint_on("freebsd"), "Start an OpenAI-style server on port 8080 of this computer.");
        assert_eq!(engine_name_on("macos"), "MLX");
        assert_eq!(engine_name_on("windows"), "Local engine");
        assert_eq!(engine_name_on("linux"), "Local engine");
        let first = first_status();
        assert_eq!(first.start_hint, start_hint_on(std::env::consts::OS));
        assert_eq!(first.name, engine_name_on(std::env::consts::OS));
    }

    #[test]
    fn the_report_carries_the_models_the_app_can_download() {
        let offered = first_status().offers;
        assert_eq!(offered, crate::downloads::offers());
        if cfg!(target_os = "macos") {
            let names: Vec<&str> = offered.iter().map(|offer| offer.name.as_str()).collect();
            assert_eq!(names, ["Phi-4", "gpt-oss 20B", "Nemotron 3.5 Lightning", "Gemma 4 26B", "Llama 3.1 8B"]);
        }
    }

    #[test]
    fn only_the_picked_makers_models_are_reported() {
        // Brad, 2026-11-17: "user can pick google, gemini, meta from the app
        // if they want those instead."
        let folder = std::env::temp_dir().join(format!("rusty-engine-makers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let makers = Makers::new(folder.join("makers"), &[]);
        makers.set(&["Google".to_string(), "Meta".to_string()]).unwrap();
        let model = |name: &str, maker: &str| ModelInfo { id: format!("someone/{name}"), name: name.to_string(), maker: maker.to_string(), context: 8 };
        let offer = |name: &str, maker: &str| crate::downloads::Offer {
            id: format!("someone/{name}"),
            name: name.to_string(),
            maker: maker.to_string(),
            license: "MIT".to_string(),
            bytes: 1,
        };
        let mut status = first_status();
        status.models = vec![model("Phi-4", "Microsoft"), model("Gemma 4 26B", "Google")];
        status.hidden = 2;
        status.on_disk = vec![model("Phi-4", "Microsoft"), model("Llama 3.1 8B", "Meta")];
        status.offers = vec![offer("Gemma 4 26B", "Google"), offer("gpt-oss 20B", "OpenAI"), offer("Llama 3.1 8B", "Meta")];
        let shown = for_makers(status, &makers);
        let names = |models: &[ModelInfo]| models.iter().map(|model| model.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&shown.models), ["Gemma 4 26B"]);
        assert_eq!(names(&shown.set_aside), ["Phi-4"], "a saved chat still names the model that wrote it");
        assert_eq!(names(&shown.on_disk), ["Llama 3.1 8B"]);
        let offered: Vec<&str> = shown.offers.iter().map(|offer| offer.name.as_str()).collect();
        assert_eq!(offered, ["Gemma 4 26B", "Llama 3.1 8B"]);
        assert_eq!(shown.hidden, 2, "a model of a maker not picked is on the list, so it is not hidden");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_engine_address_stays_on_this_computer() {
        for good in ["http://127.0.0.1:8080/v1", "http://localhost:8080/v1/", "http://[::1]:9000/v1"] {
            assert!(checked_base_url(good).is_ok(), "{good} should be accepted");
        }
        for bad in [
            "https://127.0.0.1:8080/v1",
            "http://example.com/v1",
            "http://10.0.0.5:8080/v1",
            "http://127.0.0.1.example.com/v1",
            "http://user:pass@127.0.0.1:8080/v1",
            "file:///etc/hosts",
            "not an address",
        ] {
            assert!(checked_base_url(bad).is_err(), "{bad} should be refused");
        }
        assert_eq!(checked_base_url("http://localhost:8080/v1/").unwrap(), "http://localhost:8080/v1");
    }

    #[test]
    fn stream_lines_survive_being_cut_anywhere() {
        let stream = ": keepalive 1/1\n\ndata: {\"text\": \"2\u{202f}+\u{202f}2\"}\n\ndata: [DONE]\n\n".as_bytes();
        for cut in 1..stream.len() {
            let mut lines = SseLines::default();
            let mut payloads = lines.push(&stream[..cut]);
            payloads.extend(lines.push(&stream[cut..]));
            assert_eq!(payloads, ["{\"text\": \"2\u{202f}+\u{202f}2\"}", "[DONE]"], "cut at {cut}");
        }
    }

    #[test]
    fn events_carry_their_kind() {
        let json = serde_json::to_value(ChatEvent::Answer { text: "4".to_string() }).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "answer", "text": "4" }));
    }

    /// Needs the engine running with Phi-4 on hand: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_two_plus_two() {
        let ids = tauri::async_runtime::block_on(fetch_model_ids(&base_url().unwrap())).expect("the engine is not answering");
        let model = allowed_models(&ids).into_iter().next().expect("no listed model on the engine");
        let request = ChatRequest {
            model: model.id,
            messages: vec![ChatMessage { role: "user".to_string(), content: "What is 2+2? One short sentence.".to_string() }],
            max_tokens: Some(200),
        };
        let (_stop, mut stopped) = watch::channel(false);
        let mut events = Vec::new();
        let done = tauri::async_runtime::block_on(run_chat(&request, &mut stopped, |event| events.push(event))).unwrap();
        assert!(done.answer.contains('4'), "answer was {:?}", done.answer);
        assert!(!done.answer.contains("<|"));
        assert_eq!(rebuilt(&events).answer.trim(), done.answer);
        assert!(done.completion_tokens.unwrap_or(0) > 0);
    }
}
