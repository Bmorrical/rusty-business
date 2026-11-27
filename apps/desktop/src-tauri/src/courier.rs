//! The courier door: an MCP server on your private network, for a courier app
//! you bring and pair.
//!
//! Brad, 2026-11-25: "no fork. LAN only there is an API exposed, bring your
//! own courier app, we shape it for my needs to start", "that has a circuit
//! breaker too", and "the api with the courier is an mcp on the RB side that
//! any courier client can connect to".
//!
//! - It listens only while its circuit breaker and the LAN gate are both on.
//!   The breaker is kept the way the gates are: anything but a plain "on"
//!   reads as off. Turning either off stops everything at once, mid-answer
//!   too, and turns away anything that arrives.
//! - It answers only a peer on a private network: this computer, the RFC 1918
//!   ranges, link-local addresses, and IPv6 unique-local addresses.
//! - A courier app pairs the way a phone pairs with a speaker. The window
//!   shows a six-digit code for five minutes; the courier app sends it once
//!   and gets a key back, once. Five wrong codes void it. This computer keeps
//!   only the key's SHA-256 and the name the courier app gave, in a file only
//!   its owner can read. The key is never in the page, a log, or a saved
//!   chat.
//! - It speaks MCP over Streamable HTTP at `POST /mcp`, one request to a
//!   connection. A request a browser sends carries an Origin, and is turned
//!   away, so no web page can reach it.
//! - Its tools reach the chat engine's models only: never an engine outside
//!   this computer, and never a picture.
//!
//! It is written by hand on tokio, which the app already builds, so no
//! package is added for it.

use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tauri::Manager;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};

use crate::engine::{ChatDone, ChatEvent, Chats};
use crate::gate::{Gate, LanGate};

/// The port the courier door listens on, on every address of this computer.
/// Only a peer on a private network is answered.
pub const COURIER_PORT: u16 = 4710;

/// The most a request's head may be, and its body.
const HEAD_LIMIT: usize = 16 * 1024;
const BODY_LIMIT: usize = 1024 * 1024;
/// How long a courier app has to send its whole request.
const READ_TIME: Duration = Duration::from_secs(10);
/// The most connections answered at once. More are closed as they arrive.
const MOST_AT_ONCE: usize = 16;

/// How long a pairing code lasts, and how many wrong codes void it.
const CODE_LIFE: Duration = Duration::from_secs(5 * 60);
const CODE_TRIES: u8 = 5;
/// The most courier apps paired at once, and the longest name one may give.
const MOST_PAIRED: usize = 20;
const LONGEST_NAME: usize = 60;

/// The MCP versions the door speaks, newest first.
const VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

const NOT_OPEN: &str =
    "No pairing is open. Press Pair a courier app in Rusty Business™ to show a code.";
const WRONG_CODE: &str = "That code is not the one Rusty Business™ shows.";
const VOIDED: &str =
    "Too many wrong codes, so that code was voided. Show a new one in Rusty Business™.";
const BREAKER_OFF: &str = "Turn on the courier breaker first.";

// ---------------------------------------------------------------------------
// The breaker, the paired courier apps, and pairing
// ---------------------------------------------------------------------------

/// The courier door's state: its breaker, the courier apps paired, the code
/// shown while one pairs, and the questions in flight.
#[derive(Clone)]
pub struct Courier(Arc<Inner>);

struct Inner {
    breaker: Gate,
    file: PathBuf,
    paired: Mutex<Vec<Paired>>,
    pairing: Mutex<Option<Pairing>>,
    asks: Mutex<HashMap<String, watch::Sender<bool>>>,
    listening: Mutex<Listening>,
    at_once: AtomicUsize,
}

/// A courier app paired: its name, and its key's SHA-256, never the key.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Paired {
    id: String,
    name: String,
    hash: String,
    /// Seconds since 1970 when it paired.
    since: u64,
}

struct Pairing {
    code: String,
    until: Instant,
    tries: u8,
}

#[derive(Clone, Default)]
struct Listening {
    address: Option<String>,
    error: Option<String>,
}

/// What the page shows of the courier door. No key is in it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourierStatus {
    pub on: bool,
    pub port: u16,
    /// Where a courier app reaches it, while it listens.
    pub address: Option<String>,
    /// The address a courier app is given, built here so the page holds no
    /// web address of its own.
    pub place: Option<String>,
    pub error: Option<String>,
    pub paired: Vec<PairedView>,
    /// The code to type into the courier app, while one is pairing.
    pub code: Option<String>,
    pub code_seconds: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedView {
    pub id: String,
    pub name: String,
    pub since: u64,
}

/// Why a pairing was turned away.
#[derive(Debug, PartialEq)]
enum NoPair {
    NotOpen,
    Wrong,
    Voided,
    Name,
    Full,
    Unkept(String),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|_| "Could not make a random number.".to_string())?;
    Ok(bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn digest(key: &str) -> String {
    hex(&Sha256::digest(key.as_bytes()))
}

/// Equal, in the same time however soon the two differ.
fn same(one: &[u8], other: &[u8]) -> bool {
    one.len() == other.len()
        && one
            .iter()
            .zip(other)
            .fold(0u8, |differ, (a, b)| differ | (a ^ b))
            == 0
}

fn well_formed_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// A name a courier app may give itself: words a person can read, short.
fn good_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= LONGEST_NAME
        && name.trim() == name
        && !name.chars().any(char::is_control)
}

/// Six digits, each as likely as the next.
fn new_code() -> Result<String, String> {
    loop {
        let draw = u32::from_le_bytes(random::<4>()?);
        // 4,000,000,000 is a whole number of millions, so nothing is favored.
        if draw < 4_000_000_000 {
            return Ok(format!("{:06}", draw % 1_000_000));
        }
    }
}

fn seconds_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Keeps the courier apps paired in a file only its owner can read, written
/// whole and then put in place.
fn keep(file: &Path, paired: &[Paired]) -> Result<(), String> {
    let unkept =
        |error: std::io::Error| format!("The app could not keep the courier apps paired: {error}.");
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder).map_err(unkept)?;
    }
    let text = serde_json::to_string_pretty(paired).map_err(|error| error.to_string())?;
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

/// The courier apps paired, as kept. A file that cannot be read pairs none,
/// and an entry that is not well formed is passed over.
fn kept(file: &Path) -> Vec<Paired> {
    let all: Vec<Paired> = fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let mut paired: Vec<Paired> = Vec::new();
    for entry in all {
        if well_formed_hash(&entry.hash)
            && good_name(&entry.name)
            && !entry.id.is_empty()
            && !paired.iter().any(|other| other.id == entry.id)
            && paired.len() < MOST_PAIRED
        {
            paired.push(entry);
        }
    }
    paired
}

/// Takes a question's place among those in flight, and gives it up however
/// the question ends, the breaker cutting it off included.
struct InFlight<'a> {
    courier: &'a Courier,
    key: String,
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        lock(&self.courier.0.asks).remove(&self.key);
    }
}

/// Counts a connection being answered, and stops counting it however it ends.
struct Answering(Courier);

impl Drop for Answering {
    fn drop(&mut self) {
        self.0.0.at_once.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Courier {
    /// The breaker as the person last left it (off, unless the file says
    /// plainly "on"), and the courier apps paired.
    pub fn new(breaker: PathBuf, file: PathBuf) -> Self {
        let paired = kept(&file);
        Self(Arc::new(Inner {
            breaker: Gate::new(breaker),
            file,
            paired: Mutex::new(paired),
            pairing: Mutex::new(None),
            asks: Mutex::new(HashMap::new()),
            listening: Mutex::new(Listening::default()),
            at_once: AtomicUsize::new(0),
        }))
    }

    pub fn is_on(&self) -> bool {
        self.0.breaker.is_open()
    }

    /// Turns the breaker on or off. Off ends the pairing shown, and every
    /// question in flight, at once.
    pub fn set(&self, open: bool) -> Result<bool, String> {
        let set = self.0.breaker.set(open);
        if !open {
            *lock(&self.0.pairing) = None;
            for stop in lock(&self.0.asks).values() {
                let _ = stop.send(true);
            }
        }
        set
    }

    pub fn status(&self, now: Instant) -> CourierStatus {
        let listening = lock(&self.0.listening).clone();
        let on = self.is_on();
        let mut pairing = lock(&self.0.pairing);
        if pairing.as_ref().is_some_and(|shown| now >= shown.until) {
            *pairing = None;
        }
        CourierStatus {
            on,
            port: COURIER_PORT,
            place: listening
                .address
                .as_ref()
                .filter(|address| on && address.contains(':'))
                .map(|address| format!("http://{address}/mcp")),
            address: listening.address.filter(|_| on),
            error: listening.error.filter(|_| on),
            paired: lock(&self.0.paired)
                .iter()
                .map(|entry| PairedView {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                    since: entry.since,
                })
                .collect(),
            code: pairing.as_ref().map(|shown| shown.code.clone()),
            code_seconds: pairing.as_ref().map_or(0, |shown| {
                shown.until.saturating_duration_since(now).as_secs()
            }),
        }
    }

    /// Shows a new code, in place of any shown before.
    fn start_pairing(&self, now: Instant) -> Result<String, String> {
        if !self.is_on() {
            return Err(BREAKER_OFF.to_string());
        }
        if lock(&self.0.paired).len() >= MOST_PAIRED {
            return Err(format!(
                "{MOST_PAIRED} courier apps are paired, the most there can be. Unpair one first."
            ));
        }
        let code = new_code()?;
        *lock(&self.0.pairing) = Some(Pairing {
            code: code.clone(),
            until: now + CODE_LIFE,
            tries: CODE_TRIES,
        });
        Ok(code)
    }

    fn stop_pairing(&self) {
        *lock(&self.0.pairing) = None;
    }

    /// A courier app sends the code shown. The right code, in time, pairs it
    /// and gives back its key, once: (id, key).
    fn pair(&self, code: &str, name: &str, now: Instant) -> Result<(String, String), NoPair> {
        let mut pairing = lock(&self.0.pairing);
        let Some(shown) = pairing.as_mut() else {
            return Err(NoPair::NotOpen);
        };
        if now >= shown.until {
            *pairing = None;
            return Err(NoPair::NotOpen);
        }
        if !same(code.as_bytes(), shown.code.as_bytes()) {
            shown.tries = shown.tries.saturating_sub(1);
            if shown.tries == 0 {
                *pairing = None;
                return Err(NoPair::Voided);
            }
            return Err(NoPair::Wrong);
        }
        if !good_name(name) {
            return Err(NoPair::Name);
        }
        let mut paired = lock(&self.0.paired);
        if paired.len() >= MOST_PAIRED {
            return Err(NoPair::Full);
        }
        let key = hex(&random::<32>().map_err(NoPair::Unkept)?);
        let entry = Paired {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            hash: digest(&key),
            since: seconds_now(),
        };
        let mut next = paired.clone();
        next.push(entry.clone());
        keep(&self.0.file, &next).map_err(NoPair::Unkept)?;
        *paired = next;
        // A code pairs once.
        *pairing = None;
        Ok((entry.id, key))
    }

    /// Unpairs a courier app, and stops any question it has in flight.
    fn unpair(&self, id: &str) -> Result<(), String> {
        let mut paired = lock(&self.0.paired);
        let next: Vec<Paired> = paired
            .iter()
            .filter(|entry| entry.id != id)
            .cloned()
            .collect();
        if next.len() == paired.len() {
            return Err("That courier app is not paired.".to_string());
        }
        keep(&self.0.file, &next)?;
        *paired = next;
        let prefix = format!("{id} ");
        for (key, stop) in lock(&self.0.asks).iter() {
            if key.starts_with(&prefix) {
                let _ = stop.send(true);
            }
        }
        Ok(())
    }

    /// The courier app a key belongs to. Every paired key is compared, in
    /// the same time, whichever matches.
    fn who(&self, key: &str) -> Option<String> {
        let hash = digest(key);
        let mut found = None;
        for entry in lock(&self.0.paired).iter() {
            if same(hash.as_bytes(), entry.hash.as_bytes()) {
                found = Some(entry.id.clone());
            }
        }
        found
    }

    fn set_listening(&self, listening: Listening) {
        *lock(&self.0.listening) = listening;
    }
}

// ---------------------------------------------------------------------------
// Who may knock
// ---------------------------------------------------------------------------

/// True for this computer and for addresses on a private network.
pub(crate) fn private_peer(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => private_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => private_v4(v4),
            None => private_v6(v6),
        },
    }
}

fn private_v4(ip: Ipv4Addr) -> bool {
    ip.is_loopback() || ip.is_private() || ip.is_link_local()
}

fn private_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    // fc00::/7 is unique-local, and fe80::/10 link-local.
    ip.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
}

/// This computer's address on the private network, as a courier app would
/// reach it. Nothing is sent to find it: a UDP socket only picks its route.
fn this_address() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.168.255.254:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (private_peer(ip) && !ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

// ---------------------------------------------------------------------------
// HTTP, by hand: one request to a connection
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.headers
            .iter()
            .filter(move |(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// A header sent once. Sent twice, it counts as not sent.
    fn header<'a>(&'a self, name: &'a str) -> Option<&'a str> {
        let mut values = self.all(name);
        let first = values.next()?;
        values.next().is_none().then_some(first)
    }

    fn json(&self) -> bool {
        self.header("content-type").is_some_and(|kind| {
            kind.split(';')
                .next()
                .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))
        })
    }

    fn takes_stream(&self) -> bool {
        self.all("accept")
            .any(|accept| accept.to_ascii_lowercase().contains("text/event-stream"))
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn token_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// Reads one request: a head of at most 16 KiB, and a body of at most 1 MiB
/// whose length is said up front. Gives back the status to refuse it with.
async fn read_request<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Request, u16> {
    let mut buffer = Vec::with_capacity(2048);
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(at) = find(&buffer, b"\r\n\r\n") {
            break at;
        }
        if buffer.len() >= HEAD_LIMIT {
            return Err(431);
        }
        let read = stream.read(&mut chunk).await.map_err(|_| 400u16)?;
        if read == 0 {
            return Err(400);
        }
        buffer.extend_from_slice(&chunk[..read]);
    };
    if end > HEAD_LIMIT {
        return Err(431);
    }
    let head = std::str::from_utf8(&buffer[..end]).map_err(|_| 400u16)?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(path), Some(version), None) =
        (first.next(), first.next(), first.next(), first.next())
    else {
        return Err(400);
    };
    if method.is_empty() || !method.bytes().all(token_char) || !path.starts_with('/') {
        return Err(400);
    }
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(505);
    }
    let mut headers = Vec::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(400u16)?;
        if name.is_empty() || !name.bytes().all(token_char) {
            return Err(400);
        }
        headers.push((name.to_string(), value.trim().to_string()));
    }
    if headers.len() > 100 {
        return Err(431);
    }
    let mut request = Request {
        method: method.to_string(),
        path: path.to_string(),
        headers,
        body: Vec::new(),
    };
    // A body in pieces is not taken: its length is said up front, once.
    if request.all("transfer-encoding").next().is_some() {
        return Err(400);
    }
    let length = match request.all("content-length").count() {
        0 if request.method == "POST" => return Err(411),
        0 => 0,
        1 => {
            let said = request.header("content-length").unwrap_or_default();
            if said.is_empty() || !said.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(400);
            }
            said.parse::<usize>().map_err(|_| 413u16)?
        }
        _ => return Err(400),
    };
    if length > BODY_LIMIT {
        return Err(413);
    }
    let mut body = buffer.split_off(end + 4);
    if body.len() > length {
        // One request to a connection: nothing may follow it.
        return Err(400);
    }
    let have = body.len();
    body.resize(length, 0);
    stream
        .read_exact(&mut body[have..])
        .await
        .map_err(|_| 400u16)?;
    request.body = body;
    Ok(request)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        505 => "HTTP Version Not Supported",
        _ => "Internal Server Error",
    }
}

const COMMON: &str =
    "Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n";

async fn send<S: AsyncWrite + Unpin>(
    stream: &mut S,
    status: u16,
    body: Option<&Value>,
    extra: &str,
) -> std::io::Result<()> {
    let text = body.map(Value::to_string).unwrap_or_default();
    let kind = if body.is_some() {
        "Content-Type: application/json\r\n"
    } else {
        ""
    };
    let head = format!(
        "HTTP/1.1 {status} {}\r\n{kind}Content-Length: {}\r\n{COMMON}{extra}\r\n",
        reason(status),
        text.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(text.as_bytes()).await?;
    stream.flush().await
}

async fn refuse<S: AsyncWrite + Unpin>(stream: &mut S, status: u16, words: &str, extra: &str) {
    let _ = send(stream, status, Some(&json!({ "error": words })), extra).await;
}

fn refusal_words(status: u16) -> &'static str {
    match status {
        408 => "The request took too long to arrive.",
        411 => "Say the body's length with Content-Length.",
        413 => "The request is larger than 1 MiB.",
        431 => "The request's headers are larger than 16 KiB.",
        505 => "Only HTTP/1.1 is spoken here.",
        _ => "The request could not be read.",
    }
}

// ---------------------------------------------------------------------------
// What answers: the chat engine's models
// ---------------------------------------------------------------------------

/// The models a courier app may ask, and asking one. The app's own answers
/// through the engine; the tests bring their own.
pub(crate) trait Answers: Send + Sync {
    fn models(&self) -> impl Future<Output = Result<Vec<String>, String>> + Send;
    fn ask(
        &self,
        model: &str,
        question: &str,
        stopped: &mut watch::Receiver<bool>,
        emit: impl FnMut(ChatEvent) + Send,
    ) -> impl Future<Output = Result<ChatDone, String>> + Send;
}

#[derive(Clone)]
struct AppEngine(tauri::AppHandle);

impl Answers for AppEngine {
    async fn models(&self) -> Result<Vec<String>, String> {
        let lan = self.0.state::<LanGate>();
        let makers = self.0.state::<crate::makers::Makers>();
        let off = self.0.state::<crate::deactivated::Deactivated>().ids();
        crate::engine::courier_models(&lan, &makers, &off).await
    }

    async fn ask(
        &self,
        model: &str,
        question: &str,
        stopped: &mut watch::Receiver<bool>,
        emit: impl FnMut(ChatEvent) + Send,
    ) -> Result<ChatDone, String> {
        let lan = self.0.state::<LanGate>();
        let chats = self.0.state::<Chats>();
        crate::engine::courier_ask(&lan, &chats, model, question, stopped, emit).await
    }
}

// ---------------------------------------------------------------------------
// The door: /pair and /mcp
// ---------------------------------------------------------------------------

/// Answers one connection: reads its one request, and answers it.
pub(crate) async fn answer<S, E>(stream: &mut S, courier: &Courier, engine: &E)
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    E: Answers,
{
    let request = match tokio::time::timeout(READ_TIME, read_request(stream)).await {
        Err(_) => return refuse(stream, 408, refusal_words(408), "").await,
        Ok(Err(status)) => return refuse(stream, status, refusal_words(status), "").await,
        Ok(Ok(request)) => request,
    };
    // A browser always says where a page came from; a courier app has no
    // page. So no web page, on any site, can reach the door.
    if request.all("origin").next().is_some() {
        return refuse(stream, 403, "A web page cannot reach the courier door.", "").await;
    }
    match request.path.as_str() {
        "/pair" => pair_route(stream, courier, &request).await,
        "/mcp" => mcp_route(stream, courier, engine, &request).await,
        _ => {
            refuse(
                stream,
                404,
                "Nothing is here. MCP is at /mcp, and pairing at /pair.",
                "",
            )
            .await
        }
    }
}

#[derive(Deserialize)]
struct PairAsk {
    code: String,
    name: String,
}

async fn pair_route<S: AsyncWrite + Unpin>(stream: &mut S, courier: &Courier, request: &Request) {
    if request.method != "POST" {
        return refuse(stream, 405, "Pair with POST.", "Allow: POST\r\n").await;
    }
    if !request.json() {
        return refuse(stream, 415, "Send JSON.", "").await;
    }
    let Ok(ask) = serde_json::from_slice::<PairAsk>(&request.body) else {
        return refuse(stream, 400, "Send the code and the courier app's name.", "").await;
    };
    match courier.pair(ask.code.trim(), ask.name.trim(), Instant::now()) {
        Ok((id, key)) => {
            let _ = send(stream, 200, Some(&json!({ "id": id, "key": key })), "").await;
        }
        Err(NoPair::NotOpen) => refuse(stream, 403, NOT_OPEN, "").await,
        Err(NoPair::Wrong) => refuse(stream, 401, WRONG_CODE, "").await,
        Err(NoPair::Voided) => refuse(stream, 403, VOIDED, "").await,
        Err(NoPair::Name) => {
            let words = format!("Give the courier app a name of 1 to {LONGEST_NAME} characters.");
            refuse(stream, 400, &words, "").await;
        }
        Err(NoPair::Full) => refuse(stream, 403, "No more courier apps can be paired.", "").await,
        Err(NoPair::Unkept(_)) => {
            refuse(
                stream,
                500,
                "Rusty Business™ could not keep the pairing.",
                "",
            )
            .await;
        }
    }
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn rpc_result(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn tool_text(text: &str, error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": error })
}

fn tools() -> Value {
    json!({ "tools": [
        {
            "name": "models",
            "title": "Models",
            "description": "The models on this Rusty Business™ install that a courier app may ask, one id to a line.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "ask",
            "title": "Ask a model",
            "description": "Asks one of the models a question, in words. The answer streams back as progress while it comes, when the request carries a progress token, and the result is the whole answer.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "model": { "type": "string", "description": "A model id from the models tool." },
                    "question": { "type": "string", "description": "The question, in words." }
                },
                "required": ["model", "question"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        }
    ]})
}

fn initialized(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|asked| VERSIONS.contains(asked))
        .unwrap_or(VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "rusty-business",
            "title": "Rusty Business™",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": "Ask the models tool which models this install offers, then ask one with the ask tool."
    })
}

/// What a message to /mcp comes to.
#[derive(Debug, PartialEq)]
enum Reply {
    /// A notification, or a courier app's answer: nothing to say back.
    Accepted,
    Json(Value),
    Models(Value),
    Ask {
        id: Value,
        model: String,
        question: String,
        progress: Option<Value>,
    },
}

/// Works out what a message asks, before anything is asked of a model.
fn read_message(courier: &Courier, who: &str, message: &Value) -> Reply {
    let null = Value::Null;
    if message.is_array() {
        return Reply::Json(rpc_error(&null, -32600, "Send one message at a time."));
    }
    if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Reply::Json(rpc_error(&null, -32600, "That is not JSON-RPC 2.0."));
    }
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A courier app's own answer, which the door never asks for.
        return if message.get("id").is_some()
            && (message.get("result").is_some() || message.get("error").is_some())
        {
            Reply::Accepted
        } else {
            Reply::Json(rpc_error(&null, -32600, "A message needs a method."))
        };
    };
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    let Some(id) = message
        .get("id")
        .filter(|id| id.is_string() || id.is_number())
    else {
        if message.get("id").is_some() {
            return Reply::Json(rpc_error(&null, -32600, "An id is a string or a number."));
        }
        if method == "notifications/cancelled"
            && let Some(asked) = params.get("requestId")
            && let Some(stop) = lock(&courier.0.asks).get(&format!("{who} {asked}"))
        {
            let _ = stop.send(true);
        }
        return Reply::Accepted;
    };
    match method {
        "initialize" => Reply::Json(rpc_result(id, initialized(&params))),
        "ping" => Reply::Json(rpc_result(id, json!({}))),
        "tools/list" => Reply::Json(rpc_result(id, tools())),
        "tools/call" => {
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match params.get("name").and_then(Value::as_str) {
                Some("models") => Reply::Models(id.clone()),
                Some("ask") => {
                    let model = arguments
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::trim);
                    let question = arguments.get("question").and_then(Value::as_str);
                    match (model, question) {
                        (Some(model), Some(question))
                            if !model.is_empty() && !question.trim().is_empty() =>
                        {
                            Reply::Ask {
                                id: id.clone(),
                                model: model.to_string(),
                                question: question.to_string(),
                                progress: params
                                    .get("_meta")
                                    .and_then(|meta| meta.get("progressToken"))
                                    .filter(|token| token.is_string() || token.is_number())
                                    .cloned(),
                            }
                        }
                        _ => {
                            Reply::Json(rpc_error(id, -32602, "Ask needs a model and a question."))
                        }
                    }
                }
                _ => Reply::Json(rpc_error(
                    id,
                    -32602,
                    "There is no such tool. Ask tools/list.",
                )),
            }
        }
        _ => Reply::Json(rpc_error(
            id,
            -32601,
            "That method is not one the door answers.",
        )),
    }
}

async fn mcp_route<S, E>(stream: &mut S, courier: &Courier, engine: &E, request: &Request)
where
    S: AsyncWrite + Unpin + Send,
    E: Answers,
{
    if request.method != "POST" {
        // No stream opens from this side, so GET is not answered.
        return refuse(
            stream,
            405,
            "Send MCP messages with POST.",
            "Allow: POST\r\n",
        )
        .await;
    }
    let who = request
        .header("authorization")
        .and_then(|said| said.strip_prefix("Bearer "))
        .and_then(|key| courier.who(key.trim()));
    let Some(who) = who else {
        let words = "Pair this courier app first, and send its key as a Bearer token.";
        return refuse(stream, 401, words, "WWW-Authenticate: Bearer\r\n").await;
    };
    if let Some(version) = request.header("mcp-protocol-version")
        && !VERSIONS.contains(&version)
    {
        return refuse(
            stream,
            400,
            "That MCP version is not one the door speaks.",
            "",
        )
        .await;
    }
    if !request.json() {
        return refuse(stream, 415, "Send JSON.", "").await;
    }
    let Ok(message) = serde_json::from_slice::<Value>(&request.body) else {
        let error = rpc_error(&Value::Null, -32700, "The message is not JSON.");
        let _ = send(stream, 400, Some(&error), "").await;
        return;
    };
    match read_message(courier, &who, &message) {
        Reply::Accepted => {
            let _ = send(stream, 202, None, "").await;
        }
        Reply::Json(reply) => {
            let _ = send(stream, 200, Some(&reply), "").await;
        }
        Reply::Models(id) => {
            let result = match engine.models().await {
                Ok(models) if models.is_empty() => tool_text("No model is ready to answer.", true),
                Ok(models) => {
                    let mut result = tool_text(&models.join("\n"), false);
                    result["structuredContent"] = json!({ "models": models });
                    result
                }
                Err(error) => tool_text(&error, true),
            };
            let _ = send(stream, 200, Some(&rpc_result(&id, result)), "").await;
        }
        Reply::Ask {
            id,
            model,
            question,
            progress,
        } => {
            let streamed = request.takes_stream();
            ask(
                stream,
                courier,
                engine,
                &who,
                Asked {
                    id,
                    model,
                    question,
                    progress,
                },
                streamed,
            )
            .await;
        }
    }
}

struct Asked {
    id: Value,
    model: String,
    question: String,
    progress: Option<Value>,
}

/// One server-sent event, holding one JSON-RPC message.
fn event(message: &Value) -> String {
    format!("event: message\ndata: {message}\n\n")
}

async fn ask<S, E>(
    stream: &mut S,
    courier: &Courier,
    engine: &E,
    who: &str,
    asked: Asked,
    streamed: bool,
) where
    S: AsyncWrite + Unpin + Send,
    E: Answers,
{
    let Asked {
        id,
        model,
        question,
        progress,
    } = asked;
    let say = async |stream: &mut S, result: Value| {
        let message = rpc_result(&id, result);
        if streamed {
            let _ = stream.write_all(event(&message).as_bytes()).await;
            let _ = stream.flush().await;
        } else {
            let _ = send(stream, 200, Some(&message), "").await;
        }
    };
    let key = format!("{who} {id}");
    let (stop, mut stopped) = watch::channel(false);
    let taken = {
        let mut asks = lock(&courier.0.asks);
        let taken = asks.contains_key(&key);
        if !taken {
            asks.insert(key.clone(), stop);
        }
        taken
    };
    if taken {
        let error = rpc_error(
            &id,
            -32600,
            "A question with that id is already being answered.",
        );
        let _ = send(stream, 200, Some(&error), "").await;
        return;
    }
    let _in_flight = InFlight { courier, key };
    if streamed {
        let head = format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n{COMMON}\r\n");
        if stream.write_all(head.as_bytes()).await.is_err() {
            return;
        }
    }
    match engine.models().await {
        Ok(models) if models.iter().any(|offered| offered == &model) => {}
        Ok(_) => {
            let words =
                "That model is not one a courier app can ask. Ask the models tool for the list.";
            return say(stream, tool_text(words, true)).await;
        }
        Err(error) => return say(stream, tool_text(&error, true)).await,
    }
    let (pieces, mut arriving) = mpsc::unbounded_channel::<ChatEvent>();
    let done = {
        let asking = engine.ask(&model, &question, &mut stopped, move |piece| {
            let _ = pieces.send(piece);
        });
        tokio::pin!(asking);
        let mut sent = 0u64;
        let mut tell = async |stream: &mut S, piece: ChatEvent| -> bool {
            let (Some(token), true, ChatEvent::Answer { text }) = (&progress, streamed, piece)
            else {
                return true;
            };
            sent += text.chars().count() as u64;
            let note = json!({
                "jsonrpc": "2.0",
                "method": "notifications/progress",
                "params": { "progressToken": token, "progress": sent, "message": text }
            });
            stream.write_all(event(&note).as_bytes()).await.is_ok() && stream.flush().await.is_ok()
        };
        loop {
            tokio::select! {
                biased;
                Some(piece) = arriving.recv() => {
                    // The courier app left: the question stops with it.
                    if !tell(stream, piece).await {
                        break None;
                    }
                }
                done = &mut asking => {
                    // What came before the end is told before the end.
                    let mut left = false;
                    while let Ok(piece) = arriving.try_recv() {
                        if !tell(stream, piece).await {
                            left = true;
                            break;
                        }
                    }
                    break (!left).then_some(done);
                }
            }
        }
    };
    let Some(done) = done else {
        return;
    };
    let result = match done {
        Ok(done) if done.stopped => {
            let text = if done.answer.is_empty() {
                "The question was stopped.".to_string()
            } else {
                format!(
                    "The question was stopped. What came before it stopped:\n\n{}",
                    done.answer
                )
            };
            tool_text(&text, true)
        }
        Ok(done) => tool_text(&done.answer, false),
        Err(error) => tool_text(&error, true),
    };
    say(stream, result).await;
}

// ---------------------------------------------------------------------------
// Listening, only while the breaker and the LAN gate are on
// ---------------------------------------------------------------------------

/// Ready once the breaker or the LAN gate goes off, even for a moment.
async fn closing(breaker: &Gate, lan: &Gate) {
    tokio::select! {
        () = breaker.closed() => {}
        () = lan.closed() => {}
    }
}

/// Ready once the breaker and the LAN gate are both on.
async fn both_on(breaker: &Gate, lan: &Gate) {
    loop {
        breaker.opened().await;
        lan.opened().await;
        if breaker.is_open() && lan.is_open() {
            return;
        }
    }
}

/// Answers on the listener until the breaker or the LAN gate goes off. Each
/// connection ends then too, mid-answer or not.
pub(crate) async fn listen<E>(listener: &TcpListener, courier: &Courier, lan: &Gate, engine: &E)
where
    E: Answers + Clone + 'static,
{
    let off = closing(&courier.0.breaker, lan);
    tokio::pin!(off);
    loop {
        tokio::select! {
            biased;
            () = &mut off => return,
            accepted = listener.accept() => {
                let Ok((mut socket, peer)) = accepted else {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                };
                if !private_peer(peer.ip())
                    || courier.0.at_once.fetch_add(1, Ordering::SeqCst) >= MOST_AT_ONCE
                {
                    if private_peer(peer.ip()) {
                        courier.0.at_once.fetch_sub(1, Ordering::SeqCst);
                    }
                    drop(socket);
                    continue;
                }
                let counted = Answering(courier.clone());
                let _ = socket.set_nodelay(true);
                let (courier, lan, engine) = (courier.clone(), lan.clone(), engine.clone());
                tauri::async_runtime::spawn(async move {
                    let _counted = counted;
                    tokio::select! {
                        biased;
                        () = closing(&courier.0.breaker, &lan) => {}
                        () = answer(&mut socket, &courier, &engine) => {
                            let _ = socket.shutdown().await;
                        }
                    }
                });
            }
        }
    }
}

/// Opens the door each time the breaker and the LAN gate are both on, and
/// closes it each time either goes off. It never ends.
async fn serve<E>(courier: Courier, lan: Gate, engine: E, port: u16)
where
    E: Answers + Clone + 'static,
{
    loop {
        both_on(&courier.0.breaker, &lan).await;
        match TcpListener::bind(("0.0.0.0", port)).await {
            Ok(listener) => {
                let address =
                    this_address().map(|ip| std::net::SocketAddr::new(ip, port).to_string());
                courier.set_listening(Listening {
                    address: Some(address.unwrap_or_else(|| format!("port {port}"))),
                    error: None,
                });
                listen(&listener, &courier, &lan, &engine).await;
                courier.set_listening(Listening::default());
            }
            Err(error) => {
                courier.set_listening(Listening {
                    address: None,
                    error: Some(format!(
                        "The courier door could not open port {port}: {error}."
                    )),
                });
                tokio::select! {
                    () = closing(&courier.0.breaker, &lan) => {}
                    () = tokio::time::sleep(Duration::from_secs(5)) => {}
                }
            }
        }
    }
}

/// Starts watching the breaker and the LAN gate, once, as the app opens.
pub fn start(app: &tauri::AppHandle) {
    let courier = app.state::<Courier>().inner().clone();
    let lan = app.state::<LanGate>().gate();
    let engine = AppEngine(app.clone());
    tauri::async_runtime::spawn(serve(courier, lan, engine, COURIER_PORT));
}

// ---------------------------------------------------------------------------
// What the page asks
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn courier_status(courier: tauri::State<'_, Courier>) -> CourierStatus {
    courier.status(Instant::now())
}

/// Turns the courier breaker on or off. Off stops everything at once.
#[tauri::command]
pub fn set_courier(
    open: bool,
    courier: tauri::State<'_, Courier>,
) -> Result<CourierStatus, String> {
    courier.set(open)?;
    Ok(courier.status(Instant::now()))
}

/// Shows a code for a courier app to pair with.
#[tauri::command]
pub fn pair_courier(courier: tauri::State<'_, Courier>) -> Result<CourierStatus, String> {
    courier.start_pairing(Instant::now())?;
    Ok(courier.status(Instant::now()))
}

#[tauri::command]
pub fn stop_pairing_courier(courier: tauri::State<'_, Courier>) -> CourierStatus {
    courier.stop_pairing();
    courier.status(Instant::now())
}

#[tauri::command]
pub fn unpair_courier(
    id: String,
    courier: tauri::State<'_, Courier>,
) -> Result<CourierStatus, String> {
    courier.unpair(&id)?;
    Ok(courier.status(Instant::now()))
}

#[cfg(test)]
mod tests {
    // Brad, 2026-11-25: "no fork. LAN only there is an API exposed, bring
    // your own courier app, we shape it for my needs to start", "that has a
    // circuit breaker too", and "the api with the courier is an mcp on the
    // RB side that any courier client can connect to".
    use super::*;
    use std::sync::atomic::AtomicBool;
    use tauri::async_runtime::block_on;
    use tokio::io::duplex;

    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("rusty-courier-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    fn courier(name: &str, on: bool) -> (Courier, PathBuf) {
        let place = folder(name);
        let courier = Courier::new(place.join("courier-breaker"), place.join("couriers.json"));
        if on {
            courier.set(true).unwrap();
        }
        (courier, place)
    }

    /// A courier app paired, and its key.
    fn paired(courier: &Courier) -> String {
        let code = courier.start_pairing(Instant::now()).unwrap();
        courier
            .pair(&code, "Test courier", Instant::now())
            .unwrap()
            .1
    }

    /// An engine that answers in pieces, or waits until it is stopped.
    #[derive(Clone, Default)]
    struct Fake {
        waits: bool,
        asked: Arc<AtomicBool>,
    }

    impl Answers for Fake {
        async fn models(&self) -> Result<Vec<String>, String> {
            Ok(vec!["phi-4".to_string()])
        }

        async fn ask(
            &self,
            _model: &str,
            question: &str,
            stopped: &mut watch::Receiver<bool>,
            mut emit: impl FnMut(ChatEvent) + Send,
        ) -> Result<ChatDone, String> {
            self.asked.store(true, Ordering::SeqCst);
            if self.waits {
                let _ = stopped.wait_for(|stop| *stop).await;
                return Ok(ChatDone {
                    stopped: true,
                    ..ChatDone::default()
                });
            }
            for text in ["Hello, ", "courier."] {
                emit(ChatEvent::Answer {
                    text: text.to_string(),
                });
            }
            Ok(ChatDone {
                answer: format!("Hello, courier. You asked: {question}"),
                ..ChatDone::default()
            })
        }
    }

    fn post(path: &str, key: Option<&str>, body: &Value, extra: &str) -> String {
        let body = body.to_string();
        let auth = key
            .map(|key| format!("Authorization: Bearer {key}\r\n"))
            .unwrap_or_default();
        format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n{auth}{extra}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    /// Sends one request through the door and reads all that comes back.
    fn exchange(courier: &Courier, engine: &Fake, request: &str) -> String {
        block_on(async {
            let (mut client, mut server) = duplex(64 * 1024);
            client.write_all(request.as_bytes()).await.unwrap();
            answer(&mut server, courier, engine).await;
            drop(server);
            let mut reply = String::new();
            client.read_to_string(&mut reply).await.unwrap();
            reply
        })
    }

    fn status_of(reply: &str) -> u16 {
        reply[9..12].parse().unwrap()
    }

    fn body_of(reply: &str) -> Value {
        serde_json::from_str(&reply[reply.find("\r\n\r\n").unwrap() + 4..]).unwrap()
    }

    fn call(id: u64, name: &str, arguments: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": arguments } })
    }

    #[test]
    fn the_breaker_is_off_by_default_and_reads_only_a_plain_on() {
        let place = folder("default");
        let file = place.join("courier-breaker");
        assert!(
            !Courier::new(file.clone(), place.join("c.json")).is_on(),
            "nothing kept"
        );
        fs::create_dir_all(&place).unwrap();
        for kept in ["", "off", "yes", "true", "1", "ON", "on please"] {
            fs::write(&file, kept).unwrap();
            assert!(
                !Courier::new(file.clone(), place.join("c.json")).is_on(),
                "{kept:?} read as on"
            );
        }
        fs::write(&file, "on\n").unwrap();
        assert!(Courier::new(file, place.join("c.json")).is_on());
        fs::remove_dir_all(place).unwrap();
    }

    #[test]
    fn the_page_is_told_where_the_door_is_only_while_it_listens() {
        let (courier, place) = courier("place", true);
        courier.set_listening(Listening {
            address: Some("192.168.1.20:4710".into()),
            error: None,
        });
        let shown = courier.status(Instant::now());
        assert_eq!(shown.place.as_deref(), Some("http://192.168.1.20:4710/mcp"));
        // Without an address of its own, the page is told only the port.
        courier.set_listening(Listening {
            address: Some("port 4710".into()),
            error: None,
        });
        assert_eq!(courier.status(Instant::now()).place, None);
        courier.set_listening(Listening {
            address: Some("192.168.1.20:4710".into()),
            error: None,
        });
        courier.set(false).unwrap();
        let shown = courier.status(Instant::now());
        assert_eq!((shown.address, shown.place), (None, None));
        fs::remove_dir_all(place).ok();
    }

    #[test]
    fn only_a_private_network_may_knock() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.9",
            "172.31.255.1",
            "192.168.1.20",
            "169.254.3.4",
            "::1",
            "fd12:3456::1",
            "fe80::1",
            "::ffff:192.168.1.20",
        ] {
            assert!(private_peer(ip.parse().unwrap()), "{ip} was turned away");
        }
        for ip in [
            "8.8.8.8",
            "172.32.0.1",
            "100.64.0.1",
            "192.0.2.1",
            "2001:db8::1",
            "2606:4700::1111",
            "::ffff:8.8.8.8",
            "0.0.0.0",
        ] {
            assert!(!private_peer(ip.parse().unwrap()), "{ip} was let in");
        }
    }

    #[test]
    fn a_code_pairs_once_and_the_key_is_kept_only_as_its_hash() {
        let (courier, place) = courier("pair", true);
        let now = Instant::now();
        assert_eq!(courier.pair("123456", "Courier", now), Err(NoPair::NotOpen));
        let code = courier.start_pairing(now).unwrap();
        assert_eq!(code.len(), 6);
        assert!(code.bytes().all(|byte| byte.is_ascii_digit()));
        let (id, key) = courier.pair(&code, "My courier", now).unwrap();
        assert_eq!(key.len(), 64);
        assert_eq!(courier.who(&key), Some(id.clone()));
        assert_eq!(courier.who(&key.replace(&key[..1], "x")), None);
        // Once: the same code again pairs nothing.
        assert_eq!(courier.pair(&code, "Another", now), Err(NoPair::NotOpen));

        let text = fs::read_to_string(place.join("couriers.json")).unwrap();
        assert!(!text.contains(&key), "the key itself was kept");
        assert!(text.contains(&digest(&key)));
        assert!(text.contains("My courier"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(place.join("couriers.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "others can read the paired file");
        }
        // The page sees the name, never the key or its hash.
        let shown = serde_json::to_string(&courier.status(now)).unwrap();
        assert!(shown.contains("My courier"));
        assert!(!shown.contains(&key) && !shown.contains(&digest(&key)));

        // Kept for next time, and unpaired for good.
        let again = Courier::new(place.join("courier-breaker"), place.join("couriers.json"));
        assert_eq!(again.who(&key), Some(id.clone()));
        again.unpair(&id).unwrap();
        assert_eq!(again.who(&key), None);
        assert_eq!(
            Courier::new(place.join("courier-breaker"), place.join("couriers.json")).who(&key),
            None
        );
        fs::remove_dir_all(place).unwrap();
    }

    #[test]
    fn a_code_lasts_five_minutes_and_five_wrong_tries_void_it() {
        let (courier, place) = courier("tries", true);
        let now = Instant::now();
        let code = courier.start_pairing(now).unwrap();
        assert_eq!(courier.status(now).code_seconds, 300);
        assert_eq!(
            courier.pair(&code, "Late", now + CODE_LIFE),
            Err(NoPair::NotOpen),
            "an old code paired"
        );
        let code = courier.start_pairing(now).unwrap();
        let wrong = if code == "000000" { "000001" } else { "000000" };
        for _ in 0..4 {
            assert_eq!(courier.pair(wrong, "Guess", now), Err(NoPair::Wrong));
        }
        assert_eq!(courier.pair(wrong, "Guess", now), Err(NoPair::Voided));
        assert_eq!(
            courier.pair(&code, "Right, too late", now),
            Err(NoPair::NotOpen)
        );
        assert!(courier.status(now).code.is_none());
        fs::remove_dir_all(place).ok();
    }

    #[test]
    fn with_the_breaker_off_no_code_is_shown_and_turning_it_off_ends_one() {
        let (courier, place) = courier("off-code", false);
        assert_eq!(
            courier.start_pairing(Instant::now()),
            Err(BREAKER_OFF.to_string())
        );
        courier.set(true).unwrap();
        let code = courier.start_pairing(Instant::now()).unwrap();
        courier.set(false).unwrap();
        assert_eq!(
            courier.pair(&code, "Courier", Instant::now()),
            Err(NoPair::NotOpen)
        );
        fs::remove_dir_all(place).ok();
    }

    #[test]
    fn a_paired_courier_app_speaks_mcp_and_gets_the_answer() {
        let (courier, place) = courier("mcp", true);
        let key = paired(&courier);
        let engine = Fake::default();

        let hello = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } } });
        let reply = exchange(&courier, &engine, &post("/mcp", Some(&key), &hello, ""));
        assert_eq!(status_of(&reply), 200, "{reply}");
        let body = body_of(&reply);
        assert_eq!(body["result"]["protocolVersion"], "2025-06-18");
        assert!(body["result"]["capabilities"]["tools"].is_object());

        let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert_eq!(
            status_of(&exchange(
                &courier,
                &engine,
                &post("/mcp", Some(&key), &note, "")
            )),
            202
        );

        let list = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
        let body = body_of(&exchange(
            &courier,
            &engine,
            &post("/mcp", Some(&key), &list, ""),
        ));
        let names: Vec<&str> = body["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["models", "ask"]);

        let body = body_of(&exchange(
            &courier,
            &engine,
            &post("/mcp", Some(&key), &call(3, "models", json!({})), ""),
        ));
        assert_eq!(body["result"]["content"][0]["text"], "phi-4");

        let question = call(
            4,
            "ask",
            json!({ "model": "phi-4", "question": "Who are you?" }),
        );
        let body = body_of(&exchange(
            &courier,
            &engine,
            &post("/mcp", Some(&key), &question, ""),
        ));
        assert_eq!(body["result"]["isError"], false);
        assert_eq!(
            body["result"]["content"][0]["text"],
            "Hello, courier. You asked: Who are you?"
        );

        // A model the courier app may not ask is never asked.
        let other = call(5, "ask", json!({ "model": "grok-4", "question": "Hi" }));
        let engine = Fake::default();
        let body = body_of(&exchange(
            &courier,
            &engine,
            &post("/mcp", Some(&key), &other, ""),
        ));
        assert_eq!(body["result"]["isError"], true);
        assert!(
            !engine.asked.load(Ordering::SeqCst),
            "a model off the list was asked"
        );
        fs::remove_dir_all(place).ok();
    }

    #[test]
    fn the_answer_streams_back_as_it_comes() {
        let (courier, place) = courier("stream", true);
        let key = paired(&courier);
        let mut question = call(7, "ask", json!({ "model": "phi-4", "question": "Hi" }));
        question["params"]["_meta"] = json!({ "progressToken": "t1" });
        let reply = exchange(
            &courier,
            &Fake::default(),
            &post(
                "/mcp",
                Some(&key),
                &question,
                "Accept: application/json, text/event-stream\r\n",
            ),
        );
        assert!(reply.contains("Content-Type: text/event-stream"), "{reply}");
        let messages: Vec<Value> = reply
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .map(|data| serde_json::from_str(data).unwrap())
            .collect();
        assert_eq!(messages.len(), 3, "{reply}");
        assert_eq!(messages[0]["method"], "notifications/progress");
        assert_eq!(messages[0]["params"]["progressToken"], "t1");
        assert_eq!(messages[0]["params"]["message"], "Hello, ");
        assert_eq!(messages[1]["params"]["progress"], 15);
        assert_eq!(messages[2]["id"], 7);
        assert_eq!(
            messages[2]["result"]["content"][0]["text"],
            "Hello, courier. You asked: Hi"
        );
        fs::remove_dir_all(place).ok();
    }

    #[test]
    fn an_unpaired_courier_app_or_a_web_page_is_turned_away() {
        let (courier, place) = courier("away", true);
        let key = paired(&courier);
        let engine = Fake::default();
        let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });

        let reply = exchange(&courier, &engine, &post("/mcp", None, &list, ""));
        assert_eq!(status_of(&reply), 401);
        assert!(reply.contains("WWW-Authenticate: Bearer"));
        let wrong = "0".repeat(64);
        assert_eq!(
            status_of(&exchange(
                &courier,
                &engine,
                &post("/mcp", Some(&wrong), &list, "")
            )),
            401
        );
        // A page in a browser carries an Origin, the right key or not.
        let page = post("/mcp", Some(&key), &list, "Origin: http://example.com\r\n");
        assert_eq!(status_of(&exchange(&courier, &engine, &page)), 403);
        let page = post(
            "/pair",
            None,
            &json!({ "code": "1", "name": "x" }),
            "Origin: null\r\n",
        );
        assert_eq!(status_of(&exchange(&courier, &engine, &page)), 403);
        // Two keys at once count as none.
        let two = post(
            "/mcp",
            Some(&key),
            &list,
            &format!("Authorization: Bearer {key}\r\n"),
        );
        assert_eq!(status_of(&exchange(&courier, &engine, &two)), 401);
        assert!(!engine.asked.load(Ordering::SeqCst));
        fs::remove_dir_all(place).ok();
    }

    #[test]
    fn pairing_over_the_door_gives_the_key_once() {
        let (courier, place) = courier("door-pair", true);
        let engine = Fake::default();
        let none = exchange(
            &courier,
            &engine,
            &post("/pair", None, &json!({ "code": "123456", "name": "C" }), ""),
        );
        assert_eq!(status_of(&none), 403);
        let code = courier.start_pairing(Instant::now()).unwrap();
        let reply = exchange(
            &courier,
            &engine,
            &post(
                "/pair",
                None,
                &json!({ "code": code, "name": "Courier" }),
                "",
            ),
        );
        assert_eq!(status_of(&reply), 200, "{reply}");
        let key = body_of(&reply)["key"].as_str().unwrap().to_string();
        let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
        assert_eq!(
            status_of(&exchange(
                &courier,
                &engine,
                &post("/mcp", Some(&key), &list, "")
            )),
            200
        );
        let again = exchange(
            &courier,
            &engine,
            &post(
                "/pair",
                None,
                &json!({ "code": code, "name": "Courier" }),
                "",
            ),
        );
        assert_eq!(status_of(&again), 403, "a code paired twice");
        fs::remove_dir_all(place).ok();
    }

    #[test]
    fn requests_the_door_does_not_take() {
        let (courier, place) = courier("bad", true);
        let engine = Fake::default();
        let get = "GET /mcp HTTP/1.1\r\nHost: x\r\n\r\n";
        assert_eq!(status_of(&exchange(&courier, &engine, get)), 405);
        let chunked = "POST /mcp HTTP/1.1\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\n\r\n0\r\n\r\n";
        assert_eq!(status_of(&exchange(&courier, &engine, chunked)), 400);
        let no_length = "POST /pair HTTP/1.1\r\nContent-Type: application/json\r\n\r\n";
        assert_eq!(status_of(&exchange(&courier, &engine, no_length)), 411);
        let large = format!(
            "POST /pair HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            BODY_LIMIT + 1
        );
        assert_eq!(status_of(&exchange(&courier, &engine, &large)), 413);
        let tall = format!(
            "POST /pair HTTP/1.1\r\nX-Pad: {}\r\n\r\n",
            "a".repeat(HEAD_LIMIT)
        );
        assert_eq!(status_of(&exchange(&courier, &engine, &tall)), 431);
        let elsewhere = "POST /admin HTTP/1.1\r\nContent-Length: 0\r\n\r\n";
        assert_eq!(status_of(&exchange(&courier, &engine, elsewhere)), 404);
        let key = paired(&courier);
        let batch = json!([{ "jsonrpc": "2.0", "id": 1, "method": "ping" }]);
        let body = body_of(&exchange(
            &courier,
            &engine,
            &post("/mcp", Some(&key), &batch, ""),
        ));
        assert_eq!(body["error"]["code"], -32600);
        let unknown = json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/list" });
        let body = body_of(&exchange(
            &courier,
            &engine,
            &post("/mcp", Some(&key), &unknown, ""),
        ));
        assert_eq!(body["error"]["code"], -32601);
        let old = post(
            "/mcp",
            Some(&key),
            &unknown,
            "MCP-Protocol-Version: 1999-01-01\r\n",
        );
        assert_eq!(status_of(&exchange(&courier, &engine, &old)), 400);
        fs::remove_dir_all(place).ok();
    }

    #[test]
    fn the_door_listens_only_while_the_breaker_and_the_lan_gate_are_on() {
        block_on(async {
            let (courier, place) = courier("listen", false);
            let lan = Gate::new(place.join("lan-gate"));
            lan.set(true).unwrap();
            let engine = Fake::default();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            // Off: the door gives up at once.
            tokio::time::timeout(
                Duration::from_secs(2),
                listen(&listener, &courier, &lan, &engine),
            )
            .await
            .expect("the door listened with the breaker off");

            courier.set(true).unwrap();
            let key = paired(&courier);
            let knock = async {
                let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
                let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" });
                socket
                    .write_all(post("/mcp", Some(&key), &list, "").as_bytes())
                    .await
                    .unwrap();
                let mut reply = String::new();
                socket.read_to_string(&mut reply).await.unwrap();
                assert_eq!(status_of(&reply), 200, "{reply}");
                // The LAN gate off closes the door.
                lan.set(false).unwrap();
            };
            let (listened, ()) = tokio::join!(
                tokio::time::timeout(
                    Duration::from_secs(5),
                    listen(&listener, &courier, &lan, &engine)
                ),
                knock
            );
            listened.expect("the door stayed open with the LAN gate off");
            fs::remove_dir_all(place).ok();
        });
    }

    #[test]
    fn turning_the_breaker_off_stops_an_answer_part_way() {
        block_on(async {
            let (courier, place) = courier("mid-answer", true);
            let lan = Gate::new(place.join("lan-gate"));
            lan.set(true).unwrap();
            let key = paired(&courier);
            let engine = Fake {
                waits: true,
                ..Fake::default()
            };
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let knock = async {
                let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
                let question = call(1, "ask", json!({ "model": "phi-4", "question": "Wait" }));
                socket
                    .write_all(
                        post(
                            "/mcp",
                            Some(&key),
                            &question,
                            "Accept: text/event-stream\r\n",
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                // The stream's head comes first, then the model is asked.
                let mut head = [0u8; 12];
                socket.read_exact(&mut head).await.unwrap();
                tokio::time::timeout(Duration::from_secs(2), async {
                    while !engine.asked.load(Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .expect("the model was never asked");
                courier.set(false).unwrap();
                let mut rest = String::new();
                let _ =
                    tokio::time::timeout(Duration::from_secs(3), socket.read_to_string(&mut rest))
                        .await
                        .expect("the answer went on with the breaker off");
                assert!(
                    !rest.contains("\"result\""),
                    "an answer came after the breaker went off: {rest}"
                );
            };
            let (listened, ()) = tokio::join!(
                tokio::time::timeout(
                    Duration::from_secs(5),
                    listen(&listener, &courier, &lan, &engine)
                ),
                knock
            );
            listened.expect("the door stayed open with the breaker off");
            assert!(
                lock(&courier.0.asks).is_empty(),
                "a question stayed in flight"
            );
            fs::remove_dir_all(place).ok();
        });
    }

    #[test]
    fn a_courier_app_can_cancel_its_own_question_only() {
        let (courier, place) = courier("cancel", true);
        let key = paired(&courier);
        let other = paired(&courier);
        let (stop, stopped) = watch::channel(false);
        let mine = courier.who(&key).unwrap();
        lock(&courier.0.asks).insert(format!("{mine} 9"), stop);
        let cancel = json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": 9 } });
        let engine = Fake::default();
        assert_eq!(
            status_of(&exchange(
                &courier,
                &engine,
                &post("/mcp", Some(&other), &cancel, "")
            )),
            202
        );
        assert!(!*stopped.borrow(), "another courier app stopped it");
        assert_eq!(
            status_of(&exchange(
                &courier,
                &engine,
                &post("/mcp", Some(&key), &cancel, "")
            )),
            202
        );
        assert!(*stopped.borrow());
        fs::remove_dir_all(place).ok();
    }
}
