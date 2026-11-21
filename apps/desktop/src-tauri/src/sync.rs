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
//!
//! A key this version of the format does not know is merged as any other
//! value is, so a newer file loses nothing by passing through here. The two
//! copies may be on different lines of the format: the merged board is on
//! the higher one, and the copy from the lower line takes nothing away that
//! only the higher line holds.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use crate::board::{Board, Epic, Extra, HISTORY_LIMIT, Slice, Status, Story};

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

/// Whether two copies hold the same board. When a copy was taken, the revs
/// it carries, who wrote it, and the version of the format it is on are
/// bookkeeping and do not count.
pub fn same(a: &Board, b: &Board) -> bool {
    a.name == b.name
        && a.epics == b.epics
        && a.statuses == b.statuses
        && a.extra == b.extra
        && a.meta == b.meta
        && a.data == b.data
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

/// A list of statuses, short enough for one line of a terminal.
fn named(list: &[Status]) -> String {
    let names: Vec<&str> = list.iter().map(|status| status.name.as_str()).collect();
    quoted(&names.join(", "))
}

/// The line of the format a board is on.
fn line(board: &Board) -> u32 {
    board.level().map_or(0, |(line, _)| line)
}

/// A copy from a lower line of the format, raised to the line `high` is on.
/// What only the higher line holds comes from `high`: the statuses, what its
/// two blocks hold beside their own keys, and each key the lower copy does
/// not have. A row's status id comes too, while the row's status reads the
/// same on both. So a file on a lower line changes what it names, and takes
/// away nothing it has no place for.
fn lifted(low: &Board, high: &Board) -> Board {
    fn fill(extra: &mut Extra, from: &Extra) {
        for (key, value) in from {
            extra.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    let mut board = low.clone();
    board.format = high.format.clone();
    board.version = high.version.clone();
    board.statuses = high.statuses.clone();
    board.meta = high.meta.clone();
    board.data = high.data.clone();
    fill(&mut board.extra, &high.extra);
    let stories = placed(high);
    for epic in &mut board.epics {
        if let Some(above) = high
            .epics
            .iter()
            .find(|above| above.release == epic.release)
        {
            if epic.status_id.is_empty() && epic.status == above.status {
                epic.status_id = above.status_id.clone();
            }
            fill(&mut epic.extra, &above.extra);
        }
        for story in &mut epic.stories {
            let Some((_, above)) = stories.get(&story.number) else {
                continue;
            };
            if story.status_id.is_empty() && story.status == above.status {
                story.status_id = above.status_id.clone();
            }
            fill(&mut story.extra, &above.extra);
            for slice in &mut story.slices {
                if let Some(above) = above.slices.iter().find(|above| above.id == slice.id) {
                    fill(&mut slice.extra, &above.extra);
                }
            }
        }
    }
    board
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
    fn choose<T: Clone + PartialEq>(
        &mut self,
        what: impl FnOnce() -> String,
        base: Option<&T>,
        mine: &T,
        theirs: &T,
        show: impl Fn(&T) -> String,
    ) -> T {
        if mine == theirs || base == Some(theirs) {
            return mine.clone();
        }
        if base == Some(mine) {
            return theirs.clone();
        }
        self.conflicts.push(Conflict {
            what: what(),
            mine: show(mine),
            theirs: show(theirs),
        });
        match self.keep {
            Side::Mine => mine.clone(),
            Side::Theirs => theirs.clone(),
        }
    }

    /// One run of words, three ways.
    fn pick(
        &mut self,
        at: &str,
        field: &str,
        base: Option<&str>,
        mine: &str,
        theirs: &str,
    ) -> String {
        self.choose(
            || format!("{at}, {field}"),
            base.as_ref(),
            &mine,
            &theirs,
            |text| quoted(text),
        )
        .to_string()
    }

    /// A slice's number, which one side may not have. The side that has one
    /// gives it, since a backend that keeps no number takes none away.
    fn number(&mut self, at: &str, base: Option<u64>, mine: u64, theirs: u64) -> u64 {
        if mine == 0 || theirs == 0 {
            return mine.max(theirs);
        }
        self.choose(
            || format!("{at}, number"),
            base.as_ref(),
            &mine,
            &theirs,
            |number| number.to_string(),
        )
    }

    /// The keys this version of the format does not know, each one three
    /// ways. With no copy to compare against, a key only one side has stays.
    fn extras(&mut self, at: &str, base: Option<&Extra>, mine: &Extra, theirs: &Extra) -> Extra {
        if mine == theirs {
            return mine.clone();
        }
        let keys: BTreeSet<&String> = mine.keys().chain(theirs.keys()).collect();
        let mut kept = Extra::new();
        for key in keys {
            let (here, there) = (mine.get(key).cloned(), theirs.get(key).cloned());
            let value = match (base, &here, &there) {
                (None, None, _) => there,
                (None, _, None) => here,
                _ => {
                    let was = base.map(|base| base.get(key).cloned());
                    self.choose(
                        || format!("{at}, key \"{key}\""),
                        was.as_ref(),
                        &here,
                        &there,
                        |value| match value {
                            Some(value) => quoted(&value.to_string()),
                            None => "(none)".to_string(),
                        },
                    )
                }
            };
            if let Some(value) = value {
                kept.insert(key.clone(), value);
            }
        }
        kept
    }

    /// One of a board's lists of statuses, three ways, as one whole list.
    /// With no copy to compare against, a side with no list takes the other's.
    fn statuses(
        &mut self,
        what: &str,
        base: Option<&Vec<Status>>,
        mine: &Vec<Status>,
        theirs: &Vec<Status>,
    ) -> Vec<Status> {
        if base.is_none() && (mine.is_empty() || theirs.is_empty()) {
            return if mine.is_empty() { theirs } else { mine }.clone();
        }
        self.choose(
            || format!("The board, {what} statuses"),
            base,
            mine,
            theirs,
            |list| named(list),
        )
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
            number: self.number(at, base.map(|was| was.number), mine.number, theirs.number),
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
            extra: self.extras(at, base.map(|was| &was.extra), &mine.extra, &theirs.extra),
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
            status_id: self.pick(
                at,
                "status id",
                base.map(|was| was.status_id.as_str()),
                &mine.status_id,
                &theirs.status_id,
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
            extra: self.extras(at, base.map(|was| &was.extra), &mine.extra, &theirs.extra),
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
            status_id: self.pick(
                at,
                "status id",
                base.map(|was| was.status_id.as_str()),
                &mine.status_id,
                &theirs.status_id,
            ),
            stories: Vec::new(),
            extra: self.extras(at, base.map(|was| &was.extra), &mine.extra, &theirs.extra),
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
/// now holds, and says who wrote the file. It is on the newer of the two
/// versions of the format, so it is never written back as an older one.
pub fn merge(base: Option<&Board>, mine: &Board, theirs: &Board, keep: Option<Side>) -> Merged {
    // A copy on a lower line of the format is raised first, so that what the
    // lower line has no place for is not read as taken away.
    let top = if line(theirs) >= line(mine) {
        theirs
    } else {
        mine
    };
    let raised = |board| -> Cow<Board> {
        if line(board) < line(top) {
            Cow::Owned(lifted(board, top))
        } else {
            Cow::Borrowed(board)
        }
    };
    let (mine, theirs, base) = (raised(mine), raised(theirs), base.map(raised));
    let (mine, theirs, base) = (mine.as_ref(), theirs.as_ref(), base.as_deref());
    let newest = if theirs.level() >= mine.level() {
        theirs
    } else {
        mine
    };
    let mut board = Board {
        format: newest.format.clone(),
        version: newest.version.clone(),
        id: mine.id.clone(),
        name: mine.name.clone(),
        written_by: theirs.written_by.clone(),
        written_at: theirs.written_at.clone(),
        rev: theirs.rev.clone(),
        history: theirs.history.clone(),
        pulled_at: mine.pulled_at,
        ..Board::default()
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
        board.statuses = whole.statuses.clone();
        board.extra = whole.extra.clone();
        board.meta = whole.meta.clone();
        board.data = whole.data.clone();
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
    board.statuses.stories = merge.statuses(
        "story",
        base.map(|was| &was.statuses.stories),
        &mine.statuses.stories,
        &theirs.statuses.stories,
    );
    board.statuses.releases = merge.statuses(
        "release",
        base.map(|was| &was.statuses.releases),
        &mine.statuses.releases,
        &theirs.statuses.releases,
    );
    board.statuses.extra = merge.extras(
        "The board's statuses",
        base.map(|was| &was.statuses.extra),
        &mine.statuses.extra,
        &theirs.statuses.extra,
    );
    board.extra = merge.extras(
        "The board",
        base.map(|was| &was.extra),
        &mine.extra,
        &theirs.extra,
    );
    board.meta = merge.extras(
        "The board's meta block",
        base.map(|was| &was.meta),
        &mine.meta,
        &theirs.meta,
    );
    board.data = merge.extras(
        "The board's data block",
        base.map(|was| &was.data),
        &mine.data,
        &theirs.data,
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
    use crate::board::{FORMAT, FORMAT_V1};

    /// Line 2, step 0.
    const VERSION: &str = "2.0";

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
            version: VERSION.to_string(),
            id: "road".to_string(),
            name: "A road".to_string(),
            rev: "r1".to_string(),
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
            ..Board::default()
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

    fn status(id: &str, name: &str) -> Status {
        Status {
            id: id.to_string(),
            name: name.to_string(),
            ..Status::default()
        }
    }

    fn key(extra: &mut Extra, name: &str, value: serde_json::Value) {
        extra.insert(name.to_string(), value);
    }

    /// The made-up road on this app's line, with what only that line holds:
    /// statuses, a status id on a row, a slice's number, and keys this app
    /// does not know.
    fn full() -> Board {
        let mut board = road();
        board.statuses.stories = vec![status("ready", "Ready"), status("building", "Building")];
        board.statuses.releases = vec![status("open", "Open"), status("out", "Out")];
        board.epics[0].status = "Open".to_string();
        board.epics[0].status_id = "open".to_string();
        key(&mut board.extra, "colour", serde_json::json!("green"));
        key(&mut board.meta, "owner", serde_json::json!("a team"));
        key(&mut board.data, "sprints", serde_json::json!([1, 2]));
        key(&mut board.epics[0].extra, "due", serde_json::json!("soon"));
        let story = at(&mut board, 409);
        story.status = "Ready".to_string();
        story.status_id = "ready".to_string();
        key(&mut story.extra, "points", serde_json::json!(3));
        story.slices[0].number = 7001;
        key(
            &mut story.slices[0].extra,
            "files",
            serde_json::json!(["a.rs"]),
        );
        board
    }

    /// The same board as a file on line 1 holds it: no statuses, no status
    /// ids, and none of the keys line 1 has no place for.
    fn lower(board: &Board) -> Board {
        let mut low = board.clone();
        low.format = FORMAT_V1.to_string();
        low.version.clear();
        low.statuses = Default::default();
        low.extra.clear();
        low.meta.clear();
        low.data.clear();
        for epic in &mut low.epics {
            epic.status_id.clear();
            epic.extra.clear();
            for story in &mut epic.stories {
                story.status_id.clear();
                story.extra.clear();
                for slice in &mut story.slices {
                    slice.extra.clear();
                }
            }
        }
        low
    }

    #[test]
    fn a_key_this_app_does_not_know_is_merged_like_any_other() {
        // "so someone doesn't have to remap when the next party wants to
        // iterate it's logic ahead."
        let base = full();
        let mut mine = full();
        at(&mut mine, 412).title = "Sortable headers".to_string();
        let mut theirs = full();
        key(&mut theirs.extra, "colour", serde_json::json!("blue"));
        key(&mut theirs.meta, "team", serde_json::json!(4));
        theirs.data.remove("sprints");
        key(
            &mut theirs.epics[0].extra,
            "due",
            serde_json::json!("later"),
        );
        key(
            &mut at(&mut theirs, 409).extra,
            "points",
            serde_json::json!(5),
        );
        key(
            &mut at(&mut theirs, 409).slices[0].extra,
            "owner",
            serde_json::json!({"name": "pat"}),
        );
        key(
            &mut theirs.statuses.extra,
            "note",
            serde_json::json!("as agreed"),
        );

        let mut board = clean(Some(&base), &mine, &theirs);
        assert_eq!(board.extra["colour"], "blue");
        assert_eq!(board.meta["owner"], "a team");
        assert_eq!(board.meta["team"], 4);
        assert!(board.data.is_empty());
        assert_eq!(board.epics[0].extra["due"], "later");
        assert_eq!(board.statuses.extra["note"], "as agreed");
        let story = at(&mut board, 409);
        assert_eq!(story.extra["points"], 5);
        assert_eq!(story.slices[0].extra["files"][0], "a.rs");
        assert_eq!(story.slices[0].extra["owner"]["name"], "pat");
        assert_eq!(at(&mut board, 412).title, "Sortable headers");

        // The same key changed two ways is a conflict, named by its key.
        key(
            &mut at(&mut mine, 409).extra,
            "points",
            serde_json::json!(8),
        );
        mine.epics[0].extra.remove("due");
        let merged = merge(Some(&base), &mine, &theirs, None);
        assert_eq!(
            merged.conflicts,
            [
                conflict("Story #409, key \"points\"", "\"8\"", "\"5\""),
                conflict("Release \"1.2.0\", key \"due\"", "(none)", "\"\"later\"\""),
            ]
        );
        let mut kept = merge(Some(&base), &mine, &theirs, Some(Side::Theirs)).board;
        assert_eq!(at(&mut kept, 409).extra["points"], 5);
        assert_eq!(kept.epics[0].extra["due"], "later");
        let mut kept = merge(Some(&base), &mine, &theirs, Some(Side::Mine)).board;
        assert_eq!(at(&mut kept, 409).extra["points"], 8);
        assert!(!kept.epics[0].extra.contains_key("due"));

        // With no copy to compare against, a key only one side has stays.
        let mut mine = full();
        key(&mut mine.extra, "mine", serde_json::json!(true));
        let mut theirs = full();
        key(&mut theirs.extra, "theirs", serde_json::json!(true));
        let board = clean(None, &mine, &theirs);
        assert!(board.extra.contains_key("mine") && board.extra.contains_key("theirs"));
    }

    #[test]
    fn a_slices_number_is_never_taken_away_by_a_side_that_has_none() {
        let base = full();
        let mut mine = full();
        at(&mut mine, 409).slices[0].lane = "written".to_string();
        let mut theirs = full();
        at(&mut theirs, 409).slices[0].number = 0;
        at(&mut theirs, 409).slices[1].number = 7002;

        let mut board = clean(Some(&base), &mine, &theirs);
        let slices = &at(&mut board, 409).slices;
        assert_eq!((slices[0].number, slices[1].number), (7001, 7002));
        assert_eq!(slices[0].lane, "written");
        let mut board = clean(Some(&base), &theirs, &mine);
        assert_eq!(at(&mut board, 409).slices[0].number, 7001);

        // Two numbers for one slice is a conflict.
        at(&mut mine, 409).slices[0].number = 7005;
        at(&mut theirs, 409).slices[0].number = 7009;
        let merged = merge(Some(&base), &mine, &theirs, None);
        assert_eq!(
            merged.conflicts,
            [conflict("Story #409, slice a, number", "7005", "7009")]
        );
    }

    #[test]
    fn a_status_id_and_the_lists_of_statuses_are_merged_three_ways() {
        let base = full();
        let mut mine = full();
        at(&mut mine, 412).title = "Sortable headers".to_string();
        let mut theirs = full();
        at(&mut theirs, 409).status = "Building".to_string();
        at(&mut theirs, 409).status_id = "building".to_string();
        theirs.epics[0].status_id = "out".to_string();
        theirs.statuses.stories[0].name = "Ready to slice".to_string();
        theirs.statuses.releases.push(status("parked", "Parked"));

        let mut board = clean(Some(&base), &mine, &theirs);
        assert_eq!(at(&mut board, 409).status_id, "building");
        assert_eq!(board.epics[0].status_id, "out");
        assert_eq!(board.statuses, theirs.statuses);

        // Each list is one whole thing. Changed two ways, it is a conflict.
        at(&mut mine, 409).status_id = "done".to_string();
        mine.epics[0].status_id = "open-again".to_string();
        mine.statuses.stories.swap(0, 1);
        mine.statuses.releases.clear();
        let merged = merge(Some(&base), &mine, &theirs, None);
        assert_eq!(
            merged.conflicts,
            [
                conflict(
                    "The board, story statuses",
                    "\"Building, Ready\"",
                    "\"Ready to slice, Building\""
                ),
                conflict(
                    "The board, release statuses",
                    "(empty)",
                    "\"Open, Out, Parked\""
                ),
                conflict("Story #409, status id", "\"done\"", "\"building\""),
                conflict("Release \"1.2.0\", status id", "\"open-again\"", "\"out\""),
            ]
        );
        let kept = merge(Some(&base), &mine, &theirs, Some(Side::Theirs)).board;
        assert_eq!(kept.statuses, theirs.statuses);

        // With no copy to compare against, a side with no list takes the
        // other's, and that is no conflict.
        let mut mine = full();
        mine.statuses.stories.clear();
        let mut theirs = full();
        theirs.statuses.releases.clear();
        let board = clean(None, &mine, &theirs);
        assert_eq!(board.statuses, full().statuses);
    }

    #[test]
    fn a_file_on_a_lower_line_takes_away_nothing_it_has_no_place_for() {
        // This copy is on line 2, and the file is a board v1 file of the same
        // board with one title changed, as a backend still on line 1 writes.
        let base = full();
        let mut theirs = lower(&full());
        theirs.rev = "r2".to_string();
        at(&mut theirs, 412).title = "Sortable headers".to_string();
        assert_eq!((line(&base), line(&theirs)), (2, 1));

        let mut board = clean(Some(&base), &base, &theirs);
        assert_eq!(
            (board.format.as_str(), board.version.as_str()),
            (FORMAT, VERSION)
        );
        assert_eq!(board.rev, "r2");
        assert_eq!(at(&mut board, 412).title, "Sortable headers");
        let mut expected = full();
        at(&mut expected, 412).title = "Sortable headers".to_string();
        assert!(same(&board, &expected));

        // Both sides changed, so the merge goes field by field, and still
        // nothing of line 2 is lost.
        let mut mine = full();
        at(&mut mine, 413).title = "An empty list".to_string();
        let mut board = clean(Some(&base), &mine, &theirs);
        at(&mut expected, 413).title = "An empty list".to_string();
        assert!(same(&board, &expected));
        assert_eq!(at(&mut board, 409).slices[0].number, 7001);

        // The file names a new status for a story, in words alone. The id
        // that went with the old words does not stay beside the new ones.
        at(&mut theirs, 409).status = "Building".to_string();
        let mut board = clean(Some(&base), &mine, &theirs);
        let story = at(&mut board, 409);
        assert_eq!(
            (story.status.as_str(), story.status_id.as_str()),
            ("Building", "")
        );
        assert_eq!(board.epics[0].status_id, "open");

        // The other way: this copy came from a board v1 file, and the file
        // is now on line 2. The copy moves up, and takes what line 2 holds.
        let (base, mine) = (lower(&full()), lower(&full()));
        let theirs = full();
        let board = clean(Some(&base), &mine, &theirs);
        assert_eq!(
            (board.format.as_str(), board.version.as_str()),
            (FORMAT, VERSION)
        );
        assert!(same(&board, &full()));

        // Two copies on line 1 stay on line 1.
        let board = clean(Some(&base), &mine, &base);
        assert_eq!(
            (board.format.as_str(), board.version.as_str()),
            (FORMAT_V1, "")
        );
        assert_eq!(line(&board), 1);
    }

    #[test]
    fn a_backend_that_moves_up_a_line_and_names_its_statuses_is_no_conflict() {
        // On line 1 the backend wrote the value it stores as each row's
        // status. Its first line 2 file writes the name people read, with
        // the stored value beside it as the id. So every row's `status`
        // reads differently from the last copy, on rows nobody touched here.
        let mut base = lower(&full());
        for epic in &mut base.epics {
            epic.status = "in_progress".to_string();
            for story in &mut epic.stories {
                story.status = "in_progress".to_string();
            }
        }
        let mut theirs = full();
        theirs.rev = "r2".to_string();
        theirs.statuses.stories = vec![status("in_progress", "In Progress")];
        theirs.statuses.releases = vec![status("in_progress", "In Progress")];
        for epic in &mut theirs.epics {
            epic.status = "In Progress".to_string();
            epic.status_id = "in_progress".to_string();
            for story in &mut epic.stories {
                story.status = "In Progress".to_string();
                story.status_id = "in_progress".to_string();
            }
        }
        assert_eq!((line(&base), line(&theirs)), (1, 2));

        // This copy is as the last one was, so the file is taken whole.
        let board = clean(Some(&base), &base, &theirs);
        assert!(same(&board, &theirs));
        assert_eq!(board.rev, "r2");

        // A title changed here stays, and is still no conflict: the status
        // on that row is the backend's change alone.
        let mut mine = base.clone();
        at(&mut mine, 413).title = "An empty list".to_string();
        let mut board = clean(Some(&base), &mine, &theirs);
        let story = at(&mut board, 413);
        assert_eq!(
            (
                story.title.as_str(),
                story.status.as_str(),
                story.status_id.as_str()
            ),
            ("An empty list", "In Progress", "in_progress")
        );
        let rows: Vec<&Story> = board.epics.iter().flat_map(|epic| &epic.stories).collect();
        assert!(
            rows.iter()
                .all(|story| story.status == "In Progress" && story.status_id == "in_progress")
        );
        assert!(
            board
                .epics
                .iter()
                .all(|epic| epic.status == "In Progress" && epic.status_id == "in_progress")
        );

        // Only a row whose status was changed here too is a conflict, and
        // it is the one row.
        at(&mut mine, 412).status = "blocked".to_string();
        let merged = merge(Some(&base), &mine, &theirs, None);
        assert_eq!(
            merged.conflicts,
            [conflict(
                "Story #412, status",
                "\"blocked\"",
                "\"In Progress\""
            )]
        );
    }

    #[test]
    fn the_merged_board_is_on_the_newer_of_the_two_versions() {
        let base = road();
        let mut newer = road();
        newer.version = "2.7".to_string();
        assert_eq!(clean(Some(&base), &base, &newer).version, "2.7");
        assert_eq!(clean(Some(&base), &newer, &base).version, "2.7");

        // Field by field too, whichever side holds it.
        let mut mine = road();
        at(&mut mine, 412).title = "Sortable headers".to_string();
        at(&mut newer, 413).title = "An empty list".to_string();
        assert_eq!(clean(Some(&base), &mine, &newer).version, "2.7");
        mine.version = "2.10".to_string();
        assert_eq!(clean(Some(&base), &mine, &newer).version, "2.10");
    }
}
