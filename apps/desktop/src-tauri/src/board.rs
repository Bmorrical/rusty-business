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

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The open format a board file names in its `format` field.
pub const FORMAT: &str = "board v1";
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

/// The made-up board the app carries, so the view has something to show
/// before a board of the person's own arrives.
const SAMPLE: &str = include_str!("../sample-board.json");

fn story() -> String {
    "story".to_string()
}

/// One small piece of a story: a brief a model can finish in one sitting.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Slice {
    #[serde(default)]
    pub id: String,
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
    #[serde(default)]
    pub status: String,
    #[serde(default, rename = "move", skip_serializing_if = "String::is_empty")]
    pub turn: String,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub slices: Vec<Slice>,
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
    #[serde(default)]
    pub stories: Vec<Story>,
}

/// A board's snapshot, as it was when it was pulled.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// The name of this state of the board, given by whoever last wrote the
    /// copy that is shared. A file nobody keeps in step has none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rev: String,
    /// The revs this state came from, the newest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<String>,
    /// When this copy was taken, in milliseconds. The app stamps it.
    #[serde(default)]
    pub pulled_at: u64,
    #[serde(default)]
    pub epics: Vec<Epic>,
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
/// halfway leaves the old file whole.
fn write(path: &Path, text: &str, what: &str) -> Result<(), String> {
    let partial = path.with_extension("json.partial");
    fs::write(&partial, text).map_err(failed(what))?;
    own(&partial, 0o600);
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
        if self.format != FORMAT {
            return Err(format!("This is not a \"{FORMAT}\" file."));
        }
        too_long("The board's name", &self.name, NAME_LIMIT)?;
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

    /// A board from the text of a file, checked against the format.
    fn parse(text: &str) -> Result<Board, String> {
        let board: Board = serde_json::from_str(text)
            .map_err(|error| format!("This file is not a board: {error}."))?;
        board.check()?;
        Ok(board)
    }

    /// A board as the text of a file. It is checked first, and is never more
    /// than a file may be.
    fn json(board: &Board) -> Result<String, String> {
        board.check()?;
        let text = serde_json::to_string_pretty(board)
            .map_err(|_| "Could not write the board.".to_string())?;
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
        let text = serde_json::to_string_pretty(&board)
            .map_err(|_| "Could not write the board.".to_string())?;
        if text.len() as u64 > SIZE_LIMIT {
            return Err("That board is larger than 4 MB.".to_string());
        }
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
        write(&path, &json, "Could not save the note")?;
        Ok(drafts)
    }

    /// Puts the made-up sample board in the folder, as a pull would, and
    /// opens it.
    pub fn sample(&self) -> Result<Local, String> {
        let board: Board = serde_json::from_str(SAMPLE)
            .map_err(|_| "Could not read the sample board.".to_string())?;
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

    fn scratch() -> (Boards, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("rusty-business-test-{}", uuid::Uuid::new_v4()));
        (Boards::new(dir.join("boards")), dir)
    }

    fn sample() -> Board {
        serde_json::from_str(SAMPLE).unwrap()
    }

    fn first_story(board: &mut Board) -> &mut Story {
        &mut board.epics[0].stories[0]
    }

    #[test]
    fn the_sample_board_is_a_board_v1_file() {
        let board = sample();
        assert_eq!(board.format, FORMAT);
        assert_eq!(board.check(), Ok(()));
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
            Some("This is not a \"board v1\" file.")
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
        let text = serde_json::to_string(&board).unwrap();
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
        let mut text = serde_json::to_string(&sample()).unwrap();
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
        fs::write(&elsewhere, serde_json::to_string(&sample()).unwrap()).unwrap();
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
    fn a_story_with_no_kind_is_a_story_and_what_the_format_does_not_name_is_left_out() {
        let text = r#"{"format":"board v1","id":"small","secret":"x","epics":[
            {"release":"1.0","stories":[{"number":1,"title":"One","extra":true}]}]}"#;
        let board: Board = serde_json::from_str(text).unwrap();
        assert_eq!(board.check(), Ok(()));
        assert_eq!(board.epics[0].stories[0].kind, "story");
        let written = serde_json::to_string(&board).unwrap();
        assert!(!written.contains("secret") && !written.contains("extra"));
        assert!(!written.contains("\"move\""));
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
        let text = serde_json::to_string(&board).unwrap();
        assert!(text.contains(r#""rev":"2f6c1e0a9b7d4c53","history":["r-2","r-1"]"#));
        assert_eq!(Boards::parse(&text).unwrap(), board);
        let text = serde_json::to_string(&sample()).unwrap();
        assert!(!text.contains("\"rev\"") && !text.contains("\"history\""));

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
        fs::write(at("road.json"), serde_json::to_string(&board).unwrap()).unwrap();
        assert_eq!(Boards::outside(&at("road.json")), Ok(Some(board)));

        // The file says which board it is. Its name does not.
        fs::write(at("road.txt"), serde_json::to_string(&sample()).unwrap()).unwrap();
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
            fs::write(at("nameless.json"), serde_json::to_string(&board).unwrap()).unwrap();
            assert_eq!(
                Boards::outside(&at("nameless.json")).err().unwrap(),
                "That file does not say which board it is. Its id is up to 64 letters, digits, and hyphens.",
                "{id}"
            );
        }

        let mut board = sample();
        board.name = "  ".to_string();
        fs::write(at("unnamed.json"), serde_json::to_string(&board).unwrap()).unwrap();
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
            "This is not a \"board v1\" file."
        );

        let mut text = serde_json::to_string(&sample()).unwrap();
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
        let text = serde_json::to_string(&sample()).unwrap();
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
        let text = serde_json::to_string(&other).unwrap();
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
