//! Boards: a road of epics, stories, and slices, one JSON file per board in
//! the app's own data folder, beside the saved chats.
//!
//! The Workflow view reads only these copies, so it opens with both gates
//! off. Nothing here uses the network, takes a path from the page, or
//! deletes a file. A pull replaces a board's snapshot. What the person wrote,
//! their drafts, lives in a second file that a pull never touches.

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
    /// When this copy was taken, in milliseconds. The app stamps it.
    #[serde(default)]
    pub pulled_at: u64,
    #[serde(default)]
    pub epics: Vec<Epic>,
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
    fn check(&self) -> Result<(), String> {
        if self.format != FORMAT {
            return Err(format!("This is not a \"{FORMAT}\" file."));
        }
        too_long("The board's name", &self.name, NAME_LIMIT)?;
        too_many("The board has epics,", self.epics.len(), EPIC_LIMIT)?;
        let mut numbers = BTreeSet::new();
        for epic in &self.epics {
            let at = format!("Release {}", shown(&epic.release));
            too_long("A release's name", &epic.release, NAME_LIMIT)?;
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

    fn stories(&self) -> usize {
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

    fn ready(&self) -> Result<(), String> {
        for dir in [self.dir.clone(), self.drafts_dir()] {
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

    fn read(&self, id: &str) -> Result<Board, String> {
        let text = Self::text(&self.snapshot(id)?, "Could not read the board")?;
        let mut board: Board = serde_json::from_str(&text)
            .map_err(|error| format!("This file is not a board: {error}."))?;
        board.check()?;
        // The file's name is the board's id, whatever the file says inside.
        board.id = id.to_string();
        if board.name.trim().is_empty() {
            board.name = id.to_string();
        }
        Ok(board)
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
        let mode = |path: PathBuf| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let folder = dir.join("boards");
        assert_eq!(mode(folder.clone()), 0o700);
        assert_eq!(mode(folder.join("drafts")), 0o700);
        assert_eq!(mode(folder.join("sample.json")), 0o600);
        assert_eq!(mode(folder.join("drafts").join("sample.json")), 0o600);
    }

    #[test]
    fn numbers_in_a_message_are_grouped() {
        assert_eq!(grouped(7), "7");
        assert_eq!(grouped(4_000), "4,000");
        assert_eq!(grouped(20_000), "20,000");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }
}
