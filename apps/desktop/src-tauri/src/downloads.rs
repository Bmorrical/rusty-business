//! Downloading the app's models onto this computer.
//!
//! When none of the app's models is on this computer, the page offers them
//! with their download sizes, and Download brings one from Hugging Face into
//! the models folder the engine reads. The engine lists it the next time it is
//! asked, so nothing needs a restart.
//!
//! Six rules live here:
//! - only a model on the app's list is downloaded, at the revision pinned in
//!   `pinned-models.json` on a Mac, or in `pinned-gguf-models.json` on Windows
//!   and Linux, and only the files listed there;
//! - only from Hugging Face, over https: a redirect anywhere else is refused,
//!   and no proxy, account, token, or cookie is used;
//! - every file's size and SHA-256 are checked as it arrives, and a mismatch
//!   throws the whole download away;
//! - the models folder is only added to: a finished model moves in whole, and
//!   only where there is no folder for it yet;
//! - one download at a time, and Cancel stops it and leaves nothing behind;
//! - nothing is asked for while the outbound gate is off, and turning it off
//!   stops a download as Cancel does, then says why.
//!
//! A download is put together in the app's own folder, beside `sessions`, and
//! moves into the models folder in one step once every file has been checked.
//! Nothing already in the models folder is written over, moved, or removed.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::future::Future;
use std::io::{BufWriter, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::ipc::Channel;
use tokio::sync::watch;

use crate::gate::{Gate, LanGate};
use crate::{engine, models, remote};

/// Where the app downloads from.
const SOURCE: &str = "https://huggingface.co";

/// Hugging Face's own hosts. A download may be handed on to a host under one
/// of these, and to nowhere else.
const HOSTS: [&str; 2] = ["huggingface.co", "hf.co"];

/// How many times one file may be handed on. Hugging Face uses one or two.
const MOST_HOPS: usize = 5;

/// Room left on the disk after a download, so a download never fills it.
const ROOM: u64 = 2_000_000_000;

/// How often the page is told how far a download has come.
const EVERY: Duration = Duration::from_millis(250);

/// How long the app waits to reach Hugging Face.
const CONNECT_WAIT: Duration = Duration::from_secs(15);

/// How long the app waits for the next part of a file once it is coming.
const READ_WAIT: Duration = Duration::from_secs(60);

/// Everything that is not a model's folder in the app's own download folder
/// is left alone.
const STAGED: &str = "models--";

// ---------------------------------------------------------------------------
// The models the app can download
// ---------------------------------------------------------------------------

/// One model the app can download: its id on Hugging Face, the revision the
/// app was tried with, and every file in it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pinned {
    pub id: String,
    pub revision: String,
    pub files: Vec<PinnedFile>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

impl Pinned {
    /// The whole download, in bytes.
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|file| file.bytes).sum()
    }
}

/// The models the app can download on a Mac, as MLX models. The list is built
/// into the app, and its tests check it.
static PINNED: LazyLock<Vec<Pinned>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../pinned-models.json")).unwrap_or_default()
});

/// The same models as GGUF files, which `llama-server` serves on Windows and
/// Linux: from the maker's own copy first, then ggml-org's.
static GGUF_PINNED: LazyLock<Vec<Pinned>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../pinned-gguf-models.json")).unwrap_or_default()
});

/// The list for this system's engine.
fn pins_here() -> &'static [Pinned] {
    pins_for(models::Layout::here())
}

/// The list for the engine that reads models laid out this way.
fn pins_for(layout: models::Layout) -> &'static [Pinned] {
    match layout {
        models::Layout::Hub => &PINNED,
        models::Layout::Gguf => &GGUF_PINNED,
    }
}

/// True when a pin can be used as it is: an id with one slash, a revision that
/// is a whole commit, and plain file names, each with its size and SHA-256.
fn usable(pin: &Pinned) -> bool {
    let hex_of = |text: &str, length: usize| {
        text.len() == length
            && text
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    };
    let plain = |text: &str| {
        !text.is_empty()
            && !text.starts_with('.')
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    };
    let mut parts = pin.id.split('/');
    let id = matches!((parts.next(), parts.next(), parts.next()), (Some(owner), Some(name), None) if plain(owner) && plain(name));
    let names: BTreeSet<&str> = pin.files.iter().map(|file| file.name.as_str()).collect();
    id && hex_of(&pin.revision, 40)
        && !pin.files.is_empty()
        && names.len() == pin.files.len()
        && pin
            .files
            .iter()
            .all(|file| plain(&file.name) && file.bytes > 0 && hex_of(&file.sha256, 64))
}

/// The pin with exactly this id, when it is usable and on the app's list.
fn pinned<'a>(pins: &'a [Pinned], id: &str) -> Option<&'a Pinned> {
    pins.iter().find(|pin| pin.id == id).filter(|pin| {
        usable(pin) && !engine::allowed_models(std::slice::from_ref(&pin.id)).is_empty()
    })
}

/// The name the app gives a model, or its id when it has none.
fn name_of(id: &str) -> String {
    engine::allowed_models(&[id.to_string()])
        .into_iter()
        .next()
        .map_or_else(|| id.to_string(), |model| model.name)
}

/// A model the page can offer, with the size of its download.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub id: String,
    pub name: String,
    pub maker: String,
    pub license: String,
    pub bytes: u64,
}

/// Every model the app can download for this system's engine, in the list's
/// order. The page shows them only while none of them is on the disk.
pub fn offers() -> Vec<Offer> {
    offers_from(pins_here())
}

pub fn remote_offers() -> Vec<Offer> {
    offers_from(&GGUF_PINNED)
}

pub(crate) fn remote_model(id: &str) -> Option<String> {
    let pin = pinned(&GGUF_PINNED, id)?;
    let file = pin.files.first()?.name.strip_suffix(".gguf")?;
    // llama.cpp indexes cached models by the token at the end of the
    // filename. This is usually the quantization, and is IT for Gemma.
    let (_, tag) = file.rsplit_once('-')?;
    Some(format!("{id}:{}", tag.to_ascii_uppercase()))
}

fn offers_from(pins: &[Pinned]) -> Vec<Offer> {
    let ids: Vec<String> = pins.iter().map(|pin| pin.id.clone()).collect();
    engine::allowed_models(&ids)
        .into_iter()
        .filter_map(|model| {
            let bytes = pinned(pins, &model.id)?.bytes();
            let license = engine::license_of(&model.name)?.to_string();
            Some(Offer {
                id: model.id,
                name: model.name,
                maker: model.maker,
                license,
                bytes,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The plan, checked before anything is downloaded
// ---------------------------------------------------------------------------

/// What the models folder holds where the model's folder would go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum There {
    Nothing,
    Whole,
    /// Something that is not a whole model: a download that stopped, or
    /// anything else.
    Part,
}

/// What the app knows before it downloads anything.
#[derive(Clone, Debug, PartialEq)]
pub struct Facts {
    /// The model is on the app's list, and pinned.
    pub listed: bool,
    /// The app could work out where the models folder is.
    pub hub: bool,
    pub there: There,
    /// Another download is running.
    pub busy: bool,
    /// The app's own folder and the models folder are on one disk, so a
    /// finished model can move across in one step.
    pub same_disk: bool,
    /// Free space on that disk, when the app could find out.
    pub free: Option<u64>,
    /// What the download needs: its own size, and room to spare.
    pub needed: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Why {
    NotListed,
    NoModelsFolder,
    Busy,
    PartThere,
    OtherDisk,
    NoRoom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// The model is there already.
    Nothing,
    Fetch,
    Cannot(Why),
}

pub fn plan(facts: &Facts) -> Step {
    if !facts.listed {
        return Step::Cannot(Why::NotListed);
    }
    if !facts.hub {
        return Step::Cannot(Why::NoModelsFolder);
    }
    if facts.there == There::Whole {
        return Step::Nothing;
    }
    if facts.busy {
        return Step::Cannot(Why::Busy);
    }
    if facts.there == There::Part {
        return Step::Cannot(Why::PartThere);
    }
    if !facts.same_disk {
        return Step::Cannot(Why::OtherDisk);
    }
    if facts.free.is_some_and(|free| free < facts.needed) {
        return Step::Cannot(Why::NoRoom);
    }
    Step::Fetch
}

/// Why nothing was downloaded, in one sentence the page shows as it is.
pub fn sentence(why: Why, name: &str, needed: u64) -> String {
    match why {
        Why::NotListed => "That model is not on this app's list.".to_string(),
        Why::NoModelsFolder => "The app could not work out where the models folder is.".to_string(),
        Why::Busy => "Another model is downloading, and the app downloads one at a time.".to_string(),
        Why::PartThere => format!("The models folder already holds part of {name}, and the app does not change what is there."),
        Why::OtherDisk => "The models folder is on a different disk from the app's own folder, which the app cannot download to yet.".to_string(),
        Why::NoRoom => format!("{name} needs {} free on the disk, and there is less than that.", size(needed)),
    }
}

fn size(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

fn facts(
    pin: Option<&Pinned>,
    layout: models::Layout,
    hub: Option<&Path>,
    staging: &Path,
    busy: bool,
) -> Facts {
    let there = match (pin, hub) {
        (Some(pin), Some(hub)) => what_is_there(layout, &hub.join(layout.folder_name(&pin.id))),
        _ => There::Nothing,
    };
    Facts {
        listed: pin.is_some(),
        hub: hub.is_some(),
        there,
        busy,
        same_disk: hub.is_some_and(|hub| same_disk(hub, staging)),
        free: free_bytes(staging),
        needed: pin.map_or(0, |pin| pin.bytes().saturating_add(ROOM)),
    }
}

/// Anything at all where the folder would go, even a link to nothing, is
/// there. Only a whole model counts as the model.
fn what_is_there(layout: models::Layout, folder: &Path) -> There {
    match fs::symlink_metadata(folder) {
        Err(error) if error.kind() == ErrorKind::NotFound => There::Nothing,
        _ if layout.whole(folder) => There::Whole,
        _ => There::Part,
    }
}

/// The nearest folder that is there, at `folder` or above it.
#[cfg(unix)]
fn nearest(folder: &Path) -> Option<&Path> {
    folder.ancestors().find(|above| above.exists())
}

#[cfg(unix)]
fn same_disk(one: &Path, other: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let disk = |folder: &Path| {
        nearest(folder)
            .and_then(|found| fs::metadata(found).ok())
            .map(|found| found.dev())
    };
    matches!((disk(one), disk(other)), (Some(one), Some(other)) if one == other)
}

#[cfg(not(unix))]
fn same_disk(_: &Path, _: &Path) -> bool {
    true
}

/// The space this computer's user may still fill on the disk that holds
/// `folder`.
#[cfg(unix)]
#[allow(clippy::unnecessary_cast)] // The two fields are different types on different systems.
fn free_bytes(folder: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;
    let name = CString::new(nearest(folder)?.as_os_str().as_bytes()).ok()?;
    let mut disk = MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `name` is a C string that lives through the call, and `disk` is
    // space for the one record statvfs fills in.
    if unsafe { libc::statvfs(name.as_ptr(), disk.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: statvfs returned 0, so it filled the record in.
    let disk = unsafe { disk.assume_init() };
    Some((disk.f_bavail as u64).saturating_mul(disk.f_frsize as u64))
}

#[cfg(not(unix))]
fn free_bytes(_: &Path) -> Option<u64> {
    None
}

// ---------------------------------------------------------------------------
// Hugging Face, and nowhere else
// ---------------------------------------------------------------------------

/// True for an https address on one of Hugging Face's hosts, with no account
/// in it and no port of its own.
fn from_hugging_face(url: &reqwest::Url) -> bool {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return false;
    }
    // `domain` is empty for an address given as numbers.
    let Some(host) = url.domain() else {
        return false;
    };
    HOSTS.iter().any(|own| {
        host == *own
            || host
                .strip_suffix(own)
                .is_some_and(|above| above.ends_with('.'))
    })
}

/// A redirect is followed only to Hugging Face, and only a few times.
/// `previous` counts the addresses already asked, the first one included.
fn may_follow(next: &reqwest::Url, previous: usize) -> bool {
    previous <= MOST_HOPS && from_hugging_face(next)
}

fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if may_follow(attempt.url(), attempt.previous().len()) {
            attempt.follow()
        } else {
            attempt.error("sent off Hugging Face")
        }
    })
}

/// What Download says while the outbound gate is off.
const GATE_OFF: &str = "The WAN gate is off. Turn it on to download a model.";

/// What a download says when the gate is turned off part way.
const GATE_TURNED_OFF: &str = "The WAN gate was turned off, so the download stopped.";

/// The one client downloads use, through the outbound gate. It sends no
/// account, token, or cookie: no such thing is ever added to a request here,
/// and it keeps no cookies.
pub(crate) fn client(gate: &Gate) -> Result<reqwest::Client, String> {
    gate.client(GATE_OFF, |builder| {
        builder
            .no_proxy()
            .https_only(true)
            .redirect(redirect_policy())
            .referer(false)
            .user_agent(concat!("rusty-business/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(CONNECT_WAIT)
            .read_timeout(READ_WAIT)
    })
}

fn file_url(pin: &Pinned, file: &PinnedFile) -> String {
    format!("{SOURCE}/{}/resolve/{}/{}", pin.id, pin.revision, file.name)
}

/// A file as it arrives.
pub(crate) type Chunks = Pin<Box<dyn Stream<Item = Result<Vec<u8>, String>> + Send>>;

/// A file being asked for. The tests ask a stand-in instead of Hugging Face.
pub(crate) type Opening = Pin<Box<dyn Future<Output = Result<Chunks, String>> + Send>>;

const SENT_ELSEWHERE: &str = "Hugging Face sent the download somewhere else, so the app stopped.";

pub(crate) fn open_url(client: reqwest::Client, url: String) -> Opening {
    Box::pin(async move {
        let response = client.get(&url).send().await.map_err(|error| {
            if error.is_redirect() {
                SENT_ELSEWHERE.to_string()
            } else {
                "The app could not reach Hugging Face.".to_string()
            }
        })?;
        // The redirect rule already holds to this. Checked once more here.
        if !from_hugging_face(response.url()) {
            return Err(SENT_ELSEWHERE.to_string());
        }
        if !response.status().is_success() {
            return Err(format!(
                "Hugging Face answered {}.",
                response.status().as_u16()
            ));
        }
        let chunks = response.bytes_stream().map(|chunk| {
            chunk
                .map(|bytes| bytes.to_vec())
                .map_err(|error| error.to_string())
        });
        Ok(Box::pin(chunks) as Chunks)
    })
}

// ---------------------------------------------------------------------------
// Downloading, with every file checked as it arrives
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Fetched {
    Whole,
    Stopped,
}

/// How far a download has come, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

/// Finishes when Cancel is pressed, and never otherwise.
pub(crate) async fn cancelled(cancel: &mut watch::Receiver<bool>) {
    while !*cancel.borrow_and_update() {
        if cancel.changed().await.is_err() {
            // The app is going away, and the download with it.
            return;
        }
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Writes one file into `snapshot`, checking its size and SHA-256 as it
/// arrives. Until both match, it is only a `.part` file.
async fn fetch_file(
    mut chunks: Chunks,
    file: &PinnedFile,
    snapshot: &Path,
    cancel: &mut watch::Receiver<bool>,
    moved: &mut impl FnMut(u64),
) -> Result<Fetched, String> {
    let cannot_write =
        |error: std::io::Error| format!("The app could not write {}: {error}.", file.name);
    let wrong_size = || {
        format!(
            "{} was not the size the app expects, so the download was thrown away.",
            file.name
        )
    };
    let landing = snapshot.join(format!("{}.part", file.name));
    let opened = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&landing)
        .map_err(cannot_write)?;
    let mut out = BufWriter::with_capacity(1 << 20, opened);
    let mut hash = Sha256::new();
    let mut count: u64 = 0;
    loop {
        let next = tokio::select! {
            biased;
            _ = cancelled(cancel) => return Ok(Fetched::Stopped),
            next = chunks.next() => next,
        };
        let Some(chunk) = next else {
            break;
        };
        let chunk = chunk.map_err(|_| {
            format!(
                "The connection to Hugging Face dropped during {}.",
                file.name
            )
        })?;
        count = count.saturating_add(chunk.len() as u64);
        if count > file.bytes {
            return Err(wrong_size());
        }
        hash.update(&chunk);
        out.write_all(&chunk).map_err(cannot_write)?;
        moved(chunk.len() as u64);
    }
    out.flush().map_err(cannot_write)?;
    drop(out);
    if count != file.bytes {
        return Err(wrong_size());
    }
    if hex(hash.finalize().as_slice()) != file.sha256 {
        return Err(format!(
            "{} did not match the checksum the app expects, so the download was thrown away.",
            file.name
        ));
    }
    fs::rename(&landing, snapshot.join(&file.name)).map_err(cannot_write)?;
    Ok(Fetched::Whole)
}

/// Puts a whole model together in `staged`, laid out the way the models
/// folder lays out a model. In the Hugging Face cache, that is its files
/// under `snapshots/<revision>`, and `refs/main` naming that revision. In the
/// app's own models folder, it is the files alone.
async fn fetch_model(
    pin: &Pinned,
    layout: models::Layout,
    staged: &Path,
    open: &mut impl FnMut(String) -> Opening,
    cancel: &mut watch::Receiver<bool>,
    report: &mut impl FnMut(Progress),
) -> Result<Fetched, String> {
    let cannot = |error: std::io::Error| format!("The app could not write the download: {error}.");
    let snapshot = match layout {
        models::Layout::Hub => staged.join("snapshots").join(&pin.revision),
        models::Layout::Gguf => staged.to_path_buf(),
    };
    fs::create_dir_all(&snapshot).map_err(cannot)?;
    if layout == models::Layout::Hub {
        fs::create_dir_all(staged.join("refs")).map_err(cannot)?;
    }
    if fetch_files(pin, &snapshot, open, cancel, report).await? == Fetched::Stopped {
        return Ok(Fetched::Stopped);
    }
    if layout == models::Layout::Hub {
        // No line ending: the engine reads the revision exactly as written.
        fs::write(staged.join("refs").join("main"), &pin.revision).map_err(cannot)?;
    }
    if !layout.whole(staged) {
        return Err(
            "The download finished without a whole model, so it was thrown away.".to_string(),
        );
    }
    Ok(Fetched::Whole)
}

/// Writes every file of a pin into `snapshot`, one after another, each
/// checked as it arrives.
async fn fetch_files(
    pin: &Pinned,
    snapshot: &Path,
    open: &mut impl FnMut(String) -> Opening,
    cancel: &mut watch::Receiver<bool>,
    report: &mut impl FnMut(Progress),
) -> Result<Fetched, String> {
    let total = pin.bytes();
    let mut done = 0;
    let mut told = Instant::now();
    report(Progress { done, total });
    for file in &pin.files {
        let chunks = tokio::select! {
            biased;
            _ = cancelled(cancel) => return Ok(Fetched::Stopped),
            opened = open(file_url(pin, file)) => opened?,
        };
        let mut moved = |bytes: u64| {
            done += bytes;
            if told.elapsed() >= EVERY {
                told = Instant::now();
                report(Progress { done, total });
            }
        };
        if fetch_file(chunks, file, snapshot, cancel, &mut moved).await? == Fetched::Stopped {
            return Ok(Fetched::Stopped);
        }
        report(Progress { done, total });
    }
    Ok(Fetched::Whole)
}

/// Moves a finished model into the models folder in one step, and only where
/// there is no folder for it yet.
fn move_in(staged: &Path, hub: &Path, folder: &str, name: &str) -> Result<(), String> {
    let cannot = |error: std::io::Error| {
        format!("The app could not move {name} into the models folder: {error}.")
    };
    let taken = || {
        format!(
            "Something put a folder for {name} in the models folder while it downloaded, so the app left that folder as it is."
        )
    };
    fs::create_dir_all(hub).map_err(cannot)?;
    let target = hub.join(folder);
    if fs::symlink_metadata(&target).is_ok() {
        return Err(taken());
    }
    rename_new(staged, &target).map_err(|error| {
        if error.kind() == ErrorKind::AlreadyExists {
            taken()
        } else {
            cannot(error)
        }
    })
}

/// Renames `from` to `to`, and refuses when anything is at `to` already, even
/// an empty folder, so nothing is ever written over.
#[cfg(target_os = "macos")]
fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let from = CString::new(from.as_os_str().as_bytes())?;
    let to = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both are C strings that live through the call.
    if unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Elsewhere the check in `move_in` is the guard. A plain rename still never
/// writes over a folder with anything in it.
#[cfg(not(target_os = "macos"))]
fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

// ---------------------------------------------------------------------------
// One download at a time
// ---------------------------------------------------------------------------

/// What a download came to.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadReport {
    /// The model is whole in the models folder.
    pub whole: bool,
    /// The download was stopped. The remote server may keep partial files.
    pub stopped: bool,
}

pub struct Downloads {
    /// The app's own folder for downloads still being put together.
    staging: PathBuf,
    /// The running download's Cancel, when one is running.
    running: Mutex<Option<ActiveDownload>>,
}

struct ActiveDownload {
    cancel: watch::Sender<bool>,
    remote: bool,
}

/// Frees the slot when a download ends, however it ends.
struct Running<'a>(&'a Downloads);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.slot().take();
    }
}

impl Downloads {
    pub fn new(staging: PathBuf) -> Self {
        Self {
            staging,
            running: Mutex::new(None),
        }
    }

    fn slot(&self) -> MutexGuard<'_, Option<ActiveDownload>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// True while a download is running.
    pub fn running(&self) -> bool {
        self.slot().is_some()
    }

    /// Stops the download that is running, if one is.
    pub fn stop(&self) {
        if let Some(active) = self.slot().as_ref() {
            let _ = active.cancel.send(true);
        }
    }

    /// The WAN gate can stop a local transfer without affecting a LAN install.
    pub fn stop_local(&self) {
        if let Some(active) = self.slot().as_ref().filter(|active| !active.remote) {
            let _ = active.cancel.send(true);
        }
    }

    async fn download_remote(
        &self,
        lan: &LanGate,
        base: &str,
        id: &str,
        report: impl FnMut(Progress),
    ) -> Result<DownloadReport, String> {
        lan.run(async {
            let model =
                remote_model(id).ok_or_else(|| sentence(Why::NotListed, &name_of(id), 0))?;
            let mut cancel = {
                let mut slot = self.slot();
                if slot.is_some() {
                    return Err(sentence(Why::Busy, &name_of(id), 0));
                }
                let (sender, receiver) = watch::channel(false);
                *slot = Some(ActiveDownload {
                    cancel: sender,
                    remote: true,
                });
                receiver
            };
            let _running = Running(self);
            remote::download(base, &model, &mut cancel, report).await
        })
        .await
    }

    /// Throws away what downloads left in the app's own folder: a download
    /// that stopped, failed, or was running when the app last quit. Nothing
    /// outside that folder is touched, and a link is removed, not followed.
    pub fn clear(&self) {
        let Ok(entries) = fs::read_dir(&self.staging) else {
            return;
        };
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(STAGED))
            {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }

    async fn download(
        &self,
        id: &str,
        pins: &[Pinned],
        layout: models::Layout,
        hub: Option<PathBuf>,
        mut open: impl FnMut(String) -> Opening,
        mut report: impl FnMut(Progress),
    ) -> Result<DownloadReport, String> {
        let pin = pinned(pins, id);
        let name = name_of(id);
        let mut cancel = {
            let mut slot = self.slot();
            let facts = facts(pin, layout, hub.as_deref(), &self.staging, slot.is_some());
            match plan(&facts) {
                Step::Nothing => {
                    return Ok(DownloadReport {
                        whole: true,
                        stopped: false,
                    });
                }
                Step::Cannot(why) => return Err(sentence(why, &name, facts.needed)),
                Step::Fetch => {}
            }
            let (sender, receiver) = watch::channel(false);
            *slot = Some(ActiveDownload {
                cancel: sender,
                remote: false,
            });
            receiver
        };
        let _running = Running(self);
        // The plan fetches only a model that is pinned, into a folder it knows.
        let (Some(pin), Some(hub)) = (pin, hub) else {
            return Err(sentence(Why::NotListed, &name, 0));
        };
        // Staged under the cache's name on every system, so `clear` knows it.
        let staged = self.staging.join(models::folder_name(&pin.id));
        let folder = layout.folder_name(&pin.id);
        self.clear();
        let fetched = fetch_model(pin, layout, &staged, &mut open, &mut cancel, &mut report).await;
        let result = match fetched {
            Ok(Fetched::Whole) => move_in(&staged, &hub, &folder, &name).map(|()| DownloadReport {
                whole: true,
                stopped: false,
            }),
            Ok(Fetched::Stopped) => Ok(DownloadReport {
                whole: false,
                stopped: true,
            }),
            Err(error) => Err(error),
        };
        self.clear();
        result
    }

    /// Downloads files the app uses itself, not a chat model: every file of
    /// `pin` into `home`, as the folder `folder`, by the same rules as a
    /// model. They are put together in the app's own download folder and
    /// move in whole, only where nothing is there yet. Nothing is asked for
    /// while the outbound gate is off.
    #[expect(
        clippy::too_many_arguments,
        reason = "what to fetch and where it goes, plus the gate it waits on"
    )]
    pub(crate) async fn download_own(
        &self,
        gate: &Gate,
        pin: &Pinned,
        name: &str,
        home: &Path,
        folder: &str,
        mut open: impl FnMut(String) -> Opening,
        mut report: impl FnMut(Progress),
    ) -> Result<DownloadReport, String> {
        if !gate.is_open() {
            return Err(GATE_OFF.to_string());
        }
        if !usable(pin) || !models::plain_name(folder) {
            return Err(sentence(Why::NotListed, name, 0));
        }
        let mut cancel = {
            let mut slot = self.slot();
            let there = what_is_there_own(pin, &home.join(folder));
            let facts = Facts {
                listed: true,
                hub: true,
                there,
                busy: slot.is_some(),
                same_disk: same_disk(home, &self.staging),
                free: free_bytes(&self.staging),
                needed: pin.bytes().saturating_add(ROOM),
            };
            match plan(&facts) {
                Step::Nothing => {
                    return Ok(DownloadReport {
                        whole: true,
                        stopped: false,
                    });
                }
                Step::Cannot(Why::PartThere) => {
                    return Err(format!(
                        "The app's folder already holds part of {name}, and the app does not change what is there."
                    ));
                }
                Step::Cannot(why) => return Err(sentence(why, name, facts.needed)),
                Step::Fetch => {}
            }
            let (sender, receiver) = watch::channel(false);
            *slot = Some(ActiveDownload {
                cancel: sender,
                remote: false,
            });
            receiver
        };
        let _running = Running(self);
        let watching = gate.clone();
        let mut gated = move |url: String| -> Opening {
            if watching.is_open() {
                open(url)
            } else {
                Box::pin(async { Err(GATE_TURNED_OFF.to_string()) })
            }
        };
        // Staged under the cache's kind of name, so `clear` knows it.
        let staged = self.staging.join(models::folder_name(&pin.id));
        self.clear();
        let fetched = match fs::create_dir_all(&staged) {
            Ok(()) => fetch_files(pin, &staged, &mut gated, &mut cancel, &mut report).await,
            Err(error) => Err(format!("The app could not write the download: {error}.")),
        };
        let result = match fetched {
            Ok(Fetched::Whole) => move_in(&staged, home, folder, name).map(|()| DownloadReport {
                whole: true,
                stopped: false,
            }),
            Ok(Fetched::Stopped) if !gate.is_open() => Err(GATE_TURNED_OFF.to_string()),
            Ok(Fetched::Stopped) => Ok(DownloadReport {
                whole: false,
                stopped: true,
            }),
            Err(error) => Err(error),
        };
        self.clear();
        result
    }
}

/// What is where files the app uses itself would go: whole only when every
/// file of `pin` is there at its size.
fn what_is_there_own(pin: &Pinned, folder: &Path) -> There {
    match fs::symlink_metadata(folder) {
        Err(error) if error.kind() == ErrorKind::NotFound => There::Nothing,
        _ if whole_own(pin, folder) => There::Whole,
        _ => There::Part,
    }
}

/// True when every file of `pin` is in `folder`, as a plain file at its size.
pub(crate) fn whole_own(pin: &Pinned, folder: &Path) -> bool {
    pin.files.iter().all(|file| {
        fs::symlink_metadata(folder.join(&file.name))
            .is_ok_and(|meta| meta.is_file() && meta.len() == file.bytes)
    })
}

/// A download while the outbound gate stays on. Each file is asked for only
/// while it is on, and a download stopped by turning it off says so.
#[expect(
    clippy::too_many_arguments,
    reason = "the download's own arguments, plus the gate it waits on"
)]
async fn download_through(
    downloads: &Downloads,
    gate: &Gate,
    id: &str,
    pins: &[Pinned],
    layout: models::Layout,
    hub: Option<PathBuf>,
    mut open: impl FnMut(String) -> Opening,
    report: impl FnMut(Progress),
) -> Result<DownloadReport, String> {
    if !gate.is_open() {
        return Err(GATE_OFF.to_string());
    }
    let watching = gate.clone();
    let gated = move |url: String| -> Opening {
        if watching.is_open() {
            open(url)
        } else {
            Box::pin(async { Err(GATE_TURNED_OFF.to_string()) })
        }
    };
    let done = downloads
        .download(id, pins, layout, hub, gated, report)
        .await?;
    if done.stopped && !gate.is_open() {
        return Err(GATE_TURNED_OFF.to_string());
    }
    Ok(done)
}

#[tauri::command]
pub async fn download_model(
    downloads: tauri::State<'_, Downloads>,
    gate: tauri::State<'_, Gate>,
    lan: tauri::State<'_, LanGate>,
    id: String,
    on_progress: Channel<Progress>,
) -> Result<DownloadReport, String> {
    let base = engine::base_url()?;
    let report = move |progress: Progress| {
        let _ = on_progress.send(progress);
    };
    if engine::is_remote(&base) {
        return downloads.download_remote(&lan, &base, &id, report).await;
    }
    let hub = models::models_folder();
    let client = client(&gate)?;
    let open = move |url: String| open_url(client.clone(), url);
    let layout = models::Layout::here();
    let done = download_through(
        &downloads,
        &gate,
        &id,
        pins_here(),
        layout,
        hub,
        open,
        report,
    )
    .await?;
    if reloads_after(layout, &done) {
        engine::reload_models(&base).await;
    }
    Ok(done)
}

/// True when the engine is asked to read its models folder again: once a
/// model has moved in whole, on Windows and Linux. `llama-server` reads the
/// folder when it starts, so a model downloaded while it runs is not listed
/// until then. On a Mac, `mlx_lm.server` looks in the cache each time.
fn reloads_after(layout: models::Layout, done: &DownloadReport) -> bool {
    layout == models::Layout::Gguf && done.whole
}

#[tauri::command]
pub fn stop_download(downloads: tauri::State<'_, Downloads>) {
    downloads.stop();
}

#[cfg(test)]
mod tests {
    // Brad, on the first run: "Picker with sizes (Recommended)". On what the
    // app does by itself: "it should search and up the local models and stack
    // ... one click ready". And on the engine: "Inside the app (Recommended)",
    // so the picker downloads models only, from Hugging Face only, and only
    // when Download is pressed.

    use super::*;
    use crate::models::Layout;
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    use tauri::async_runtime::block_on;

    #[test]
    fn a_closed_lan_gate_never_starts_a_server_download() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let downloads = Downloads::new(PathBuf::new());
        let lan = LanGate::new(PathBuf::new());
        assert!(
            block_on(downloads.download_remote(&lan, &base, "microsoft/phi-4-gguf", |_| {}))
                .unwrap_err()
                .contains("The LAN gate is off")
        );
        listener.set_nonblocking(true).unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert!(!downloads.running());
    }

    #[test]
    fn wan_shutdown_only_cancels_downloads_onto_this_computer() {
        let downloads = Downloads::new(PathBuf::new());
        for remote in [false, true] {
            let (cancel, signal) = watch::channel(false);
            *downloads.slot() = Some(ActiveDownload { cancel, remote });
            downloads.stop_local();
            assert_eq!(*signal.borrow(), !remote);
            downloads.stop();
            assert!(*signal.borrow());
        }
    }

    const PHI: &str = "mlx-community/phi-4-4bit";
    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

    type Files = Vec<(String, Vec<u8>)>;

    fn scratch() -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("rusty-business-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn sha(body: &[u8]) -> String {
        hex(Sha256::digest(body).as_slice())
    }

    /// A small model: the files the engine asks for, and one weights file.
    fn files() -> Files {
        vec![
            ("config.json".to_string(), b"{}".to_vec()),
            (
                "model.safetensors.index.json".to_string(),
                br#"{"weight_map":{"layers.0.weight":"model.safetensors"}}"#.to_vec(),
            ),
            ("tokenizer_config.json".to_string(), b"{}".to_vec()),
            (
                "model.safetensors".to_string(),
                (0..300_000u32).map(|n| (n % 251) as u8).collect(),
            ),
        ]
    }

    fn pin_for(id: &str, files: &Files) -> Pinned {
        Pinned {
            id: id.to_string(),
            revision: REVISION.to_string(),
            files: files
                .iter()
                .map(|(name, body)| PinnedFile {
                    name: name.clone(),
                    bytes: body.len() as u64,
                    sha256: sha(body),
                })
                .collect(),
        }
    }

    fn with_body(files: &Files, name: &str, body: Vec<u8>) -> Files {
        files
            .iter()
            .map(|(file, old)| {
                (
                    file.clone(),
                    if file == name {
                        body.clone()
                    } else {
                        old.clone()
                    },
                )
            })
            .collect()
    }

    /// Serves `files` from memory in small parts, and keeps every address it
    /// is asked for. As with Hugging Face, a file is asked for only once the
    /// app waits on it.
    fn serving(files: Files, asked: Arc<Mutex<Vec<String>>>) -> impl FnMut(String) -> Opening {
        move |url: String| {
            let asked = asked.clone();
            let body = files
                .iter()
                .find(|(name, _)| url.ends_with(&format!("/{name}")))
                .map(|(_, body)| body.clone());
            Box::pin(async move {
                asked.lock().unwrap().push(url);
                let body = body.ok_or_else(|| "Hugging Face answered 404.".to_string())?;
                let parts: Vec<Result<Vec<u8>, String>> =
                    body.chunks(65_536).map(|part| Ok(part.to_vec())).collect();
                Ok(Box::pin(futures_util::stream::iter(parts)) as Chunks)
            })
        }
    }

    /// The app's own client, with the gate on.
    fn open_client() -> reqwest::Client {
        client(&Gate::at(true)).unwrap()
    }

    fn never_asked() -> (Arc<Mutex<Vec<String>>>, impl FnMut(String) -> Opening) {
        let asked = Arc::new(Mutex::new(Vec::new()));
        (asked.clone(), serving(Vec::new(), asked))
    }

    /// Every file and folder under `folder`, with its size.
    fn everything_in(folder: &Path) -> Vec<String> {
        let mut seen = Vec::new();
        let mut waiting = vec![folder.to_path_buf()];
        while let Some(next) = waiting.pop() {
            let Ok(entries) = fs::read_dir(&next) else {
                continue;
            };
            for entry in entries.flatten() {
                let found = fs::symlink_metadata(entry.path()).unwrap();
                seen.push(format!(
                    "{} {}",
                    entry.path().strip_prefix(folder).unwrap().display(),
                    found.len()
                ));
                if found.is_dir() {
                    waiting.push(entry.path());
                }
            }
        }
        seen.sort();
        seen
    }

    fn names(found: &[engine::ModelInfo]) -> Vec<&str> {
        found.iter().map(|model| model.name.as_str()).collect()
    }

    /// The models folder, the app's folder, and its downloads, in a scratch
    /// folder of their own.
    struct Place {
        hub: PathBuf,
        staging: PathBuf,
        downloads: Downloads,
    }

    fn place() -> Place {
        let root = scratch();
        let staging = root.join("downloads");
        Place {
            hub: root.join("hub"),
            downloads: Downloads::new(staging.clone()),
            staging,
        }
    }

    fn whole_model(hub: &Path, files: &Files) {
        let folder = hub.join(models::folder_name(PHI));
        let snapshot = folder.join("snapshots").join(REVISION);
        fs::create_dir_all(&snapshot).unwrap();
        fs::create_dir_all(folder.join("refs")).unwrap();
        fs::write(folder.join("refs").join("main"), REVISION).unwrap();
        for (name, body) in files {
            fs::write(snapshot.join(name), body).unwrap();
        }
    }

    fn base() -> Facts {
        Facts {
            listed: true,
            hub: true,
            there: There::Nothing,
            busy: false,
            same_disk: true,
            free: Some(100),
            needed: 50,
        }
    }

    // -- The list ------------------------------------------------------------

    #[test]
    fn every_model_on_the_list_is_pinned_and_nothing_else_is() {
        let read: Vec<Pinned> =
            serde_json::from_str(include_str!("../pinned-models.json")).unwrap();
        assert_eq!(read.len(), PINNED.len());
        assert_eq!(
            names_of(&offers_from(&PINNED)),
            [
                "Phi-4",
                "gpt-oss 20B",
                "Nemotron 3.5 Lightning",
                "Gemma 4 26B",
                "Llama 3.1 8B"
            ]
        );
        let ids: BTreeSet<&str> = PINNED.iter().map(|pin| pin.id.as_str()).collect();
        assert_eq!(ids.len(), PINNED.len(), "each model is pinned once");
        for pin in PINNED.iter() {
            assert!(usable(pin), "{} is not usable as pinned", pin.id);
            assert_eq!(
                engine::allowed_models(std::slice::from_ref(&pin.id)).len(),
                1,
                "{} is on the list",
                pin.id
            );
        }
    }

    fn names_of(offered: &[Offer]) -> Vec<&str> {
        offered.iter().map(|offer| offer.name.as_str()).collect()
    }

    #[test]
    fn each_pinned_model_has_the_files_the_engine_asks_for() {
        for pin in PINNED.iter() {
            let names: BTreeSet<&str> = pin.files.iter().map(|file| file.name.as_str()).collect();
            for needed in [
                "config.json",
                "model.safetensors.index.json",
                "tokenizer_config.json",
            ] {
                assert!(names.contains(needed), "{} has {needed}", pin.id);
            }
            assert!(
                names.iter().any(|name| name.ends_with(".safetensors")),
                "{} has weights",
                pin.id
            );
        }
    }

    #[test]
    fn the_sizes_offered_are_the_whole_downloads() {
        let sizes: Vec<(&str, u64)> = PINNED
            .iter()
            .map(|pin| (pin.id.as_str(), pin.bytes()))
            .collect();
        assert_eq!(
            sizes,
            [
                (PHI, 8_256_384_748),
                ("mlx-community/gpt-oss-20b-MXFP4-Q8", 12_104_215_835),
                (
                    "mlx-community/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-4bit",
                    17_792_581_692
                ),
                ("mlx-community/gemma-4-26b-a4b-it-4bit", 15_373_586_252),
                ("mlx-community/Llama-3.1-8B-Instruct-4bit", 4_534_808_112),
            ]
        );
        let offered: Vec<u64> = offers_from(&PINNED)
            .iter()
            .map(|offer| offer.bytes)
            .collect();
        assert_eq!(
            offered,
            [
                8_256_384_748,
                12_104_215_835,
                17_792_581_692,
                15_373_586_252,
                4_534_808_112
            ]
        );
        let licenses: Vec<String> = offers_from(&PINNED)
            .into_iter()
            .map(|offer| offer.license)
            .collect();
        assert_eq!(
            licenses,
            [
                "MIT",
                "Apache 2.0",
                "OpenMDW 1.1",
                "Apache 2.0",
                "Llama 3.1 Community License"
            ]
        );
    }

    #[test]
    fn a_pin_that_is_not_plain_is_not_used() {
        let good = pin_for(PHI, &files());
        assert!(usable(&good));
        let mut cases: Vec<(&str, Pinned)> = Vec::new();
        let mut bad = |what: &'static str, change: &dyn Fn(&mut Pinned)| {
            let mut pin = good.clone();
            change(&mut pin);
            cases.push((what, pin));
        };
        bad("a revision that is a branch", &|pin| {
            pin.revision = "main".to_string()
        });
        bad("a revision in capitals", &|pin| {
            pin.revision = pin.revision.to_uppercase()
        });
        bad("an id with two slashes", &|pin| {
            pin.id = "mlx-community/phi-4/4bit".to_string()
        });
        bad("an id with no slash", &|pin| {
            pin.id = "phi-4-4bit".to_string()
        });
        bad("a file in a folder", &|pin| {
            pin.files[0].name = "nested/config.json".to_string()
        });
        bad("a file above the folder", &|pin| {
            pin.files[0].name = "..".to_string()
        });
        bad("a hidden file", &|pin| {
            pin.files[0].name = ".config.json".to_string()
        });
        bad("a file named twice", &|pin| {
            pin.files[1].name = pin.files[0].name.clone()
        });
        bad("a file with no size", &|pin| pin.files[0].bytes = 0);
        bad("a checksum in capitals", &|pin| {
            pin.files[0].sha256 = pin.files[0].sha256.to_uppercase()
        });
        bad("a checksum cut short", &|pin| {
            pin.files[0].sha256.truncate(40)
        });
        bad("no files", &|pin| pin.files.clear());
        for (what, pin) in cases {
            assert!(!usable(&pin), "{what}");
            assert!(
                pinned(&[pin], PHI).is_none() || what.starts_with("an id"),
                "{what}"
            );
        }
    }

    // Brad, 2026-11-19: "add an engine to linux and windows distro's", with
    // its models "Maker first, then ggml-org (Recommended)". On Windows and
    // Linux, the same five models come as GGUF files for `llama-server`.

    const FIVE: [&str; 5] = [
        "Phi-4",
        "gpt-oss 20B",
        "Nemotron 3.5 Lightning",
        "Gemma 4 26B",
        "Llama 3.1 8B",
    ];

    #[test]
    fn every_gguf_model_is_pinned_from_the_maker_first_then_ggml_org() {
        // "Maker first, then ggml-org (Recommended)"
        let read: Vec<Pinned> =
            serde_json::from_str(include_str!("../pinned-gguf-models.json")).unwrap();
        assert_eq!(read.len(), GGUF_PINNED.len());
        assert_eq!(
            names_of(&offers_from(&GGUF_PINNED)),
            FIVE,
            "the same five models as on a Mac"
        );
        let owners: Vec<&str> = GGUF_PINNED
            .iter()
            .map(|pin| pin.id.split('/').next().unwrap())
            .collect();
        assert_eq!(
            owners,
            ["microsoft", "ggml-org", "ggml-org", "google", "ggml-org"]
        );
        let makers = [
            ("Microsoft", "microsoft"),
            ("OpenAI", "openai"),
            ("NVIDIA", "nvidia"),
            ("Google", "google"),
            ("Meta", "meta-llama"),
        ];
        for pin in GGUF_PINNED.iter() {
            assert!(usable(pin), "{} is not usable as pinned", pin.id);
            let model = engine::allowed_models(std::slice::from_ref(&pin.id));
            assert_eq!(model.len(), 1, "{} is on the list", pin.id);
            let maker = makers
                .iter()
                .find(|(name, _)| *name == model[0].maker)
                .map(|(_, owner)| *owner)
                .expect("a maker the test knows");
            let owner = pin.id.split('/').next().unwrap();
            assert!(
                owner == maker || owner == "ggml-org",
                "{} comes from {maker} or ggml-org",
                pin.id
            );
        }
    }

    #[test]
    fn each_gguf_pin_is_a_model_llama_server_loads_whole() {
        // "add an engine to linux and windows distro's"
        for pin in GGUF_PINNED.iter() {
            assert!(
                pin.files.iter().all(|file| file.name.ends_with(".gguf")),
                "{} is GGUF files only",
                pin.id
            );
            let folder = scratch().join(Layout::Gguf.folder_name(&pin.id));
            fs::create_dir_all(&folder).unwrap();
            for file in &pin.files {
                fs::write(folder.join(&file.name), "weights").unwrap();
            }
            assert!(
                models::whole_gguf(&folder),
                "{} lands as a whole model",
                pin.id
            );
            // llama-server names the model by its folder, and the app knows
            // the model by that name.
            let by_folder = engine::allowed_models(&[Layout::Gguf.folder_name(&pin.id)]);
            assert_eq!(
                by_folder.first().map(|model| model.name.clone()),
                Some(name_of(&pin.id)),
                "{}",
                pin.id
            );
        }
    }

    #[test]
    fn the_gguf_sizes_offered_are_the_whole_downloads() {
        let offered: Vec<u64> = offers_from(&GGUF_PINNED)
            .iter()
            .map(|offer| offer.bytes)
            .collect();
        assert_eq!(
            offered,
            [
                9_053_114_560,
                12_109_566_624,
                18_898_091_584,
                14_439_363_584,
                6_036_116_480
            ]
        );
        let licenses: Vec<String> = offers_from(&GGUF_PINNED)
            .into_iter()
            .map(|offer| offer.license)
            .collect();
        assert_eq!(
            licenses,
            [
                "MIT",
                "Apache 2.0",
                "OpenMDW 1.1",
                "Apache 2.0",
                "Llama 3.1 Community License"
            ]
        );
    }

    #[test]
    fn every_system_is_offered_the_five_models() {
        // Windows and Linux were offered nothing before llama-server.
        assert_eq!(names_of(&offers()), FIVE);
        let gguf: Vec<&str> = pins_for(Layout::Gguf)
            .iter()
            .map(|pin| pin.id.as_str())
            .collect();
        assert_eq!(gguf.len(), 5);
        assert!(
            gguf.iter().all(|id| id.to_lowercase().contains("gguf")),
            "Windows and Linux get the GGUF list: {gguf:?}"
        );
        let mlx: Vec<&str> = pins_for(Layout::Hub)
            .iter()
            .map(|pin| pin.id.as_str())
            .collect();
        assert!(
            mlx.iter().all(|id| id.starts_with("mlx-community/")),
            "a Mac gets the MLX list: {mlx:?}"
        );
        assert_eq!(
            names_of(&offers_from(pins_for(Layout::Hub))),
            names_of(&offers_from(pins_for(Layout::Gguf)))
        );
    }

    #[test]
    fn only_a_model_on_the_list_is_offered() {
        let files = files();
        let pins = vec![
            pin_for("mlx-community/some-other-model", &files),
            pin_for(PHI, &files),
        ];
        assert_eq!(names_of(&offers_from(&pins)), ["Phi-4"]);
        assert_eq!(offers_from(&pins)[0].bytes, pins[1].bytes());
    }

    // -- The plan ------------------------------------------------------------

    #[test]
    fn the_whole_plan_is_checked_in_order() {
        assert_eq!(plan(&base()), Step::Fetch);
        let cases: [(&str, Facts, Step); 10] = [
            (
                "not on the list",
                Facts {
                    listed: false,
                    ..base()
                },
                Step::Cannot(Why::NotListed),
            ),
            (
                "not on the list beats all",
                Facts {
                    listed: false,
                    hub: false,
                    busy: true,
                    ..base()
                },
                Step::Cannot(Why::NotListed),
            ),
            (
                "no models folder",
                Facts {
                    hub: false,
                    ..base()
                },
                Step::Cannot(Why::NoModelsFolder),
            ),
            (
                "already whole",
                Facts {
                    there: There::Whole,
                    ..base()
                },
                Step::Nothing,
            ),
            (
                "already whole, while busy",
                Facts {
                    there: There::Whole,
                    busy: true,
                    free: Some(0),
                    ..base()
                },
                Step::Nothing,
            ),
            (
                "busy",
                Facts {
                    busy: true,
                    there: There::Part,
                    ..base()
                },
                Step::Cannot(Why::Busy),
            ),
            (
                "part there",
                Facts {
                    there: There::Part,
                    same_disk: false,
                    ..base()
                },
                Step::Cannot(Why::PartThere),
            ),
            (
                "another disk",
                Facts {
                    same_disk: false,
                    free: Some(0),
                    ..base()
                },
                Step::Cannot(Why::OtherDisk),
            ),
            (
                "no room",
                Facts {
                    free: Some(49),
                    ..base()
                },
                Step::Cannot(Why::NoRoom),
            ),
            (
                "free space unknown",
                Facts {
                    free: None,
                    ..base()
                },
                Step::Fetch,
            ),
        ];
        for (what, facts, step) in cases {
            assert_eq!(plan(&facts), step, "{what}");
        }
        assert_eq!(
            plan(&Facts {
                free: Some(50),
                ..base()
            }),
            Step::Fetch,
            "exactly enough room"
        );
    }

    #[test]
    fn each_reason_is_one_sentence() {
        for why in [
            Why::NotListed,
            Why::NoModelsFolder,
            Why::Busy,
            Why::PartThere,
            Why::OtherDisk,
            Why::NoRoom,
        ] {
            let said = sentence(why, "Phi-4", 10_256_384_748);
            assert!(said.ends_with('.') && !said.contains('\n'), "{said:?}");
            assert!(said.starts_with(char::is_uppercase), "{said:?}");
            assert_eq!(said.matches(". ").count(), 0, "one sentence: {said:?}");
        }
        assert_eq!(
            sentence(Why::NoRoom, "Phi-4", 10_256_384_748),
            "Phi-4 needs 10.3 GB free on the disk, and there is less than that."
        );
        assert_eq!(
            sentence(Why::PartThere, "Phi-4", 0),
            "The models folder already holds part of Phi-4, and the app does not change what is there."
        );
    }

    #[test]
    fn the_room_needed_is_the_download_and_some_to_spare() {
        let pin = pin_for(PHI, &files());
        let found = facts(Some(&pin), Layout::Hub, None, &scratch(), false);
        // Two gigabytes left over, so a download never fills the disk.
        assert_eq!(found.needed, pin.bytes() + 2_000_000_000);
        assert!(!found.hub);
    }

    // -- Hugging Face, and nowhere else ---------------------------------------

    #[test]
    fn a_download_comes_only_from_hugging_face() {
        let url = |text: &str| reqwest::Url::parse(text).unwrap();
        for good in [
            "https://huggingface.co/mlx-community/phi-4-4bit/resolve/abc/config.json",
            "https://hf.co/mlx-community/phi-4-4bit",
            "https://us.aws.cdn.hf.co/xet-bridge-us/abc",
            "https://cas-bridge.xethub.hf.co/xet-bridge-us/abc",
            "https://cdn-lfs.huggingface.co/repos/abc",
            "https://HuggingFace.co/api/resolve-cache/abc",
            "https://huggingface.co:443/abc",
        ] {
            assert!(from_hugging_face(&url(good)), "{good}");
        }
        for bad in [
            "http://huggingface.co/abc",
            "https://huggingface.co:8443/abc",
            "https://someone@huggingface.co/abc",
            "https://someone:secret@huggingface.co/abc",
            "https://huggingface.co.example.com/abc",
            "https://evilhuggingface.co/abc",
            "https://hf.co.example/abc",
            "https://example.com/huggingface.co/abc",
            "https://13.33.33.33/abc",
            "https://[::1]/abc",
            "https://localhost/abc",
            "ftp://huggingface.co/abc",
        ] {
            assert!(!from_hugging_face(&url(bad)), "{bad}");
        }
    }

    #[test]
    fn a_redirect_is_followed_only_to_hugging_face_and_only_a_few_times() {
        let home = reqwest::Url::parse("https://cas-bridge.xethub.hf.co/abc").unwrap();
        let away = reqwest::Url::parse("https://example.com/abc").unwrap();
        assert!(may_follow(&home, 1));
        assert!(may_follow(&home, MOST_HOPS));
        assert!(!may_follow(&home, MOST_HOPS + 1));
        assert!(!may_follow(&away, 1));
        assert_eq!(
            file_url(&pin_for(PHI, &files()), &pin_for(PHI, &files()).files[0]),
            format!("https://huggingface.co/{PHI}/resolve/{REVISION}/config.json")
        );
    }

    /// Answers every caller with `answer`, and counts the callers.
    fn answer_at(listener: TcpListener, answer: String) -> Arc<AtomicUsize> {
        let callers = Arc::new(AtomicUsize::new(0));
        let counted = callers.clone();
        thread::spawn(move || {
            for mut caller in listener.incoming().flatten() {
                counted.fetch_add(1, Ordering::SeqCst);
                let _ = caller.set_read_timeout(Some(Duration::from_secs(1)));
                let mut asked = [0u8; 4096];
                let _ = caller.read(&mut asked);
                let _ = caller.write_all(answer.as_bytes());
            }
        });
        callers
    }

    #[test]
    fn a_redirect_off_hugging_face_is_refused() {
        // The stand-in speaks plain http, which the app's own client refuses
        // outright, so this client has the app's redirect rule and nothing
        // else. Each refusal comes before anything is asked of the new place.
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(redirect_policy())
            .build()
            .unwrap();
        for place in [
            "https://example.com/model.safetensors",
            "http://huggingface.co/model.safetensors",
            "https://huggingface.co.example.com/model.safetensors",
            "/next",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let callers = answer_at(
                listener,
                format!(
                    "HTTP/1.1 302 Found\r\nLocation: {place}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ),
            );
            let error =
                block_on(client.get(format!("http://127.0.0.1:{port}/start")).send()).unwrap_err();
            assert!(error.is_redirect(), "{place}: {error:?}");
            assert_eq!(
                callers.load(Ordering::SeqCst),
                1,
                "{place}: the stand-in was asked once"
            );
        }
    }

    #[test]
    fn the_apps_client_refuses_plain_http_before_it_connects() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let callers = answer_at(
            listener,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
        );
        let error = block_on(
            open_client()
                .get(format!("http://127.0.0.1:{port}/"))
                .send(),
        )
        .unwrap_err();
        assert!(error.is_builder(), "{error:?}");
        thread::sleep(Duration::from_millis(100));
        assert_eq!(callers.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn the_apps_client_uses_no_proxy_and_the_redirect_rule_and_sends_no_referer() {
        // A client cannot be asked how it was built, so this reads the builder.
        let built = include_str!("downloads.rs")
            .split_once("fn client(gate: &Gate)")
            .and_then(|(_, rest)| rest.split_once("\n}\n"))
            .expect("the app's client")
            .0;
        assert!(
            built.contains("gate.client(GATE_OFF,"),
            "the client does not go through the gate: {built}"
        );
        for setting in [
            ".no_proxy()",
            ".https_only(true)",
            ".redirect(redirect_policy())",
            ".referer(false)",
        ] {
            assert!(
                built.contains(setting),
                "{setting} is missing from: {built}"
            );
        }
    }

    #[test]
    fn llama_server_is_asked_to_read_its_folder_once_a_model_is_whole() {
        // Brad, 2026-11-19: "add an engine to linux and windows distro's".
        // `llama-server` reads its models folder when it starts, so a model
        // downloaded while it runs would not be listed until then.
        let whole = DownloadReport {
            whole: true,
            stopped: false,
        };
        let stopped = DownloadReport {
            whole: false,
            stopped: true,
        };
        assert!(reloads_after(models::Layout::Gguf, &whole));
        assert!(!reloads_after(models::Layout::Gguf, &stopped));
        assert!(
            !reloads_after(models::Layout::Hub, &whole),
            "mlx_lm.server looks in the cache each time"
        );
        let command = include_str!("downloads.rs")
            .split_once("pub async fn download_model(")
            .and_then(|(_, rest)| rest.split_once("\n}\n"))
            .expect("the download command")
            .0;
        assert!(
            command.contains("reloads_after(layout, &done)")
                && command.contains("engine::reload_models(&base).await"),
            "{command}"
        );
    }

    #[test]
    fn an_answer_from_anywhere_but_hugging_face_is_not_read() {
        // A client with no rules of its own, so only the app's last check is
        // left to stop it.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let callers = answer_at(
            listener,
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi".to_string(),
        );
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let error = block_on(open_url(
            client,
            format!("http://127.0.0.1:{port}/model.safetensors"),
        ))
        .err();
        assert_eq!(error.as_deref(), Some(SENT_ELSEWHERE));
        assert_eq!(callers.load(Ordering::SeqCst), 1);
    }

    // -- Downloading -----------------------------------------------------------

    #[test]
    fn a_model_downloads_whole_and_the_disk_then_lists_it() {
        let place = place();
        let files = files();
        let pins = vec![pin_for(PHI, &files)];
        let asked = Arc::new(Mutex::new(Vec::new()));
        let mut told = Vec::new();
        let report = block_on(place.downloads.download(
            PHI,
            &pins,
            Layout::Hub,
            Some(place.hub.clone()),
            serving(files.clone(), asked.clone()),
            |progress| told.push(progress),
        ))
        .unwrap();
        assert_eq!(
            report,
            DownloadReport {
                whole: true,
                stopped: false
            }
        );
        assert_eq!(names(&models::models_in(&place.hub)), ["Phi-4"]);

        let wanted: Vec<String> = files
            .iter()
            .map(|(name, _)| format!("https://huggingface.co/{PHI}/resolve/{REVISION}/{name}"))
            .collect();
        assert_eq!(
            *asked.lock().unwrap(),
            wanted,
            "each pinned file is asked for once, in order"
        );

        let folder = place.hub.join(models::folder_name(PHI));
        assert_eq!(
            fs::read_to_string(folder.join("refs").join("main")).unwrap(),
            REVISION,
            "no line ending"
        );
        for (name, body) in &files {
            assert_eq!(
                &fs::read(folder.join("snapshots").join(REVISION).join(name)).unwrap(),
                body,
                "{name}"
            );
        }
        assert!(
            !everything_in(&folder)
                .iter()
                .any(|seen| seen.contains(".part"))
        );
        assert!(
            everything_in(&place.staging).is_empty(),
            "nothing is left in the app's folder"
        );
        assert!(place.downloads.slot().is_none(), "the slot is free again");

        let total = pins[0].bytes();
        assert_eq!(told.first(), Some(&Progress { done: 0, total }));
        assert_eq!(told.last(), Some(&Progress { done: total, total }));
        assert!(told.windows(2).all(|pair| pair[0].done <= pair[1].done));
    }

    const PHI_GGUF: &str = "microsoft/phi-4-gguf";

    fn gguf_files() -> Files {
        vec![(
            "phi-4-Q4_K.gguf".to_string(),
            (0..300_000u32).map(|n| (n % 241) as u8).collect(),
        )]
    }

    #[test]
    fn a_gguf_model_downloads_whole_into_the_apps_own_folder() {
        // "add an engine to linux and windows distro's"
        let place = place();
        let files = gguf_files();
        let pins = vec![pin_for(PHI_GGUF, &files)];
        let asked = Arc::new(Mutex::new(Vec::new()));
        let report = block_on(place.downloads.download(
            PHI_GGUF,
            &pins,
            Layout::Gguf,
            Some(place.hub.clone()),
            serving(files.clone(), asked.clone()),
            |_| {},
        ))
        .unwrap();
        assert_eq!(
            report,
            DownloadReport {
                whole: true,
                stopped: false
            }
        );
        assert_eq!(names(&models::gguf_models_in(&place.hub)), ["Phi-4"]);
        assert_eq!(
            *asked.lock().unwrap(),
            [format!(
                "https://huggingface.co/{PHI_GGUF}/resolve/{REVISION}/phi-4-Q4_K.gguf"
            )]
        );

        let folder = place.hub.join("microsoft--phi-4-gguf");
        assert_eq!(
            fs::read(folder.join("phi-4-Q4_K.gguf")).unwrap(),
            files[0].1
        );
        assert_eq!(
            everything_in(&folder),
            ["phi-4-Q4_K.gguf 300000"],
            "the file alone: no refs, no snapshots, nothing part way"
        );
        assert!(
            everything_in(&place.staging).is_empty(),
            "nothing is left in the app's folder"
        );

        // Once it is whole, nothing is asked for again.
        let (asked, open) = never_asked();
        let again = block_on(place.downloads.download(
            PHI_GGUF,
            &pins,
            Layout::Gguf,
            Some(place.hub.clone()),
            open,
            |_| {},
        ))
        .unwrap();
        assert_eq!(
            again,
            DownloadReport {
                whole: true,
                stopped: false
            }
        );
        assert!(asked.lock().unwrap().is_empty());
    }

    #[test]
    fn a_gguf_file_that_does_not_match_its_checksum_leaves_nothing() {
        let place = place();
        let pins = vec![pin_for(PHI_GGUF, &gguf_files())];
        let served = with_body(&gguf_files(), "phi-4-Q4_K.gguf", vec![7; 300_000]);
        let asked = Arc::new(Mutex::new(Vec::new()));
        let error = block_on(place.downloads.download(
            PHI_GGUF,
            &pins,
            Layout::Gguf,
            Some(place.hub.clone()),
            serving(served, asked),
            |_| {},
        ))
        .unwrap_err();
        assert!(error.contains("checksum"), "{error}");
        assert!(everything_in(&place.hub).is_empty(), "nothing moved in");
        assert!(
            everything_in(&place.staging).is_empty(),
            "nothing is left in the app's folder"
        );
    }

    #[test]
    fn part_of_a_gguf_model_in_the_apps_own_folder_is_left_as_it_is() {
        let place = place();
        let pins = vec![pin_for(PHI_GGUF, &gguf_files())];
        let folder = place.hub.join("microsoft--phi-4-gguf");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("phi-4-Q4_K.gguf.part"), "half").unwrap();
        let before = everything_in(&place.hub);
        let (asked, open) = never_asked();
        let error = block_on(place.downloads.download(
            PHI_GGUF,
            &pins,
            Layout::Gguf,
            Some(place.hub.clone()),
            open,
            |_| {},
        ))
        .unwrap_err();
        assert_eq!(error, sentence(Why::PartThere, "Phi-4", 0));
        assert!(asked.lock().unwrap().is_empty());
        assert_eq!(everything_in(&place.hub), before);
    }

    fn refused(place: &Place, served: Files, pins: &[Pinned]) -> String {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let error = block_on(place.downloads.download(
            PHI,
            pins,
            Layout::Hub,
            Some(place.hub.clone()),
            serving(served, asked),
            |_| {},
        ))
        .unwrap_err();
        assert!(
            everything_in(&place.hub).is_empty(),
            "the models folder is not changed: {error}"
        );
        assert!(
            everything_in(&place.staging).is_empty(),
            "nothing is left in the app's folder: {error}"
        );
        assert!(
            place.downloads.slot().is_none(),
            "the slot is free again: {error}"
        );
        error
    }

    #[test]
    fn a_file_that_does_not_match_its_checksum_throws_the_download_away() {
        let files = files();
        let pins = vec![pin_for(PHI, &files)];
        let mut changed = files[3].1.clone();
        changed[1000] ^= 1;
        let error = refused(
            &place(),
            with_body(&files, "model.safetensors", changed),
            &pins,
        );
        assert_eq!(
            error,
            "model.safetensors did not match the checksum the app expects, so the download was thrown away."
        );
    }

    #[test]
    fn a_file_of_the_wrong_size_throws_the_download_away() {
        let files = files();
        let pins = vec![pin_for(PHI, &files)];
        let short = files[3].1[..299_999].to_vec();
        let mut long = files[3].1.clone();
        long.extend_from_slice(&[0; 70_000]);
        for served in [
            with_body(&files, "model.safetensors", short),
            with_body(&files, "model.safetensors", long),
        ] {
            let error = refused(&place(), served, &pins);
            assert_eq!(
                error,
                "model.safetensors was not the size the app expects, so the download was thrown away."
            );
        }
    }

    #[test]
    fn a_file_that_is_too_long_is_stopped_before_the_extra_is_written() {
        let files = files();
        let pin = pin_for(PHI, &files);
        let snapshot = scratch();
        let (_keep, mut cancel) = watch::channel(false);
        let chunks: Chunks = Box::pin(futures_util::stream::iter(vec![
            Ok(files[0].1.clone()),
            Ok(b"and more".to_vec()),
        ]));
        let error = block_on(fetch_file(
            chunks,
            &pin.files[0],
            &snapshot,
            &mut cancel,
            &mut |_| {},
        ))
        .unwrap_err();
        assert!(error.contains("was not the size"), "{error}");
        assert!(!snapshot.join("config.json").exists());
        assert_eq!(
            fs::read(snapshot.join("config.json.part")).unwrap(),
            b"{}",
            "the extra part was never written"
        );
    }

    #[test]
    fn a_dropped_connection_throws_the_download_away() {
        let place = place();
        let files = files();
        let pins = vec![pin_for(PHI, &files)];
        // Every file comes whole but the weights, which stop part way.
        let open = move |url: String| -> Opening {
            let body = files
                .iter()
                .find(|(name, _)| url.ends_with(&format!("/{name}")))
                .map(|(_, body)| body.clone())
                .unwrap();
            let parts = if url.ends_with("/model.safetensors") {
                vec![Ok(body[..1000].to_vec()), Err("reset".to_string())]
            } else {
                vec![Ok(body)]
            };
            Box::pin(async move { Ok(Box::pin(futures_util::stream::iter(parts)) as Chunks) })
        };
        let error = block_on(place.downloads.download(
            PHI,
            &pins,
            Layout::Hub,
            Some(place.hub.clone()),
            open,
            |_| {},
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "The connection to Hugging Face dropped during model.safetensors."
        );
        assert!(everything_in(&place.hub).is_empty());
        assert!(everything_in(&place.staging).is_empty());
        assert!(place.downloads.slot().is_none());
    }

    #[test]
    fn a_file_hugging_face_does_not_have_stops_the_download() {
        let files = files();
        let pins = vec![pin_for(PHI, &files)];
        let served: Files = files
            .iter()
            .filter(|(name, _)| name != "tokenizer_config.json")
            .cloned()
            .collect();
        assert_eq!(
            refused(&place(), served, &pins),
            "Hugging Face answered 404."
        );
    }

    #[test]
    fn a_download_that_is_not_a_whole_model_does_not_move_in() {
        // Every file matches its pin, but the pin leaves out a file the engine
        // asks for, or a weights file the index names.
        let files = files();
        let no_tokenizer: Files = files
            .iter()
            .filter(|(name, _)| name != "tokenizer_config.json")
            .cloned()
            .collect();
        let index = br#"{"weight_map":{"layers.0.weight":"model.safetensors","layers.1.weight":"model-2.safetensors"}}"#.to_vec();
        let weights_left_out = with_body(&files, "model.safetensors.index.json", index);
        for served in [no_tokenizer, weights_left_out] {
            let pins = vec![pin_for(PHI, &served)];
            let error = refused(&place(), served, &pins);
            assert_eq!(
                error,
                "The download finished without a whole model, so it was thrown away."
            );
        }
    }

    #[test]
    fn cancel_stops_the_download_and_leaves_nothing_behind() {
        let place = Arc::new(place());
        let files = files();
        let pins = vec![pin_for(PHI, &files)];
        let stopper = place.clone();
        let parts_seen = Arc::new(AtomicUsize::new(0));
        let seen = parts_seen.clone();
        // The weights file stops coming part way, and Cancel is pressed while
        // the app waits for the rest.
        let open = move |url: String| -> Opening {
            let stopper = stopper.clone();
            let seen = seen.clone();
            let body = files
                .iter()
                .find(|(name, _)| url.ends_with(&format!("/{name}")))
                .map(|(_, body)| body.clone())
                .unwrap();
            let weights = url.ends_with("/model.safetensors");
            Box::pin(async move {
                if !weights {
                    return Ok(Box::pin(futures_util::stream::iter(vec![Ok(body)])) as Chunks);
                }
                let first = futures_util::stream::iter(vec![Ok(body[..1000].to_vec())]);
                let then = futures_util::stream::once(async move {
                    seen.fetch_add(1, Ordering::SeqCst);
                    stopper.downloads.stop();
                    std::future::pending::<Result<Vec<u8>, String>>().await
                });
                Ok(Box::pin(first.chain(then)) as Chunks)
            })
        };
        // If Cancel went unheard, the rest would never come, so the test gives
        // up after a while instead of waiting with it.
        let stopping = place.downloads.download(
            PHI,
            &pins,
            Layout::Hub,
            Some(place.hub.clone()),
            open,
            |_| {},
        );
        let report =
            block_on(async { tokio::time::timeout(Duration::from_secs(30), stopping).await })
                .expect("Cancel was not heard")
                .unwrap();
        assert_eq!(
            report,
            DownloadReport {
                whole: false,
                stopped: true
            }
        );
        assert_eq!(
            parts_seen.load(Ordering::SeqCst),
            1,
            "Cancel came part way through the weights"
        );
        assert!(
            everything_in(&place.hub).is_empty(),
            "the models folder is not changed"
        );
        assert!(
            everything_in(&place.staging).is_empty(),
            "nothing is left in the app's folder"
        );
        assert!(place.downloads.slot().is_none(), "the slot is free again");
    }

    #[test]
    fn cancel_before_a_file_is_opened_asks_for_nothing_more() {
        let place = place();
        let files = files();
        let pins = vec![pin_for(PHI, &files)];
        let asked = Arc::new(Mutex::new(Vec::new()));
        let report = block_on(place.downloads.download(
            PHI,
            &pins,
            Layout::Hub,
            Some(place.hub.clone()),
            serving(files, asked.clone()),
            |_| place.downloads.stop(),
        ))
        .unwrap();
        assert_eq!(
            report,
            DownloadReport {
                whole: false,
                stopped: true
            }
        );
        assert!(asked.lock().unwrap().is_empty());
        assert!(everything_in(&place.staging).is_empty());
    }

    // Brad, 2026-11-17: "there is a hard switch / circuit breaker to hard stop
    // all outbound communication", and "off by default, everything stops".
    #[test]
    fn with_the_gate_off_nothing_is_downloaded() {
        let place = place();
        let files = files();
        let pins = vec![pin_for(PHI, &files)];
        let (asked, open) = never_asked();
        let error = block_on(download_through(
            &place.downloads,
            &Gate::at(false),
            PHI,
            &pins,
            Layout::Hub,
            Some(place.hub.clone()),
            open,
            |_| {},
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "The WAN gate is off. Turn it on to download a model."
        );
        assert_eq!(client(&Gate::at(false)).unwrap_err(), GATE_OFF);
        assert!(asked.lock().unwrap().is_empty());
        assert!(everything_in(&place.hub).is_empty());
        assert!(everything_in(&place.staging).is_empty());
    }

    #[test]
    fn turning_the_gate_off_stops_a_download_and_says_why() {
        let files = files();
        let pins = vec![pin_for(PHI, &files)];
        // As the switch does: the gate turns off, and the download is stopped.
        let first = place();
        let gate = Gate::at(true);
        let asked = Arc::new(Mutex::new(Vec::new()));
        let switch = gate.clone();
        let turn_off = |_| {
            let _ = switch.set(false);
            first.downloads.stop();
        };
        let error = block_on(download_through(
            &first.downloads,
            &gate,
            PHI,
            &pins,
            Layout::Hub,
            Some(first.hub.clone()),
            serving(files.clone(), asked.clone()),
            turn_off,
        ))
        .unwrap_err();
        assert_eq!(error, GATE_TURNED_OFF);
        assert!(asked.lock().unwrap().is_empty());
        assert!(everything_in(&first.hub).is_empty());
        assert!(everything_in(&first.staging).is_empty());
        assert!(first.downloads.slot().is_none(), "the slot is free again");

        // Even unheard, the next file is not asked for.
        let second = place();
        let gate = Gate::at(true);
        let switch = gate.clone();
        let error = block_on(download_through(
            &second.downloads,
            &gate,
            PHI,
            &pins,
            Layout::Hub,
            Some(second.hub.clone()),
            serving(files, asked.clone()),
            |_| {
                let _ = switch.set(false);
            },
        ))
        .unwrap_err();
        assert_eq!(error, GATE_TURNED_OFF);
        assert!(asked.lock().unwrap().is_empty());
        assert!(everything_in(&second.staging).is_empty());

        // Cancel with the gate on is still a plain stop.
        let third = place();
        let report = block_on(download_through(
            &third.downloads,
            &Gate::at(true),
            PHI,
            &pins,
            Layout::Hub,
            Some(third.hub.clone()),
            never_asked().1,
            |_| {
                third.downloads.stop();
            },
        ))
        .unwrap();
        assert_eq!(
            report,
            DownloadReport {
                whole: false,
                stopped: true
            }
        );
    }

    #[test]
    fn one_download_at_a_time() {
        let place = place();
        let files = files();
        let pins = vec![pin_for(PHI, &files)];
        *place.downloads.slot() = Some(ActiveDownload {
            cancel: watch::channel(false).0,
            remote: false,
        });
        let (asked, open) = never_asked();
        let error = block_on(place.downloads.download(
            PHI,
            &pins,
            Layout::Hub,
            Some(place.hub.clone()),
            open,
            |_| {},
        ))
        .unwrap_err();
        assert_eq!(
            error,
            "Another model is downloading, and the app downloads one at a time."
        );
        assert!(asked.lock().unwrap().is_empty());
        assert!(
            place.downloads.slot().is_some(),
            "the running download keeps its slot"
        );
    }

    #[test]
    fn a_model_already_whole_is_not_downloaded_again() {
        let place = place();
        let files = files();
        let pins = vec![pin_for(PHI, &files)];
        whole_model(&place.hub, &files);
        let before = everything_in(&place.hub);
        let (asked, open) = never_asked();
        let report = block_on(place.downloads.download(
            PHI,
            &pins,
            Layout::Hub,
            Some(place.hub.clone()),
            open,
            |_| {},
        ))
        .unwrap();
        assert_eq!(
            report,
            DownloadReport {
                whole: true,
                stopped: false
            }
        );
        assert!(asked.lock().unwrap().is_empty());
        assert_eq!(everything_in(&place.hub), before);
    }

    #[test]
    fn part_of_a_model_in_the_models_folder_is_left_as_it_is() {
        let files = files();
        let pins = vec![pin_for(PHI, &files)];
        type Make<'a> = &'a dyn Fn(&Path);
        let parts: [(&str, Make); 4] = [
            ("a download that stopped", &|hub| {
                whole_model(hub, &files);
                fs::remove_file(
                    hub.join(models::folder_name(PHI))
                        .join("snapshots")
                        .join(REVISION)
                        .join("model.safetensors"),
                )
                .unwrap();
            }),
            ("an empty folder", &|hub| {
                fs::create_dir_all(hub.join(models::folder_name(PHI))).unwrap()
            }),
            ("a file", &|hub| {
                fs::create_dir_all(hub).unwrap();
                fs::write(hub.join(models::folder_name(PHI)), "").unwrap();
            }),
            ("a link to nothing", &|hub| {
                fs::create_dir_all(hub).unwrap();
                #[cfg(unix)]
                std::os::unix::fs::symlink(hub.join("gone"), hub.join(models::folder_name(PHI)))
                    .unwrap();
                #[cfg(not(unix))]
                fs::write(hub.join(models::folder_name(PHI)), "").unwrap();
            }),
        ];
        for (what, make) in parts {
            let place = place();
            make(&place.hub);
            let before = everything_in(&place.hub);
            let (asked, open) = never_asked();
            let error = block_on(place.downloads.download(
                PHI,
                &pins,
                Layout::Hub,
                Some(place.hub.clone()),
                open,
                |_| {},
            ))
            .unwrap_err();
            assert_eq!(
                error,
                "The models folder already holds part of Phi-4, and the app does not change what is there.",
                "{what}"
            );
            assert!(asked.lock().unwrap().is_empty(), "{what}");
            assert_eq!(everything_in(&place.hub), before, "{what}");
        }
    }

    #[test]
    fn a_model_not_on_the_list_or_with_no_models_folder_is_refused() {
        let place = place();
        let files = files();
        let pins = vec![
            pin_for(PHI, &files),
            pin_for("mlx-community/some-other-model", &files),
        ];
        for id in [
            "mlx-community/some-other-model",
            "mlx-community/phi-4-8bit",
            "mlx-community/PHI-4-4bit",
            "",
        ] {
            let (asked, open) = never_asked();
            let error = block_on(place.downloads.download(
                id,
                &pins,
                Layout::Hub,
                Some(place.hub.clone()),
                open,
                |_| {},
            ))
            .unwrap_err();
            assert_eq!(error, "That model is not on this app's list.", "{id}");
            assert!(asked.lock().unwrap().is_empty(), "{id}");
        }
        let (asked, open) = never_asked();
        let error = block_on(
            place
                .downloads
                .download(PHI, &pins, Layout::Hub, None, open, |_| {}),
        )
        .unwrap_err();
        assert_eq!(
            error,
            "The app could not work out where the models folder is."
        );
        assert!(asked.lock().unwrap().is_empty());
        assert!(everything_in(&place.hub).is_empty());
    }

    #[test]
    fn a_folder_that_appears_while_downloading_is_left_as_it_is() {
        let place = place();
        let staged = place.staging.join(models::folder_name(PHI));
        fs::create_dir_all(staged.join("refs")).unwrap();
        let target = place.hub.join(models::folder_name(PHI));
        fs::create_dir_all(&target).unwrap();
        let error = move_in(&staged, &place.hub, &models::folder_name(PHI), "Phi-4").unwrap_err();
        assert_eq!(
            error,
            "Something put a folder for Phi-4 in the models folder while it downloaded, so the app left that folder as it is."
        );
        assert!(
            staged.join("refs").is_dir(),
            "the download stays where it was"
        );
        assert!(
            everything_in(&target).is_empty(),
            "the folder that appeared is not changed"
        );
    }

    #[test]
    fn a_missing_models_folder_is_made() {
        let place = place();
        let staged = place.staging.join(models::folder_name(PHI));
        fs::create_dir_all(staged.join("refs")).unwrap();
        assert!(!place.hub.exists());
        move_in(&staged, &place.hub, &models::folder_name(PHI), "Phi-4").unwrap();
        assert!(
            place
                .hub
                .join(models::folder_name(PHI))
                .join("refs")
                .is_dir()
        );
        assert!(!staged.exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_rename_never_writes_over_an_empty_folder() {
        let root = scratch();
        fs::create_dir_all(root.join("from").join("inside")).unwrap();
        fs::create_dir_all(root.join("to")).unwrap();
        let error = rename_new(&root.join("from"), &root.join("to")).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::AlreadyExists);
        assert!(root.join("from").join("inside").is_dir());
        rename_new(&root.join("from"), &root.join("new")).unwrap();
        assert!(root.join("new").join("inside").is_dir());
    }

    #[test]
    fn clearing_touches_only_the_apps_own_downloads() {
        let root = scratch();
        let staging = root.join("downloads");
        let outside = root.join("outside");
        fs::create_dir_all(outside.join("keep")).unwrap();
        fs::create_dir_all(staging.join(models::folder_name(PHI)).join("snapshots")).unwrap();
        fs::write(staging.join("notes.txt"), "kept").unwrap();
        fs::create_dir_all(staging.join("kept").join("inside")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, staging.join("models--linked")).unwrap();
        Downloads::new(staging.clone()).clear();
        assert!(!staging.join(models::folder_name(PHI)).exists());
        assert!(staging.join("notes.txt").is_file());
        assert!(
            staging.join("kept").join("inside").is_dir(),
            "a folder no download made is kept"
        );
        assert!(
            outside.join("keep").is_dir(),
            "a link is removed, not followed"
        );
        Downloads::new(root.join("not-there")).clear();
    }

    #[test]
    fn what_a_download_left_when_the_app_quit_is_thrown_away_at_the_next_start() {
        let put_together = include_str!("lib.rs");
        let setup = put_together
            .split_once(".setup(")
            .and_then(|(_, rest)| rest.split_once("Ok(())"))
            .expect("the app's setup")
            .0;
        let made = setup
            .find("downloads::Downloads::new(data.join(\"downloads\"))")
            .expect("the app keeps its downloads beside its chats");
        let cleared = setup
            .find("downloads.clear();")
            .expect("and clears them at the start");
        let kept = setup
            .find("app.manage(downloads);")
            .expect("the commands need the downloads the app keeps");
        assert!(made < cleared && cleared < kept, "{setup}");
        let commands = put_together
            .split_once("generate_handler![")
            .and_then(|(_, rest)| rest.split_once(']'))
            .expect("the list of commands")
            .0;
        assert!(
            commands.contains("downloads::download_model,")
                && commands.contains("downloads::stop_download,"),
            "{commands}"
        );
    }

    #[test]
    fn free_space_and_the_disk_can_be_read() {
        let temp = std::env::temp_dir();
        if cfg!(unix) {
            assert!(free_bytes(&temp).is_some_and(|free| free > 0));
            assert!(
                free_bytes(&temp.join("not-there-yet").join("below")).is_some(),
                "the nearest folder is asked"
            );
        }
        assert!(same_disk(&temp, &temp.join("not-there-yet")));
    }

    #[test]
    fn the_page_is_told_under_the_names_it_reads() {
        let offer = Offer {
            id: PHI.to_string(),
            name: "Phi-4".to_string(),
            maker: "Microsoft".to_string(),
            license: "MIT".to_string(),
            bytes: 8,
        };
        assert_eq!(
            serde_json::to_value(&offer).unwrap(),
            serde_json::json!({ "id": PHI, "name": "Phi-4", "maker": "Microsoft", "license": "MIT", "bytes": 8 })
        );
        assert_eq!(
            serde_json::to_value(Progress { done: 1, total: 2 }).unwrap(),
            serde_json::json!({ "done": 1, "total": 2 })
        );
        assert_eq!(
            serde_json::to_value(DownloadReport {
                whole: false,
                stopped: true
            })
            .unwrap(),
            serde_json::json!({ "whole": false, "stopped": true })
        );
    }

    /// Run by hand, as it reaches Hugging Face:
    /// `cargo test -- --ignored the_small_pinned_files_match_hugging_face`.
    /// Every pinned file under 30 MB is fetched through the app's own client
    /// and checked, which takes in both kinds of redirect Hugging Face uses,
    /// and every weights file a model names must be pinned.
    #[test]
    #[ignore = "reaches Hugging Face"]
    fn the_small_pinned_files_match_hugging_face() {
        let client = open_client();
        let (_keep, mut cancel) = watch::channel(false);
        for pin in PINNED.iter() {
            let folder = scratch();
            for file in pin.files.iter().filter(|file| file.bytes < 30_000_000) {
                let chunks = block_on(open_url(client.clone(), file_url(pin, file))).unwrap();
                let fetched =
                    block_on(fetch_file(chunks, file, &folder, &mut cancel, &mut |_| {})).unwrap();
                assert_eq!(fetched, Fetched::Whole, "{} {}", pin.id, file.name);
                println!("{} {} checked", pin.id, file.name);
            }
            let index: serde_json::Value = serde_json::from_slice(
                &fs::read(folder.join("model.safetensors.index.json")).unwrap(),
            )
            .unwrap();
            let pinned_names: BTreeSet<&str> =
                pin.files.iter().map(|file| file.name.as_str()).collect();
            for weights in index["weight_map"].as_object().unwrap().values() {
                assert!(
                    pinned_names.contains(weights.as_str().unwrap()),
                    "{} pins {weights}",
                    pin.id
                );
            }
            fs::remove_dir_all(&folder).unwrap();
        }
    }

    /// Run by hand, as it downloads Phi-4 (8.3 GB) from Hugging Face:
    /// `RUSTY_DOWNLOAD_TO=<an empty folder> cargo test -- --ignored a_whole_model_downloads_from_hugging_face`.
    #[test]
    #[ignore = "downloads 8.3 GB from Hugging Face"]
    fn a_whole_model_downloads_from_hugging_face() {
        let to = PathBuf::from(
            std::env::var("RUSTY_DOWNLOAD_TO").expect("RUSTY_DOWNLOAD_TO names an empty folder"),
        );
        let hub = to.join("hub");
        let downloads = Downloads::new(to.join("downloads"));
        let client = open_client();
        let started = Instant::now();
        let mut last = Instant::now();
        let report = block_on(downloads.download(
            PHI,
            &PINNED,
            Layout::Hub,
            Some(hub.clone()),
            move |url| open_url(client.clone(), url),
            |progress| {
                if last.elapsed() >= Duration::from_secs(15) {
                    last = Instant::now();
                    println!(
                        "{} of {} bytes after {:?}",
                        progress.done,
                        progress.total,
                        started.elapsed()
                    );
                }
            },
        ))
        .unwrap();
        println!("finished after {:?}", started.elapsed());
        assert_eq!(
            report,
            DownloadReport {
                whole: true,
                stopped: false
            }
        );
        assert_eq!(names(&models::models_in(&hub)), ["Phi-4"]);
        assert!(everything_in(&to.join("downloads")).is_empty());
    }
}
