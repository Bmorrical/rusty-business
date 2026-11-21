//! Merging two copies of one board: the copy on this computer, and the copy
//! in a file kept somewhere else.
//!
//! Nothing here reads or writes a file. `merge` takes the two copies and,
//! when there is one, the copy both came from, and gives back one board that
//! holds what each side changed. Where both changed the same thing, it says
//! so, and the caller decides whether anything is written.
//!
//! A board's `rev` names one state of the copy that is shared. Its `history`
//! lists the revs that state came from, the newest first. That is all a
//! program needs to tell whether one copy came from another.

use std::collections::{BTreeMap, BTreeSet};

use crate::board::{Board, Epic, FORMAT, HISTORY_LIMIT, Slice, Story};

/// One of the two copies: the one on this computer, or the one in the file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Side {
    Mine,
    Theirs,
}

/// One thing both sides changed, each in its own way.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    /// Where it is: "Story #412, title".
    pub what: String,
    /// What this computer's copy has there.
    pub mine: String,
    /// What the file has there.
    pub theirs: String,
}

/// What a merge gives back: the board, and every conflict it met. With no
/// side to keep, a board that has conflicts is not for writing.
#[derive(Clone, Debug, PartialEq)]
pub struct Merged {
    pub board: Board,
    pub conflicts: Vec<Conflict>,
}

/// Whether two copies hold the same board. When a copy was taken, and the
/// revs it carries, are bookkeeping and do not count.
pub fn same(a: &Board, b: &Board) -> bool {
    a.name == b.name && a.epics == b.epics
}

/// Whether `theirs` came from `base`: it is the same state, or `base` is in
/// its history. A file nobody stamps has no rev, and neither does the copy
/// kept of it, so two empty revs count as the same state.
pub fn descends(theirs: &Board, base: &Board) -> bool {
    base.rev == theirs.rev || (!base.rev.is_empty() && theirs.history.contains(&base.rev))
}

/// A board ready to be written where others read it: it takes a new rev, and
/// lists the revs it came from, the newest first.
pub fn stamped(mut board: Board, from: Option<&Board>, rev: String) -> Board {
    let mut history = Vec::new();
    if let Some(from) = from {
        if !from.rev.is_empty() {
            history.push(from.rev.clone());
        }
        history.extend(from.history.iter().cloned());
    }
    history.truncate(HISTORY_LIMIT);
    board.rev = rev;
    board.history = history;
    board.pulled_at = 0;
    board
}

/// A value from a board, short enough for one line of a terminal.
fn quoted(text: &str) -> String {
    if text.is_empty() {
        return "(empty)".to_string();
    }
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let short: String = line.chars().take(60).collect();
    if short.len() < line.len() {
        format!("\"{short}...\"")
    } else {
        format!("\"{short}\"")
    }
}

/// A list of notes that both sides may have added to or taken from. Notes
/// never conflict: one that either side added stays, and one that either
/// side took away goes. With nothing to compare against, every note stays.
fn notes(base: Option<&[String]>, mine: &[String], theirs: &[String]) -> Vec<String> {
    if mine == theirs || base == Some(theirs) {
        return mine.to_vec();
    }
    if base == Some(mine) {
        return theirs.to_vec();
    }
    let gone = |note: &String, other: &[String]| {
        base.is_some_and(|base| base.contains(note)) && !other.contains(note)
    };
    let mut kept: Vec<String> = mine
        .iter()
        .filter(|note| !gone(note, theirs))
        .cloned()
        .collect();
    for note in theirs {
        if !kept.contains(note) && !gone(note, mine) {
            kept.push(note.clone());
        }
    }
    kept
}

/// The order of what stays. The side that moved things leads, and what the
/// other side added goes in after whatever it followed there. When both
/// moved things, or there is nothing to compare against, `keep` leads.
fn order<K: Clone + Ord>(
    base: Option<&[K]>,
    mine: &[K],
    theirs: &[K],
    stays: &BTreeSet<K>,
    keep: Side,
) -> Vec<K> {
    let shared = |list: &[K]| -> Vec<K> {
        list.iter()
            .filter(|key| mine.contains(key) && theirs.contains(key))
            .filter(|key| base.is_none_or(|base| base.contains(key)))
            .cloned()
            .collect()
    };
    let moved = |list: &[K]| base.is_none_or(|base| shared(list) != shared(base));
    let lead = match (moved(mine), moved(theirs)) {
        (true, false) => Side::Mine,
        (false, true) => Side::Theirs,
        _ => keep,
    };
    let (first, second) = match lead {
        Side::Mine => (mine, theirs),
        Side::Theirs => (theirs, mine),
    };
    let mut out: Vec<K> = first
        .iter()
        .filter(|key| stays.contains(key))
        .cloned()
        .collect();
    let mut after = None;
    for key in second.iter().filter(|key| stays.contains(key)) {
        let at = match out.iter().position(|placed| placed == key) {
            Some(at) => at,
            None => {
                let at = after.map_or(0, |after| after + 1);
                out.insert(at, key.clone());
                at
            }
        };
        after = Some(at);
    }
    // Nothing that stays is ever left out, whatever the two lists held.
    for key in stays {
        if !out.contains(key) {
            out.push(key.clone());
        }
    }
    out
}

/// One merge in progress: the side that wins a conflict, and those met.
struct Merge {
    keep: Side,
    conflicts: Vec<Conflict>,
}

impl Merge {
    /// One value, three ways. A side that left it alone takes the other's.
    fn pick(
        &mut self,
        at: &str,
        field: &str,
        base: Option<&str>,
        mine: &str,
        theirs: &str,
    ) -> String {
        if mine == theirs || base == Some(theirs) {
            return mine.to_string();
        }
        if base == Some(mine) {
            return theirs.to_string();
        }
        self.conflicts.push(Conflict {
            what: format!("{at}, {field}"),
            mine: quoted(mine),
            theirs: quoted(theirs),
        });
        match self.keep {
            Side::Mine => mine.to_string(),
            Side::Theirs => theirs.to_string(),
        }
    }

    /// Whether something only one side has stays. `unchanged` is `None` when
    /// the copy both came from never had it, so that side added it. When it
    /// did, the other side took it away: that stands if `holder` left it as
    /// it was, and is a conflict if `holder` changed it.
    fn stays(&mut self, at: &str, holder: Side, unchanged: Option<bool>) -> bool {
        match unchanged {
            None => true,
            Some(true) => false,
            Some(false) => {
                let (mine, theirs) = match holder {
                    Side::Mine => ("changed", "removed"),
                    Side::Theirs => ("removed", "changed"),
                };
                self.conflicts.push(Conflict {
                    what: at.to_string(),
                    mine: mine.to_string(),
                    theirs: theirs.to_string(),
                });
                self.keep == holder
            }
        }
    }

    fn slice(&mut self, at: &str, base: Option<&Slice>, mine: &Slice, theirs: &Slice) -> Slice {
        if mine == theirs {
            return mine.clone();
        }
        Slice {
            id: mine.id.clone(),
            title: self.pick(
                at,
                "title",
                base.map(|was| was.title.as_str()),
                &mine.title,
                &theirs.title,
            ),
            detail: self.pick(
                at,
                "detail",
                base.map(|was| was.detail.as_str()),
                &mine.detail,
                &theirs.detail,
            ),
            lane: self.pick(
                at,
                "lane",
                base.map(|was| was.lane.as_str()),
                &mine.lane,
                &theirs.lane,
            ),
            turn: self.pick(
                at,
                "move",
                base.map(|was| was.turn.as_str()),
                &mine.turn,
                &theirs.turn,
            ),
            notes: notes(
                base.map(|was| was.notes.as_slice()),
                &mine.notes,
                &theirs.notes,
            ),
        }
    }

    fn slices(
        &mut self,
        at: &str,
        base: Option<&[Slice]>,
        mine: &[Slice],
        theirs: &[Slice],
    ) -> Vec<Slice> {
        let find = |list: &[Slice], id: &str| list.iter().find(|slice| slice.id == id).cloned();
        let ids =
            |list: &[Slice]| -> Vec<String> { list.iter().map(|slice| slice.id.clone()).collect() };
        let every: BTreeSet<String> = ids(mine).into_iter().chain(ids(theirs)).collect();
        let mut kept = BTreeMap::new();
        for id in every {
            let at = format!("{at}, slice {id}");
            let was = base.and_then(|base| find(base, &id));
            let slice = match (find(mine, &id), find(theirs, &id)) {
                (Some(mine), Some(theirs)) => Some(self.slice(&at, was.as_ref(), &mine, &theirs)),
                (Some(mine), None) => self
                    .stays(&at, Side::Mine, was.map(|was| was == mine))
                    .then_some(mine),
                (None, Some(theirs)) => self
                    .stays(&at, Side::Theirs, was.map(|was| was == theirs))
                    .then_some(theirs),
                (None, None) => None,
            };
            if let Some(slice) = slice {
                kept.insert(id, slice);
            }
        }
        let stays = kept.keys().cloned().collect();
        let base = base.map(ids);
        order(base.as_deref(), &ids(mine), &ids(theirs), &stays, self.keep)
            .into_iter()
            .filter_map(|id| kept.remove(&id))
            .collect()
    }

    fn story(&mut self, at: &str, base: Option<&Story>, mine: &Story, theirs: &Story) -> Story {
        if mine == theirs {
            return mine.clone();
        }
        Story {
            number: mine.number,
            kind: self.pick(
                at,
                "kind",
                base.map(|was| was.kind.as_str()),
                &mine.kind,
                &theirs.kind,
            ),
            title: self.pick(
                at,
                "title",
                base.map(|was| was.title.as_str()),
                &mine.title,
                &theirs.title,
            ),
            detail: self.pick(
                at,
                "detail",
                base.map(|was| was.detail.as_str()),
                &mine.detail,
                &theirs.detail,
            ),
            status: self.pick(
                at,
                "status",
                base.map(|was| was.status.as_str()),
                &mine.status,
                &theirs.status,
            ),
            turn: self.pick(
                at,
                "move",
                base.map(|was| was.turn.as_str()),
                &mine.turn,
                &theirs.turn,
            ),
            notes: notes(
                base.map(|was| was.notes.as_slice()),
                &mine.notes,
                &theirs.notes,
            ),
            slices: self.slices(
                at,
                base.map(|was| was.slices.as_slice()),
                &mine.slices,
                &theirs.slices,
            ),
        }
    }

    /// An epic's own words. Its stories are placed apart from it.
    fn epic(&mut self, at: &str, base: Option<&Epic>, mine: &Epic, theirs: &Epic) -> Epic {
        Epic {
            release: mine.release.clone(),
            title: self.pick(
                at,
                "title",
                base.map(|was| was.title.as_str()),
                &mine.title,
                &theirs.title,
            ),
            detail: self.pick(
                at,
                "detail",
                base.map(|was| was.detail.as_str()),
                &mine.detail,
                &theirs.detail,
            ),
            status: self.pick(
                at,
                "status",
                base.map(|was| was.status.as_str()),
                &mine.status,
                &theirs.status,
            ),
            stories: Vec::new(),
        }
    }
}

/// Every story on a board by its number, with the release it sits under.
fn placed(board: &Board) -> BTreeMap<u64, (&str, &Story)> {
    board
        .epics
        .iter()
        .flat_map(|epic| {
            epic.stories
                .iter()
                .map(|story| (story.number, (epic.release.as_str(), story)))
        })
        .collect()
}

fn releases(board: &Board) -> Vec<String> {
    board
        .epics
        .iter()
        .map(|epic| epic.release.clone())
        .collect()
}

/// The numbers of the stories under one release, in their order.
fn under(board: &Board, release: &str) -> Vec<u64> {
    board
        .epics
        .iter()
        .filter(|epic| epic.release == release)
        .flat_map(|epic| epic.stories.iter().map(|story| story.number))
        .collect()
}

/// An epic with its stories set aside, to tell whether its own words changed.
fn bare(epic: &Epic) -> Epic {
    Epic {
        stories: Vec::new(),
        ..epic.clone()
    }
}

/// Merges this computer's copy with the file's. `base` is the copy both came
/// from, or `None` when there is none to trust: then nothing is taken away,
/// and every difference between the two is a conflict.
///
/// A release is known by its name, a story by its number, and a slice by its
/// id within its story. `keep` settles every conflict for one side. The
/// merged board carries the file's rev, since that is the shared state it
/// now holds.
pub fn merge(base: Option<&Board>, mine: &Board, theirs: &Board, keep: Option<Side>) -> Merged {
    let mut board = Board {
        format: FORMAT.to_string(),
        id: mine.id.clone(),
        name: mine.name.clone(),
        rev: theirs.rev.clone(),
        history: theirs.history.clone(),
        pulled_at: mine.pulled_at,
        epics: Vec::new(),
    };
    // A side that changed nothing takes the other whole, in the other's order.
    let whole = if same(mine, theirs) || base.is_some_and(|base| same(base, mine)) {
        Some(theirs)
    } else if base.is_some_and(|base| same(base, theirs)) {
        Some(mine)
    } else {
        None
    };
    if let Some(whole) = whole {
        board.name = whole.name.clone();
        board.epics = whole.epics.clone();
        return Merged {
            board,
            conflicts: Vec::new(),
        };
    }

    let mut merge = Merge {
        keep: keep.unwrap_or(Side::Mine),
        conflicts: Vec::new(),
    };
    board.name = merge.pick(
        "The board",
        "name",
        base.map(|was| was.name.as_str()),
        &mine.name,
        &theirs.name,
    );

    // The stories first: which stay, what each holds, and where each sits.
    let (was, here, there) = (base.map(placed), placed(mine), placed(theirs));
    let numbers: BTreeSet<u64> = here.keys().chain(there.keys()).copied().collect();
    let mut stories: BTreeMap<u64, (String, Story)> = BTreeMap::new();
    for number in numbers {
        let at = format!("Story #{number}");
        let was = was.as_ref().and_then(|was| was.get(&number));
        let story = match (here.get(&number), there.get(&number)) {
            (Some(&(here, mine)), Some(&(there, theirs))) => {
                let release = merge.pick(&at, "release", was.map(|was| was.0), here, there);
                let story = merge.story(&at, was.map(|was| was.1), mine, theirs);
                Some((release, story))
            }
            (Some(&only), None) => merge
                .stays(&at, Side::Mine, was.map(|was| *was == only))
                .then(|| (only.0.to_string(), only.1.clone())),
            (None, Some(&only)) => merge
                .stays(&at, Side::Theirs, was.map(|was| *was == only))
                .then(|| (only.0.to_string(), only.1.clone())),
            (None, None) => None,
        };
        if let Some(story) = story {
            stories.insert(number, story);
        }
    }

    // Then the releases. One with a story under it always stays.
    let find = |board: &Board, release: &str| {
        board
            .epics
            .iter()
            .find(|epic| epic.release == release)
            .map(bare)
    };
    let names: BTreeSet<String> = releases(mine).into_iter().chain(releases(theirs)).collect();
    let mut epics: BTreeMap<String, Epic> = BTreeMap::new();
    for release in names {
        let at = format!("Release {}", quoted(&release));
        let was = base.and_then(|base| find(base, &release));
        let used = stories.values().any(|(under, _)| *under == release);
        let epic = match (find(mine, &release), find(theirs, &release)) {
            (Some(mine), Some(theirs)) => Some(merge.epic(&at, was.as_ref(), &mine, &theirs)),
            (Some(mine), None) => {
                (used || merge.stays(&at, Side::Mine, was.map(|was| was == mine))).then_some(mine)
            }
            (None, Some(theirs)) => (used
                || merge.stays(&at, Side::Theirs, was.map(|was| was == theirs)))
            .then_some(theirs),
            (None, None) => None,
        };
        if let Some(epic) = epic {
            epics.insert(release, epic);
        }
    }

    // Last, the order: of the releases, and of the stories under each.
    let stays = epics.keys().cloned().collect();
    let before = base.map(releases);
    let road = order(
        before.as_deref(),
        &releases(mine),
        &releases(theirs),
        &stays,
        merge.keep,
    );
    for release in road {
        let Some(mut epic) = epics.remove(&release) else {
            continue;
        };
        let stays = stories
            .iter()
            .filter(|(_, (under, _))| *under == release)
            .map(|(number, _)| *number)
            .collect();
        let before = base.map(|base| under(base, &release));
        let numbers = order(
            before.as_deref(),
            &under(mine, &release),
            &under(theirs, &release),
            &stays,
            merge.keep,
        );
        epic.stories = numbers
            .into_iter()
            .filter_map(|number| stories.remove(&number).map(|(_, story)| story))
            .collect();
        board.epics.push(epic);
    }

    Merged {
        board,
        conflicts: merge.conflicts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(id: &str, lane: &str) -> Slice {
        Slice {
            id: id.to_string(),
            title: format!("Slice {id}"),
            lane: lane.to_string(),
            ..Slice::default()
        }
    }

    fn story(number: u64, title: &str, slices: &[(&str, &str)]) -> Story {
        Story {
            number,
            kind: "story".to_string(),
            title: title.to_string(),
            slices: slices.iter().map(|(id, lane)| slice(id, lane)).collect(),
            ..Story::default()
        }
    }

    fn epic(release: &str, title: &str, stories: Vec<Story>) -> Epic {
        Epic {
            release: release.to_string(),
            title: title.to_string(),
            stories,
            ..Epic::default()
        }
    }

    /// A made-up road, as both sides last agreed on it.
    fn road() -> Board {
        Board {
            format: FORMAT.to_string(),
            id: "road".to_string(),
            name: "A road".to_string(),
            rev: "r1".to_string(),
            history: Vec::new(),
            pulled_at: 7,
            epics: vec![
                epic(
                    "1.2.0",
                    "Lists",
                    vec![
                        story(409, "Sort a list", &[("a", "main"), ("b", "written")]),
                        story(412, "Column headers", &[("a", "sliced")]),
                        story(413, "An empty state", &[]),
                    ],
                ),
                epic(
                    "1.3.0",
                    "Sharing",
                    vec![
                        story(420, "Print a list", &[]),
                        story(421, "Save a list", &[]),
                    ],
                ),
            ],
        }
    }

    fn at(board: &mut Board, number: u64) -> &mut Story {
        board
            .epics
            .iter_mut()
            .flat_map(|epic| &mut epic.stories)
            .find(|story| story.number == number)
            .unwrap()
    }

    fn slice_ids(board: &mut Board, number: u64) -> Vec<String> {
        at(board, number)
            .slices
            .iter()
            .map(|slice| slice.id.clone())
            .collect()
    }

    fn conflict(what: &str, mine: &str, theirs: &str) -> Conflict {
        Conflict {
            what: what.to_string(),
            mine: mine.to_string(),
            theirs: theirs.to_string(),
        }
    }

    /// A merge that met no conflict, with the board it made.
    fn clean(base: Option<&Board>, mine: &Board, theirs: &Board) -> Board {
        let merged = merge(base, mine, theirs, None);
        assert_eq!(merged.conflicts, Vec::<Conflict>::new());
        assert_eq!(merged.board.check(), Ok(()));
        merged.board
    }

    #[test]
    fn a_side_that_changed_nothing_takes_the_other_whole() {
        let base = road();
        let mut theirs = road();
        theirs.rev = "r2".to_string();
        theirs.history = vec!["r1".to_string()];
        theirs.name = "A longer road".to_string();
        at(&mut theirs, 412).title = "Sortable headers".to_string();
        at(&mut theirs, 409)
            .notes
            .push("Checked on a phone.".to_string());
        theirs.epics[0].stories.swap(0, 2);
        theirs.epics.swap(0, 1);

        // This copy stood still, so it takes the file's, order and all. It
        // stays this computer's copy, and now holds the file's state.
        let board = clean(Some(&base), &base, &theirs);
        assert_eq!(board.name, "A longer road");
        assert_eq!(board.epics, theirs.epics);
        assert_eq!((board.id.as_str(), board.pulled_at), ("road", 7));
        assert_eq!(board.rev, "r2");
        assert_eq!(board.history, ["r1"]);

        // The file stood still, so this copy is kept as it is.
        let mine = theirs.clone();
        let board = clean(Some(&base), &mine, &base);
        assert_eq!(board.name, "A longer road");
        assert_eq!(board.epics, mine.epics);
        assert_eq!(board.rev, "r1");
        assert!(board.history.is_empty());

        // Two copies that already agree need nothing to compare against.
        assert!(same(&clean(None, &mine, &theirs), &theirs));
    }

    // "bi-directional sync with conflict resolution"
    #[test]
    fn changes_to_different_things_are_both_kept() {
        let base = road();
        let (mut mine, mut theirs) = (road(), road());
        at(&mut mine, 409).title = "Sort any list".to_string();
        at(&mut mine, 409).slices[1].lane = "checked".to_string();
        mine.epics[1].title = "Sharing a list".to_string();
        at(&mut theirs, 409).detail = "Every column, both ways.".to_string();
        at(&mut theirs, 409).slices[0].title = "The arrow".to_string();
        at(&mut theirs, 409).slices[1].turn = "you".to_string();
        at(&mut theirs, 412).status = "Building".to_string();
        theirs.epics[1].status = "Scheduled".to_string();
        theirs.name = "The road".to_string();

        let mut board = clean(Some(&base), &mine, &theirs);
        assert_eq!(board.name, "The road");
        assert_eq!(board.epics[1].title, "Sharing a list");
        assert_eq!(board.epics[1].status, "Scheduled");
        assert_eq!(at(&mut board, 412).status, "Building");
        let story = at(&mut board, 409);
        assert_eq!(story.title, "Sort any list");
        assert_eq!(story.detail, "Every column, both ways.");
        assert_eq!(story.slices[0].title, "The arrow");
        assert_eq!(story.slices[1].lane, "checked");
        assert_eq!(story.slices[1].turn, "you");

        // The same change made on both sides is one change, not a conflict.
        at(&mut mine, 412).status = "Building".to_string();
        let mut board = clean(Some(&base), &mine, &theirs);
        assert_eq!(at(&mut board, 412).status, "Building");
    }

    #[test]
    fn the_same_thing_changed_two_ways_is_a_conflict_settled_for_one_side() {
        let base = road();
        let (mut mine, mut theirs) = (road(), road());
        at(&mut mine, 412).title = "Sortable headers".to_string();
        at(&mut theirs, 412).title = "Headers that sort".to_string();
        at(&mut mine, 409).slices[1].lane = "checked".to_string();
        at(&mut theirs, 409).slices[1].lane = "reviewed".to_string();
        at(&mut mine, 413).detail = "word ".repeat(40);
        at(&mut theirs, 413).detail = "Short.".to_string();
        at(&mut theirs, 420).status = "Scheduled".to_string();

        let met = [
            conflict("Story #409, slice b, lane", "\"checked\"", "\"reviewed\""),
            conflict(
                "Story #412, title",
                "\"Sortable headers\"",
                "\"Headers that sort\"",
            ),
            conflict(
                "Story #413, detail",
                &format!("\"{}...\"", "word ".repeat(12)),
                "\"Short.\"",
            ),
        ];
        assert_eq!(merge(Some(&base), &mine, &theirs, None).conflicts, met);

        let kept = merge(Some(&base), &mine, &theirs, Some(Side::Mine));
        assert_eq!(kept.conflicts, met);
        let mut board = kept.board;
        assert_eq!(at(&mut board, 412).title, "Sortable headers");
        assert_eq!(at(&mut board, 409).slices[1].lane, "checked");
        // What only one side changed is kept, whichever side wins.
        assert_eq!(at(&mut board, 420).status, "Scheduled");

        let kept = merge(Some(&base), &mine, &theirs, Some(Side::Theirs));
        assert_eq!(kept.conflicts, met);
        let mut board = kept.board;
        assert_eq!(at(&mut board, 412).title, "Headers that sort");
        assert_eq!(at(&mut board, 409).slices[1].lane, "reviewed");
        assert_eq!(at(&mut board, 413).detail, "Short.");
        assert_eq!(at(&mut board, 420).status, "Scheduled");

        assert_eq!(quoted(""), "(empty)");
        assert_eq!(quoted("two\n  lines"), "\"two lines\"");
    }

    #[test]
    fn what_each_side_added_is_kept() {
        let base = road();
        let (mut mine, mut theirs) = (road(), road());
        mine.epics[0].stories.insert(1, story(414, "A filter", &[]));
        at(&mut mine, 409).slices.push(slice("c", "sliced"));
        theirs.epics[1]
            .stories
            .push(story(422, "Share a link", &[("a", "sliced")]));
        theirs
            .epics
            .push(epic("1.4.0", "Later", vec![story(430, "Themes", &[])]));
        at(&mut theirs, 409).slices.push(slice("d", ""));

        let mut board = clean(Some(&base), &mine, &theirs);
        assert_eq!(releases(&board), ["1.2.0", "1.3.0", "1.4.0"]);
        assert_eq!(under(&board, "1.2.0"), [409, 414, 412, 413]);
        assert_eq!(under(&board, "1.3.0"), [420, 421, 422]);
        assert_eq!(under(&board, "1.4.0"), [430]);
        assert_eq!(at(&mut board, 422).slices.len(), 1);
        // What the file added goes in after what it followed there.
        assert_eq!(slice_ids(&mut board, 409), ["a", "b", "d", "c"]);
    }

    #[test]
    fn what_one_side_removed_is_removed_unless_the_other_changed_it() {
        let base = road();
        let (mut mine, mut theirs) = (road(), road());
        theirs.epics[0].stories.retain(|story| story.number == 409);
        at(&mut theirs, 409).slices.retain(|slice| slice.id != "b");
        mine.epics[1].stories.retain(|story| story.number != 421);
        at(&mut mine, 412).title = "Sortable headers".to_string();

        // 413, 421, and slice b were left alone by the other side, so they
        // go. 412 was changed here and removed there.
        let met = [conflict("Story #412", "changed", "removed")];
        let kept = merge(Some(&base), &mine, &theirs, Some(Side::Mine));
        assert_eq!(kept.conflicts, met);
        let mut board = kept.board;
        assert_eq!(under(&board, "1.2.0"), [409, 412]);
        assert_eq!(under(&board, "1.3.0"), [420]);
        assert_eq!(slice_ids(&mut board, 409), ["a"]);
        assert_eq!(at(&mut board, 412).title, "Sortable headers");

        let kept = merge(Some(&base), &mine, &theirs, Some(Side::Theirs));
        assert_eq!(kept.conflicts, met);
        assert_eq!(under(&kept.board, "1.2.0"), [409]);

        // A slice, the same way, with the sides the other way round.
        let (mut mine, mut theirs) = (road(), road());
        at(&mut mine, 409).slices.retain(|slice| slice.id != "b");
        at(&mut theirs, 409).slices[1].lane = "checked".to_string();
        let met = [conflict("Story #409, slice b", "removed", "changed")];
        let kept = merge(Some(&base), &mine, &theirs, Some(Side::Mine));
        assert_eq!(kept.conflicts, met);
        let mut board = kept.board;
        assert_eq!(slice_ids(&mut board, 409), ["a"]);
        let kept = merge(Some(&base), &mine, &theirs, Some(Side::Theirs));
        let mut board = kept.board;
        assert_eq!(slice_ids(&mut board, 409), ["a", "b"]);
        assert_eq!(at(&mut board, 409).slices[1].lane, "checked");
    }

    #[test]
    fn a_story_moved_to_another_release_moves() {
        let base = road();
        let (mut mine, mut theirs) = (road(), road());
        let moved = theirs.epics[0].stories.pop().unwrap();
        theirs.epics[1].stories.insert(0, moved);
        at(&mut mine, 413).title = "An empty state for the list".to_string();

        let mut board = clean(Some(&base), &mine, &theirs);
        assert_eq!(under(&board, "1.2.0"), [409, 412]);
        assert_eq!(under(&board, "1.3.0"), [413, 420, 421]);
        assert_eq!(at(&mut board, 413).title, "An empty state for the list");

        // Moved two ways, it is a conflict like any other.
        let moved = mine.epics[0].stories.pop().unwrap();
        mine.epics.push(epic("1.4.0", "Later", vec![moved]));
        let met = [conflict("Story #413, release", "\"1.4.0\"", "\"1.3.0\"")];
        let kept = merge(Some(&base), &mine, &theirs, Some(Side::Mine));
        assert_eq!(kept.conflicts, met);
        assert_eq!(under(&kept.board, "1.4.0"), [413]);
        assert_eq!(under(&kept.board, "1.3.0"), [420, 421]);
        let kept = merge(Some(&base), &mine, &theirs, Some(Side::Theirs));
        assert_eq!(kept.conflicts, met);
        assert_eq!(under(&kept.board, "1.3.0"), [413, 420, 421]);
        // The release this side added stays, with nothing under it yet.
        assert_eq!(releases(&kept.board), ["1.2.0", "1.3.0", "1.4.0"]);
        assert_eq!(under(&kept.board, "1.4.0"), Vec::<u64>::new());
    }

    #[test]
    fn notes_on_the_board_are_added_to_and_taken_from_by_both_sides() {
        let mut base = road();
        at(&mut base, 409).notes = vec!["One.".to_string(), "Two.".to_string()];
        let (mut mine, mut theirs) = (base.clone(), base.clone());
        at(&mut mine, 409).notes = vec!["Two.".to_string(), "Three.".to_string()];
        at(&mut theirs, 409).notes.push("Four.".to_string());

        let mut board = clean(Some(&base), &mine, &theirs);
        assert_eq!(at(&mut board, 409).notes, ["Two.", "Three.", "Four."]);

        // With nothing to compare against, every note stays.
        let mut board = clean(None, &mine, &theirs);
        assert_eq!(
            at(&mut board, 409).notes,
            ["Two.", "Three.", "One.", "Four."]
        );
    }

    #[test]
    fn the_side_that_moved_things_sets_the_order() {
        let base = road();
        let (mut mine, mut theirs) = (road(), road());
        theirs.epics[0].stories.rotate_right(1);
        theirs.epics.swap(0, 1);
        mine.epics[0].stories.insert(1, story(414, "A filter", &[]));

        let board = clean(Some(&base), &mine, &theirs);
        assert_eq!(releases(&board), ["1.3.0", "1.2.0"]);
        assert_eq!(under(&board, "1.2.0"), [413, 409, 414, 412]);

        // When both moved things, the order is the kept side's: this
        // computer's, unless the file's is asked for. It is not a conflict.
        mine.epics[0].stories.swap(0, 2);
        assert_eq!(under(&mine, "1.2.0"), [412, 414, 409, 413]);
        let board = clean(Some(&base), &mine, &theirs);
        assert_eq!(under(&board, "1.2.0"), [412, 414, 409, 413]);
        let kept = merge(Some(&base), &mine, &theirs, Some(Side::Theirs));
        assert!(kept.conflicts.is_empty());
        // 414 was added here after 412, so it follows 412 in the file's order.
        assert_eq!(under(&kept.board, "1.2.0"), [413, 409, 412, 414]);
    }

    #[test]
    fn with_nothing_to_compare_against_nothing_is_taken_away() {
        let (mut mine, mut theirs) = (road(), road());
        mine.epics[0].stories.push(story(414, "A filter", &[]));
        at(&mut mine, 409).title = "Sort any list".to_string();
        theirs.epics[0].stories.retain(|story| story.number != 413);
        theirs.epics[1]
            .stories
            .push(story(422, "Share a link", &[]));
        at(&mut theirs, 409).title = "Sorting".to_string();
        at(&mut theirs, 409).slices.remove(1);

        let merged = merge(None, &mine, &theirs, None);
        assert_eq!(
            merged.conflicts,
            [conflict(
                "Story #409, title",
                "\"Sort any list\"",
                "\"Sorting\""
            )]
        );
        let mut board = merged.board;
        assert_eq!(under(&board, "1.2.0"), [409, 412, 413, 414]);
        assert_eq!(under(&board, "1.3.0"), [420, 421, 422]);
        assert_eq!(slice_ids(&mut board, 409), ["a", "b"]);
    }

    #[test]
    fn a_release_goes_with_its_stories_and_stays_while_one_remains() {
        let base = road();
        let (mut mine, mut theirs) = (road(), road());
        theirs.epics.pop();
        at(&mut mine, 409).title = "Sort any list".to_string();
        let board = clean(Some(&base), &mine, &theirs);
        assert_eq!(releases(&board), ["1.2.0"]);
        assert_eq!(board.stories(), 3);

        // A story added under it here keeps it, with that story alone.
        mine.epics[1].stories.push(story(422, "Share a link", &[]));
        let board = clean(Some(&base), &mine, &theirs);
        assert_eq!(releases(&board), ["1.2.0", "1.3.0"]);
        assert_eq!(under(&board, "1.3.0"), [422]);

        // Its own words changed here, and it was removed there.
        let mut mine = road();
        mine.epics[1].title = "Sharing a list".to_string();
        let met = [conflict("Release \"1.3.0\"", "changed", "removed")];
        let kept = merge(Some(&base), &mine, &theirs, Some(Side::Mine));
        assert_eq!(kept.conflicts, met);
        assert_eq!(releases(&kept.board), ["1.2.0", "1.3.0"]);
        assert_eq!(kept.board.epics[1].title, "Sharing a list");
        let kept = merge(Some(&base), &mine, &theirs, Some(Side::Theirs));
        assert_eq!(kept.conflicts, met);
        assert_eq!(releases(&kept.board), ["1.2.0"]);
    }

    // "any user can use the import export and build their own backend"
    #[test]
    fn a_file_comes_from_a_copy_when_the_copy_is_in_its_history() {
        let base = road();
        let mut theirs = road();
        assert!(descends(&theirs, &base));
        theirs.rev = "r3".to_string();
        assert!(!descends(&theirs, &base));
        theirs.history = vec!["r2".to_string(), "r1".to_string()];
        assert!(descends(&theirs, &base));
        theirs.history = vec!["r2".to_string(), "r0".to_string()];
        assert!(!descends(&theirs, &base));

        // A file nobody stamps has no rev. Two of those are the same state,
        // and one that gains a rev no longer is.
        let (mut plain, mut copy) = (road(), road());
        plain.rev.clear();
        copy.rev.clear();
        assert!(descends(&plain, &copy));
        assert!(!descends(&theirs, &copy));
    }

    #[test]
    fn a_stamped_board_names_its_state_and_lists_where_it_came_from() {
        let mut from = road();
        from.rev = "r2".to_string();
        from.history = vec!["r1".to_string()];

        let sent = stamped(road(), Some(&from), "r3".to_string());
        assert_eq!(sent.rev, "r3");
        assert_eq!(sent.history, ["r2", "r1"]);
        assert_eq!(sent.pulled_at, 0);
        assert!(same(&sent, &road()));
        assert!(descends(&sent, &from));
        assert_eq!(sent.check(), Ok(()));

        // The first one written comes from nothing.
        let first = stamped(road(), None, "r1".to_string());
        assert!(first.history.is_empty());

        // A copy with no rev adds none, and the list never outgrows its limit.
        from.rev.clear();
        from.history = (0..HISTORY_LIMIT).map(|n| format!("r{n}")).collect();
        let sent = stamped(road(), Some(&from), "next".to_string());
        assert_eq!(sent.history, from.history);
        from.rev = "newest".to_string();
        let sent = stamped(road(), Some(&from), "next".to_string());
        assert_eq!(sent.history.len(), HISTORY_LIMIT);
        assert_eq!(sent.history[0], "newest");
        assert_eq!(sent.history[1], "r0");
        assert_eq!(sent.check(), Ok(()));
    }
}
