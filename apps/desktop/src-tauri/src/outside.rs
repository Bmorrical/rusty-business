//! Engines outside this computer: Grok by xAI, GPT by OpenAI, and Claude by
//! Anthropic.
//!
//! It answers these asks: "user can add their outside inference, start with
//! Grok and Grok ai. user adds it manually, they will need to provide or
//! specify a path to a key, now, the Rusty Business™ can query Grok for the
//! same question. ... you can use grok by it's self too." And, of the engines
//! after Grok: "and claude and codex/gpt and gemmini".
//!
//! The rules, each with a test, are the same for every engine:
//!
//! - The person gives the path to a file that holds their key from the
//!   engine's maker. The app keeps the path, never the key. Rust reads the key
//!   from the file each time it asks, and the key never reaches the page, the
//!   app's own files, or a message.
//! - An engine is asked only while the outbound gate is on, over https, at its
//!   maker's API alone, an address no setting changes. The gate is looked at
//!   before the key is read, and turning it off stops an answer under way.
//! - Only an engine's models for text, as its maker lists them, are offered,
//!   and a model is asked only of the engine that listed it. Each answers
//!   alone or as one of the models asked, and waits for no turn: it is not on
//!   this computer.
//! - Each engine keeps its own path and models. Removing one forgets its path
//!   and leaves the others as they were. The key file is never touched.

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

use crate::engine::{self, ChatDone, ChatEvent, ChatRequest, ModelInfo, ReplyStream, Taken};
use crate::gate::Gate;

/// The one host asked for each engine, and its API.
pub const XAI_HOST: &str = "api.x.ai";
const XAI_BASE: &str = "https://api.x.ai/v1";
pub const OPENAI_HOST: &str = "api.openai.com";
const OPENAI_BASE: &str = "https://api.openai.com/v1";
pub const ANTHROPIC_HOST: &str = "api.anthropic.com";
const ANTHROPIC_BASE: &str = "https://api.anthropic.com/v1";

/// The version of Anthropic's API the app speaks, sent with every call.
const ANTHROPIC_VERSION: &str = "2026-10-21";

const UNKNOWN: &str = "The app does not know that outside engine.";
const NOT_OUTSIDE: &str = "That model is not one an outside engine listed.";

/// How long the app waits to reach an engine. A reasoning model can think for
/// minutes, so an answer has no overall limit; Stop and the gate end it.
const CONNECT_WAIT: Duration = Duration::from_secs(10);

/// How long a list of models may take, whole.
const LIST_WAIT: Duration = Duration::from_secs(30);

/// The most of a list the app reads.
const MOST_BYTES: usize = 1_000_000;

/// The longest path taken, and the biggest key file read.
const PATH_MOST: usize = 4096;
const KEY_FILE_MOST: u64 = 1024;

/// A key's length, from the shortest to the longest taken. OpenAI's project
/// keys run past 160 characters.
const KEY_SHORTEST: usize = 20;
const KEY_LONGEST: usize = 256;

/// The words in an OpenAI model's id that mark it as one that does not answer
/// text in a chat: sound, pictures, search, tools of its own, or another API.
const NOT_FOR_CHAT: [&str; 13] =
    ["audio", "realtime", "tts", "transcribe", "image", "search", "instruct", "codex", "pro", "deep", "research", "embedding", "moderation"];

// ---------------------------------------------------------------------------
// The engines
// ---------------------------------------------------------------------------

/// An engine outside this computer, asked at its maker's API alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Engine {
    Xai,
    OpenAi,
    Anthropic,
}

impl Engine {
    /// Every one, in the order the page shows them.
    const ALL: [Self; 3] = [Self::Xai, Self::OpenAi, Self::Anthropic];

    /// The page's name for it.
    fn id(self) -> &'static str {
        match self {
            Self::Xai => "xai",
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
        }
    }

    /// What the page's ids for its models start with.
    fn prefix(self) -> &'static str {
        match self {
            Self::Xai => "xai:",
            Self::OpenAi => "openai:",
            Self::Anthropic => "anthropic:",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Xai => "Grok",
            Self::OpenAi => "GPT",
            Self::Anthropic => "Claude",
        }
    }

    fn maker(self) -> &'static str {
        match self {
            Self::Xai => "xAI",
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
        }
    }

    fn host(self) -> &'static str {
        match self {
            Self::Xai => XAI_HOST,
            Self::OpenAi => OPENAI_HOST,
            Self::Anthropic => ANTHROPIC_HOST,
        }
    }

    fn base(self) -> &'static str {
        match self {
            Self::Xai => XAI_BASE,
            Self::OpenAi => OPENAI_BASE,
            Self::Anthropic => ANTHROPIC_BASE,
        }
    }

    /// The most models kept from its maker's list. OpenAI and Anthropic list
    /// years of models, so only their newest are kept.
    fn most_models(self) -> usize {
        match self {
            Self::Xai => 30,
            Self::OpenAi | Self::Anthropic => 12,
        }
    }

    /// The call with the person's key on it, the way its maker asks for it.
    /// xAI and OpenAI take it as a bearer token; Anthropic takes it in its own
    /// header, with the version of its API.
    fn signed(self, builder: reqwest::RequestBuilder, key: &Key) -> reqwest::RequestBuilder {
        match self {
            Self::Xai | Self::OpenAi => builder.bearer_auth(&key.0),
            // read_key takes printable ASCII alone, so the key always makes a
            // header. It is marked sensitive, so nothing prints it.
            Self::Anthropic => match reqwest::header::HeaderValue::from_str(&key.0) {
                Ok(mut value) => {
                    value.set_sensitive(true);
                    builder.header("x-api-key", value).header("anthropic-version", ANTHROPIC_VERSION)
                }
                Err(_) => builder,
            },
        }
    }

    /// Where a chat is asked.
    fn chat_path(self) -> &'static str {
        match self {
            Self::Xai | Self::OpenAi => "chat/completions",
            Self::Anthropic => "messages",
        }
    }

    /// The engine the page names `id`.
    fn named(id: &str) -> Result<Self, String> {
        Self::ALL.into_iter().find(|engine| engine.id() == id).ok_or_else(|| UNKNOWN.to_string())
    }

    /// The engine a page id for a model names, and the model's id at its maker.
    fn of_model(id: &str) -> Option<(Self, &str)> {
        Self::ALL.into_iter().find_map(|engine| id.strip_prefix(engine.prefix()).map(|model| (engine, model)))
    }

    /// True for an id that reads as one of this engine's models for text and
    /// nothing more.
    fn fits(self, id: &str) -> bool {
        let plain = id.len() <= 100 && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
        plain
            && match self {
                Self::Xai => id.starts_with("grok-"),
                Self::OpenAi => openai_fits(id),
                Self::Anthropic => id.starts_with("claude-"),
            }
    }

    /// A model's name as the page shows it.
    fn name_of(self, id: &str) -> String {
        match self {
            Self::Xai => id.split('-').filter(|word| !word.is_empty()).map(capital).collect::<Vec<_>>().join(" "),
            Self::OpenAi => openai_name(id),
            Self::Anthropic => claude_name(id),
        }
    }

    /// True for an https address on this engine's API, with no account in it
    /// and no port of its own.
    fn owns(self, url: &reqwest::Url) -> bool {
        url.scheme() == "https" && url.username().is_empty() && url.password().is_none() && url.port().is_none() && url.domain() == Some(self.host())
    }

    /// What an engine says while the outbound gate is off.
    fn gate_off(self) -> String {
        format!("The gate is off. Turn it on to ask {}.", self.name())
    }

    /// What adding an engine says while the outbound gate is off.
    fn gate_off_find(self) -> String {
        format!("{} is added. Turn on the gate to find its models.", self.name())
    }

    /// What an answer says when the gate is turned off part way through.
    fn gate_went_off(self) -> String {
        format!("The gate went off, so {} stopped.", self.name())
    }

    fn not_added(self) -> String {
        format!("{} is not added. Add it on the Installers page with the path to your {} key.", self.name(), self.maker())
    }

    fn not_listed(self) -> String {
        format!("That {} model is not on the list. Refresh {}'s models on the Installers page.", self.name(), self.name())
    }

    fn no_path(self) -> String {
        format!("Give the path to the file that holds your {} key.", self.maker())
    }

    fn whole_path(self) -> String {
        format!("Give the whole path to the file that holds your {} key, starting with / or ~/, not the key itself.", self.maker())
    }

    fn unreachable(self) -> String {
        format!("{} could not be reached.", self.name())
    }

    fn unreadable(self) -> String {
        format!("{}'s list of models could not be read.", self.maker())
    }

    fn none_listed(self) -> String {
        format!("{} listed no {} models for text.", self.maker(), self.name())
    }

    fn elsewhere(self) -> String {
        format!("{} answered from somewhere else.", self.maker())
    }

    /// What an answer says when the model turned the question down.
    fn declined(self) -> String {
        format!("{} declined to answer this one.", self.name())
    }
}

/// A word with its first letter a capital, when it starts with a letter.
fn capital(word: &str) -> String {
    let mut letters = word.chars();
    match letters.next() {
        Some(first) if first.is_ascii_alphabetic() => first.to_ascii_uppercase().to_string() + letters.as_str(),
        _ => word.to_string(),
    }
}

/// "gpt-5-mini", "chatgpt-4o-latest", and "o4-mini" fit. A dated copy, such as
/// "gpt-4o-2024-08-06", does not, nor does a model for sound, as
/// "gpt-4o-audio-preview" is.
fn openai_fits(id: &str) -> bool {
    let family = id.starts_with("gpt-")
        || id.starts_with("chatgpt-")
        || id.strip_prefix('o').is_some_and(|rest| rest.starts_with(|letter: char| letter.is_ascii_digit()));
    family && id.split('-').all(|word| !NOT_FOR_CHAT.contains(&word) && !(word.len() >= 4 && word.bytes().all(|byte| byte.is_ascii_digit())))
}

/// "gpt-5-mini" reads "GPT-5 Mini", "chatgpt-4o-latest" "ChatGPT-4o Latest",
/// and "o4-mini" "o4 Mini".
fn openai_name(id: &str) -> String {
    let words: Vec<&str> = id.split('-').filter(|word| !word.is_empty()).collect();
    let (head, rest) = match words.as_slice() {
        ["gpt", version, rest @ ..] => (format!("GPT-{version}"), rest),
        ["chatgpt", version, rest @ ..] => (format!("ChatGPT-{version}"), rest),
        [first, rest @ ..] => ((*first).to_string(), rest),
        [] => return String::new(),
    };
    std::iter::once(head).chain(rest.iter().map(|word| capital(word))).collect::<Vec<_>>().join(" ")
}

/// True for a word that is a date, such as the "20250219" in
/// "claude-3-7-sonnet-20250219".
fn is_date(word: &str) -> bool {
    word.len() == 8 && word.bytes().all(|byte| byte.is_ascii_digit())
}

/// "claude-opus-5-5" reads "Claude Opus 5.5", "claude-haiku-4-5-20251001"
/// "Claude Haiku 4.5", and "claude-3-7-sonnet-20250219" "Claude 3.7 Sonnet".
fn claude_name(id: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut number = false;
    for word in id.strip_prefix("claude-").unwrap_or(id).split('-').filter(|word| !word.is_empty() && !is_date(word)) {
        let digits = word.bytes().all(|byte| byte.is_ascii_digit());
        match words.last_mut() {
            // A run of numbers is one version: 4, 5 reads 4.5.
            Some(last) if digits && number => {
                last.push('.');
                last.push_str(word);
            }
            _ => words.push(capital(word)),
        }
        number = digits;
    }
    std::iter::once("Claude".to_string()).chain(words).collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// The key
// ---------------------------------------------------------------------------

/// The person's key, read from their file for one call and dropped after it.
/// It has no `Debug` and no `Display`, so nothing can print it.
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
fn checked_path(engine: Engine, given: &str) -> Result<PathBuf, String> {
    let given = given.trim();
    if given.is_empty() {
        return Err(engine.no_path());
    }
    if given.chars().count() > PATH_MOST {
        return Err(engine.whole_path());
    }
    let path = match given.strip_prefix("~/").or_else(|| given.strip_prefix("~\\")) {
        Some(rest) => home().ok_or_else(|| engine.whole_path())?.join(rest),
        None => PathBuf::from(given),
    };
    if !path.is_absolute() {
        return Err(engine.whole_path());
    }
    Ok(path)
}

// ---------------------------------------------------------------------------
// The engines' models
// ---------------------------------------------------------------------------

/// One of an engine's models as kept: its id at the maker, and its window, 0
/// when the maker does not say.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
struct Found {
    id: String,
    #[serde(default)]
    context: u32,
}

impl Found {
    fn info(&self, engine: Engine) -> ModelInfo {
        ModelInfo { id: format!("{}{}", engine.prefix(), self.id), name: engine.name_of(&self.id), maker: engine.maker().to_string(), context: self.context }
    }
}

/// The Grok models for text in xAI's list, each once, in its order.
fn xai_models(answer: &[u8]) -> Result<Vec<String>, String> {
    let engine = Engine::Xai;
    let value: Value = serde_json::from_slice(answer).map_err(|_| engine.unreadable())?;
    let listed = value.get("models").and_then(Value::as_array).ok_or_else(|| engine.unreadable())?;
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
        if text && engine.fits(id) && !ids.iter().any(|kept| kept == id) {
            ids.push(id.to_string());
        }
        if ids.len() == engine.most_models() {
            break;
        }
    }
    if ids.is_empty() {
        return Err(engine.none_listed());
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

/// The GPT models for text in OpenAI's list, each once, newest first.
fn openai_models(answer: &[u8]) -> Result<Vec<String>, String> {
    let engine = Engine::OpenAi;
    let value: Value = serde_json::from_slice(answer).map_err(|_| engine.unreadable())?;
    let listed = value.get("data").and_then(Value::as_array).ok_or_else(|| engine.unreadable())?;
    let mut found: Vec<(i64, &str)> = Vec::new();
    for entry in listed {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if engine.fits(id) && !found.iter().any(|(_, kept)| *kept == id) {
            found.push((entry.get("created").and_then(Value::as_i64).unwrap_or(0), id));
        }
    }
    // Models made at the same time keep the list's order.
    found.sort_by_key(|(created, _)| std::cmp::Reverse(*created));
    let ids: Vec<String> = found.into_iter().take(engine.most_models()).map(|(_, id)| id.to_string()).collect();
    if ids.is_empty() {
        return Err(engine.none_listed());
    }
    Ok(ids)
}

/// The Claude models in Anthropic's list, each once, newest first, with their
/// windows. A dated copy is left out when the same model is listed without
/// its date.
fn anthropic_models(answer: &[u8]) -> Result<Vec<Found>, String> {
    let engine = Engine::Anthropic;
    let value: Value = serde_json::from_slice(answer).map_err(|_| engine.unreadable())?;
    let listed = value.get("data").and_then(Value::as_array).ok_or_else(|| engine.unreadable())?;
    let mut found: Vec<(&str, Found)> = Vec::new();
    for entry in listed {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if engine.fits(id) && !found.iter().any(|(_, kept)| kept.id == id) {
            let created = entry.get("created_at").and_then(Value::as_str).unwrap_or("");
            let context = entry.get("max_input_tokens").and_then(Value::as_u64).map_or(0, |context| u32::try_from(context).unwrap_or(u32::MAX));
            found.push((created, Found { id: id.to_string(), context }));
        }
    }
    let undated = |id: &str| id.rsplit_once('-').filter(|(_, last)| is_date(last)).map(|(model, _)| model.to_string());
    let listed_ids: Vec<String> = found.iter().map(|(_, kept)| kept.id.clone()).collect();
    found.retain(|(_, kept)| undated(&kept.id).is_none_or(|model| !listed_ids.contains(&model)));
    // Anthropic writes each time the same way, so the newest sorts last as
    // text. Models made at the same time keep the list's order.
    found.sort_by(|(one, _), (other, _)| other.cmp(one));
    let models: Vec<Found> = found.into_iter().take(engine.most_models()).map(|(_, kept)| kept).collect();
    if models.is_empty() {
        return Err(engine.none_listed());
    }
    Ok(models)
}

// ---------------------------------------------------------------------------
// Asking an engine
// ---------------------------------------------------------------------------

/// The one client for an engine, through the outbound gate. It follows no
/// redirect and sends no cookie; the key goes only in the call it is read for.
fn client(engine: Engine, gate: &Gate) -> Result<reqwest::Client, String> {
    gate.client(&engine.gate_off(), |builder| {
        builder
            .no_proxy()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .referer(false)
            .user_agent(concat!("rusty-business/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(CONNECT_WAIT)
    })
}

/// Why the engine's maker turned a call down, from its status alone. What it
/// writes back is never shown: it could repeat what was sent.
fn status_error(engine: Engine, code: u16) -> String {
    let maker = engine.maker();
    match code {
        401 | 403 => format!("{maker} did not take the key ({code}). Check the key file."),
        402 => format!("{maker} says the account needs credits or billing set up (402). Check your {maker} account."),
        404 => format!("{maker} answered 404. It may no longer offer that model: refresh {}'s models on the Installers page.", engine.name()),
        429 => format!("{maker} says too many requests, or no credits left (429). Wait a moment, or check your {maker} account."),
        500..=599 => format!("{maker} is not answering right now ({code})."),
        _ => format!("{maker} answered {code}."),
    }
}

/// An answer, read up to `most` bytes and no further.
async fn read_capped<B: AsRef<[u8]>, E>(engine: Engine, chunks: impl Stream<Item = Result<B, E>>, most: usize) -> Result<Vec<u8>, String> {
    let mut chunks = std::pin::pin!(chunks);
    let mut answer = Vec::new();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|_| format!("{} stopped answering part way through.", engine.maker()))?;
        if answer.len() + chunk.as_ref().len() > most {
            return Err(format!("{}'s answer was too large.", engine.maker()));
        }
        answer.extend_from_slice(chunk.as_ref());
    }
    Ok(answer)
}

/// One of the engine's lists, such as `models`.
async fn list(engine: Engine, client: &reqwest::Client, key: &Key, which: &str) -> Result<Vec<u8>, String> {
    let response = engine
        .signed(client.get(format!("{}/{which}", engine.base())), key)
        .timeout(LIST_WAIT)
        .send()
        .await
        .map_err(|_| engine.unreachable())?;
    if !engine.owns(response.url()) {
        return Err(engine.elsewhere());
    }
    if !response.status().is_success() {
        return Err(status_error(engine, response.status().as_u16()));
    }
    read_capped(engine, response.bytes_stream(), MOST_BYTES).await
}

/// The engine's models for text, with their windows where its maker gives
/// them.
async fn find_models(engine: Engine, client: &reqwest::Client, key: &Key) -> Result<Vec<Found>, String> {
    match engine {
        Engine::Xai => {
            let ids = xai_models(&list(engine, client, key, "language-models").await?)?;
            // The windows are a nicety: without them the models still answer.
            let windows = list(engine, client, key, "models").await.map(|answer| windows(&answer)).unwrap_or_default();
            Ok(ids.into_iter().map(|id| Found { context: windows.get(&id).copied().unwrap_or(0), id }).collect())
        }
        // OpenAI's list gives no windows.
        Engine::OpenAi => Ok(openai_models(&list(engine, client, key, "models").await?)?.into_iter().map(|id| Found { id, context: 0 }).collect()),
        // Anthropic lists 20 a page unless asked for more.
        Engine::Anthropic => anthropic_models(&list(engine, client, key, "models?limit=100").await?),
    }
}

// ---------------------------------------------------------------------------
// What the app keeps
// ---------------------------------------------------------------------------

/// What is kept for one engine: the path to the key, never the key, and the
/// models found. The models stay after the engine is removed, so saved chats
/// still name them.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Kept {
    #[serde(default)]
    key_path: Option<PathBuf>,
    #[serde(default)]
    models: Vec<Found>,
}

impl Kept {
    /// Only what reads right is kept: a whole path, and the engine's ids,
    /// each once.
    fn cleaned(mut self, engine: Engine) -> Self {
        self.key_path = self.key_path.filter(|path| path.is_absolute());
        let mut models: Vec<Found> = Vec::new();
        for found in self.models {
            if engine.fits(&found.id) && !models.iter().any(|kept| kept.id == found.id) {
                models.push(found);
            }
        }
        models.truncate(engine.most_models());
        self.models = models;
        self
    }
}

/// What is kept for every engine. A file kept before an engine came along
/// still reads, with nothing kept for it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
struct KeptFile {
    #[serde(default)]
    xai: Kept,
    #[serde(default)]
    openai: Kept,
    #[serde(default)]
    anthropic: Kept,
}

impl KeptFile {
    fn of(&self, engine: Engine) -> &Kept {
        match engine {
            Engine::Xai => &self.xai,
            Engine::OpenAi => &self.openai,
            Engine::Anthropic => &self.anthropic,
        }
    }

    fn of_mut(&mut self, engine: Engine) -> &mut Kept {
        match engine {
            Engine::Xai => &mut self.xai,
            Engine::OpenAi => &mut self.openai,
            Engine::Anthropic => &mut self.anthropic,
        }
    }

    fn cleaned(mut self) -> Self {
        for engine in Engine::ALL {
            let kept = std::mem::take(self.of_mut(engine));
            *self.of_mut(engine) = kept.cleaned(engine);
        }
        self
    }
}

/// The outside engines added, kept in the app's own folder.
pub struct Outside {
    file: PathBuf,
    kept: Mutex<KeptFile>,
}

/// An outside engine as the page shows it. It holds the key's path, never
/// the key.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutsideStatus {
    id: &'static str,
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
            .map(KeptFile::cleaned)
            .unwrap_or_default();
        Self { file, kept: Mutex::new(kept) }
    }

    fn lock(&self) -> MutexGuard<'_, KeptFile> {
        self.kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Changes what is kept for one engine, and keeps it for the next time
    /// the app opens. Nothing changes when it cannot be kept.
    fn change(&self, engine: Engine, how: impl FnOnce(&mut Kept)) -> Result<(), String> {
        let mut kept = self.lock();
        let mut next = kept.clone();
        how(next.of_mut(engine));
        let text = serde_json::to_string_pretty(&next).map_err(|_| format!("The app could not keep {}'s settings.", engine.name()))?;
        match self.file.parent() {
            Some(folder) => fs::create_dir_all(folder),
            None => Ok(()),
        }
        .and_then(|()| fs::write(&self.file, text + "\n"))
        .map_err(|error| format!("The app could not keep {}'s settings: {error}.", engine.name()))?;
        *kept = next;
        Ok(())
    }

    fn key_path(&self, engine: Engine) -> Option<PathBuf> {
        self.lock().of(engine).key_path.clone()
    }

    /// True when `id`, an id at the engine's maker, is one of the models found.
    fn knows(&self, engine: Engine, id: &str) -> bool {
        self.lock().of(engine).models.iter().any(|found| found.id == id)
    }

    /// An engine as the page shows it. Nothing is asked of its maker.
    fn status(&self, engine: Engine) -> OutsideStatus {
        let kept = self.lock().of(engine).clone();
        OutsideStatus {
            id: engine.id(),
            name: engine.name(),
            maker: engine.maker(),
            host: engine.host(),
            added: kept.key_path.is_some(),
            key_path: kept.key_path.as_ref().map(|path| path.display().to_string()),
            models: kept.models.iter().map(|found| found.info(engine)).collect(),
            warning: kept.key_path.as_deref().and_then(warning_for),
            error: None,
        }
    }

    /// Every engine as the page shows it, in the page's order.
    pub fn statuses(&self) -> Vec<OutsideStatus> {
        Engine::ALL.into_iter().map(|engine| self.status(engine)).collect()
    }

    /// Adds an engine with the path to the person's key. The path and the key
    /// in it are checked first, and nothing is kept when either is refused.
    /// With the gate on, the engine's models are found at once.
    async fn add(&self, engine: Engine, gate: &Gate, given: &str) -> Result<OutsideStatus, String> {
        let path = checked_path(engine, given)?;
        drop(read_key(&path)?);
        self.change(engine, |kept| kept.key_path = Some(path))?;
        if !gate.is_open() {
            return Ok(OutsideStatus { error: Some(engine.gate_off_find()), ..self.status(engine) });
        }
        // The path is kept even when the maker cannot be asked yet.
        Ok(self.refresh(engine, gate).await.unwrap_or_else(|error| OutsideStatus { error: Some(error), ..self.status(engine) }))
    }

    /// Asks the engine's maker for its models, unless the outbound gate is
    /// off. A refresh that fails keeps the models found before.
    async fn refresh(&self, engine: Engine, gate: &Gate) -> Result<OutsideStatus, String> {
        let client = client(engine, gate)?;
        let path = self.key_path(engine).ok_or_else(|| engine.not_added())?;
        let key = read_key(&path)?;
        let found = tokio::select! {
            found = find_models(engine, &client, &key) => found?,
            () = gate.closed() => return Err(engine.gate_off()),
        };
        drop(key);
        self.change(engine, |kept| kept.models = found)?;
        Ok(self.status(engine))
    }

    /// Forgets the path to the engine's key. The key file is never touched,
    /// and the models found stay, so saved chats still name them.
    fn remove(&self, engine: Engine) -> Result<OutsideStatus, String> {
        self.change(engine, |kept| kept.key_path = None)?;
        Ok(self.status(engine))
    }
}

/// True for a page id that names a model of an outside engine.
pub fn is_outside(id: &str) -> bool {
    Engine::of_model(id).is_some()
}

/// Asks the engine that listed the model, unless the outbound gate is off:
/// the same question the models on this computer get, with the same events
/// back.
pub async fn run_chat(
    gate: &Gate,
    outside: &Outside,
    request: &ChatRequest,
    cancel: &mut watch::Receiver<bool>,
    emit: impl FnMut(ChatEvent),
) -> Result<ChatDone, String> {
    let (engine, model) = Engine::of_model(&request.model).ok_or_else(|| NOT_OUTSIDE.to_string())?;
    if !outside.knows(engine, model) {
        return Err(engine.not_listed());
    }
    engine::checked_messages(&request.messages)?;
    let most = request.max_tokens.unwrap_or(engine::DEFAULT_MAX_TOKENS);
    let body = match engine {
        Engine::Xai | Engine::OpenAi => serde_json::json!({
            "model": model,
            "messages": request.messages,
            "stream": true,
            "stream_options": { "include_usage": true },
            "max_completion_tokens": most,
        }),
        Engine::Anthropic => claude_body(model, request, most)?,
    };
    // The gate first: with it off, the key is not even read.
    let client = client(engine, gate)?;
    let path = outside.key_path(engine).ok_or_else(|| engine.not_added())?;
    let key = read_key(&path)?;
    let sending = engine.signed(client.post(format!("{}/{}", engine.base(), engine.chat_path())), &key).json(&body).send();
    drop(key);
    let response = tokio::select! {
        _ = cancel.changed() => return Ok(ChatDone { stopped: true, ..ChatDone::default() }),
        () = gate.closed() => return Err(engine.gate_went_off()),
        response = sending => response.map_err(|_| engine.unreachable())?,
    };
    if !engine.owns(response.url()) {
        return Err(engine.elsewhere());
    }
    if !response.status().is_success() {
        return Err(status_error(engine, response.status().as_u16()));
    }
    read_answer(engine, gate, response.bytes_stream(), cancel, emit).await
}

/// Reads the engine's answer as it comes, until it ends, Stop is pressed, or
/// the gate is turned off.
async fn read_answer<B: AsRef<[u8]>, E>(
    engine: Engine,
    gate: &Gate,
    reply: impl Stream<Item = Result<B, E>>,
    cancel: &mut watch::Receiver<bool>,
    emit: impl FnMut(ChatEvent),
) -> Result<ChatDone, String> {
    let gate_off = async {
        gate.closed().await;
        engine.gate_went_off()
    };
    let broke = format!("{} stopped answering part way through.", engine.name());
    match engine {
        Engine::Xai | Engine::OpenAi => engine::read_reply(reply, cancel, gate_off, emit, &broke).await,
        Engine::Anthropic => {
            let done = engine::read_stream(reply, cancel, gate_off, emit, &broke, claude_payload).await?;
            if !done.stopped && done.finish_reason.as_deref() == Some("refusal") {
                return Err(engine.declined());
            }
            Ok(done)
        }
    }
}

// ---------------------------------------------------------------------------
// How Claude is asked
// ---------------------------------------------------------------------------

/// What a chat says when Claude cannot take its turns.
const CLAUDE_TURNS: &str = "Claude needs a chat that starts and ends with a message of yours.";

/// The chat as Anthropic's API takes it. The system's words go on top, not
/// in a turn. Empty turns are dropped, and two turns in a row from one side
/// are joined, since each turn must answer the other. A chat that starts or
/// ends with an answer is refused: Claude takes no answer to carry on.
/// No temperature is sent, nor any setting for thinking, so each model
/// thinks the way its maker made it.
fn claude_body(model: &str, request: &ChatRequest, most: u32) -> Result<Value, String> {
    let mut system: Vec<&str> = Vec::new();
    let mut turns: Vec<(&str, String)> = Vec::new();
    for message in &request.messages {
        let text = message.content.trim();
        if text.is_empty() {
            continue;
        }
        if message.role == "system" {
            system.push(text);
            continue;
        }
        match turns.last_mut() {
            Some((role, joined)) if *role == message.role => {
                joined.push_str("\n\n");
                joined.push_str(&message.content);
            }
            _ => turns.push((message.role.as_str(), message.content.clone())),
        }
    }
    let user = |turn: Option<&(&str, String)>| turn.is_some_and(|(role, _)| *role == "user");
    if !user(turns.first()) || !user(turns.last()) {
        return Err(CLAUDE_TURNS.to_string());
    }
    let messages: Vec<Value> = turns.into_iter().map(|(role, content)| serde_json::json!({ "role": role, "content": content })).collect();
    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": most,
        "messages": messages,
        "stream": true,
    });
    if !system.is_empty() {
        body["system"] = Value::from(system.join("\n\n"));
    }
    Ok(body)
}

/// A count of tokens in one of Claude's pieces, where it gives one.
fn tokens_at(value: &Value, pointer: &str) -> Option<u32> {
    value.pointer(pointer).and_then(Value::as_u64).map(|count| u32::try_from(count).unwrap_or(u32::MAX))
}

/// One piece of Claude's stream. Each says what it is: the start, a piece of
/// the answer or of its thinking, why it stopped, the end, or an error.
fn claude_payload(payload: &str, reply: &mut ReplyStream, done: &mut ChatDone) -> Taken {
    let Ok(value) = serde_json::from_str::<Value>(payload) else {
        return Ok((Vec::new(), false));
    };
    match value.get("type").and_then(Value::as_str) {
        Some("message_start") => {
            done.prompt_tokens = tokens_at(&value, "/message/usage/input_tokens").or(done.prompt_tokens);
        }
        Some("content_block_delta") => {
            let text = |field: &str| value.pointer(&format!("/delta/{field}")).and_then(Value::as_str);
            let events = match value.pointer("/delta/type").and_then(Value::as_str) {
                Some("text_delta") => text("text").map(|text| reply.push_content(text)),
                Some("thinking_delta") => text("thinking").map(|text| reply.push_reasoning(text)),
                _ => None,
            };
            return Ok((events.unwrap_or_default(), false));
        }
        Some("message_delta") => {
            if let Some(reason) = value.pointer("/delta/stop_reason").and_then(Value::as_str) {
                done.finish_reason = Some(claude_finish(reason));
            }
            done.completion_tokens = tokens_at(&value, "/usage/output_tokens").or(done.completion_tokens);
            done.prompt_tokens = tokens_at(&value, "/usage/input_tokens").or(done.prompt_tokens);
        }
        Some("message_stop") => return Ok((Vec::new(), true)),
        Some("error") => return Err(claude_error(value.pointer("/error/type").and_then(Value::as_str))),
        _ => {}
    }
    Ok((Vec::new(), false))
}

/// Why Claude stopped, in the words the other engines use: "stop" when it
/// finished, "length" when it ran out of room. Any other reason, a refusal
/// among them, is kept as Anthropic gives it.
fn claude_finish(reason: &str) -> String {
    match reason {
        "end_turn" | "stop_sequence" => "stop",
        "max_tokens" | "model_context_window_exceeded" => "length",
        other => other,
    }
    .to_string()
}

/// What an error in Claude's stream says, from its kind alone. What Anthropic
/// writes with it is never shown.
fn claude_error(kind: Option<&str>) -> String {
    match kind {
        Some("overloaded_error") => "Anthropic is busy right now. Ask Claude again in a moment.".to_string(),
        Some("rate_limit_error") => "Anthropic says too many requests. Wait a moment, then ask again.".to_string(),
        _ => "Claude stopped answering part way through.".to_string(),
    }
}

// ---------------------------------------------------------------------------
// What the page asks for
// ---------------------------------------------------------------------------

/// Every outside engine as kept. Nothing is asked of any maker.
#[tauri::command]
pub fn outside_status(outside: tauri::State<'_, Outside>) -> Vec<OutsideStatus> {
    outside.statuses()
}

/// Adds the engine the page names with the path to the person's key. The
/// page sends the path, never the key.
#[tauri::command]
pub async fn add_outside(
    engine: String,
    key_path: String,
    outside: tauri::State<'_, Outside>,
    gate: tauri::State<'_, Gate>,
) -> Result<OutsideStatus, String> {
    outside.add(Engine::named(&engine)?, &gate, &key_path).await
}

/// Asks the engine's maker for its models again.
#[tauri::command]
pub async fn refresh_outside(engine: String, outside: tauri::State<'_, Outside>, gate: tauri::State<'_, Gate>) -> Result<OutsideStatus, String> {
    outside.refresh(Engine::named(&engine)?, &gate).await
}

/// Forgets the path to the engine's key.
#[tauri::command]
pub fn remove_outside(engine: String, outside: tauri::State<'_, Outside>) -> Result<OutsideStatus, String> {
    outside.remove(Engine::named(&engine)?)
}

#[cfg(test)]
mod tests {
    // Brad, 2026-11-17: "user can add their outside inference, start with
    // Grok and Grok ai. user adds it manually, they will need to provide or
    // specify a path to a key, now, the Rusty Business™ can query Grok for the
    // same question." And: "you can use grok by it's self too." And, of the
    // engines after Grok: "and claude and codex/gpt and gemmini".

    use super::*;
    use tauri::async_runtime::block_on;

    /// Plainly not keys anyone holds.
    const FAKE_KEY: &str = "xai-not-a-real-key-only-for-these-tests-0123456789";
    const FAKE_OPENAI_KEY: &str = "sk-not-a-real-key-only-for-these-tests-0123456789";
    const FAKE_ANTHROPIC_KEY: &str = "sk-ant-not-a-real-key-only-for-these-tests-0123456789";

    const XAI: Engine = Engine::Xai;
    const OPENAI: Engine = Engine::OpenAi;
    const ANTHROPIC: Engine = Engine::Anthropic;

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

    fn found(id: &str) -> Vec<Found> {
        vec![Found { id: id.into(), context: 0 }]
    }

    #[test]
    fn the_key_never_leaves_rust() {
        let place = folder("key");
        let path = key_file(&place, &format!("{FAKE_KEY}\n"));
        let outside = Outside::new(place.join("outside.json"));
        let status = block_on(outside.add(XAI, &Gate::at(false), path.to_str().unwrap())).unwrap();
        assert!(status.added);
        assert_eq!(status.key_path.as_deref(), path.to_str());
        assert_eq!(status.error, Some(XAI.gate_off_find()));
        assert_eq!(read_key(&path).unwrap().0, FAKE_KEY);

        // Not in what the page is told, not in the app's own file.
        let told = serde_json::to_string(&outside.statuses()).unwrap();
        assert!(!told.contains(FAKE_KEY) && !told.contains("not-a-real-key"), "{told}");
        let kept = fs::read_to_string(place.join("outside.json")).unwrap();
        assert!(!kept.contains(FAKE_KEY) && kept.contains("keyPath"), "{kept}");
        // The page is told these names, and no other, for every engine.
        let told: Value = serde_json::from_str(&told).unwrap();
        let told = told.as_array().unwrap();
        assert_eq!(told.len(), Engine::ALL.len());
        for engine in told {
            let mut names: Vec<&str> = engine.as_object().unwrap().keys().map(String::as_str).collect();
            names.sort_unstable();
            assert_eq!(names, ["added", "error", "host", "id", "keyPath", "maker", "models", "name", "warning"]);
        }
        let named = |engine: &Value| [&engine["id"], &engine["name"], &engine["maker"], &engine["host"]].map(|value| value.as_str().unwrap().to_string());
        assert_eq!(named(&told[0]), ["xai", "Grok", "xAI", "api.x.ai"]);
        assert_eq!(named(&told[1]), ["openai", "GPT", "OpenAI", "api.openai.com"]);
        assert_eq!(named(&told[2]), ["anthropic", "Claude", "Anthropic", "api.anthropic.com"]);

        // Not in any message, whatever goes wrong.
        let mut errors = vec![
            chat(&Gate::at(false), &outside, &request("xai:grok-4.7")).unwrap_err(),
            block_on(outside.refresh(XAI, &Gate::at(false))).unwrap_err(),
            checked_path(XAI, FAKE_KEY).unwrap_err(),
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
        for (engine, key) in [(XAI, FAKE_KEY), (OPENAI, FAKE_OPENAI_KEY), (ANTHROPIC, FAKE_ANTHROPIC_KEY)] {
            for pasted in [key, &format!("  {key}\n"), "xai-abc/def"] {
                let error = block_on(outside.add(engine, &Gate::at(true), pasted)).unwrap_err();
                assert_eq!(error, engine.whole_path());
                assert!(!error.contains(pasted.trim()));
            }
            assert_eq!(block_on(outside.add(engine, &Gate::at(true), "   ")).unwrap_err(), engine.no_path());
            assert_eq!(block_on(outside.add(engine, &Gate::at(true), &"/a".repeat(PATH_MOST))).unwrap_err(), engine.whole_path());
        }
        assert_eq!(OPENAI.whole_path(), "Give the whole path to the file that holds your OpenAI key, starting with / or ~/, not the key itself.");
        assert!(!place.join("outside.json").exists(), "a refused path was kept");
        assert!(outside.statuses().iter().all(|status| !status.added));
        // A whole path, or one from the home folder, is taken.
        assert_eq!(checked_path(XAI, " /Users/me/.config/xai-key ").unwrap(), PathBuf::from("/Users/me/.config/xai-key"));
        if let Some(home) = home() {
            assert_eq!(checked_path(OPENAI, "~/.config/openai/key").unwrap(), home.join(".config/openai/key"));
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
            format!("OPENAI_API_KEY = {FAKE_OPENAI_KEY}"),
            format!("{FAKE_KEY}é"),
            "k".repeat(KEY_LONGEST + 1),
        ] {
            assert_eq!(read_key(&key_file(&place, &text)).err().unwrap(), not_just, "{text:?}");
        }
        // Space around the key is fine, and so is a key as long as OpenAI's
        // project keys.
        assert_eq!(read_key(&key_file(&place, &format!("\n  {FAKE_KEY}  \n"))).unwrap().0, FAKE_KEY);
        let long = format!("sk-proj-{}", "n".repeat(160));
        assert_eq!(read_key(&key_file(&place, &long)).unwrap().0, long);
        // A path that is not whole is refused before any file is looked at.
        for relative in ["xai-key", "./xai-key", "config/xai-key", "~xai-key"] {
            assert_eq!(checked_path(XAI, relative).unwrap_err(), XAI.whole_path(), "{relative}");
        }
        // Adding an engine reads the file first, and keeps no path it refused.
        let outside = Outside::new(place.join("outside.json"));
        let missing = place.join("missing");
        assert!(block_on(outside.add(OPENAI, &Gate::at(false), missing.to_str().unwrap())).unwrap_err().starts_with("There is no key file"));
        let short = key_file(&place, "short");
        assert_eq!(block_on(outside.add(XAI, &Gate::at(false), short.to_str().unwrap())).unwrap_err(), not_just);
        assert!(!place.join("outside.json").exists(), "a path to no key was kept");
        assert!(outside.statuses().iter().all(|status| !status.added));
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
        let status = block_on(outside.add(XAI, &Gate::at(false), path.to_str().unwrap())).unwrap();
        assert_eq!(status.warning, Some(warning));
        fs::remove_dir_all(&place).unwrap();
    }

    // Brad, 2026-11-17: "off by default, everything stops".
    #[test]
    fn with_the_gate_off_grok_is_not_asked_and_the_key_is_not_read() {
        let place = folder("gate");
        let outside = Outside::new(place.join("outside.json"));
        for (engine, model) in [(XAI, "grok-4.7"), (OPENAI, "gpt-5-mini"), (ANTHROPIC, "claude-opus-5-5")] {
            // A path to no file at all: reading it would say so.
            outside.change(engine, |kept| kept.key_path = Some(place.join("no-such-key"))).unwrap();
            outside.change(engine, |kept| kept.models = found(model)).unwrap();
            let asked = request(&format!("{}{model}", engine.prefix()));
            assert_eq!(chat(&Gate::at(false), &outside, &asked).unwrap_err(), engine.gate_off());
            assert_eq!(block_on(outside.refresh(engine, &Gate::at(false))).unwrap_err(), engine.gate_off());
            assert_eq!(client(engine, &Gate::at(false)).unwrap_err(), engine.gate_off());
            // With it on, the key is read, and its absence said, before
            // anything is sent.
            assert!(chat(&Gate::at(true), &outside, &asked).unwrap_err().starts_with("There is no key file"));
        }
        assert_eq!(OPENAI.gate_off(), "The gate is off. Turn it on to ask GPT.");
        assert_eq!(ANTHROPIC.gate_off(), "The gate is off. Turn it on to ask Claude.");
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn only_grok_models_found_are_asked() {
        let place = folder("models");
        let outside = Outside::new(place.join("outside.json"));
        outside.change(XAI, |kept| kept.models = found("grok-4.7")).unwrap();
        for model in ["xai:grok-9", "xai:", "xai:../grok-4.7"] {
            assert_eq!(chat(&Gate::at(true), &outside, &request(model)).unwrap_err(), XAI.not_listed(), "{model}");
        }
        for model in ["grok-4.7", "phi-4", "openai", "XAI:grok-4.7"] {
            assert_eq!(chat(&Gate::at(true), &outside, &request(model)).unwrap_err(), NOT_OUTSIDE, "{model}");
        }
        let mut empty = request("xai:grok-4.7");
        empty.messages.clear();
        assert_eq!(chat(&Gate::at(true), &outside, &empty).unwrap_err(), "There is nothing to send.");
        assert!(is_outside("xai:grok-4.7") && is_outside("openai:gpt-5") && is_outside("anthropic:claude-opus-5-5"));
        assert!(!is_outside("mlx-community/phi-4-4bit") && !is_outside("grok-4.7") && !is_outside("claude-opus-5-5") && !is_outside("Anthropic:claude-opus-5-5"));
        fs::remove_dir_all(&place).unwrap();
    }

    // Brad, 2026-11-17: "and claude and codex/gpt and gemmini".
    #[test]
    fn a_model_is_asked_only_of_the_engine_that_listed_it() {
        let place = folder("which");
        let outside = Outside::new(place.join("outside.json"));
        outside.change(XAI, |kept| kept.models = found("grok-4.7")).unwrap();
        outside.change(OPENAI, |kept| kept.models = found("gpt-5-mini")).unwrap();
        outside.change(ANTHROPIC, |kept| kept.models = found("claude-opus-5-5")).unwrap();
        // Each engine's model, named as another's, is refused before the
        // gate or the key is looked at.
        for (model, engine) in [
            ("openai:grok-4.7", OPENAI),
            ("xai:gpt-5-mini", XAI),
            ("anthropic:gpt-5-mini", ANTHROPIC),
            ("anthropic:grok-4.7", ANTHROPIC),
            ("openai:claude-opus-5-5", OPENAI),
            ("xai:claude-opus-5-5", XAI),
        ] {
            assert_eq!(chat(&Gate::at(false), &outside, &request(model)).unwrap_err(), engine.not_listed(), "{model}");
        }
        // Each as its own reaches the gate.
        for (model, engine) in [("openai:gpt-5-mini", OPENAI), ("xai:grok-4.7", XAI), ("anthropic:claude-opus-5-5", ANTHROPIC)] {
            assert_eq!(chat(&Gate::at(false), &outside, &request(model)).unwrap_err(), engine.gate_off(), "{model}");
        }
        assert_eq!(Engine::of_model("openai:gpt-5-mini"), Some((OPENAI, "gpt-5-mini")));
        assert_eq!(Engine::of_model("xai:grok-4.7"), Some((XAI, "grok-4.7")));
        assert_eq!(Engine::of_model("anthropic:claude-opus-5-5"), Some((ANTHROPIC, "claude-opus-5-5")));
        assert_eq!(Engine::of_model("google:gemini"), None);
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn an_engine_the_app_does_not_know_is_refused() {
        assert_eq!(Engine::named("xai"), Ok(XAI));
        assert_eq!(Engine::named("openai"), Ok(OPENAI));
        assert_eq!(Engine::named("anthropic"), Ok(ANTHROPIC));
        for unknown in ["", "XAI", "OpenAI", "Anthropic", "claude", "google", "gemini", "openai ", "anthropic ", "../xai"] {
            assert_eq!(Engine::named(unknown), Err(UNKNOWN.to_string()), "{unknown:?}");
        }
        // Each engine's names stand apart.
        for engine in Engine::ALL {
            assert_eq!(engine.prefix(), format!("{}:", engine.id()));
            assert_eq!(Engine::ALL.iter().filter(|other| other.id() == engine.id() || other.host() == engine.host()).count(), 1);
        }
    }

    #[test]
    fn grok_is_asked_at_xais_api_alone_over_https() {
        assert_eq!(XAI_BASE, "https://api.x.ai/v1");
        assert_eq!(XAI_BASE, format!("https://{XAI_HOST}/v1"));
        assert!(XAI.owns(&reqwest::Url::parse("https://api.x.ai/v1/chat/completions").unwrap()));
        for away in [
            "http://api.x.ai/v1/chat/completions",
            "https://api.x.ai:8443/v1",
            "https://someone@api.x.ai/v1",
            "https://api.x.ai.example.com/v1",
            "https://x.ai/v1",
            "https://evilapi.x.ai/v1",
            "https://127.0.0.1/v1",
            "https://api.openai.com/v1",
            "https://api.anthropic.com/v1",
        ] {
            assert!(!XAI.owns(&reqwest::Url::parse(away).unwrap()), "{away}");
        }
        // The client goes over https only: a stand-in on plain http is never
        // reached.
        for engine in Engine::ALL {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let sent = block_on(client(engine, &Gate::at(true)).unwrap().get(format!("http://127.0.0.1:{port}/v1/models")).send());
            assert!(sent.is_err());
            listener.set_nonblocking(true).unwrap();
            assert!(listener.accept().is_err(), "the client reached the stand-in over plain http");
        }
    }

    #[test]
    fn gpt_is_asked_at_openais_api_alone_over_https() {
        assert_eq!(OPENAI_BASE, "https://api.openai.com/v1");
        assert_eq!(OPENAI_BASE, format!("https://{OPENAI_HOST}/v1"));
        assert!(OPENAI.owns(&reqwest::Url::parse("https://api.openai.com/v1/chat/completions").unwrap()));
        for away in [
            "http://api.openai.com/v1/chat/completions",
            "https://api.openai.com:8443/v1",
            "https://someone:pass@api.openai.com/v1",
            "https://api.openai.com.example.com/v1",
            "https://openai.com/v1",
            "https://chatgpt.com/v1",
            "https://notapi.openai.com/v1",
            "https://api.x.ai/v1",
            "https://api.anthropic.com/v1",
        ] {
            assert!(!OPENAI.owns(&reqwest::Url::parse(away).unwrap()), "{away}");
        }
    }

    // Brad, 2026-11-17: "and claude and codex/gpt and gemmini".
    #[test]
    fn claude_is_asked_at_anthropics_api_alone_over_https() {
        assert_eq!(ANTHROPIC_BASE, "https://api.anthropic.com/v1");
        assert_eq!(ANTHROPIC_BASE, format!("https://{ANTHROPIC_HOST}/v1"));
        assert_eq!(format!("{}/{}", ANTHROPIC.base(), ANTHROPIC.chat_path()), "https://api.anthropic.com/v1/messages");
        assert_eq!(format!("{}/{}", OPENAI.base(), OPENAI.chat_path()), "https://api.openai.com/v1/chat/completions");
        assert_eq!(format!("{}/{}", XAI.base(), XAI.chat_path()), "https://api.x.ai/v1/chat/completions");
        assert!(ANTHROPIC.owns(&reqwest::Url::parse("https://api.anthropic.com/v1/messages").unwrap()));
        for away in [
            "http://api.anthropic.com/v1/messages",
            "https://api.anthropic.com:8443/v1",
            "https://someone@api.anthropic.com/v1",
            "https://api.anthropic.com.example.com/v1",
            "https://anthropic.com/v1",
            "https://claude.ai/v1",
            "https://notapi.anthropic.com/v1",
            "https://api.openai.com/v1",
            "https://api.x.ai/v1",
        ] {
            assert!(!ANTHROPIC.owns(&reqwest::Url::parse(away).unwrap()), "{away}");
        }
    }

    // Brad, 2026-11-17: "and claude and codex/gpt and gemmini".
    #[test]
    fn claude_is_signed_with_its_own_header() {
        let signed = |engine: Engine, key: &str| {
            let url = format!("{}/{}", engine.base(), engine.chat_path());
            engine.signed(client(engine, &Gate::at(true)).unwrap().post(url), &Key(key.into())).build().unwrap()
        };
        let asked = signed(ANTHROPIC, FAKE_ANTHROPIC_KEY);
        let headers = asked.headers();
        let key = headers.get("x-api-key").expect("the key goes in x-api-key");
        assert_eq!(key.to_str().unwrap(), FAKE_ANTHROPIC_KEY);
        assert!(key.is_sensitive(), "the key would show in a debug print");
        // The version Claude's stream was read against. Moving it means
        // reading the stream again, so the test moves with it.
        assert_eq!(headers.get("anthropic-version").and_then(|version| version.to_str().ok()), Some("2026-10-21"));
        assert!(headers.get(reqwest::header::AUTHORIZATION).is_none());
        assert_eq!(asked.url().as_str(), "https://api.anthropic.com/v1/messages");
        // The other engines take a bearer key, and no Anthropic header.
        for (engine, key) in [(XAI, FAKE_KEY), (OPENAI, FAKE_OPENAI_KEY)] {
            let asked = signed(engine, key);
            let bearer = asked.headers().get(reqwest::header::AUTHORIZATION).expect("a bearer key");
            assert_eq!(bearer.to_str().unwrap(), format!("Bearer {key}"));
            assert!(bearer.is_sensitive());
            assert!(asked.headers().get("x-api-key").is_none() && asked.headers().get("anthropic-version").is_none());
        }
        // A key that cannot go in a header is not sent at all.
        for bad in ["sk-ant-bad\nkey", "sk-ant-bad\u{7f}key"] {
            assert!(signed(ANTHROPIC, bad).headers().get("x-api-key").is_none(), "{bad:?}");
        }
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
            { "id": "gpt-5" },
            { "id": 7 },
            { "name": "no id" },
        ]});
        let ids = xai_models(listed.to_string().as_bytes()).unwrap();
        assert_eq!(ids, ["grok-4.7", "grok-420-reasoning", "grok-4.3"]);
        assert_eq!(xai_models(b"not json").unwrap_err(), XAI.unreadable());
        assert_eq!(xai_models(b"{}").unwrap_err(), XAI.unreadable());
        assert_eq!(xai_models(br#"{"models":[{"id":"grok-imagine-image","output_modalities":["image"]}]}"#).unwrap_err(), "xAI listed no Grok models for text.");
        let many: Vec<Value> = (0..50).map(|n| serde_json::json!({ "id": format!("grok-{n}") })).collect();
        assert_eq!(xai_models(serde_json::json!({ "models": many }).to_string().as_bytes()).unwrap().len(), XAI.most_models());

        let windows = windows(br#"{"data":[{"id":"grok-4.7","context_length":256000},{"id":"grok-4.3","context_length":null},{"id":"big","context_length":99999999999}]}"#);
        assert_eq!(windows.get("grok-4.7"), Some(&256_000));
        assert_eq!(windows.get("grok-4.3"), None);
        assert_eq!(windows.get("big"), Some(&u32::MAX));

        assert_eq!(XAI.name_of("grok-4.7"), "Grok 4.7");
        assert_eq!(XAI.name_of("grok-420-reasoning"), "Grok 420 Reasoning");
        assert_eq!(
            Found { id: "grok-4.7".into(), context: 256_000 }.info(XAI),
            ModelInfo { id: "xai:grok-4.7".into(), name: "Grok 4.7".into(), maker: "xAI".into(), context: 256_000 }
        );
    }

    // Brad, 2026-11-17: "and claude and codex/gpt and gemmini".
    #[test]
    fn openais_list_gives_gpt_models_for_text_newest_first_with_their_names() {
        let listed = serde_json::json!({ "object": "list", "data": [
            { "id": "gpt-4o", "created": 1_715_367_049, "owned_by": "system" },
            { "id": "gpt-5-mini", "created": 1_754_425_928 },
            { "id": "gpt-5", "created": 1_754_425_777 },
            { "id": "o4-mini", "created": 1_744_225_351 },
            { "id": "chatgpt-4o-latest", "created": 1_723_515_131 },
            { "id": "gpt-3.5-turbo-16k" },
            // A dated copy of a model already listed.
            { "id": "gpt-4o-2024-08-06", "created": 1_722_814_719 },
            { "id": "gpt-4-0613", "created": 1_686_588_896 },
            // Models for sound, pictures, search, other APIs, or none of text.
            { "id": "gpt-4o-audio-preview", "created": 1_800_000_000 },
            { "id": "gpt-realtime", "created": 1_800_000_000 },
            { "id": "gpt-4o-mini-tts" },
            { "id": "gpt-4o-transcribe" },
            { "id": "gpt-image-1" },
            { "id": "gpt-4o-search-preview" },
            { "id": "gpt-3.5-turbo-instruct" },
            { "id": "gpt-5-codex" },
            { "id": "gpt-5-pro" },
            { "id": "o3-deep-research" },
            { "id": "text-embedding-3-large" },
            { "id": "omni-moderation-latest" },
            { "id": "dall-e-3" },
            { "id": "whisper-1" },
            { "id": "tts-1" },
            { "id": "davinci-002" },
            { "id": "grok-4.7" },
            // Starts with o, but is no o-series model.
            { "id": "omni-latest", "created": 1_800_000_000 },
            // Not ids at all.
            { "id": "gpt-5/../x" },
            { "id": "gpt-<b>" },
            { "id": format!("gpt-{}", "x".repeat(100)) },
            { "id": 7 },
            { "id": "gpt-5", "created": 1 },
        ]});
        let ids = openai_models(listed.to_string().as_bytes()).unwrap();
        assert_eq!(ids, ["gpt-5-mini", "gpt-5", "o4-mini", "chatgpt-4o-latest", "gpt-4o", "gpt-3.5-turbo-16k"]);
        assert_eq!(openai_models(b"not json").unwrap_err(), "OpenAI's list of models could not be read.");
        assert_eq!(openai_models(br#"{"models":[]}"#).unwrap_err(), OPENAI.unreadable());
        assert_eq!(openai_models(br#"{"data":[{"id":"dall-e-3"}]}"#).unwrap_err(), "OpenAI listed no GPT models for text.");
        // Only the newest are kept.
        let many: Vec<Value> = (0..50).map(|n| serde_json::json!({ "id": format!("gpt-{n}"), "created": n })).collect();
        let newest = openai_models(serde_json::json!({ "data": many }).to_string().as_bytes()).unwrap();
        assert_eq!(newest.len(), OPENAI.most_models());
        assert_eq!(newest.first().map(String::as_str), Some("gpt-49"));

        for (id, name) in [
            ("gpt-5-mini", "GPT-5 Mini"),
            ("gpt-4.1-nano", "GPT-4.1 Nano"),
            ("gpt-4o", "GPT-4o"),
            ("chatgpt-4o-latest", "ChatGPT-4o Latest"),
            ("o4-mini", "o4 Mini"),
            ("o3", "o3"),
            ("gpt-3.5-turbo-16k", "GPT-3.5 Turbo 16k"),
        ] {
            assert_eq!(OPENAI.name_of(id), name, "{id}");
        }
        assert_eq!(
            Found { id: "gpt-5-mini".into(), context: 0 }.info(OPENAI),
            ModelInfo { id: "openai:gpt-5-mini".into(), name: "GPT-5 Mini".into(), maker: "OpenAI".into(), context: 0 }
        );
    }

    // Brad, 2026-11-17: "and claude and codex/gpt and gemmini".
    #[test]
    fn anthropics_list_gives_claude_models_newest_first_with_their_names() {
        let listed = serde_json::json!({ "data": [
            { "type": "model", "id": "claude-sonnet-5-5", "display_name": "Claude Sonnet 5.5", "created_at": "2026-09-30T00:00:00Z", "max_input_tokens": 1_000_000 },
            { "id": "claude-opus-5-5", "created_at": "2026-10-21T00:00:00Z", "max_input_tokens": 1_000_000 },
            { "id": "claude-haiku-4-5-20251001", "created_at": "2025-10-01T00:00:00Z", "max_input_tokens": 200_000 },
            // A dated copy of a model also listed without its date.
            { "id": "claude-opus-5-5-20261021", "created_at": "2026-10-21T00:00:00Z" },
            { "id": "claude-3-7-sonnet-20250219", "created_at": "2025-02-19T00:00:00Z", "max_input_tokens": 99_999_999_999_u64 },
            { "id": "claude-opus-5-5", "created_at": "2001-01-01T00:00:00Z" },
            { "id": "claude-old" },
            // Not Claude, or not ids at all.
            { "id": "gpt-5", "created_at": "2030-01-01T00:00:00Z" },
            { "id": "grok-4.7" },
            { "id": "claude-<b>" },
            { "id": "claude-opus/../x" },
            { "id": format!("claude-{}", "x".repeat(100)) },
            { "id": 7 },
            { "display_name": "no id" },
        ], "has_more": false });
        let models = anthropic_models(listed.to_string().as_bytes()).unwrap();
        let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
        assert_eq!(ids, ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-4-5-20251001", "claude-3-7-sonnet-20250219", "claude-old"]);
        let contexts: Vec<u32> = models.iter().map(|model| model.context).collect();
        assert_eq!(contexts, [1_000_000, 1_000_000, 200_000, u32::MAX, 0]);
        assert_eq!(anthropic_models(b"not json").unwrap_err(), "Anthropic's list of models could not be read.");
        assert_eq!(anthropic_models(br#"{"models":[]}"#).unwrap_err(), ANTHROPIC.unreadable());
        assert_eq!(anthropic_models(br#"{"data":[{"id":"gpt-5"}]}"#).unwrap_err(), "Anthropic listed no Claude models for text.");
        // Only the newest are kept.
        let many: Vec<Value> = (10..60).map(|n| serde_json::json!({ "id": format!("claude-{n}"), "created_at": format!("2026-01-01T00:00:{n}Z") })).collect();
        let newest = anthropic_models(serde_json::json!({ "data": many }).to_string().as_bytes()).unwrap();
        assert_eq!(newest.len(), ANTHROPIC.most_models());
        assert_eq!(newest.first().map(|model| model.id.as_str()), Some("claude-59"));
        assert_eq!(newest.last().map(|model| model.id.as_str()), Some("claude-48"));

        for (id, name) in [
            ("claude-opus-5-5", "Claude Opus 5.5"),
            ("claude-sonnet-5", "Claude Sonnet 5"),
            ("claude-haiku-4-5-20251001", "Claude Haiku 4.5"),
            ("claude-3-7-sonnet-20250219", "Claude 3.7 Sonnet"),
            ("claude-fable-5-2", "Claude Fable 5.2"),
        ] {
            assert_eq!(ANTHROPIC.name_of(id), name, "{id}");
        }
        assert_eq!(
            Found { id: "claude-opus-5-5".into(), context: 1_000_000 }.info(ANTHROPIC),
            ModelInfo { id: "anthropic:claude-opus-5-5".into(), name: "Claude Opus 5.5".into(), maker: "Anthropic".into(), context: 1_000_000 }
        );
    }

    // Brad, 2026-11-17: "and claude and codex/gpt and gemmini".
    #[test]
    fn claude_is_asked_with_the_system_on_top_and_turns_that_take_turns() {
        let message = |role: &str, content: &str| engine::ChatMessage { role: role.into(), content: content.into() };
        let asked = |messages: Vec<engine::ChatMessage>| ChatRequest { model: "anthropic:claude-opus-5-5".into(), messages, max_tokens: None };
        let long = asked(vec![
            message("system", " Be brief. "),
            message("system", "   "),
            message("user", "What is 2+2?"),
            message("assistant", "4"),
            message("assistant", ""),
            message("assistant", "Anything else?"),
            message("system", "Use plain words."),
            message("user", "And 3+3?"),
            message("user", "Thanks."),
        ]);
        let body = claude_body("claude-opus-5-5", &long, engine::DEFAULT_MAX_TOKENS).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-opus-5-5",
                "max_tokens": 4096,
                "system": "Be brief.\n\nUse plain words.",
                "messages": [
                    { "role": "user", "content": "What is 2+2?" },
                    { "role": "assistant", "content": "4\n\nAnything else?" },
                    { "role": "user", "content": "And 3+3?\n\nThanks." },
                ],
                "stream": true,
            })
        );
        // No setting each model makes its own: no temperature, no thinking.
        let mut names: Vec<&str> = body.as_object().unwrap().keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, ["max_tokens", "messages", "model", "stream", "system"]);
        // No system, no system field.
        let plain = claude_body("claude-opus-5-5", &request("anthropic:claude-opus-5-5"), 100).unwrap();
        assert!(plain.get("system").is_none());
        assert_eq!(plain["max_tokens"], 100);
        // A chat that starts or ends with an answer, or has no turn, is refused.
        for messages in [
            vec![message("assistant", "Hello."), message("user", "Hi.")],
            vec![message("user", "Hi."), message("assistant", "Hello.")],
            vec![message("system", "Be brief."), message("user", "  ")],
        ] {
            assert_eq!(claude_body("claude-opus-5-5", &asked(messages), 100).unwrap_err(), CLAUDE_TURNS);
        }
        // And before the gate, or the key, is looked at.
        let place = folder("turns");
        let outside = Outside::new(place.join("outside.json"));
        outside.change(ANTHROPIC, |kept| kept.models = found("claude-opus-5-5")).unwrap();
        let ends_with_an_answer = asked(vec![message("user", "Hi."), message("assistant", "Hello.")]);
        assert_eq!(chat(&Gate::at(false), &outside, &ends_with_an_answer).unwrap_err(), CLAUDE_TURNS);
        assert_eq!(chat(&Gate::at(false), &outside, &asked(vec![message("user", "Hi.")])).unwrap_err(), ANTHROPIC.gate_off());
        fs::remove_dir_all(&place).unwrap();
    }

    // Brad, 2026-11-17: "and claude and codex/gpt and gemmini".
    #[test]
    fn claudes_stream_is_read_as_it_comes() {
        let event = |kind: &str, data: Value| format!("event: {kind}\ndata: {data}\n\n");
        let text = |text: &str| event("content_block_delta", serde_json::json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "text_delta", "text": text } }));
        let stopped = |reason: &str| event("message_delta", serde_json::json!({ "type": "message_delta", "delta": { "stop_reason": reason }, "usage": { "output_tokens": 12 } }));
        let start = event("message_start", serde_json::json!({ "type": "message_start", "message": { "id": "msg_1", "usage": { "input_tokens": 20, "output_tokens": 1 } } }));
        let stop = event("message_stop", serde_json::json!({ "type": "message_stop" }));
        let read = |pieces: Vec<String>| {
            let (_stop, mut cancelled) = watch::channel(false);
            let mut heard = String::new();
            let reply = futures_util::stream::iter(pieces.into_iter().map(|piece| Ok::<_, std::io::Error>(piece.into_bytes())));
            let done = block_on(read_answer(ANTHROPIC, &Gate::at(true), reply, &mut cancelled, |event| heard.push_str(&format!("{event:?}"))));
            (done, heard)
        };

        let (done, heard) = read(vec![
            start.clone(),
            event("ping", serde_json::json!({ "type": "ping" })),
            event("content_block_start", serde_json::json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "thinking", "thinking": "" } })),
            event("content_block_delta", serde_json::json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "thinking_delta", "thinking": "Two and two." } })),
            event("content_block_delta", serde_json::json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "signature_delta", "signature": "c2lnbmVk" } })),
            "data: not json\n\n".to_string(),
            text("2+2 "),
            text("is 4."),
            stopped("end_turn"),
            stop.clone(),
            // Nothing after the end is read.
            text(" More."),
        ]);
        let done = done.unwrap();
        assert_eq!((done.thinking.as_str(), done.answer.as_str()), ("Two and two.", "2+2 is 4."));
        assert_eq!((done.prompt_tokens, done.completion_tokens, done.finish_reason.as_deref()), (Some(20), Some(12), Some("stop")));
        assert!(!heard.contains("c2lnbmVk") && !heard.contains("More"), "{heard}");

        // Out of room reads as the other engines say it.
        for (reason, said) in [("max_tokens", "length"), ("model_context_window_exceeded", "length"), ("stop_sequence", "stop"), ("pause_turn", "pause_turn")] {
            let (done, _) = read(vec![start.clone(), text("2+2"), stopped(reason), stop.clone()]);
            assert_eq!(done.unwrap().finish_reason.as_deref(), Some(said), "{reason}");
        }

        // A refusal is said plainly, after what came before it.
        let (done, heard) = read(vec![start.clone(), text("I can "), stopped("refusal"), stop.clone()]);
        assert_eq!(done.unwrap_err(), "Claude declined to answer this one.");
        assert!(heard.contains("I can"), "{heard}");

        // An error part way through is said from its kind alone.
        for (kind, said) in [
            ("overloaded_error", "Anthropic is busy right now. Ask Claude again in a moment."),
            ("rate_limit_error", "Anthropic says too many requests. Wait a moment, then ask again."),
            ("api_error", "Claude stopped answering part way through."),
        ] {
            let error = event("error", serde_json::json!({ "type": "error", "error": { "type": kind, "message": "the words sent were: secret" } }));
            let (done, _) = read(vec![start.clone(), text("2+2"), error]);
            let said_back = done.unwrap_err();
            assert_eq!(said_back, said);
            assert!(!said_back.contains("secret"));
        }
        assert_eq!(claude_error(None), "Claude stopped answering part way through.");
    }

    #[test]
    fn xais_refusals_are_said_plainly() {
        assert_eq!(status_error(XAI, 401), "xAI did not take the key (401). Check the key file.");
        assert_eq!(status_error(XAI, 403), "xAI did not take the key (403). Check the key file.");
        assert!(status_error(XAI, 404).contains("refresh Grok's models on the Installers page"));
        assert!(status_error(XAI, 429).contains("too many requests"));
        assert_eq!(status_error(XAI, 503), "xAI is not answering right now (503).");
        assert_eq!(status_error(XAI, 418), "xAI answered 418.");
        assert_eq!(status_error(OPENAI, 401), "OpenAI did not take the key (401). Check the key file.");
        assert_eq!(status_error(OPENAI, 429), "OpenAI says too many requests, or no credits left (429). Wait a moment, or check your OpenAI account.");
        assert_eq!(status_error(ANTHROPIC, 401), "Anthropic did not take the key (401). Check the key file.");
        assert_eq!(status_error(ANTHROPIC, 402), "Anthropic says the account needs credits or billing set up (402). Check your Anthropic account.");
        assert_eq!(status_error(ANTHROPIC, 529), "Anthropic is not answering right now (529).");
        assert!(status_error(ANTHROPIC, 404).contains("refresh Claude's models on the Installers page"));
        let chunks = |sizes: &[usize]| futures_util::stream::iter(sizes.iter().map(|size| Ok::<_, ()>(vec![b'x'; *size])).collect::<Vec<_>>());
        assert_eq!(block_on(read_capped(XAI, chunks(&[400, 600]), 1000)).unwrap().len(), 1000);
        assert_eq!(block_on(read_capped(XAI, chunks(&[400, 601]), 1000)).unwrap_err(), "xAI's answer was too large.");
        assert_eq!(block_on(read_capped(OPENAI, chunks(&[1001]), 1000)).unwrap_err(), "OpenAI's answer was too large.");
    }

    #[test]
    fn turning_the_gate_off_stops_grok_part_way_through() {
        // A piece of an answer, as each engine sends it.
        let piece = |engine: Engine, text: &str| {
            let data = match engine {
                Engine::Xai | Engine::OpenAi => serde_json::json!({ "choices": [{ "delta": { "content": text } }] }),
                Engine::Anthropic => serde_json::json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": text } }),
            };
            Ok::<_, std::io::Error>(format!("data: {data}\n\n").into_bytes())
        };
        // A stand-in for the engine that sends two pieces, then nothing more.
        let reply = |engine: Engine| futures_util::stream::iter(vec![piece(engine, "2+2 "), piece(engine, "is")]).chain(futures_util::stream::pending());
        for engine in Engine::ALL {
            let gate = Gate::at(true);
            let switch = gate.clone();
            let turned_off = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(200));
                let _ = switch.set(false);
            });
            let (_stop, mut stopped) = watch::channel(false);
            let mut heard = String::new();
            // Were the gate not heard, the stand-in would never end, so the
            // wait is short and the test fails rather than hangs.
            let reading = read_answer(engine, &gate, reply(engine), &mut stopped, |event| heard.push_str(&format!("{event:?}")));
            let ended = block_on(async { tokio::time::timeout(Duration::from_secs(10), reading).await }).expect("turning the gate off did not stop the engine");
            assert_eq!(ended.unwrap_err(), engine.gate_went_off());
            assert!(heard.contains("2+2") && heard.contains("is"), "{heard}");
            turned_off.join().unwrap();
        }

        // Stop keeps what came.
        for engine in [OPENAI, ANTHROPIC] {
            let (stop, mut stopped) = watch::channel(false);
            let stopper = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(200));
                let _ = stop.send(true);
                stop
            });
            let done = block_on(read_answer(engine, &Gate::at(true), reply(engine), &mut stopped, |_| {})).unwrap();
            assert!(done.stopped);
            assert_eq!(done.answer, "2+2 is");
            drop(stopper.join().unwrap());
        }

        // Stop after Claude says it declines, but before its stream ends,
        // reads as stopped: the person ended the turn.
        let declining = serde_json::json!({ "type": "message_delta", "delta": { "stop_reason": "refusal" }, "usage": { "output_tokens": 3 } });
        let declined = Ok::<_, std::io::Error>(format!("data: {declining}\n\n").into_bytes());
        let reply = futures_util::stream::iter(vec![piece(ANTHROPIC, "I can "), declined]).chain(futures_util::stream::pending());
        let (stop, mut stopped) = watch::channel(false);
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            let _ = stop.send(true);
            stop
        });
        let done = block_on(read_answer(ANTHROPIC, &Gate::at(true), reply, &mut stopped, |_| {})).unwrap();
        assert!(done.stopped);
        assert_eq!((done.answer.as_str(), done.finish_reason.as_deref()), ("I can", Some("refusal")));
        drop(stopper.join().unwrap());
    }

    #[test]
    fn removing_grok_forgets_the_path_and_keeps_the_models() {
        let place = folder("remove");
        let path = key_file(&place, FAKE_KEY);
        let outside = Outside::new(place.join("outside.json"));
        block_on(outside.add(XAI, &Gate::at(false), path.to_str().unwrap())).unwrap();
        outside.change(XAI, |kept| kept.models = vec![Found { id: "grok-4.7".into(), context: 256_000 }]).unwrap();
        let status = outside.remove(XAI).unwrap();
        assert!(!status.added && status.key_path.is_none());
        assert_eq!(status.models.len(), 1, "saved chats would lose the model's name");
        assert!(path.exists(), "the key file was touched");
        assert_eq!(fs::read_to_string(&path).unwrap(), FAKE_KEY);
        // Removed stays removed.
        let again = Outside::new(place.join("outside.json"));
        assert!(!again.status(XAI).added);
        assert_eq!(again.status(XAI).models.len(), 1);
        assert_eq!(chat(&Gate::at(true), &again, &request("xai:grok-4.7")).unwrap_err(), XAI.not_added());
        assert_eq!(XAI.not_added(), "Grok is not added. Add it on the Installers page with the path to your xAI key.");
        fs::remove_dir_all(&place).unwrap();
    }

    // Brad, 2026-11-17: "and claude and codex/gpt and gemmini".
    #[test]
    fn each_engine_keeps_its_own_key_path_and_models() {
        let place = folder("each");
        let path = key_file(&place, FAKE_OPENAI_KEY);
        let outside = Outside::new(place.join("outside.json"));
        block_on(outside.add(XAI, &Gate::at(false), path.to_str().unwrap())).unwrap();
        outside.change(XAI, |kept| kept.models = found("grok-4.7")).unwrap();
        let status = block_on(outside.add(OPENAI, &Gate::at(false), path.to_str().unwrap())).unwrap();
        assert_eq!((status.id, status.added, status.error), ("openai", true, Some(OPENAI.gate_off_find())));
        outside.change(OPENAI, |kept| kept.models = found("gpt-5-mini")).unwrap();
        // Removing GPT leaves Grok as it was.
        outside.remove(OPENAI).unwrap();
        let again = Outside::new(place.join("outside.json"));
        let [grok, gpt] = [again.status(XAI), again.status(OPENAI)];
        assert!(grok.added && !gpt.added);
        assert_eq!(grok.models.iter().map(|model| model.id.as_str()).collect::<Vec<_>>(), ["xai:grok-4.7"]);
        assert_eq!(gpt.models.iter().map(|model| model.id.as_str()).collect::<Vec<_>>(), ["openai:gpt-5-mini"]);
        assert_eq!(chat(&Gate::at(true), &again, &request("openai:gpt-5-mini")).unwrap_err(), OPENAI.not_added());
        let kept = fs::read_to_string(place.join("outside.json")).unwrap();
        assert!(!kept.contains(FAKE_OPENAI_KEY) && kept.contains("\"openai\""), "{kept}");
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn a_kept_file_is_read_with_care() {
        let place = folder("kept");
        let file = place.join("outside.json");
        for text in ["", "not json", "[]", r#"{"xai":7}"#, r#"{"openai":"x"}"#] {
            fs::write(&file, text).unwrap();
            assert_eq!(*Outside::new(file.clone()).lock(), KeptFile::default(), "{text:?}");
        }
        // A file kept before GPT came along still reads.
        fs::write(&file, r#"{"xai":{"keyPath":"/keys/xai","models":[{"id":"grok-4.7"}]}}"#).unwrap();
        let kept = Outside::new(file.clone()).lock().clone();
        assert_eq!(kept.xai, Kept { key_path: Some(PathBuf::from("/keys/xai")), models: found("grok-4.7") });
        assert_eq!(kept.openai, Kept::default());
        assert_eq!(kept.anthropic, Kept::default());
        // Only each engine's own models, each once, are kept.
        fs::write(
            &file,
            r#"{"xai":{"keyPath":"relative/key","models":[{"id":"grok-4.7","context":1},{"id":"grok-4.7"},{"id":"../x"},{"id":"phi-4"},{"id":"gpt-5"}]},
               "openai":{"keyPath":"/keys/openai","models":[{"id":"gpt-5"},{"id":"grok-4.7"},{"id":"gpt-image-1"},{"id":"gpt-5"}]},
               "anthropic":{"keyPath":"/keys/anthropic","models":[{"id":"claude-opus-5-5","context":1000000},{"id":"gpt-5"},{"id":"claude-opus-5-5"},{"id":"claude-<b>"}]}}"#,
        )
        .unwrap();
        let kept = Outside::new(file.clone()).lock().clone();
        assert_eq!(kept.xai, Kept { key_path: None, models: vec![Found { id: "grok-4.7".into(), context: 1 }] });
        assert_eq!(kept.openai, Kept { key_path: Some(PathBuf::from("/keys/openai")), models: found("gpt-5") });
        assert_eq!(kept.anthropic, Kept { key_path: Some(PathBuf::from("/keys/anthropic")), models: vec![Found { id: "claude-opus-5-5".into(), context: 1_000_000 }] });
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    #[ignore = "asks xAI itself with the key at RUSTY_XAI_KEY_FILE; run with --ignored"]
    fn grok_answers_two_plus_two() {
        answers_two_plus_two(XAI, "RUSTY_XAI_KEY_FILE");
    }

    #[test]
    #[ignore = "asks OpenAI itself with the key at RUSTY_OPENAI_KEY_FILE; run with --ignored"]
    fn gpt_answers_two_plus_two() {
        answers_two_plus_two(OPENAI, "RUSTY_OPENAI_KEY_FILE");
    }

    #[test]
    #[ignore = "asks Anthropic itself with the key at RUSTY_ANTHROPIC_KEY_FILE; run with --ignored"]
    fn claude_answers_two_plus_two() {
        answers_two_plus_two(ANTHROPIC, "RUSTY_ANTHROPIC_KEY_FILE");
    }

    fn answers_two_plus_two(engine: Engine, variable: &str) {
        let path = PathBuf::from(std::env::var(variable).unwrap_or_else(|_| panic!("{variable} names the key file")));
        let place = folder(engine.id());
        let outside = Outside::new(place.join("outside.json"));
        let gate = Gate::at(true);
        let status = block_on(outside.add(engine, &gate, path.to_str().unwrap())).unwrap();
        assert_eq!(status.error, None);
        let model = status.models.first().expect("the maker listed a model").id.clone();
        let done = chat(&gate, &outside, &request(&model)).unwrap();
        assert!(done.answer.contains('4'), "{}", done.answer);
        fs::remove_dir_all(&place).unwrap();
    }
}
