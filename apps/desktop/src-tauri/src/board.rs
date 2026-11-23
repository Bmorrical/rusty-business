//! Boards: a road of epics, stories, and slices, one JSON file per board in
//! the app's own data folder, beside the saved chats.
//!
//! The Workflow view reads only these copies, so it opens with both gates
//! off. Nothing here uses the network, takes a path from the page, or
//! deletes a file. A pull replaces a board's snapshot. What the person wrote,
//! their drafts, lives in a second file that a pull never touches.
//!
//! A board can also be kept in step with one file somewhere else, a file the
//! person names at the command line. Beside each snapshot the store keeps
//! the board as it was when the two last agreed, and where that file is.
//! Reading and writing that file happens here, under the same rules as every
//! other board file. Working out what changed happens in `sync.rs`.
//!
//! A board file is on one of two lines of the format. Line 1 is "board v1":
//! one flat object. Line 2 has a `meta` block and a `data` block, and a
//! version, `2.0`, whose second number goes up when the line gains
//! something. A key this version does not know is kept, at every level, and
//! written back. `BOARD.md` at the root of the repository is the ruleset.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// What a board file on line 2 names in `meta.format`.
pub const FORMAT: &str = "board";
/// What a board file on line 1 names in its `format` field.
pub const FORMAT_V1: &str = "board v1";
/// The newest line of the format this app reads.
const LINE: u32 = 2;
/// The format of the file that holds the person's own drafts for a board.
pub const DRAFTS_FORMAT: &str = "board drafts v1";
/// The format of the file that says where a board's shared file is kept.
pub const SOURCE_FORMAT: &str = "board source v1";

/// The five lanes a slice goes through, in order. A slice's `lane` is the
/// last one it has been through; empty means none yet.
const LANES: [&str; 5] = ["sliced", "written", "checked", "reviewed", "main"];
/// Whose move it can be. Empty leaves it to the lane.
const MOVES: [&str; 4] = ["you", "model", "check", "review"];
const KINDS: [&str; 2] = ["story", "bug"];

/// The most the app reads of one board file, and the most it writes.
const SIZE_LIMIT: u64 = 4 * 1024 * 1024;
const BOARD_LIMIT: usize = 100;
const EPIC_LIMIT: usize = 500;
const STORY_LIMIT: usize = 5_000;
const SLICE_LIMIT: usize = 100;
const NOTES_LIMIT: usize = 200;
const NUMBER_LIMIT: u64 = 999_999_999;
const NAME_LIMIT: usize = 120;
const TITLE_LIMIT: usize = 300;
const DETAIL_LIMIT: usize = 20_000;
const NOTE_LIMIT: usize = 4_000;
/// The most revs a board's `history` holds. Older ones fall off the end.
pub const HISTORY_LIMIT: usize = 50;
/// The most statuses one of a board's two lists holds.
const STATUS_LIMIT: usize = 100;
const STAMP_LIMIT: usize = 40;
const VERSION_LIMIT: usize = 16;

/// What a file says when it is no board at all.
const NOT_A_BOARD: &str = "This is not a board file.";

/// The keys of an object that this version does not know. They are kept as
/// they came and written back, so a newer file loses nothing here.
pub type Extra = BTreeMap<String, serde_json::Value>;

fn zero(number: &u64) -> bool {
    *number == 0
}

/// The made-up board the app carries, so the view has something to show
/// before a board of the person's own arrives.
const SAMPLE: &str = include_str!("../sample-board.json");

/// What a note says when one more would take the notes past the size cap.
const NOTES_FULL: &str =
    "Your notes for this board would be larger than 4 MB. Shorten or clear one, then save again.";

fn story() -> String {
    "story".to_string()
}

/// One small piece of a story: a brief a model can finish in one sitting.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Slice {
    #[serde(default)]
    pub id: String,
    /// The number its backend knows it by, when it has one. Never the match:
    /// a slice is known by its id.
    #[serde(default, skip_serializing_if = "zero")]
    pub number: u64,
    #[serde(default)]
    pub title: String,
    /// The brief: the goal, the files it may touch, and done-when.
    #[serde(default)]
    pub detail: String,
    /// The last lane it has been through, or empty.
    #[serde(default)]
    pub lane: String,
    /// Whose move it is, when the board says so. Empty leaves it to the lane.
    #[serde(default, rename = "move", skip_serializing_if = "String::is_empty")]
    pub turn: String,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A story or a bug, with the slices it has been cut into.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Story {
    #[serde(default)]
    pub number: u64,
    #[serde(default = "story")]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub detail: String,
    /// The status as words. It is shown when `status_id` names no status
    /// on the board's list, so it is never held to a list.
    #[serde(default)]
    pub status: String,
    /// The id of a status on the board's list of story statuses, or empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status_id: String,
    #[serde(default, rename = "move", skip_serializing_if = "String::is_empty")]
    pub turn: String,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub slices: Vec<Slice>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// An epic: one release on the road, and the stories under it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Epic {
    #[serde(default)]
    pub release: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub status: String,
    /// The id of a status on the board's list of release statuses, or empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status_id: String,
    #[serde(default)]
    pub stories: Vec<Story>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// One status a board's stories, or its releases, can be in. The board
/// brings its own list, so a new status is a change to a file, not to code.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// What a row's `statusId` holds. It never changes, so a status can be
    /// renamed without touching a row.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Whether work in this status is done.
    #[serde(default)]
    pub finished: bool,
    /// The ids of the statuses this one may move to. None means any, and an
    /// empty list means no move. The app shows them and enforces nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moves: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A board's two lists of statuses. Each is in the order the board shows.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Statuses {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stories: Vec<Status>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub releases: Vec<Status>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Statuses {
    pub fn is_empty(&self) -> bool {
        self.stories.is_empty() && self.releases.is_empty() && self.extra.is_empty()
    }
}

/// A board's snapshot, as it was when it was pulled. This is the board as
/// the app holds it and as the page gets it, the same for both lines of the
/// format. The file's own shape is `FileV1` or `FileV2`, below.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    /// "board v1" on line 1, and "board" on line 2.
    pub format: String,
    /// The file's version on line 2, such as "2.0". Empty on line 1. A copy
    /// keeps the version its file had, so a newer file is never written back
    /// as an older one.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub version: String,
    pub id: String,
    pub name: String,
    /// The program that last wrote the shared copy, and when. For a person
    /// to read: nothing is decided by either.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub written_by: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub written_at: String,
    /// The name of this state of the board, given by whoever last wrote the
    /// copy that is shared. A file nobody keeps in step has none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub rev: String,
    /// The revs this state came from, the newest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<String>,
    /// The statuses this board's stories and releases can be in.
    pub statuses: Statuses,
    /// When this copy was taken, in milliseconds. The app stamps it.
    pub pulled_at: u64,
    pub epics: Vec<Epic>,
    /// What the file held that this version does not know: beside `meta`
    /// and `data`, inside `meta`, and inside `data`. On line 1, the first
    /// holds the unknown keys of the file's one object.
    #[serde(skip)]
    pub extra: Extra,
    #[serde(skip)]
    pub meta: Extra,
    #[serde(skip)]
    pub data: Extra,
}

/// A board file on line 1: "board v1", one flat object.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileV1 {
    #[serde(default)]
    format: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    rev: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    history: Vec<String>,
    #[serde(default)]
    pulled_at: u64,
    #[serde(default)]
    epics: Vec<Epic>,
    #[serde(flatten)]
    extra: Extra,
}

/// A board file on line 2: what the file is, then what it holds.
#[derive(Deserialize, Serialize)]
struct FileV2 {
    #[serde(default)]
    meta: Meta,
    #[serde(default)]
    data: Data,
    #[serde(flatten)]
    extra: Extra,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    #[serde(default)]
    format: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    written_by: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    written_at: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    rev: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    history: Vec<String>,
    #[serde(default, skip_serializing_if = "Statuses::is_empty")]
    statuses: Statuses,
    #[serde(default, skip_serializing_if = "zero")]
    pulled_at: u64,
    #[serde(flatten)]
    extra: Extra,
}

#[derive(Default, Deserialize, Serialize)]
struct Data {
    #[serde(default)]
    releases: Vec<Epic>,
    #[serde(flatten)]
    extra: Extra,
}

/// What a file's top level holds, read before anything else is, to tell
/// which line the file is on.
#[derive(Deserialize)]
struct Probe {
    #[serde(default)]
    meta: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    format: Option<serde_json::Value>,
}

/// Where a board's shared file is, as the person last named it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Source {
    #[serde(default)]
    format: String,
    #[serde(default)]
    file: String,
}

/// What the person wrote about one story, on this computer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryDraft {
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub noted_at: u64,
}

/// The person's own drafts for one board, by story number.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drafts {
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub stories: BTreeMap<u64, StoryDraft>,
}

/// A board as the view shows it: the snapshot, and the drafts beside it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Local {
    pub board: Board,
    pub drafts: Drafts,
}

/// One line of the list of boards. A file that is not a board is named with
/// what is wrong with it, and left where it is.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub name: String,
    pub pulled_at: u64,
    pub stories: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// A version as its line and its step: "2.7" is line 2, step 7.
fn version(text: &str) -> Option<(u32, u32)> {
    if text.len() > VERSION_LIMIT {
        return None;
    }
    let (line, step) = text.split_once('.')?;
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    if !digits(line) || !digits(step) {
        return None;
    }
    Some((line.parse().ok()?, step.parse().ok()?))
}

/// A time as a board file writes it: UTC, to the second.
fn utc(seconds: u64) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // Days since 1970 to a date, by the era of 400 years it falls in.
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let turned = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * turned + 2) / 5 + 1;
    let month = if turned < 10 { turned + 3 } else { turned - 9 };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

/// Letters, digits, and hyphens only, so an id can never name a path.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// A status's id may hold underscores too. It names a row's status and never
/// a file, and a backend's ids are its own: it should not have to rename one
/// to write a board file.
fn valid_status_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn failed(what: &str) -> impl Fn(std::io::Error) -> String + '_ {
    move |error| format!("{what}: {error}")
}

/// 20000 as "20,000".
fn grouped(count: usize) -> String {
    let digits = count.to_string();
    let groups: Vec<&str> = digits
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|group| std::str::from_utf8(group).unwrap_or(""))
        .collect();
    groups.join(",")
}

/// A value from a file, short enough to repeat in a message.
fn shown(text: &str) -> String {
    let short: String = text.chars().take(40).collect();
    if short.len() < text.len() {
        format!("{short}...")
    } else {
        short
    }
}

fn too_long(what: &str, text: &str, limit: usize) -> Result<(), String> {
    if text.chars().count() > limit {
        return Err(format!(
            "{what} is longer than {} characters.",
            grouped(limit)
        ));
    }
    Ok(())
}

fn too_many(what: &str, count: usize, limit: usize) -> Result<(), String> {
    if count > limit {
        return Err(format!("{what} more than {}.", grouped(limit)));
    }
    Ok(())
}

fn one_of(what: &str, value: &str, known: &[&str], named: &str) -> Result<(), String> {
    if !known.contains(&value) {
        return Err(format!("{what}: \"{}\" is not {named}.", shown(value)));
    }
    Ok(())
}

/// One of a board's lists of statuses. `kind` is "story" or "release".
fn statuses_fit(kind: &str, list: &[Status]) -> Result<(), String> {
    too_many(
        &format!("The board has {kind} statuses,"),
        list.len(),
        STATUS_LIMIT,
    )?;
    let mut ids = BTreeSet::new();
    for status in list {
        if !valid_status_id(&status.id) {
            return Err(format!(
                "The id of a {kind} status is up to 64 letters, digits, hyphens, and underscores."
            ));
        }
        // A row names its status by this id, so no two may share one.
        if !ids.insert(status.id.as_str()) {
            return Err(format!("Two {kind} statuses have the id {}.", status.id));
        }
        too_long(
            &format!("The name of the {kind} status {}", status.id),
            &status.name,
            NAME_LIMIT,
        )?;
        let moves = status.moves.as_deref().unwrap_or_default();
        too_many(
            &format!("The {kind} status {} has moves,", status.id),
            moves.len(),
            STATUS_LIMIT,
        )?;
        if !moves.iter().all(|to| valid_status_id(to)) {
            return Err(format!(
                "A move of the {kind} status {} is a status's id: up to 64 letters, digits, hyphens, and underscores.",
                status.id
            ));
        }
    }
    Ok(())
}

/// The id a row gives for its status. Empty is none. It is held to the shape
/// of an id and to nothing else: an id no list has is not an error.
fn status_id_fits(at: &str, id: &str) -> Result<(), String> {
    if !id.is_empty() && !valid_status_id(id) {
        return Err(format!(
            "{at}: the status id is up to 64 letters, digits, hyphens, and underscores."
        ));
    }
    Ok(())
}

fn notes_fit(what: &str, notes: &[String]) -> Result<(), String> {
    too_many(&format!("{what} has notes,"), notes.len(), NOTES_LIMIT)?;
    for note in notes {
        too_long(&format!("{what}: a note"), note, NOTE_LIMIT)?;
    }
    Ok(())
}

/// Only the person who owns the folder reads what is in it.
fn own(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

/// Writes beside the file, then moves it into place, so a write that stops
/// halfway leaves the old file whole. The new file is the owner's alone
/// before a word goes into it, and it is on the disk before the move, so a
/// power cut cannot leave an empty file under the board's name.
fn write(path: &Path, text: &str, what: &str) -> Result<(), String> {
    let partial = path.with_extension("json.partial");
    let mut file = fs::File::create(&partial).map_err(failed(what))?;
    own(&partial, 0o600);
    file.write_all(text.as_bytes()).map_err(failed(what))?;
    file.sync_all().map_err(failed(what))?;
    drop(file);
    fs::rename(&partial, path).map_err(failed(what))
}

impl Slice {
    fn check(&self, story: u64) -> Result<(), String> {
        if !valid_id(&self.id) || self.id.len() > 16 {
            return Err(format!(
                "Story #{story}: a slice's id is up to 16 letters, digits, and hyphens."
            ));
        }
        let at = format!("Slice {} of story #{story}", self.id);
        if self.number > NUMBER_LIMIT {
            return Err(format!("{at}: its number is between 1 and 999,999,999."));
        }
        too_long(&format!("{at}: the title"), &self.title, TITLE_LIMIT)?;
        too_long(&format!("{at}: the detail"), &self.detail, DETAIL_LIMIT)?;
        if !self.lane.is_empty() {
            one_of(&at, &self.lane, &LANES, "a lane")?;
        }
        if !self.turn.is_empty() {
            one_of(&at, &self.turn, &MOVES, "a move")?;
        }
        notes_fit(&at, &self.notes)
    }
}

impl Story {
    fn check(&self) -> Result<(), String> {
        if self.number == 0 || self.number > NUMBER_LIMIT {
            return Err("A story's number is between 1 and 999,999,999.".to_string());
        }
        let at = format!("Story #{}", self.number);
        one_of(&at, &self.kind, &KINDS, "a kind (story or bug)")?;
        too_long(&format!("{at}: the title"), &self.title, TITLE_LIMIT)?;
        too_long(&format!("{at}: the detail"), &self.detail, DETAIL_LIMIT)?;
        too_long(&format!("{at}: the status"), &self.status, NAME_LIMIT)?;
        status_id_fits(&at, &self.status_id)?;
        if !self.turn.is_empty() {
            one_of(&at, &self.turn, &MOVES, "a move")?;
        }
        notes_fit(&at, &self.notes)?;
        too_many(&format!("{at} has slices,"), self.slices.len(), SLICE_LIMIT)?;
        let mut ids = BTreeSet::new();
        for slice in &self.slices {
            slice.check(self.number)?;
            if !ids.insert(slice.id.as_str()) {
                return Err(format!("{at}: two slices have the id {}.", slice.id));
            }
        }
        Ok(())
    }
}

impl Board {
    /// Why this is not a board the app can keep, in words for a person.
    pub fn check(&self) -> Result<(), String> {
        match self.level() {
            Some((line, _)) if line > LINE => {
                return Err(format!(
                    "This board file is on line {line} of the format, and this app reads lines 1 and {LINE}. Nothing was changed."
                ));
            }
            Some((1 | LINE, _)) => {}
            _ => return Err(NOT_A_BOARD.to_string()),
        }
        too_long("The board's name", &self.name, NAME_LIMIT)?;
        too_long(
            "The name of the program that wrote the board",
            &self.written_by,
            NAME_LIMIT,
        )?;
        too_long(
            "The time the board was written",
            &self.written_at,
            STAMP_LIMIT,
        )?;
        statuses_fit("story", &self.statuses.stories)?;
        statuses_fit("release", &self.statuses.releases)?;
        if !self.rev.is_empty() && !valid_id(&self.rev) {
            return Err("The board's rev is up to 64 letters, digits, and hyphens.".to_string());
        }
        too_many(
            "The board's history has revs,",
            self.history.len(),
            HISTORY_LIMIT,
        )?;
        if !self.history.iter().all(|rev| valid_id(rev)) {
            return Err(
                "A rev in the board's history is up to 64 letters, digits, and hyphens."
                    .to_string(),
            );
        }
        too_many("The board has epics,", self.epics.len(), EPIC_LIMIT)?;
        let mut numbers = BTreeSet::new();
        let mut releases = BTreeSet::new();
        for epic in &self.epics {
            let at = format!("Release {}", shown(&epic.release));
            too_long("A release's name", &epic.release, NAME_LIMIT)?;
            // A release is known by its name, so no two may share one.
            if !releases.insert(epic.release.as_str()) {
                return Err(format!(
                    "Two releases have the name \"{}\".",
                    shown(&epic.release)
                ));
            }
            too_long(&format!("{at}: the title"), &epic.title, TITLE_LIMIT)?;
            too_long(&format!("{at}: the detail"), &epic.detail, DETAIL_LIMIT)?;
            too_long(&format!("{at}: the status"), &epic.status, NAME_LIMIT)?;
            status_id_fits(&at, &epic.status_id)?;
            for story in &epic.stories {
                story.check()?;
                if !numbers.insert(story.number) {
                    return Err(format!("Two stories have the number {}.", story.number));
                }
            }
            too_many("The board has stories,", numbers.len(), STORY_LIMIT)?;
        }
        Ok(())
    }

    /// The line of the format this board is on, and its step on that line.
    /// `None` when its format and version name no line.
    pub fn level(&self) -> Option<(u32, u32)> {
        if self.format == FORMAT_V1 {
            return self.version.is_empty().then_some((1, 0));
        }
        if self.format != FORMAT {
            return None;
        }
        version(&self.version).filter(|(line, _)| *line >= LINE)
    }

    /// The board, saying this app wrote it just now. Line 1 has no place
    /// for either, so a board on line 1 comes back as it is.
    pub fn signed(mut self) -> Board {
        if self.format == FORMAT {
            self.written_by = format!("rusty-business {}", env!("CARGO_PKG_VERSION"));
            self.written_at = utc(now_ms() / 1_000);
        }
        self
    }

    fn has(&self, number: u64) -> bool {
        self.epics
            .iter()
            .any(|epic| epic.stories.iter().any(|story| story.number == number))
    }

    pub fn stories(&self) -> usize {
        self.epics.iter().map(|epic| epic.stories.len()).sum()
    }
}

impl Drafts {
    fn new() -> Self {
        Drafts {
            format: DRAFTS_FORMAT.to_string(),
            stories: BTreeMap::new(),
        }
    }
}

/// The boards kept on this computer.
pub struct Boards {
    dir: PathBuf,
    /// One write at a time, so two saves never cross.
    writing: Mutex<()>,
}

impl Boards {
    pub fn new(dir: PathBuf) -> Self {
        Boards {
            dir,
            writing: Mutex::new(()),
        }
    }

    fn drafts_dir(&self) -> PathBuf {
        self.dir.join("drafts")
    }

    /// Each board as it was when this computer and its file last agreed.
    fn synced_dir(&self) -> PathBuf {
        self.dir.join("synced")
    }

    /// Where each board's file is.
    fn sources_dir(&self) -> PathBuf {
        self.dir.join("sources")
    }

    fn ready(&self) -> Result<(), String> {
        for dir in [
            self.dir.clone(),
            self.drafts_dir(),
            self.synced_dir(),
            self.sources_dir(),
        ] {
            fs::create_dir_all(&dir).map_err(failed("Could not open the boards folder"))?;
            own(&dir, 0o700);
        }
        Ok(())
    }

    fn snapshot(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("That is not a board.".to_string());
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    fn drafts_file(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("That is not a board.".to_string());
        }
        Ok(self.drafts_dir().join(format!("{id}.json")))
    }

    fn base_file(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("That is not a board.".to_string());
        }
        Ok(self.synced_dir().join(format!("{id}.json")))
    }

    fn source_file(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("That is not a board.".to_string());
        }
        Ok(self.sources_dir().join(format!("{id}.json")))
    }

    /// Whether anything is at a path. A link counts, wherever it leads.
    fn found(path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok()
    }

    /// Reads one file, and never more of it than a board may be.
    fn text(path: &Path, what: &str) -> Result<String, String> {
        let plain = fs::symlink_metadata(path)
            .map(|found| found.file_type().is_file())
            .map_err(failed(what))?;
        if !plain {
            return Err(format!("{what}: it is not a plain file."));
        }
        let mut text = String::new();
        fs::File::open(path)
            .map_err(failed(what))?
            .take(SIZE_LIMIT + 1)
            .read_to_string(&mut text)
            .map_err(failed(what))?;
        if text.len() as u64 > SIZE_LIMIT {
            return Err(format!("{what}: it is larger than 4 MB."));
        }
        Ok(text)
    }

    /// A board from the text of a file, on either line of the format,
    /// checked against it. A file with a `meta` block is on line 2 or above.
    pub(crate) fn parse(text: &str) -> Result<Board, String> {
        let unread = |error: serde_json::Error| format!("This file is not a board: {error}.");
        // A board file is one object. Anything else is never read as one.
        if !text.trim_start().starts_with('{') {
            serde_json::from_str::<serde::de::IgnoredAny>(text).map_err(unread)?;
            return Err(NOT_A_BOARD.to_string());
        }
        let probe: Probe = serde_json::from_str(text).map_err(unread)?;
        let board = if probe.meta.is_some() {
            let file: FileV2 = serde_json::from_str(text).map_err(unread)?;
            if file.meta.format != FORMAT {
                return Err(NOT_A_BOARD.to_string());
            }
            Board {
                format: file.meta.format,
                version: file.meta.version,
                id: file.meta.id,
                name: file.meta.name,
                written_by: file.meta.written_by,
                written_at: file.meta.written_at,
                rev: file.meta.rev,
                history: file.meta.history,
                statuses: file.meta.statuses,
                pulled_at: file.meta.pulled_at,
                epics: file.data.releases,
                extra: file.extra,
                meta: file.meta.extra,
                data: file.data.extra,
            }
        } else {
            if probe.format.as_ref().and_then(|format| format.as_str()) != Some(FORMAT_V1) {
                return Err(NOT_A_BOARD.to_string());
            }
            let file: FileV1 = serde_json::from_str(text).map_err(unread)?;
            Board {
                format: file.format,
                id: file.id,
                name: file.name,
                rev: file.rev,
                history: file.history,
                pulled_at: file.pulled_at,
                epics: file.epics,
                extra: file.extra,
                ..Board::default()
            }
        };
        board.check()?;
        Ok(board)
    }

    /// A board as the text of a file, on the line its copy is on. Nothing is
    /// checked here: `json` is the way to write a board.
    pub(crate) fn written(board: &Board) -> Result<String, String> {
        let board = board.clone();
        let text = if board.format == FORMAT_V1 {
            serde_json::to_string_pretty(&FileV1 {
                format: board.format,
                id: board.id,
                name: board.name,
                rev: board.rev,
                history: board.history,
                pulled_at: board.pulled_at,
                epics: board.epics,
                extra: board.extra,
            })
        } else {
            let mut extra = board.extra;
            // A key a line 1 file had beside its own may be named for one of
            // line 2's two blocks. The block is written, and that key is not.
            extra.retain(|key, _| key != "meta" && key != "data");
            serde_json::to_string_pretty(&FileV2 {
                meta: Meta {
                    format: board.format,
                    version: board.version,
                    id: board.id,
                    name: board.name,
                    written_by: board.written_by,
                    written_at: board.written_at,
                    rev: board.rev,
                    history: board.history,
                    statuses: board.statuses,
                    pulled_at: board.pulled_at,
                    extra: board.meta,
                },
                data: Data {
                    releases: board.epics,
                    extra: board.data,
                },
                extra,
            })
        };
        text.map_err(|_| "Could not write the board.".to_string())
    }

    /// A board as the text of a file. It is checked first, and is never more
    /// than a file may be.
    fn json(board: &Board) -> Result<String, String> {
        board.check()?;
        let text = Self::written(board)?;
        if text.len() as u64 > SIZE_LIMIT {
            return Err("That board is larger than 4 MB.".to_string());
        }
        Ok(text)
    }

    /// One of the app's own files for a board.
    fn kept(path: &Path, id: &str, what: &str) -> Result<Board, String> {
        let mut board = Self::parse(&Self::text(path, what)?)?;
        // The file's name is the board's id, whatever the file says inside.
        board.id = id.to_string();
        if board.name.trim().is_empty() {
            board.name = id.to_string();
        }
        Ok(board)
    }

    fn read(&self, id: &str) -> Result<Board, String> {
        Self::kept(&self.snapshot(id)?, id, "Could not read the board")
    }

    /// The local copy of a board, or `None` when this computer has none.
    pub fn local(&self, id: &str) -> Result<Option<Board>, String> {
        if !Self::found(&self.snapshot(id)?) {
            return Ok(None);
        }
        self.read(id).map(Some)
    }

    /// A board as it was when this computer and its file last agreed, or
    /// `None` when they never have.
    pub fn base(&self, id: &str) -> Result<Option<Board>, String> {
        let path = self.base_file(id)?;
        if !Self::found(&path) {
            return Ok(None);
        }
        Self::kept(&path, id, "Could not read the last synced copy").map(Some)
    }

    /// Keeps a board as the state this computer and its file last agreed on.
    /// Neither the snapshot nor the drafts are touched.
    pub fn keep_base(&self, board: &Board) -> Result<(), String> {
        let path = self.base_file(&board.id)?;
        let text = Self::json(board)?;
        self.ready()?;
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        write(&path, &text, "Could not save the last synced copy")
    }

    /// Where a board's file is, or `None` when the person has named none.
    pub fn source(&self, id: &str) -> Result<Option<PathBuf>, String> {
        let path = self.source_file(id)?;
        if !Self::found(&path) {
            return Ok(None);
        }
        let what = "Could not read where this board's file is";
        let source: Source = serde_json::from_str(&Self::text(&path, what)?)
            .map_err(|error| format!("{what}: {error}."))?;
        if source.format != SOURCE_FORMAT || source.file.is_empty() {
            return Err(format!("{what}: it is not a \"{SOURCE_FORMAT}\" file."));
        }
        Ok(Some(PathBuf::from(source.file)))
    }

    /// Remembers where a board's file is, so the next sync needs no path.
    pub fn remember(&self, id: &str, file: &Path) -> Result<(), String> {
        let path = self.source_file(id)?;
        let Some(file) = file.to_str() else {
            return Err("That file's path is not text.".to_string());
        };
        let source = Source {
            format: SOURCE_FORMAT.to_string(),
            file: file.to_string(),
        };
        let text = serde_json::to_string_pretty(&source)
            .map_err(|_| "Could not remember the file.".to_string())?;
        self.ready()?;
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        write(&path, &text, "Could not remember the file")
    }

    /// The id of every board that has a file, in order.
    pub fn sources(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.sources_dir()) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|end| end.to_str()) == Some("json"))
            .filter_map(|path| Some(path.file_stem()?.to_str()?.to_string()))
            .filter(|id| valid_id(id))
            .collect();
        ids.sort();
        ids
    }

    /// A board file kept somewhere else, at a path the person typed, or
    /// `None` when nothing is there. It is read as any board file is: a plain
    /// file, never a link, of at most 4 MB, checked against the format. The
    /// file says which board it is.
    pub fn outside(path: &Path) -> Result<Option<Board>, String> {
        if path.extension().and_then(|end| end.to_str()) != Some("json") {
            return Err("A board file's name ends in .json.".to_string());
        }
        if !Self::found(path) {
            return Ok(None);
        }
        let mut board = Self::parse(&Self::text(path, "Could not read the file")?)?;
        if !valid_id(&board.id) {
            return Err(
                "That file does not say which board it is. Its id is up to 64 letters, digits, and hyphens."
                    .to_string(),
            );
        }
        if board.name.trim().is_empty() {
            board.name = board.id.clone();
        }
        Ok(Some(board))
    }

    /// Writes a board to a file kept somewhere else. It makes a new file, or
    /// replaces a board file of the same board, and nothing else: a file that
    /// holds anything other than this board is left as it is.
    pub fn send(&self, path: &Path, board: &Board) -> Result<(), String> {
        let text = Self::json(board)?;
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        match Self::outside(path)? {
            Some(there) if there.id != board.id => {
                return Err(format!(
                    "That file holds another board, {}. It was left as it is.",
                    there.id
                ));
            }
            _ => {}
        }
        // The half-written file goes beside it. Whatever already has that
        // name must be a plain file, so the write cannot land somewhere else.
        let partial = path.with_extension("json.partial");
        if Self::found(&partial) && !fs::symlink_metadata(&partial).is_ok_and(|at| at.is_file()) {
            return Err(format!(
                "{} is in the way, and is not a plain file.",
                partial.display()
            ));
        }
        write(path, &text, "Could not write the file")
    }

    /// The person's drafts for a board. No file yet means none yet. A file
    /// that cannot be read is an error, so a save never writes over it.
    fn read_drafts(&self, id: &str) -> Result<Drafts, String> {
        let path = self.drafts_file(id)?;
        if fs::symlink_metadata(&path).is_err() {
            return Ok(Drafts::new());
        }
        let text = Self::text(&path, "Could not read your notes for this board")?;
        let mut drafts: Drafts = serde_json::from_str(&text)
            .map_err(|error| format!("Could not read your notes for this board: {error}."))?;
        drafts.format = DRAFTS_FORMAT.to_string();
        Ok(drafts)
    }

    /// Keeps a board as the local copy, and stamps when. It replaces that
    /// board's snapshot and leaves the drafts as they are.
    pub fn pull(&self, mut board: Board) -> Result<Board, String> {
        let path = self.snapshot(&board.id)?;
        board.check()?;
        board.pulled_at = now_ms();
        let text = Self::json(&board)?;
        self.ready()?;
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        write(&path, &text, "Could not save the board")?;
        Ok(board)
    }

    /// Every board in the folder, the newest copy first. Nothing is removed:
    /// a file that is not a board is listed with what is wrong with it.
    pub fn list(&self) -> Result<Vec<Summary>, String> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Ok(Vec::new());
        };
        let mut found = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|end| end.to_str()) != Some("json") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if !valid_id(id) {
                continue;
            }
            found.push(match self.read(id) {
                Ok(board) => Summary {
                    id: board.id.clone(),
                    name: board.name.clone(),
                    pulled_at: board.pulled_at,
                    stories: board.stories(),
                    problem: None,
                },
                Err(problem) => Summary {
                    id: id.to_string(),
                    name: id.to_string(),
                    problem: Some(problem),
                    ..Summary::default()
                },
            });
            if found.len() == BOARD_LIMIT {
                break;
            }
        }
        found.sort_by(|a, b| b.pulled_at.cmp(&a.pulled_at).then_with(|| a.id.cmp(&b.id)));
        Ok(found)
    }

    pub fn load(&self, id: &str) -> Result<Local, String> {
        Ok(Local {
            board: self.read(id)?,
            drafts: self.read_drafts(id)?,
        })
    }

    /// Saves the person's note on a story, in the drafts file. An empty note
    /// clears it. The snapshot is not touched.
    pub fn note(&self, id: &str, story: u64, text: &str) -> Result<Drafts, String> {
        let path = self.drafts_file(id)?;
        let board = self.read(id)?;
        let text = text.trim_end();
        too_long("A note", text, NOTE_LIMIT)?;
        self.ready()?;
        let _one = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        let mut drafts = self.read_drafts(id)?;
        // A note for a story that has left the board can still be changed.
        if !board.has(story) && !drafts.stories.contains_key(&story) {
            return Err("That story is not on this board.".to_string());
        }
        if text.trim().is_empty() {
            drafts.stories.remove(&story);
        } else {
            drafts.stories.insert(
                story,
                StoryDraft {
                    note: text.to_string(),
                    noted_at: now_ms(),
                },
            );
        }
        let json = serde_json::to_string_pretty(&drafts)
            .map_err(|_| "Could not write the note.".to_string())?;
        // The notes are read back under the same cap, so a file past it
        // would lock every note on the board.
        if json.len() as u64 > SIZE_LIMIT {
            return Err(NOTES_FULL.to_string());
        }
        write(&path, &json, "Could not save the note")?;
        Ok(drafts)
    }

    /// Opens the made-up sample board. The first time, it is put in the
    /// folder as a pull would. A file already under the sample's name is
    /// opened as it is, or its problem is said: it is never written over.
    pub fn sample(&self) -> Result<Local, String> {
        let board =
            Self::parse(SAMPLE).map_err(|_| "Could not read the sample board.".to_string())?;
        if Self::found(&self.snapshot(&board.id)?) {
            return self.load(&board.id);
        }
        let board = self.pull(board)?;
        let drafts = self.read_drafts(&board.id)?;
        Ok(Local { board, drafts })
    }
}

#[tauri::command]
pub fn list_boards(boards: tauri::State<'_, Boards>) -> Result<Vec<Summary>, String> {
    boards.list()
}

#[tauri::command]
pub fn load_board(boards: tauri::State<'_, Boards>, id: String) -> Result<Local, String> {
    boards.load(&id)
}

#[tauri::command]
pub fn open_sample_board(boards: tauri::State<'_, Boards>) -> Result<Local, String> {
    boards.sample()
}

#[tauri::command]
pub fn save_board_note(
    boards: tauri::State<'_, Boards>,
    id: String,
    story: u64,
    text: String,
) -> Result<Drafts, String> {
    boards.note(&id, story, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The version of the sample, and of a board this app makes new: line 2, step 0.
    const VERSION: &str = "2.0";

    fn scratch() -> (Boards, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("rusty-business-test-{}", uuid::Uuid::new_v4()));
        (Boards::new(dir.join("boards")), dir)
    }

    fn sample() -> Board {
        Boards::parse(SAMPLE).unwrap()
    }

    /// A board as the text of its file, as the store writes it.
    fn text(board: &Board) -> String {
        Boards::written(board).unwrap()
    }

    fn value(text: &str) -> serde_json::Value {
        serde_json::from_str(text).unwrap()
    }

    /// The example file in `BOARD.md`, the ruleset at the root of the
    /// repository.
    fn ruleset_example() -> &'static str {
        let page = include_str!("../../../../BOARD.md");
        let from = page.find("```json\n").unwrap() + "```json\n".len();
        let to = from + page[from..].find("\n```").unwrap();
        &page[from..to]
    }

    /// A file as a backend that writes board v1 writes it.
    const V1: &str = r#"{
      "format": "board v1",
      "id": "errands",
      "name": "Errands",
      "rev": "r-7",
      "history": ["r-6", "r-5"],
      "epics": [
        {
          "release": "3.1.0",
          "title": "Lists",
          "detail": "",
          "status": "Planned",
          "stories": [
            {
              "number": 12,
              "kind": "bug",
              "title": "A sort that loses a row",
              "detail": "",
              "status": "Looking into it",
              "move": "you",
              "notes": ["Seen twice."],
              "slices": [
                {
                  "id": "a",
                  "number": 7001,
                  "title": "A failing test",
                  "detail": "",
                  "lane": "sliced",
                  "move": "model",
                  "notes": []
                }
              ]
            }
          ]
        }
      ]
    }"#;

    fn first_story(board: &mut Board) -> &mut Story {
        &mut board.epics[0].stories[0]
    }

    #[test]
    fn the_sample_board_is_a_board_file_on_this_apps_line() {
        let board = sample();
        assert_eq!(board.format, FORMAT);
        assert_eq!(board.version, VERSION);
        assert_eq!(board.level(), Some((LINE, 0)));
        assert_eq!(board.check(), Ok(()));
        // Its statuses are data, and every row names one of them by its id.
        assert!(!board.statuses.stories.is_empty() && !board.statuses.releases.is_empty());
        for epic in &board.epics {
            let status = board
                .statuses
                .releases
                .iter()
                .find(|status| status.id == epic.status_id)
                .unwrap_or_else(|| panic!("{} names no release status", epic.release));
            assert_eq!(status.name, epic.status);
            for story in &epic.stories {
                let status = board
                    .statuses
                    .stories
                    .iter()
                    .find(|status| status.id == story.status_id)
                    .unwrap_or_else(|| panic!("#{} names no story status", story.number));
                assert_eq!(status.name, story.status);
            }
        }
        assert_eq!(board.id, "sample");
        assert!(board.stories() >= 5);
        let lanes: BTreeSet<&str> = board
            .epics
            .iter()
            .flat_map(|epic| &epic.stories)
            .flat_map(|story| &story.slices)
            .map(|slice| slice.lane.as_str())
            .collect();
        for lane in LANES {
            assert!(lanes.contains(lane), "the sample shows no slice at {lane}");
        }
    }

    #[test]
    fn a_pulled_board_comes_back_whole() {
        let (boards, _) = scratch();
        let pulled = boards.pull(sample()).unwrap();
        assert!(pulled.pulled_at > 0);
        let local = boards.load("sample").unwrap();
        assert_eq!(local.board, pulled);
        assert_eq!(local.drafts, Drafts::new());
        let listed = boards.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Sample board");
        assert_eq!(listed[0].stories, pulled.stories());
        assert_eq!(listed[0].problem, None);
    }

    #[test]
    fn the_sample_never_writes_over_a_board_already_there() {
        let (boards, dir) = scratch();
        // The first time, the sample is put in the folder.
        let first = boards.sample().unwrap();
        assert_eq!(first.board.name, "Sample board");
        assert!(first.board.pulled_at > 0);

        // A board of the person's own under the sample's name, with a note.
        let mut mine = sample();
        mine.name = "My own board".to_string();
        first_story(&mut mine).title = "Mine, not the sample's".to_string();
        boards.pull(mine).unwrap();
        boards.note("sample", 412, "Keep this.").unwrap();
        let file = dir.join("boards").join("sample.json");
        let before = fs::read(&file).unwrap();

        let opened = boards.sample().unwrap();
        assert_eq!(opened.board.name, "My own board");
        assert_eq!(
            opened.board.epics[0].stories[0].title,
            "Mine, not the sample's"
        );
        assert_eq!(opened.drafts.stories[&412].note, "Keep this.");
        assert_eq!(fs::read(&file).unwrap(), before);
    }

    #[test]
    fn a_broken_file_under_the_samples_name_is_said_and_left_as_it_is() {
        let (boards, dir) = scratch();
        boards.ready().unwrap();
        let file = dir.join("boards").join("sample.json");
        fs::write(&file, "{ half a board").unwrap();

        let problem = boards.sample().map(|_| ()).unwrap_err();
        assert!(
            problem.starts_with("This file is not a board:"),
            "{problem}"
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), "{ half a board");
    }

    #[test]
    fn a_pull_replaces_the_snapshot_and_keeps_the_note() {
        let (boards, _) = scratch();
        boards.pull(sample()).unwrap();
        boards.note("sample", 412, "Ask about dates too.").unwrap();

        let mut newer = sample();
        first_story(&mut newer).title = "Changed on the road".to_string();
        boards.pull(newer).unwrap();

        let local = boards.load("sample").unwrap();
        assert_eq!(local.board.epics[0].stories[0].title, "Changed on the road");
        assert_eq!(local.drafts.stories[&412].note, "Ask about dates too.");
    }

    #[test]
    fn a_note_is_kept_apart_from_the_snapshot() {
        let (boards, dir) = scratch();
        boards.pull(sample()).unwrap();
        let snapshot = dir.join("boards").join("sample.json");
        let before = fs::read(&snapshot).unwrap();

        let drafts = boards.note("sample", 412, "Mine.  \n").unwrap();
        assert_eq!(drafts.stories[&412].note, "Mine.");
        assert!(drafts.stories[&412].noted_at > 0);
        assert_eq!(fs::read(&snapshot).unwrap(), before);

        let kept = dir.join("boards").join("drafts").join("sample.json");
        let text = fs::read_to_string(kept).unwrap();
        assert!(text.contains("board drafts v1") && text.contains("Mine."));
        assert!(!String::from_utf8(before).unwrap().contains("Mine."));
    }

    #[test]
    fn an_empty_note_clears_it_and_a_long_one_is_refused() {
        let (boards, _) = scratch();
        boards.pull(sample()).unwrap();
        boards.note("sample", 412, "Mine.").unwrap();

        let long = "x".repeat(NOTE_LIMIT + 1);
        assert_eq!(
            boards.note("sample", 412, &long),
            Err("A note is longer than 4,000 characters.".to_string())
        );
        assert_eq!(
            boards.load("sample").unwrap().drafts.stories[&412].note,
            "Mine."
        );

        let drafts = boards.note("sample", 412, "  \n").unwrap();
        assert!(drafts.stories.is_empty());
    }

    /// Notes, each as long as a note may be, with less room left under the
    /// size cap than one more of them needs.
    fn nearly_full() -> Drafts {
        let note = || StoryDraft {
            note: "x".repeat(NOTE_LIMIT),
            noted_at: 1_795_000_000_000,
        };
        let size = |drafts: &Drafts| serde_json::to_string_pretty(drafts).unwrap().len() as u64;
        let mut drafts = Drafts::new();
        drafts.stories.insert(900_000_000, note());
        let one = size(&drafts);
        drafts.stories.insert(900_000_001, note());
        let each = size(&drafts) - one;
        for more in 0..(SIZE_LIMIT - size(&drafts)) / each {
            drafts.stories.insert(900_000_002 + more, note());
        }
        assert!(size(&drafts) <= SIZE_LIMIT && size(&drafts) + each > SIZE_LIMIT);
        drafts
    }

    #[test]
    fn a_note_that_would_take_the_notes_past_the_size_cap_is_refused() {
        let (boards, dir) = scratch();
        boards.pull(sample()).unwrap();
        let kept = dir.join("boards").join("drafts").join("sample.json");
        let full = serde_json::to_string_pretty(&nearly_full()).unwrap();
        write(&kept, &full, "Could not write the notes").unwrap();

        // One more would make a file the app could not read back.
        let long = "x".repeat(NOTE_LIMIT);
        assert_eq!(
            boards.note("sample", 412, &long),
            Err(
                "Your notes for this board would be larger than 4 MB. Shorten or clear one, then save again."
                    .to_string()
            )
        );
        assert_eq!(fs::read_to_string(&kept).unwrap(), full);
        let local = boards.load("sample").unwrap();
        assert!(!local.drafts.stories.contains_key(&412));

        // Clearing one makes the room the message asks for.
        boards.note("sample", 900_000_000, "").unwrap();
        let drafts = boards.note("sample", 412, &long).unwrap();
        assert_eq!(drafts.stories[&412].note, long);
        assert_eq!(boards.load("sample").unwrap().drafts, drafts);
    }

    #[test]
    fn a_note_needs_a_story_on_the_board_unless_it_already_has_one() {
        let (boards, _) = scratch();
        boards.pull(sample()).unwrap();
        assert_eq!(
            boards.note("sample", 9, "Nobody's."),
            Err("That story is not on this board.".to_string())
        );

        boards.note("sample", 413, "Before it left.").unwrap();
        let mut newer = sample();
        newer.epics[0].stories.retain(|story| story.number != 413);
        boards.pull(newer).unwrap();

        let local = boards.load("sample").unwrap();
        assert!(!local.board.has(413));
        assert_eq!(local.drafts.stories[&413].note, "Before it left.");
        let changed = boards.note("sample", 413, "Still mine.").unwrap();
        assert_eq!(changed.stories[&413].note, "Still mine.");
    }

    #[test]
    fn an_id_cannot_reach_outside_the_folder() {
        let (boards, dir) = scratch();
        boards.pull(sample()).unwrap();
        for id in ["", "../sample", "a/b", "a.b", "sample.json", "C:\\x", "é"] {
            assert_eq!(boards.load(id).err().unwrap(), "That is not a board.");
            assert_eq!(
                boards.note(id, 412, "x").err().unwrap(),
                "That is not a board."
            );
            let mut board = sample();
            board.id = id.to_string();
            assert_eq!(boards.pull(board).err().unwrap(), "That is not a board.");
        }
        let mut names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names, ["boards"]);
    }

    #[test]
    fn a_file_that_is_not_a_board_is_named_and_left_alone() {
        let (boards, dir) = scratch();
        boards.pull(sample()).unwrap();
        let folder = dir.join("boards");
        fs::write(folder.join("notes.json"), "{ not json").unwrap();
        fs::write(folder.join("other.json"), r#"{"format":"board v9"}"#).unwrap();
        fs::write(folder.join("read me.txt"), "hello").unwrap();

        let listed = boards.list().unwrap();
        let ids: Vec<&str> = listed.iter().map(|line| line.id.as_str()).collect();
        assert_eq!(ids, ["sample", "notes", "other"]);
        assert!(
            listed[1]
                .problem
                .as_deref()
                .unwrap()
                .starts_with("This file is not a board:")
        );
        assert_eq!(
            listed[2].problem.as_deref(),
            Some("This is not a board file.")
        );
        assert!(boards.load("other").is_err());
        assert_eq!(
            fs::read_to_string(folder.join("notes.json")).unwrap(),
            "{ not json"
        );
        assert!(folder.join("other.json").exists() && folder.join("read me.txt").exists());
    }

    #[test]
    fn a_file_dropped_in_the_folder_takes_its_name_as_its_id() {
        let (boards, dir) = scratch();
        boards.ready().unwrap();
        let mut board = sample();
        board.id = "something/else".to_string();
        board.name = " ".to_string();
        let text = text(&board);
        fs::write(dir.join("boards").join("my-road.json"), text).unwrap();

        let local = boards.load("my-road").unwrap();
        assert_eq!(local.board.id, "my-road");
        assert_eq!(local.board.name, "my-road");
        assert_eq!(boards.list().unwrap()[0].id, "my-road");
    }

    #[test]
    fn a_board_too_large_is_not_read() {
        let (boards, dir) = scratch();
        boards.ready().unwrap();
        let mut text = text(&sample());
        text.push_str(&" ".repeat(SIZE_LIMIT as usize));
        fs::write(dir.join("boards").join("big.json"), text).unwrap();
        assert_eq!(
            boards.load("big").err().unwrap(),
            "Could not read the board: it is larger than 4 MB."
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_a_file_somewhere_else_is_not_read() {
        let (boards, dir) = scratch();
        boards.ready().unwrap();
        let elsewhere = dir.join("elsewhere.json");
        fs::write(&elsewhere, text(&sample())).unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.join("boards").join("linked.json")).unwrap();
        assert_eq!(
            boards.load("linked").err().unwrap(),
            "Could not read the board: it is not a plain file."
        );
    }

    #[test]
    fn a_board_that_breaks_the_format_is_refused_with_the_reason() {
        let (boards, _) = scratch();

        let mut board = sample();
        board.epics[0].stories[1].slices[0].lane = "shipped".to_string();
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "Slice a of story #412: \"shipped\" is not a lane."
        );

        let mut board = sample();
        board.epics[1].stories[0].number = 412;
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "Two stories have the number 412."
        );

        let mut board = sample();
        first_story(&mut board).kind = "epic".to_string();
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "Story #409: \"epic\" is not a kind (story or bug)."
        );

        let mut board = sample();
        first_story(&mut board).turn = "nobody".to_string();
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "Story #409: \"nobody\" is not a move."
        );

        let mut board = sample();
        first_story(&mut board).slices[1].id = "a".to_string();
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "Story #409: two slices have the id a."
        );

        let mut board = sample();
        first_story(&mut board).slices[0].id = "../a".to_string();
        assert!(boards.pull(board).is_err());

        let mut board = sample();
        first_story(&mut board).number = 0;
        assert!(boards.pull(board).is_err());

        let mut board = sample();
        first_story(&mut board).detail = "x".repeat(DETAIL_LIMIT + 1);
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "Story #409: the detail is longer than 20,000 characters."
        );

        assert!(boards.list().unwrap().is_empty());
    }

    #[test]
    fn a_story_with_no_kind_is_a_story() {
        let text = r#"{"format":"board v1","id":"small","epics":[
            {"release":"1.0","stories":[{"number":1,"title":"One"}]}]}"#;
        let board = Boards::parse(text).unwrap();
        assert_eq!(board.epics[0].stories[0].kind, "story");
        assert!(!self::text(&board).contains("\"move\""));
    }

    #[test]
    fn the_rulesets_example_is_a_board_file() {
        let board = Boards::parse(ruleset_example()).unwrap();
        assert_eq!(board.level(), Some((2, 0)));
        assert_eq!(board.id, "list-app");
        assert_eq!(board.written_by, "my-tracker 3.1");
        assert_eq!(board.written_at, "2026-11-21T15:04:05Z");
        assert_eq!(board.rev, "export-12");
        assert_eq!(board.history, ["export-11", "export-10"]);
        let names = |list: &[Status]| -> Vec<String> {
            list.iter().map(|status| status.name.clone()).collect()
        };
        assert_eq!(
            names(&board.statuses.stories),
            ["Planned", "Building", "Done"]
        );
        assert_eq!(names(&board.statuses.releases), ["Ahead", "Out"]);
        let building = &board.statuses.stories[1];
        assert_eq!(building.id, "building");
        assert!(!building.finished);
        assert_eq!(
            building.moves,
            Some(vec!["planned".to_string(), "done".to_string()])
        );
        // No moves named is any move. An empty list is none.
        assert_eq!(board.statuses.stories[0].moves, None);
        assert_eq!(board.statuses.stories[2].moves, Some(Vec::new()));
        assert!(board.statuses.stories[2].finished);
        let story = &board.epics[0].stories[0];
        assert_eq!(
            (story.status.as_str(), story.status_id.as_str()),
            ("Building", "building")
        );
        assert_eq!(board.epics[0].status_id, "ahead");
        assert_eq!(story.slices[0].number, 9001);
        // What the store writes reads back as the same board.
        assert_eq!(Boards::parse(&text(&board)).unwrap(), board);
    }

    #[test]
    fn a_file_is_a_meta_block_and_a_data_block() {
        let written = value(&text(&sample()));
        let keys = |value: &serde_json::Value| -> Vec<String> {
            value.as_object().unwrap().keys().cloned().collect()
        };
        assert_eq!(keys(&written), ["data", "meta"]);
        assert_eq!(keys(&written["data"]), ["releases"]);
        assert_eq!(
            keys(&written["meta"]),
            ["format", "id", "name", "statuses", "version"]
        );
        assert_eq!(written["meta"]["format"], "board");
        assert_eq!(written["meta"]["version"], "2.0");
        assert_eq!(written["data"]["releases"][0]["statusId"], "building");
        assert_eq!(
            written["data"]["releases"][0]["stories"][0]["statusId"],
            "on-main"
        );
    }

    #[test]
    fn a_board_v1_file_is_still_read_and_is_written_back_as_board_v1() {
        let board = Boards::parse(V1).unwrap();
        assert_eq!(board.level(), Some((1, 0)));
        assert_eq!(board.format, FORMAT_V1);
        assert_eq!((board.id.as_str(), board.rev.as_str()), ("errands", "r-7"));
        assert_eq!(board.history, ["r-6", "r-5"]);
        assert!(board.statuses.is_empty());
        let story = &board.epics[0].stories[0];
        assert_eq!((story.kind.as_str(), story.turn.as_str()), ("bug", "you"));
        assert_eq!(story.status, "Looking into it");
        assert_eq!(story.status_id, "");
        // A slice's number comes in with it and goes back out with it.
        assert_eq!(story.slices[0].number, 7001);

        // The copy stays on the line its file came on, so the program that
        // wrote the file can read it back.
        let written = value(&text(&board.clone().signed()));
        assert_eq!(written["format"], "board v1");
        assert!(written.get("meta").is_none() && written.get("data").is_none());
        assert!(written.get("writtenBy").is_none() && written.get("version").is_none());
        assert_eq!(
            written["epics"][0]["stories"][0]["slices"][0]["number"],
            7001
        );
        assert_eq!(written["epics"][0]["stories"][0]["move"], "you");
        assert_eq!(Boards::parse(&text(&board)).unwrap(), board);

        // And through the store.
        let (boards, _) = scratch();
        boards.pull(board.clone()).unwrap();
        let kept = boards.load("errands").unwrap().board;
        assert_eq!(kept.level(), Some((1, 0)));
        assert_eq!(kept.epics, board.epics);
    }

    #[test]
    fn what_a_reader_does_not_know_is_kept_and_written_back() {
        // "so someone doesn't have to remap when the next party wants to
        // iterate it's logic ahead."
        let ahead = r#"{
          "meta": {
            "format": "board", "version": "2.7", "id": "ahead", "name": "Ahead",
            "owner": {"team": "garden"},
            "statuses": {
              "stories": [{"id": "new", "name": "New", "finished": false, "colour": "green"}],
              "sprints": [{"id": "s1"}]
            }
          },
          "data": {
            "people": ["a", "b"],
            "releases": [{
              "release": "1.0", "status": "Open", "due": "soon",
              "stories": [{
                "number": 1, "title": "One", "status": "New", "statusId": "new", "points": 3,
                "slices": [{"id": "a", "title": "First", "lane": "sliced", "estimate": {"hours": 2}}]
              }]
            }]
          },
          "signature": "abc"
        }"#;
        let board = Boards::parse(ahead).unwrap();
        // The file's own step is kept, never lowered to this app's.
        assert_eq!(board.level(), Some((2, 7)));
        let written = value(&text(&board.clone().signed()));
        assert_eq!(written["meta"]["version"], "2.7");
        assert_eq!(written["signature"], "abc");
        assert_eq!(written["meta"]["owner"]["team"], "garden");
        assert_eq!(written["meta"]["statuses"]["stories"][0]["colour"], "green");
        assert_eq!(written["meta"]["statuses"]["sprints"][0]["id"], "s1");
        assert_eq!(written["data"]["people"][1], "b");
        let release = &written["data"]["releases"][0];
        assert_eq!(release["due"], "soon");
        assert_eq!(release["stories"][0]["points"], 3);
        assert_eq!(release["stories"][0]["slices"][0]["estimate"]["hours"], 2);

        // The same holds through the store, and for a board v1 file.
        let (boards, _) = scratch();
        boards.pull(board.clone()).unwrap();
        let kept = boards.load("ahead").unwrap().board;
        assert_eq!(kept.extra, board.extra);
        assert_eq!(kept.meta, board.meta);
        assert_eq!(kept.data, board.data);
        assert_eq!(kept.statuses, board.statuses);
        assert_eq!(kept.epics, board.epics);

        let old = r#"{"format":"board v1","id":"small","secret":"x","epics":[
            {"release":"1.0","stories":[{"number":1,"title":"One","extra":true}]}]}"#;
        let written = value(&text(&Boards::parse(old).unwrap()));
        assert_eq!(written["secret"], "x");
        assert_eq!(written["epics"][0]["stories"][0]["extra"], true);
    }

    #[test]
    fn a_retired_status_stays_on_the_list_and_is_written_back() {
        // A backend may retire a status. Rows that hold it keep it, so the
        // file still lists it, with `archived` beside it. This app offers no
        // status to pick, so it keeps the key and writes it back.
        let file = r#"{
          "meta": {
            "format": "board", "version": "2.0", "id": "retired", "name": "Retired",
            "statuses": {
              "stories": [
                {"id": "in_planning", "name": "In planning", "finished": false},
                {"id": "old_way", "name": "The old way", "finished": false, "archived": true}
              ]
            }
          },
          "data": {
            "releases": [{
              "release": "1.0",
              "stories": [{"number": 1, "title": "One", "status": "The old way", "statusId": "old_way"}]
            }]
          }
        }"#;
        let board = Boards::parse(file).unwrap();
        assert_eq!(board.check(), Ok(()));
        let (boards, _) = scratch();
        boards.pull(board).unwrap();
        let kept = boards.load("retired").unwrap().board;
        assert_eq!(kept.epics[0].stories[0].status_id, "old_way");
        let written = value(&text(&kept.signed()));
        let list = &written["meta"]["statuses"]["stories"];
        assert_eq!(list[1]["id"], "old_way");
        assert_eq!(list[1]["archived"], true);
        // A status that is in use says nothing: the key is absent, not false.
        assert!(list[0].get("archived").is_none());
    }

    #[test]
    fn a_file_on_a_line_above_this_apps_is_the_one_refusal() {
        let on = |version: &str| {
            format!(
                r#"{{"meta":{{"format":"board","version":"{version}","id":"x"}},"data":{{"releases":[]}}}}"#
            )
        };
        assert!(Boards::parse(&on("2.0")).is_ok());
        assert!(Boards::parse(&on("2.99")).is_ok());
        assert_eq!(
            Boards::parse(&on("3.0")).err().unwrap(),
            "This board file is on line 3 of the format, and this app reads lines 1 and 2. Nothing was changed."
        );
        // A version that is not "line.step", or a line below the meta block's
        // own, is not a board file.
        for version in [
            "",
            "2",
            "two.0",
            "1.0",
            "0.4",
            "2.0.1",
            "-2.0",
            "2.00000000000000000",
        ] {
            assert_eq!(
                Boards::parse(&on(version)).err().unwrap(),
                "This is not a board file.",
                "{version}"
            );
        }
        for text in [
            r#"{"meta":{"format":"board v1","version":"2.0","id":"x"},"data":{"releases":[]}}"#,
            r#"{"format":"board","version":"2.0","id":"x","epics":[]}"#,
            r#"{"format":2,"id":"x"}"#,
            r#"{"id":"x","epics":[]}"#,
            "[]",
            "7",
        ] {
            assert_eq!(
                Boards::parse(text).err().unwrap(),
                "This is not a board file.",
                "{text}"
            );
        }
    }

    #[test]
    fn statuses_are_data_and_are_checked() {
        let (boards, _) = scratch();
        let status = |id: &str, name: &str| Status {
            id: id.to_string(),
            name: name.to_string(),
            ..Status::default()
        };

        // Any list of names is a board's own. None is built into the app.
        let mut board = sample();
        board.statuses.stories = [
            "Planned",
            "Looking into it",
            "Sketching",
            "Trying it out",
            "Looking it over",
        ]
        .iter()
        .enumerate()
        .map(|(at, name)| status(&format!("s{at}"), name))
        .collect();
        assert_eq!(board.check(), Ok(()));

        // A row whose status id is not on the list is shown as its text. It
        // is never an error.
        first_story(&mut board).status_id = "gone".to_string();
        assert_eq!(board.check(), Ok(()));

        let mut board = sample();
        board.statuses.stories.push(status("ready", "Again"));
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "Two story statuses have the id ready."
        );

        let mut board = sample();
        board.statuses.releases[0].id = "not an id".to_string();
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "The id of a release status is up to 64 letters, digits, hyphens, and underscores."
        );

        let mut board = sample();
        board.statuses.releases[0].name = "n".repeat(NAME_LIMIT + 1);
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "The name of the release status scheduled is longer than 120 characters."
        );

        let mut board = sample();
        board.statuses.stories[0].moves = Some(vec!["a b".to_string()]);
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "A move of the story status scheduled is a status's id: up to 64 letters, digits, hyphens, and underscores."
        );
        // A move to an id the list does not have is kept as it is.
        let mut board = sample();
        board.statuses.stories[0].moves = Some(vec!["later".to_string()]);
        assert_eq!(board.check(), Ok(()));

        let mut board = sample();
        board.statuses.stories = (0..=STATUS_LIMIT)
            .map(|at| status(&format!("s{at}"), "S"))
            .collect();
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "The board has story statuses, more than 100."
        );

        let mut board = sample();
        first_story(&mut board).status_id = "../x".to_string();
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "Story #409: the status id is up to 64 letters, digits, hyphens, and underscores."
        );

        // A status's id may hold an underscore: in its list, in a move, and
        // on a row. A backend's ids are its own, and it should not have to
        // rename one to write a file.
        let mut board = sample();
        board.statuses.stories.push(Status {
            moves: Some(vec!["on_hold".to_string()]),
            ..status("in_planning", "In planning")
        });
        first_story(&mut board).status_id = "in_planning".to_string();
        assert_eq!(board.check(), Ok(()));
        // The ids that name a file, or stand in a path, stay as they were.
        let mut board = sample();
        board.id = "my_road".to_string();
        assert_eq!(boards.pull(board).err().unwrap(), "That is not a board.");
        let mut board = sample();
        board.rev = "r_2".to_string();
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "The board's rev is up to 64 letters, digits, and hyphens."
        );
        let mut board = sample();
        first_story(&mut board).slices[0].id = "a_b".to_string();
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "Story #409: a slice's id is up to 16 letters, digits, and hyphens."
        );

        let mut board = sample();
        first_story(&mut board).slices[0].number = NUMBER_LIMIT + 1;
        assert_eq!(
            boards.pull(board).err().unwrap(),
            "Slice a of story #409: its number is between 1 and 999,999,999."
        );

        assert!(boards.list().unwrap().is_empty());
    }

    #[test]
    fn a_board_this_app_writes_says_so() {
        let signed = sample().signed();
        assert_eq!(
            signed.written_by,
            format!("rusty-business {}", env!("CARGO_PKG_VERSION"))
        );
        let stamp = signed.written_at.as_bytes();
        assert_eq!(stamp.len(), 20);
        assert_eq!((stamp[4], stamp[10], stamp[19]), (b'-', b'T', b'Z'));
        let written = value(&text(&signed));
        assert_eq!(written["meta"]["writtenBy"], signed.written_by);
        assert_eq!(written["meta"]["writtenAt"], signed.written_at);

        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(951_868_799), "2000-02-29T23:59:59Z");
        assert_eq!(utc(1_795_273_445), "2026-11-21T15:04:05Z");
        assert_eq!(utc(4_107_542_400), "2100-03-01T00:00:00Z");

        let mut board = sample();
        board.written_at = "x".repeat(STAMP_LIMIT + 1);
        assert_eq!(
            board.check().err().unwrap(),
            "The time the board was written is longer than 40 characters."
        );
    }

    #[test]
    fn notes_that_cannot_be_read_are_never_written_over() {
        let (boards, dir) = scratch();
        boards.pull(sample()).unwrap();
        let kept = dir.join("boards").join("drafts").join("sample.json");
        fs::write(&kept, "{ half a file").unwrap();

        assert!(boards.load("sample").is_err());
        assert!(boards.note("sample", 412, "New.").is_err());
        assert_eq!(fs::read_to_string(&kept).unwrap(), "{ half a file");
    }

    #[cfg(unix)]
    #[test]
    fn the_folder_and_its_files_are_the_owners_alone() {
        use std::os::unix::fs::PermissionsExt;
        let (boards, dir) = scratch();
        boards.pull(sample()).unwrap();
        boards.note("sample", 412, "Mine.").unwrap();
        boards.keep_base(&sample()).unwrap();
        let file = dir.join("elsewhere.json");
        boards.remember("sample", &file).unwrap();
        boards.send(&file, &sample()).unwrap();
        let mode = |path: PathBuf| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let folder = dir.join("boards");
        assert_eq!(mode(folder.clone()), 0o700);
        assert_eq!(mode(folder.join("sample.json")), 0o600);
        for kept in ["drafts", "synced", "sources"] {
            assert_eq!(mode(folder.join(kept)), 0o700, "{kept}");
            assert_eq!(mode(folder.join(kept).join("sample.json")), 0o600, "{kept}");
        }
        // A file written somewhere else is the owner's alone too.
        assert_eq!(mode(file), 0o600);
    }

    #[test]
    fn a_rev_and_a_history_are_checked() {
        let mut board = sample();
        board.rev = "2f6c1e0a9b7d4c53".to_string();
        board.history = vec!["r-2".to_string(), "r-1".to_string()];
        assert_eq!(board.check(), Ok(()));
        // Both travel in the file, and neither is there when it is empty.
        let written = text(&board);
        assert_eq!(value(&written)["meta"]["rev"], "2f6c1e0a9b7d4c53");
        assert_eq!(value(&written)["meta"]["history"][1], "r-1");
        assert_eq!(Boards::parse(&written).unwrap(), board);
        let written = text(&sample());
        assert!(!written.contains("\"rev\"") && !written.contains("\"history\""));

        board.rev = "../r3".to_string();
        assert_eq!(
            board.check().err().unwrap(),
            "The board's rev is up to 64 letters, digits, and hyphens."
        );
        board.rev = "r".repeat(65);
        assert!(board.check().is_err());

        let mut board = sample();
        board.history = vec!["r 1".to_string()];
        assert_eq!(
            board.check().err().unwrap(),
            "A rev in the board's history is up to 64 letters, digits, and hyphens."
        );
        board.history = vec![String::new()];
        assert!(board.check().is_err());
        board.history = (0..=HISTORY_LIMIT).map(|n| format!("r{n}")).collect();
        assert_eq!(
            board.check().err().unwrap(),
            "The board's history has revs, more than 50."
        );
        board.history.pop();
        assert_eq!(board.check(), Ok(()));
    }

    #[test]
    fn two_releases_cannot_share_a_name() {
        let mut board = sample();
        board.epics[1].release = board.epics[0].release.clone();
        assert_eq!(
            board.check().err().unwrap(),
            "Two releases have the name \"1.2.0\"."
        );
    }

    #[test]
    fn the_last_synced_copy_is_kept_apart_from_the_snapshot() {
        let (boards, dir) = scratch();
        assert_eq!(boards.local("sample").unwrap(), None);
        assert_eq!(boards.base("sample").unwrap(), None);

        let pulled = boards.pull(sample()).unwrap();
        assert_eq!(boards.local("sample").unwrap(), Some(pulled));
        assert_eq!(boards.base("sample").unwrap(), None);
        let snapshot = dir.join("boards").join("sample.json");
        let before = fs::read_to_string(&snapshot).unwrap();

        let mut agreed = sample();
        agreed.rev = "r1".to_string();
        first_story(&mut agreed).title = "As both sides last had it".to_string();
        boards.keep_base(&agreed).unwrap();
        assert_eq!(boards.base("sample").unwrap(), Some(agreed));
        assert_eq!(fs::read_to_string(&snapshot).unwrap(), before);
        assert!(
            dir.join("boards")
                .join("synced")
                .join("sample.json")
                .is_file()
        );

        // Neither is listed as a board of its own.
        assert_eq!(boards.list().unwrap().len(), 1);

        // A copy that breaks the format is never kept.
        let mut broken = sample();
        broken.rev = "not a rev".to_string();
        assert!(boards.keep_base(&broken).is_err());
        assert_eq!(boards.base("sample").unwrap().unwrap().rev, "r1");
    }

    #[test]
    fn a_boards_file_is_remembered() {
        let (boards, dir) = scratch();
        assert_eq!(boards.source("sample").unwrap(), None);
        assert!(boards.sources().is_empty());

        let file = dir.join("shared").join("sample.json");
        boards.remember("sample", &file).unwrap();
        boards.remember("a-road", &dir.join("a-road.json")).unwrap();
        assert_eq!(boards.source("sample").unwrap(), Some(file));
        assert_eq!(boards.sources(), ["a-road", "sample"]);
        let kept = dir.join("boards").join("sources").join("sample.json");
        let text = fs::read_to_string(&kept).unwrap();
        assert!(text.contains("\"format\": \"board source v1\""));

        // Remembering a file reads nothing and writes nothing there.
        assert!(!dir.join("shared").exists());
        // It is not a board, and is not listed as one.
        assert!(boards.list().unwrap().is_empty());

        fs::write(
            &kept,
            "{\"format\": \"something else\", \"file\": \"/x.json\"}",
        )
        .unwrap();
        assert_eq!(
            boards.source("sample").err().unwrap(),
            "Could not read where this board's file is: it is not a \"board source v1\" file."
        );
    }

    #[test]
    fn the_sync_files_cannot_reach_outside_the_folder() {
        let (boards, dir) = scratch();
        let refused = "That is not a board.";
        for id in ["../x", "a/b", "", "a.b", "..", &"x".repeat(65)] {
            assert_eq!(boards.local(id).err().unwrap(), refused, "{id}");
            assert_eq!(boards.base(id).err().unwrap(), refused, "{id}");
            assert_eq!(boards.source(id).err().unwrap(), refused, "{id}");
            let file = dir.join("x.json");
            assert_eq!(boards.remember(id, &file).err().unwrap(), refused, "{id}");
            let mut board = sample();
            board.id = id.to_string();
            assert_eq!(boards.keep_base(&board).err().unwrap(), refused, "{id}");
        }
        // Nothing was made, not even the folder.
        assert!(!dir.exists());
    }

    #[test]
    fn a_file_somewhere_else_is_read_as_any_board_file_is() {
        let (_, dir) = scratch();
        fs::create_dir_all(&dir).unwrap();
        let at = |name: &str| dir.join(name);

        assert_eq!(Boards::outside(&at("none.json")), Ok(None));

        let mut board = sample();
        board.rev = "r1".to_string();
        fs::write(at("road.json"), text(&board)).unwrap();
        assert_eq!(Boards::outside(&at("road.json")), Ok(Some(board)));

        // The file says which board it is. Its name does not.
        fs::write(at("road.txt"), text(&sample())).unwrap();
        for name in ["road.txt", "road", "road.json.txt"] {
            assert_eq!(
                Boards::outside(&at(name)).err().unwrap(),
                "A board file's name ends in .json.",
                "{name}"
            );
        }
        for id in ["", "../sample", "a b"] {
            let mut board = sample();
            board.id = id.to_string();
            fs::write(at("nameless.json"), text(&board)).unwrap();
            assert_eq!(
                Boards::outside(&at("nameless.json")).err().unwrap(),
                "That file does not say which board it is. Its id is up to 64 letters, digits, and hyphens.",
                "{id}"
            );
        }

        let mut board = sample();
        board.name = "  ".to_string();
        fs::write(at("unnamed.json"), text(&board)).unwrap();
        assert_eq!(
            Boards::outside(&at("unnamed.json")).unwrap().unwrap().name,
            "sample"
        );

        fs::write(at("words.json"), "not a board").unwrap();
        assert!(
            Boards::outside(&at("words.json"))
                .err()
                .unwrap()
                .starts_with("This file is not a board: ")
        );
        fs::write(at("other.json"), "{\"format\": \"board v2\"}").unwrap();
        assert_eq!(
            Boards::outside(&at("other.json")).err().unwrap(),
            "This is not a board file."
        );

        let mut text = text(&sample());
        text.push_str(&" ".repeat(SIZE_LIMIT as usize));
        fs::write(at("big.json"), text).unwrap();
        assert_eq!(
            Boards::outside(&at("big.json")).err().unwrap(),
            "Could not read the file: it is larger than 4 MB."
        );

        fs::create_dir_all(at("folder.json")).unwrap();
        assert_eq!(
            Boards::outside(&at("folder.json")).err().unwrap(),
            "Could not read the file: it is not a plain file."
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_somewhere_else_is_neither_read_nor_written_through() {
        let (boards, dir) = scratch();
        fs::create_dir_all(&dir).unwrap();
        let real = dir.join("real.json");
        let text = text(&sample());
        fs::write(&real, &text).unwrap();
        let link = dir.join("link.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let refused = "Could not read the file: it is not a plain file.";
        assert_eq!(Boards::outside(&link).err().unwrap(), refused);
        assert_eq!(boards.send(&link, &sample()).err().unwrap(), refused);

        // A link where the half-written file would go is refused as well.
        let file = dir.join("road.json");
        let target = dir.join("target.txt");
        fs::write(&target, "Left alone.").unwrap();
        std::os::unix::fs::symlink(&target, dir.join("road.json.partial")).unwrap();
        assert_eq!(
            boards.send(&file, &sample()).err().unwrap(),
            format!(
                "{} is in the way, and is not a plain file.",
                dir.join("road.json.partial").display()
            )
        );
        assert!(!file.exists());
        assert_eq!(fs::read_to_string(&target).unwrap(), "Left alone.");
        assert_eq!(fs::read_to_string(&real).unwrap(), text);
    }

    // "we can start with a flat file"
    #[test]
    fn a_send_makes_a_new_file_or_replaces_its_own_board_and_nothing_else() {
        let (boards, dir) = scratch();
        fs::create_dir_all(&dir).unwrap();
        let at = |name: &str| dir.join(name);

        // A new file.
        let mut board = sample();
        board.rev = "r1".to_string();
        boards.send(&at("road.json"), &board).unwrap();
        assert_eq!(Boards::outside(&at("road.json")), Ok(Some(board.clone())));
        assert!(!at("road.json.partial").exists());
        // The local copy is not what a send writes.
        assert_eq!(boards.local("sample").unwrap(), None);

        // The same board, again.
        board.rev = "r2".to_string();
        board.history = vec!["r1".to_string()];
        first_story(&mut board).title = "A newer title".to_string();
        boards.send(&at("road.json"), &board).unwrap();
        assert_eq!(Boards::outside(&at("road.json")), Ok(Some(board.clone())));

        // Another board's file.
        let mut other = sample();
        other.id = "another".to_string();
        let text = text(&other);
        fs::write(at("another.json"), &text).unwrap();
        assert_eq!(
            boards.send(&at("another.json"), &board).err().unwrap(),
            "That file holds another board, another. It was left as it is."
        );
        assert_eq!(fs::read_to_string(at("another.json")).unwrap(), text);

        // A file that is not a board at all.
        fs::write(at("package.json"), "{\"name\": \"something\"}").unwrap();
        assert!(boards.send(&at("package.json"), &board).is_err());
        assert_eq!(
            fs::read_to_string(at("package.json")).unwrap(),
            "{\"name\": \"something\"}"
        );
        fs::write(at("words.json"), "Keep me.").unwrap();
        assert!(boards.send(&at("words.json"), &board).is_err());
        assert_eq!(fs::read_to_string(at("words.json")).unwrap(), "Keep me.");

        // A name that does not end in .json, there or not.
        fs::write(at("notes.txt"), "Keep me.").unwrap();
        for name in ["notes.txt", "new.txt", "new"] {
            assert_eq!(
                boards.send(&at(name), &board).err().unwrap(),
                "A board file's name ends in .json.",
                "{name}"
            );
        }
        assert_eq!(fs::read_to_string(at("notes.txt")).unwrap(), "Keep me.");
        assert!(!at("new.txt").exists() && !at("new").exists());

        // A board that breaks the format is never written.
        let mut broken = sample();
        broken.epics[0].stories[0].slices[0].lane = "shipped".to_string();
        assert!(boards.send(&at("broken.json"), &broken).is_err());
        assert!(!at("broken.json").exists());

        // A folder where the half-written file would go.
        fs::create_dir_all(at("blocked.json.partial")).unwrap();
        assert_eq!(
            boards.send(&at("blocked.json"), &board).err().unwrap(),
            format!(
                "{} is in the way, and is not a plain file.",
                at("blocked.json.partial").display()
            )
        );
        assert!(!at("blocked.json").exists());
    }

    #[test]
    fn numbers_in_a_message_are_grouped() {
        assert_eq!(grouped(7), "7");
        assert_eq!(grouped(4_000), "4,000");
        assert_eq!(grouped(20_000), "20,000");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }
}
