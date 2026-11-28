//! The rig: many models at once on servers on the private network.
//!
//! It answers Brad's ask, 2026-11-24: "many models at once on the LAN, a pie
//! per card, a slider per model, shares can span cards".
//!
//! The person lays out a plan: the rig's cards (up to four, each with its
//! memory), and the models served on it, each at its own address with a share
//! of one or more cards. The plan is kept in the app's own folder, readable by
//! this account alone. The app shows each card as a pie and each model's
//! share, and works out the split a server would be started with.
//!
//! A model on the rig is asked like the engine on the LAN: only while the LAN
//! gate is on, at a private address with no proxy or redirects, and only a
//! model on the app's list. Each server holds its own model, so rig models
//! wait for no turn here and many answer at once. A picture never goes to the
//! rig. A server that asks for a key gets it from the person's own file, read
//! for one chat and never shown.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::engine::{self, ChatDone, ChatEvent, ChatRequest};
use crate::gate::LanGate;
use crate::outside;
use crate::vision;

/// What a rig model's chat id starts with.
const PREFIX: &str = "rig:";
/// The most cards a rig plan holds.
const CARDS_MOST: usize = 4;
/// The most models a rig plan holds.
const MODELS_MOST: usize = 16;
/// The longest name for a card or a model.
const NAME_MOST: usize = 40;
/// The longest id a server's model may have.
const MODEL_ID_MOST: usize = 200;
/// The most memory, in GB, a card or a model may say.
const MEMORY_MOST: f64 = 4096.0;

const GONE: &str = "That model is no longer on your rig plan.";

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// One card on the rig.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub name: String,
    pub memory_gb: f64,
}

/// A model's share of one card, by the card's place in the plan.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    pub card: usize,
    pub percent: u8,
}

/// One model served on the rig.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigModel {
    /// The plan's own id for it, `m1`, `m2`, and so on. Empty for a new one.
    #[serde(default)]
    pub id: String,
    /// What the person calls it.
    pub name: String,
    /// The server's address, such as 192.168.1.20:8091.
    pub address: String,
    /// The model's id on that server. It must be on the app's list.
    pub model: String,
    /// The path to the file that holds the server's key, if it asks for one.
    #[serde(default)]
    pub key_file: Option<String>,
    /// The memory the model needs, in GB, if the person knows it.
    #[serde(default)]
    pub needs_gb: Option<f64>,
    pub shares: Vec<Share>,
}

/// The rig as the person laid it out.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub cards: Vec<Card>,
    pub models: Vec<RigModel>,
}

/// A name the window can show: trimmed, not empty, not too long, and with no
/// control characters.
fn checked_name(name: &str, what: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("Give {what} a name."));
    }
    if name.chars().count() > NAME_MOST || name.chars().any(char::is_control) {
        return Err(format!(
            "Keep {what}'s name to {NAME_MOST} letters, on one line."
        ));
    }
    Ok(name.to_string())
}

fn checked_memory(memory: f64, what: &str) -> Result<f64, String> {
    if !memory.is_finite() || memory <= 0.0 || memory > MEMORY_MOST {
        return Err(format!(
            "Give {what}'s memory in GB, more than 0 and at most {MEMORY_MOST}."
        ));
    }
    Ok(memory)
}

/// A server's address, made whole: `http://` when none is given, and `/v1`
/// when no path is. It must be on this computer or the private network.
pub(crate) fn checked_address(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    let whole = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    };
    let mut url = reqwest::Url::parse(&whole).map_err(|_| {
        "Give the server's address on your private network, such as 192.168.1.20:8091.".to_string()
    })?;
    if url.path() == "/" || url.path().is_empty() {
        url.set_path("/v1");
    }
    engine::checked_base_url(url.as_str())
}

/// The plan, checked before it is kept. Each model keeps its id, and a new
/// one is given the next free one.
pub(crate) fn checked_plan(plan: &Plan) -> Result<Plan, String> {
    if plan.cards.len() > CARDS_MOST {
        return Err(format!("A rig plan holds at most {CARDS_MOST} cards."));
    }
    if plan.models.len() > MODELS_MOST {
        return Err(format!("A rig plan holds at most {MODELS_MOST} models."));
    }
    if !plan.models.is_empty() && plan.cards.is_empty() {
        return Err("Add a card before you add a model.".to_string());
    }
    let mut cards = Vec::new();
    for (place, card) in plan.cards.iter().enumerate() {
        let what = format!("card {}", place + 1);
        cards.push(Card {
            name: checked_name(&card.name, &what)?,
            memory_gb: checked_memory(card.memory_gb, &what)?,
        });
    }

    let mut taken = vec![0u32; cards.len()];
    let mut models: Vec<RigModel> = Vec::new();
    for model in &plan.models {
        let name = checked_name(&model.name, "each model")?;
        let address = checked_address(&model.address)?;
        let id = model.model.trim();
        if id.is_empty() || id.chars().count() > MODEL_ID_MOST {
            return Err(format!("Give {name}'s model id as its server names it."));
        }
        let Some(entry) = engine::catalog_entry(id) else {
            return Err(format!("{name}: that model is not on this app's list."));
        };
        if entry.pictures {
            return Err(format!(
                "{name}: a model that reads pictures stays on this computer."
            ));
        }
        let key_file = match model.key_file.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(given) => {
                // Read once, so a path that holds no key is refused now. The
                // key is dropped at once.
                outside::lan_key(given).map_err(|error| format!("{name}: {error}"))?;
                Some(given.to_string())
            }
        };
        let needs_gb = match model.needs_gb {
            None => None,
            Some(needs) => Some(checked_memory(needs, &name)?),
        };
        if model.shares.is_empty() {
            return Err(format!("Give {name} a share of at least one card."));
        }
        let mut shares = Vec::new();
        for share in &model.shares {
            if share.card >= cards.len() {
                return Err(format!(
                    "{name} has a share of a card that is not on the plan."
                ));
            }
            if !(1..=100).contains(&share.percent) {
                return Err(format!(
                    "{name}'s share of a card is from 1 to 100 percent."
                ));
            }
            if shares.iter().any(|kept: &Share| kept.card == share.card) {
                return Err(format!(
                    "{name} has two shares of {}.",
                    cards[share.card].name
                ));
            }
            taken[share.card] += u32::from(share.percent);
            if taken[share.card] > 100 {
                return Err(format!(
                    "{} is shared past 100 percent.",
                    cards[share.card].name
                ));
            }
            shares.push(share.clone());
        }
        shares.sort_by_key(|share| share.card);
        models.push(RigModel {
            id: model.id.trim().to_string(),
            name,
            address,
            model: id.to_string(),
            key_file,
            needs_gb,
            shares,
        });
    }

    // Keep each id that is well formed and not taken twice; give the rest
    // the next free one.
    let well_formed = |id: &str| {
        id.strip_prefix('m').is_some_and(|number| {
            (1..=3).contains(&number.len()) && number.bytes().all(|b| b.is_ascii_digit())
        })
    };
    let mut kept: Vec<String> = Vec::new();
    for model in &mut models {
        if well_formed(&model.id) && !kept.contains(&model.id) {
            kept.push(model.id.clone());
        } else {
            model.id.clear();
        }
    }
    let mut next = 1;
    for model in &mut models {
        if model.id.is_empty() {
            while kept.contains(&format!("m{next}")) {
                next += 1;
            }
            model.id = format!("m{next}");
            kept.push(model.id.clone());
        }
    }
    Ok(Plan { cards, models })
}

// ---------------------------------------------------------------------------
// What the window shows
// ---------------------------------------------------------------------------

/// One model's slice of a card's pie.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Slice {
    pub id: String,
    pub name: String,
    pub percent: u8,
}

/// One card, as a pie: its slices and what is free.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardView {
    pub name: String,
    pub memory_gb: f64,
    pub slices: Vec<Slice>,
    pub free: u8,
}

/// One model, as the window shows it and the chat finds it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelView {
    pub id: String,
    /// The id the page chats with, `rig:m1`.
    pub chat_id: String,
    pub name: String,
    /// The model's name on the app's list.
    pub listed_as: String,
    /// The server, as host and port.
    pub place: String,
    /// How many cards its shares come to, such as 1.5.
    pub cards: f64,
    /// The memory its shares come to, in GB.
    pub memory_gb: f64,
    /// Whether that is enough, when the person said what it needs.
    pub room: Option<bool>,
    /// The split across the plan's cards, in their order, such as "2,2,1",
    /// for the server's --tensor-split.
    pub split: String,
    pub context: u32,
}

/// The rig plan and what it comes to.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigView {
    pub plan: Plan,
    pub cards: Vec<CardView>,
    pub models: Vec<ModelView>,
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// A model's shares as the smallest whole proportions, one per card.
fn split_of(shares: &[Share], cards: usize) -> String {
    let mut parts = vec![0u32; cards];
    for share in shares {
        if let Some(part) = parts.get_mut(share.card) {
            *part = u32::from(share.percent);
        }
    }
    let common = parts.iter().copied().fold(0, gcd).max(1);
    parts
        .iter()
        .map(|part| (part / common).to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn place_of(address: &str) -> String {
    reqwest::Url::parse(address)
        .ok()
        .and_then(|url| {
            let host = url.host_str()?.to_string();
            Some(match url.port() {
                Some(port) => format!("{host}:{port}"),
                None => host,
            })
        })
        .unwrap_or_default()
}

fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// What a checked plan comes to.
pub(crate) fn view_of(plan: &Plan) -> RigView {
    let cards = plan
        .cards
        .iter()
        .enumerate()
        .map(|(place, card)| {
            let slices: Vec<Slice> = plan
                .models
                .iter()
                .filter_map(|model| {
                    let share = model.shares.iter().find(|share| share.card == place)?;
                    Some(Slice {
                        id: model.id.clone(),
                        name: model.name.clone(),
                        percent: share.percent,
                    })
                })
                .collect();
            let used: u32 = slices.iter().map(|slice| u32::from(slice.percent)).sum();
            CardView {
                name: card.name.clone(),
                memory_gb: card.memory_gb,
                slices,
                free: u8::try_from(100u32.saturating_sub(used)).unwrap_or(0),
            }
        })
        .collect();
    let models = plan
        .models
        .iter()
        .map(|model| {
            let cards: f64 = model
                .shares
                .iter()
                .map(|share| f64::from(share.percent) / 100.0)
                .sum();
            let memory: f64 = model
                .shares
                .iter()
                .filter_map(|share| {
                    let card = plan.cards.get(share.card)?;
                    Some(card.memory_gb * f64::from(share.percent) / 100.0)
                })
                .sum();
            let entry = engine::catalog_entry(&model.model);
            ModelView {
                id: model.id.clone(),
                chat_id: format!("{PREFIX}{}", model.id),
                name: model.name.clone(),
                listed_as: entry
                    .map(|entry| entry.name.to_string())
                    .unwrap_or_default(),
                place: place_of(&model.address),
                cards: round(cards),
                memory_gb: round(memory),
                room: model.needs_gb.map(|needs| memory + 1e-9 >= needs),
                split: split_of(&model.shares, plan.cards.len()),
                context: entry.map_or(0, |entry| entry.context),
            }
        })
        .collect();
    RigView {
        plan: plan.clone(),
        cards,
        models,
    }
}

// ---------------------------------------------------------------------------
// Keeping it
// ---------------------------------------------------------------------------

/// The rig plan, kept in the app's own folder.
pub struct Rig {
    file: PathBuf,
    plan: Mutex<Plan>,
}

impl Rig {
    /// The plan as last kept. A file that cannot be read, or a plan that no
    /// longer passes its checks, is an empty plan.
    pub fn new(file: PathBuf) -> Self {
        let plan = fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str::<Plan>(&text).ok())
            .and_then(|plan| checked_plan(&plan).ok())
            .unwrap_or_default();
        Self {
            file,
            plan: Mutex::new(plan),
        }
    }

    pub fn view(&self) -> RigView {
        view_of(
            &self
                .plan
                .lock()
                .map(|plan| plan.clone())
                .unwrap_or_default(),
        )
    }

    /// Checks the plan, keeps it, and uses it from the next chat on.
    pub fn save(&self, plan: &Plan) -> Result<RigView, String> {
        let plan = checked_plan(plan)?;
        keep(&self.file, &plan)?;
        let view = view_of(&plan);
        *self
            .plan
            .lock()
            .map_err(|_| "The app could not take the rig plan.".to_string())? = plan;
        Ok(view)
    }

    /// The model on the plan with this chat id.
    fn find(&self, chat_id: &str) -> Option<RigModel> {
        let id = chat_id.strip_prefix(PREFIX)?;
        let plan = self.plan.lock().ok()?;
        plan.models.iter().find(|model| model.id == id).cloned()
    }
}

/// Writes the plan readable by this account alone, whole or not at all.
fn keep(file: &Path, plan: &Plan) -> Result<(), String> {
    let unkept = |error: std::io::Error| format!("The app could not keep the rig plan: {error}.");
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder).map_err(unkept)?;
    }
    let text = serde_json::to_string_pretty(plan).map_err(|error| error.to_string())?;
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

// ---------------------------------------------------------------------------
// Asking it
// ---------------------------------------------------------------------------

/// True for a chat id of a model on the rig.
pub fn is_rig(id: &str) -> bool {
    id.starts_with(PREFIX)
}

/// One chat with a model on the rig. It runs only while the LAN gate is on,
/// stops when the gate is turned off, and waits for no turn: each server
/// holds its own model.
pub(crate) async fn run_chat(
    lan: &LanGate,
    rig: &Rig,
    request: &ChatRequest,
    stopped: &mut watch::Receiver<bool>,
    emit: impl FnMut(ChatEvent),
) -> Result<ChatDone, String> {
    if vision::has_pictures(&request.messages) {
        return Err(vision::STAYS_HERE.to_string());
    }
    let Some(model) = rig.find(&request.model) else {
        return Err(GONE.to_string());
    };
    let asked = ChatRequest {
        model: model.model.clone(),
        ..request.clone()
    };
    lan.run(async {
        let key = match model.key_file.as_deref() {
            Some(path) => Some(outside::lan_key(path)?),
            None => None,
        };
        engine::run_chat_keyed(
            &model.address,
            &asked,
            key.as_ref().map(outside::Key::text),
            stopped,
            emit,
        )
        .await
    })
    .await
}

#[tauri::command]
pub fn rig_status(rig: tauri::State<'_, Rig>) -> RigView {
    rig.view()
}

#[tauri::command]
pub fn save_rig(rig: tauri::State<'_, Rig>, plan: Plan) -> Result<RigView, String> {
    rig.save(&plan)
}

#[cfg(test)]
mod tests {
    // Brad, 2026-11-24: "many models at once on the LAN, a pie per card, a
    // slider per model, shares can span cards".
    use super::*;
    use crate::engine::ChatMessage;
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};
    use tauri::async_runtime::block_on;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("rusty-rig-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn card(name: &str, memory_gb: f64) -> Card {
        Card {
            name: name.to_string(),
            memory_gb,
        }
    }

    fn model(name: &str, address: &str, shares: &[(usize, u8)]) -> RigModel {
        RigModel {
            name: name.to_string(),
            address: address.to_string(),
            model: "phi-4-q4".to_string(),
            shares: shares
                .iter()
                .map(|&(card, percent)| Share { card, percent })
                .collect(),
            ..RigModel::default()
        }
    }

    fn lan(place: &Path, open: bool) -> LanGate {
        let lan = LanGate::new(place.join("lan-gate"));
        if open {
            lan.gate().set(true).unwrap();
        }
        lan
    }

    fn question() -> ChatRequest {
        ChatRequest {
            model: String::new(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello?".to_string(),
                images: Vec::new(),
            }],
            max_tokens: None,
        }
    }

    fn ask(lan: &LanGate, rig: &Rig, id: &str) -> Result<ChatDone, String> {
        block_on(asked(lan, rig, id))
    }

    async fn asked(lan: &LanGate, rig: &Rig, id: &str) -> Result<ChatDone, String> {
        let (_stop, mut stopped) = watch::channel(false);
        let request = ChatRequest {
            model: id.to_string(),
            ..question()
        };
        run_chat(lan, rig, &request, &mut stopped, |_| {}).await
    }

    /// How a stand-in llama-server answers.
    #[derive(Clone, Copy)]
    enum Answer {
        /// Waits until this many stand-ins sharing `arrived` have been asked,
        /// then says "together", or "alone" after five seconds.
        Together(usize),
        /// Sends a piece every 50 ms until the app hangs up.
        Endless,
    }

    /// A stand-in llama-server on this computer for one chat. It keeps what
    /// it was sent.
    fn fake(answer: Answer, arrived: Arc<AtomicUsize>) -> (String, Arc<Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let seen = Arc::new(Mutex::new(String::new()));
        let kept = seen.clone();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let Ok(read) = stream.read(&mut buffer) else {
                    return;
                };
                if read == 0 {
                    return;
                }
                request.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&request).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())?
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        *kept.lock().unwrap() = text;
                        break;
                    }
                }
            }
            let piece = |text: &str| {
                format!(
                    "data: {}\n\n",
                    serde_json::json!({ "choices": [{ "delta": { "content": text } }] })
                )
            };
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            );
            match answer {
                Answer::Together(count) => {
                    arrived.fetch_add(1, Ordering::SeqCst);
                    let start = Instant::now();
                    while arrived.load(Ordering::SeqCst) < count
                        && start.elapsed() < Duration::from_secs(5)
                    {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    let word = if arrived.load(Ordering::SeqCst) >= count {
                        "together"
                    } else {
                        "alone"
                    };
                    let _ = stream.write_all(piece(word).as_bytes());
                    let _ = stream.write_all(b"data: [DONE]\n\n");
                }
                Answer::Endless => {
                    let start = Instant::now();
                    while start.elapsed() < Duration::from_secs(30) {
                        if stream.write_all(piece("more ").as_bytes()).is_err() {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
        });
        (address, seen)
    }

    fn rig_with(place: &Path, models: Vec<RigModel>) -> Rig {
        let rig = Rig::new(place.join("rig.json"));
        rig.save(&Plan {
            cards: vec![card("Card one", 96.0), card("Card two", 96.0)],
            models,
        })
        .unwrap();
        rig
    }

    #[test]
    fn a_plan_is_checked_before_it_is_kept() {
        let cards = vec![card("Card one", 96.0), card("Card two", 48.0)];
        let good = Plan {
            cards: cards.clone(),
            models: vec![model("Phi", "192.168.1.20:8091", &[(0, 50), (1, 25)])],
        };
        let checked = checked_plan(&good).unwrap();
        assert_eq!(checked.models[0].address, "http://192.168.1.20:8091/v1");
        assert_eq!(checked.models[0].id, "m1");
        assert!(
            checked_plan(&Plan::default()).is_ok(),
            "an empty plan is refused"
        );

        let refused = |plan: Plan, why: &str| {
            assert!(checked_plan(&plan).is_err(), "{why} was let through");
        };
        let mut five = good.clone();
        five.cards = vec![card("A", 8.0); 5];
        refused(five, "a fifth card");
        let mut many = good.clone();
        many.models = vec![model("Phi", "192.168.1.20:8091", &[(0, 1)]); 17];
        refused(many, "a seventeenth model");
        refused(
            Plan {
                cards: Vec::new(),
                models: good.models.clone(),
            },
            "a model with no card",
        );
        for (name, memory) in [
            ("", 8.0),
            ("A name far longer than forty letters, surely", 8.0),
            ("Two\nlines", 8.0),
            ("A", 0.0),
            ("A", f64::NAN),
            ("A", 5000.0),
        ] {
            let mut bad = good.clone();
            bad.cards[0] = card(name, memory);
            refused(bad, &format!("the card {name:?} at {memory} GB"));
        }
        for address in [
            "8.8.8.8:8091",
            "https://192.168.1.20:8091",
            "http://me:pw@192.168.1.20:8091",
            "api.example.com",
            "",
        ] {
            let mut bad = good.clone();
            bad.models[0].address = address.to_string();
            refused(bad, &format!("the address {address:?}"));
        }
        for id in ["", "qwen-3-8b", "deepseek-r1", "phi-3.5-vision-instruct"] {
            let mut bad = good.clone();
            bad.models[0].model = id.to_string();
            refused(bad, &format!("the model {id:?}"));
        }
        for shares in [
            vec![],
            vec![(0, 0)],
            vec![(0, 101)],
            vec![(2, 50)],
            vec![(0, 30), (0, 30)],
        ] {
            let mut bad = good.clone();
            bad.models[0].shares = shares
                .iter()
                .map(|&(card, percent)| Share { card, percent })
                .collect();
            refused(bad, &format!("the shares {shares:?}"));
        }
        let mut full = good.clone();
        full.models
            .push(model("Gemma", "192.168.1.21:8092", &[(0, 51)]));
        refused(full, "a card shared past 100 percent");
        let mut needs = good.clone();
        needs.models[0].needs_gb = Some(-1.0);
        refused(needs, "a model that needs less than nothing");
        let mut key = good.clone();
        key.models[0].key_file = Some("rig.key".to_string());
        refused(key, "a key file that is not a whole path");
        let mut missing = good;
        missing.models[0].key_file = Some("/nowhere/rig.key".to_string());
        refused(missing, "a key file that is not there");
    }

    #[test]
    fn each_model_keeps_its_id_and_a_new_one_takes_the_next_free() {
        let mut plan = Plan {
            cards: vec![card("Card one", 96.0)],
            models: vec![
                model("A", "192.168.1.20:8091", &[(0, 10)]),
                model("B", "192.168.1.20:8092", &[(0, 10)]),
                model("C", "192.168.1.20:8093", &[(0, 10)]),
                model("D", "192.168.1.20:8094", &[(0, 10)]),
            ],
        };
        plan.models[0].id = "m2".to_string();
        plan.models[2].id = "m2".to_string();
        plan.models[3].id = "rig:anything".to_string();
        let ids: Vec<String> = checked_plan(&plan)
            .unwrap()
            .models
            .into_iter()
            .map(|model| model.id)
            .collect();
        assert_eq!(ids, ["m2", "m1", "m3", "m4"]);
    }

    #[test]
    fn each_card_is_a_pie_and_a_share_can_span_cards() {
        let mut big = model("Big", "192.168.1.20:8091", &[(0, 50), (1, 50), (2, 25)]);
        big.needs_gb = Some(100.0);
        let mut small = model("Small", "192.168.1.20:8092", &[(1, 30)]);
        small.needs_gb = Some(40.0);
        let plan = checked_plan(&Plan {
            cards: vec![card("One", 96.0), card("Two", 96.0), card("Three", 48.0)],
            models: vec![big, small],
        })
        .unwrap();
        let view = view_of(&plan);
        let pies: Vec<(Vec<(String, u8)>, u8)> = view
            .cards
            .iter()
            .map(|card| {
                (
                    card.slices
                        .iter()
                        .map(|slice| (slice.name.clone(), slice.percent))
                        .collect(),
                    card.free,
                )
            })
            .collect();
        assert_eq!(
            pies,
            [
                (vec![("Big".to_string(), 50)], 50),
                (vec![("Big".to_string(), 50), ("Small".to_string(), 30)], 20),
                (vec![("Big".to_string(), 25)], 75),
            ]
        );
        let big = &view.models[0];
        assert_eq!(big.chat_id, "rig:m1");
        assert_eq!(big.place, "192.168.1.20:8091");
        assert_eq!(big.listed_as, "Phi-4");
        assert_eq!(big.cards, 1.25);
        assert_eq!(big.memory_gb, 108.0);
        assert_eq!(big.room, Some(true));
        assert_eq!(big.split, "2,2,1");
        assert_eq!(big.context, 16_384);
        let small = &view.models[1];
        assert_eq!(small.split, "0,1,0");
        assert_eq!(small.memory_gb, 28.8);
        assert_eq!(small.room, Some(false), "28.8 GB was taken as room for 40");
    }

    #[test]
    fn the_plan_is_kept_for_this_account_alone_and_read_back() {
        let place = folder("kept");
        let rig = Rig::new(place.join("rig.json"));
        assert_eq!(rig.view().plan, Plan::default(), "a new rig has a plan");
        let view = rig
            .save(&Plan {
                cards: vec![card("Card one", 96.0)],
                models: vec![model("Phi", "192.168.1.20:8091", &[(0, 40)])],
            })
            .unwrap();
        assert_eq!(
            Rig::new(place.join("rig.json")).view(),
            view,
            "the plan was not kept"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(place.join("rig.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "other accounts can read the plan");
        }
        // A refused plan changes nothing.
        assert!(
            rig.save(&Plan {
                cards: vec![card("", 1.0)],
                models: Vec::new()
            })
            .is_err()
        );
        assert_eq!(rig.view(), view);
        // A kept plan that no longer passes is an empty one.
        fs::write(
            place.join("rig.json"),
            r#"{"cards":[{"name":"","memoryGb":1}],"models":[]}"#,
        )
        .unwrap();
        assert_eq!(
            Rig::new(place.join("rig.json")).view().plan,
            Plan::default()
        );
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn two_rig_models_answer_at_once() {
        let place = folder("together");
        let arrived = Arc::new(AtomicUsize::new(0));
        let (one, _) = fake(Answer::Together(2), arrived.clone());
        let (two, _) = fake(Answer::Together(2), arrived);
        let rig = rig_with(
            &place,
            vec![
                model("One", &one, &[(0, 50)]),
                model("Two", &two, &[(1, 50)]),
            ],
        );
        let lan = lan(&place, true);
        let (first, second) = block_on(async {
            tokio::join!(asked(&lan, &rig, "rig:m1"), asked(&lan, &rig, "rig:m2"))
        });
        assert_eq!(
            first.unwrap().answer,
            "together",
            "the second waited for the first"
        );
        assert_eq!(second.unwrap().answer, "together");
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn one_rig_model_that_cannot_be_reached_holds_up_no_other() {
        let place = folder("unreached");
        let (one, _) = fake(Answer::Together(1), Arc::new(AtomicUsize::new(0)));
        let closed = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("127.0.0.1:{}", listener.local_addr().unwrap().port())
        };
        let rig = rig_with(
            &place,
            vec![
                model("Away", &closed, &[(0, 50)]),
                model("Here", &one, &[(1, 50)]),
            ],
        );
        let lan = lan(&place, true);
        let (away, here) = block_on(async {
            tokio::join!(asked(&lan, &rig, "rig:m1"), asked(&lan, &rig, "rig:m2"))
        });
        assert_eq!(away.unwrap_err(), "The engine is not answering.");
        assert_eq!(here.unwrap().answer, "together");
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn a_rig_model_is_asked_only_while_the_lan_gate_is_on() {
        let place = folder("gate");
        let arrived = Arc::new(AtomicUsize::new(0));
        let (one, seen) = fake(Answer::Together(1), arrived);
        let rig = rig_with(&place, vec![model("One", &one, &[(0, 50)])]);
        let error = ask(&lan(&place, false), &rig, "rig:m1").unwrap_err();
        assert!(error.starts_with("The LAN gate is off."), "{error}");
        assert!(
            seen.lock().unwrap().is_empty(),
            "the server was reached with the gate off"
        );
        assert_eq!(ask(&lan(&place, true), &rig, "rig:m9").unwrap_err(), GONE);
        let asked = ask(&lan(&place, true), &rig, "rig:m1").unwrap();
        assert_eq!(asked.answer, "together");
        // The server is asked for its own model id, on the app's list.
        assert!(seen.lock().unwrap().contains(r#""model":"phi-4-q4""#));
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn turning_the_lan_gate_off_stops_a_rig_answer() {
        let place = folder("closing");
        let (one, _) = fake(Answer::Endless, Arc::new(AtomicUsize::new(0)));
        let rig = rig_with(&place, vec![model("One", &one, &[(0, 50)])]);
        let lan = lan(&place, true);
        let error = block_on(async {
            let closing = async {
                tokio::time::sleep(Duration::from_millis(300)).await;
                lan.gate().set(false).unwrap();
            };
            let (answer, ()) = tokio::time::timeout(Duration::from_secs(10), async {
                tokio::join!(asked(&lan, &rig, "rig:m1"), closing)
            })
            .await
            .expect("the answer went on after the gate was turned off");
            answer.unwrap_err()
        });
        assert!(error.starts_with("The LAN gate was turned off."), "{error}");
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn the_key_goes_in_its_header_and_never_to_the_window() {
        let place = folder("key");
        // A made-up key for the test, never a real one.
        let fake_key = "rig-test-key-0000000000000000";
        let key_file = place.join("rig.key");
        fs::write(&key_file, format!("{fake_key}\n")).unwrap();
        let (one, seen) = fake(Answer::Together(1), Arc::new(AtomicUsize::new(0)));
        let mut keyed = model("One", &one, &[(0, 50)]);
        keyed.key_file = Some(key_file.to_string_lossy().into_owned());
        let rig = rig_with(&place, vec![keyed]);
        ask(&lan(&place, true), &rig, "rig:m1").unwrap();
        let sent = seen.lock().unwrap().to_lowercase();
        assert!(
            sent.contains(&format!("authorization: bearer {fake_key}")),
            "{sent}"
        );
        let shown = serde_json::to_string(&rig.view()).unwrap();
        assert!(!shown.contains(fake_key), "the window was sent the key");
        let kept = fs::read_to_string(place.join("rig.json")).unwrap();
        assert!(!kept.contains(fake_key), "the plan keeps the key itself");
        fs::remove_dir_all(&place).unwrap();
    }

    #[test]
    fn a_picture_never_goes_to_the_rig() {
        let place = folder("picture");
        let (one, seen) = fake(Answer::Together(1), Arc::new(AtomicUsize::new(0)));
        let rig = rig_with(&place, vec![model("One", &one, &[(0, 50)])]);
        let mut request = question();
        request.model = "rig:m1".to_string();
        request.messages[0].images = vec!["data:image/png;base64,AAAA".to_string()];
        let (_stop, mut stopped) = watch::channel(false);
        let error = block_on(run_chat(
            &lan(&place, true),
            &rig,
            &request,
            &mut stopped,
            |_| {},
        ))
        .unwrap_err();
        assert_eq!(error, vision::STAYS_HERE);
        assert!(seen.lock().unwrap().is_empty());
        fs::remove_dir_all(&place).unwrap();
    }
}
