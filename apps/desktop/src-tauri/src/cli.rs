//! The `board` command: keeping a board on this computer in step with one
//! file kept somewhere else, from a terminal.
//!
//! `rusty-business board ...` does its work and leaves, and the app's window
//! never opens. It reaches no network. The only paths it takes are the ones
//! the person types, and every file it reads or writes goes through
//! `board.rs`, under the rules for any board file. What changed on each side
//! is worked out in `sync.rs`.
//!
//! A pull never changes the file, and a push never changes this computer's
//! copy. When both sides changed the same thing, nothing is written until
//! the person says which side to keep.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::board::Boards;
use crate::sync::{self, Conflict, Merged, Side};

/// The name of the app's data folder, as in `tauri.conf.json`. The command
/// runs without the app, so it finds the folder itself.
pub const IDENTIFIER: &str = "net.morrical.rusty-business";

/// What the command leaves with.
const DONE: i32 = 0;
const REFUSED: i32 = 1;
const CONFLICTS: i32 = 2;

/// The most conflicts listed in full. The rest are counted.
const SHOWN: usize = 50;

const HELP: &str = "\
Keeps a board on this computer in step with one file kept somewhere else.

  rusty-business board list
  rusty-business board import <file> [--mine | --theirs]
  rusty-business board export <id> <file>
  rusty-business board pull [<id>] [--mine | --theirs]
  rusty-business board push [<id>]
  rusty-business board sync [<id>] [--mine | --theirs]

  list    Every board on this computer, and the file each one has.
  import  Brings a board file's changes into this computer's copy, and
          remembers the file.
  export  Writes this computer's copy to a board file, and remembers the file.
  pull    An import, from the file the board already has.
  push    An export, to the file the board already has.
  sync    A pull, then a push.

With no id, pull, push, and sync do every board that has a file.

When both sides changed the same thing, nothing is written and each conflict
is listed. Run it again with --mine to keep what is on this computer, or with
--theirs to keep what the file has.

It leaves with 0 when it is done, 1 when it refused or failed, and 2 when
there are conflicts to settle.";

fn say(out: &mut dyn Write, line: &str) {
    let _ = writeln!(out, "{line}");
}

fn refuse(out: &mut dyn Write, line: &str) -> i32 {
    say(out, line);
    REFUSED
}

/// A command that cannot be read: why, then how it is written.
fn wrong(out: &mut dyn Write, why: &str) -> i32 {
    say(out, why);
    say(out, "");
    refuse(out, HELP)
}

fn counted(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

/// A path as the person typed it, from the folder they typed it in.
fn absolute(file: &str) -> Result<PathBuf, String> {
    std::path::absolute(file)
        .map_err(|error| format!("Could not work out where \"{file}\" is: {error}."))
}

/// Runs the `board` command when that is what was asked for, and gives back
/// what to leave with. Anything else is the app's own start, and gets `None`.
pub fn run(args: &[String]) -> Option<i32> {
    if args.first().map(String::as_str) != Some("board") {
        return None;
    }
    let mut out = std::io::stdout();
    let Some(data) = dirs::data_dir() else {
        return Some(refuse(
            &mut out,
            "Could not find this computer's data folder.",
        ));
    };
    let boards = Boards::new(data.join(IDENTIFIER).join("boards"));
    Some(run_in(&boards, &args[1..], &mut out))
}

/// The command, on the boards it is handed, saying what it did to `out`.
pub fn run_in(boards: &Boards, args: &[String], out: &mut dyn Write) -> i32 {
    let mut keep = None;
    let mut words = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => {
                say(out, HELP);
                return DONE;
            }
            "--mine" | "--theirs" if keep.is_some() => {
                return wrong(out, "Pick one of --mine and --theirs.");
            }
            "--mine" => keep = Some(Side::Mine),
            "--theirs" => keep = Some(Side::Theirs),
            flag if flag.starts_with('-') => {
                return wrong(out, &format!("\"{flag}\" is not something board takes."));
            }
            word => words.push(word),
        }
    }
    // Only a pull can meet a conflict, so only a pull takes a side.
    let pulls = matches!(words.first(), Some(&"import" | &"pull" | &"sync"));
    if keep.is_some() && !pulls {
        return wrong(out, "Only import, pull, and sync take --mine or --theirs.");
    }
    match words.as_slice() {
        [] | ["help"] => {
            say(out, HELP);
            DONE
        }
        ["list"] => list(boards, out),
        ["import", file] => match absolute(file) {
            Ok(file) => pull(boards, None, &file, keep, out),
            Err(problem) => refuse(out, &problem),
        },
        ["export", id, file] => match absolute(file) {
            Ok(file) => push(boards, id, &file, out),
            Err(problem) => refuse(out, &problem),
        },
        ["pull"] => every(boards, None, out, &|id, file, out| {
            pull(boards, Some(id), file, keep, out)
        }),
        ["pull", id] => every(boards, Some(id), out, &|id, file, out| {
            pull(boards, Some(id), file, keep, out)
        }),
        ["push"] => every(boards, None, out, &|id, file, out| {
            push(boards, id, file, out)
        }),
        ["push", id] => every(boards, Some(id), out, &|id, file, out| {
            push(boards, id, file, out)
        }),
        ["sync"] => every(boards, None, out, &|id, file, out| {
            both(boards, id, file, keep, out)
        }),
        ["sync", id] => every(boards, Some(id), out, &|id, file, out| {
            both(boards, id, file, keep, out)
        }),
        _ => wrong(out, "That is not a board command."),
    }
}

/// Every board on this computer, and the file each one has.
fn list(boards: &Boards, out: &mut dyn Write) -> i32 {
    let listed = match boards.list() {
        Ok(listed) => listed,
        Err(problem) => return refuse(out, &problem),
    };
    if listed.is_empty() {
        say(
            out,
            "No board on this computer yet. import brings one in from a file.",
        );
        return DONE;
    }
    for board in listed {
        if let Some(problem) = board.problem {
            say(out, &format!("{}: {problem}", board.id));
            continue;
        }
        let file = match boards.source(&board.id) {
            Ok(Some(file)) => file.display().to_string(),
            Ok(None) => "no file yet".to_string(),
            Err(problem) => problem,
        };
        say(
            out,
            &format!(
                "{}: \"{}\", {}, {file}",
                board.id,
                board.name,
                counted(board.stories, "story", "stories")
            ),
        );
    }
    DONE
}

/// One board, or every board that has a file, each with its file. One that
/// fails does not stop the rest, and the worst of them is what is left with.
fn every(
    boards: &Boards,
    id: Option<&str>,
    out: &mut dyn Write,
    act: &dyn Fn(&str, &Path, &mut dyn Write) -> i32,
) -> i32 {
    let ids = match id {
        Some(id) => vec![id.to_string()],
        None => boards.sources(),
    };
    if ids.is_empty() {
        return refuse(out, "No board has a file yet. Start with import or export.");
    }
    let mut worst = DONE;
    for id in ids {
        let code = match boards.source(&id) {
            Ok(Some(file)) => act(&id, &file, out),
            Ok(None) => refuse(
                out,
                &format!("{id}: It has no file yet. Start with import or export."),
            ),
            Err(problem) => refuse(out, &format!("{id}: {problem}")),
        };
        worst = worst.max(code);
    }
    worst
}

/// Brings the file's changes into this computer's copy. The file is read and
/// never written.
fn pull(
    boards: &Boards,
    id: Option<&str>,
    file: &Path,
    keep: Option<Side>,
    out: &mut dyn Write,
) -> i32 {
    match pulled(boards, id, file, keep, out) {
        Ok(code) => code,
        Err(problem) => {
            let at = id.map_or_else(|| file.display().to_string(), str::to_string);
            refuse(out, &format!("{at}: {problem}"))
        }
    }
}

fn pulled(
    boards: &Boards,
    id: Option<&str>,
    file: &Path,
    keep: Option<Side>,
    out: &mut dyn Write,
) -> Result<i32, String> {
    let Some(theirs) = Boards::outside(file)? else {
        return Err(format!("There is no file at {}.", file.display()));
    };
    if id.is_some_and(|id| id != theirs.id) {
        return Err(format!(
            "{} holds another board, {}.",
            file.display(),
            theirs.id
        ));
    }
    let id = theirs.id.as_str();
    let mine = boards.local(id)?;
    // The copy both sides last agreed on counts only when the file came from
    // it. One that cannot be read counts as none, and then nothing is taken
    // away from either side.
    let base = boards
        .base(id)
        .ok()
        .flatten()
        .filter(|base| sync::descends(&theirs, base));
    let merged = match &mine {
        Some(mine) => sync::merge(base.as_ref(), mine, &theirs, keep),
        None => Merged {
            board: theirs.clone(),
            conflicts: Vec::new(),
        },
    };
    let count = merged.conflicts.len();
    if count > 0 && keep.is_none() {
        conflicts(out, &theirs.name, id, &merged.conflicts);
        return Ok(CONFLICTS);
    }

    // This computer's copy first, then the state the two sides now agree on,
    // then where the file is. A copy that already holds it all is left alone.
    let changed = mine
        .as_ref()
        .is_none_or(|mine| !sync::same(mine, &merged.board));
    if changed {
        boards.pull(merged.board)?;
    }
    boards.keep_base(&theirs)?;
    boards.remember(id, file)?;

    let at = file.display();
    if mine.is_none() {
        say(out, &format!("{id}: this computer now has a copy of {at}."));
    } else if changed {
        say(
            out,
            &format!("{id}: this computer's copy now holds the changes in {at}."),
        );
    } else {
        say(out, &format!("{id}: nothing new in {at}."));
    }
    if count > 0 {
        let kept = match keep {
            Some(Side::Theirs) => "what the file has",
            _ => "what is on this computer",
        };
        say(
            out,
            &format!(
                "{id}: {} settled by keeping {kept}.",
                counted(count, "conflict", "conflicts")
            ),
        );
    }
    Ok(DONE)
}

/// What both sides changed, each in its own way, for the person to settle.
fn conflicts(out: &mut dyn Write, name: &str, id: &str, met: &[Conflict]) {
    say(
        out,
        &format!("Conflicts in \"{name}\" ({id}): {}", met.len()),
    );
    for conflict in met.iter().take(SHOWN) {
        say(out, &format!("  {}", conflict.what));
        say(out, &format!("    here:     {}", conflict.mine));
        say(out, &format!("    the file: {}", conflict.theirs));
    }
    if met.len() > SHOWN {
        say(out, &format!("  and {} more", met.len() - SHOWN));
    }
    say(
        out,
        "Nothing was written. Run it again with --mine to keep what is here, or --theirs to keep what the file has.",
    );
}

/// Writes this computer's copy to the file. This computer's copy is read and
/// never written.
fn push(boards: &Boards, id: &str, file: &Path, out: &mut dyn Write) -> i32 {
    match pushed(boards, id, file, out) {
        Ok(code) => code,
        Err(problem) => refuse(out, &format!("{id}: {problem}")),
    }
}

fn pushed(boards: &Boards, id: &str, file: &Path, out: &mut dyn Write) -> Result<i32, String> {
    let Some(mine) = boards.local(id)? else {
        return Err("There is no such board on this computer.".to_string());
    };
    let base = boards.base(id).ok().flatten();
    let theirs = Boards::outside(file)?;
    let at = file.display();
    let from = match &theirs {
        Some(theirs) => {
            if theirs.id != id {
                return Err(format!(
                    "{at} holds another board, {}. It was left as it is.",
                    theirs.id
                ));
            }
            if sync::same(theirs, &mine) {
                boards.keep_base(theirs)?;
                boards.remember(id, file)?;
                say(
                    out,
                    &format!("{id}: nothing to push. {at} already holds what is here."),
                );
                return Ok(DONE);
            }
            // The file is replaced only when everything in it is already
            // here: it still holds what the two sides last agreed on.
            if !base.as_ref().is_some_and(|base| sync::same(base, theirs)) {
                return Err(format!(
                    "{at} has changes that are not here yet. Pull first, or run sync."
                ));
            }
            Some(theirs)
        }
        None => base.as_ref(),
    };
    let rev = uuid::Uuid::new_v4().simple().to_string();
    let sent = sync::stamped(mine, from, rev);
    boards.send(file, &sent)?;
    boards.keep_base(&sent)?;
    boards.remember(id, file)?;
    say(out, &format!("{id}: wrote what is here to {at}."));
    Ok(DONE)
}

/// A pull, then a push when the pull is done. A file that is not there yet
/// has nothing to pull.
fn both(boards: &Boards, id: &str, file: &Path, keep: Option<Side>, out: &mut dyn Write) -> i32 {
    let code = match Boards::outside(file) {
        Ok(None) => DONE,
        _ => pull(boards, Some(id), file, keep, out),
    };
    if code != DONE {
        return code;
    }
    push(boards, id, file, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use serde_json::{Value, json};
    use std::fs;

    /// A made-up board, as its file would hold it.
    const ROAD: &str = r#"{
      "format": "board v1",
      "id": "garden",
      "name": "Garden list",
      "epics": [
        {
          "release": "Spring",
          "title": "Beds and paths",
          "stories": [
            {
              "number": 1,
              "title": "Dig the first bed",
              "status": "Building",
              "slices": [
                { "id": "a", "title": "Mark the corners", "lane": "written" },
                { "id": "b", "title": "Turn the soil", "lane": "sliced" }
              ]
            },
            { "number": 2, "title": "Lay the path", "status": "Scheduled" }
          ]
        },
        {
          "release": "Summer",
          "title": "Water",
          "stories": [
            { "number": 3, "kind": "bug", "title": "The hose leaks at the tap", "status": "Scheduled" }
          ]
        }
      ]
    }"#;

    /// Boards in a scratch data folder, and a scratch folder for the files
    /// kept somewhere else. Neither is this computer's real data folder.
    fn scratch() -> (Boards, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("rusty-business-test-{}", uuid::Uuid::new_v4()));
        let shared = dir.join("shared");
        fs::create_dir_all(&shared).unwrap();
        (Boards::new(dir.join("data").join("boards")), shared)
    }

    fn road() -> Board {
        serde_json::from_str(ROAD).unwrap()
    }

    fn named(id: &str, name: &str) -> Board {
        Board {
            id: id.to_string(),
            name: name.to_string(),
            ..road()
        }
    }

    /// The command as typed after `rusty-business board`, and what it said.
    fn board(boards: &Boards, words: &[&str]) -> (i32, String) {
        let args: Vec<String> = words.iter().map(|word| word.to_string()).collect();
        let mut out = Vec::new();
        let code = run_in(boards, &args, &mut out);
        (code, String::from_utf8(out).unwrap())
    }

    fn put(file: &Path, board: &Board) {
        fs::write(file, serde_json::to_string_pretty(board).unwrap()).unwrap();
    }

    fn held(file: &Path) -> Board {
        serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
    }

    /// A change made in the file by whoever keeps it, with no new rev.
    fn edit(file: &Path, change: impl FnOnce(&mut Board)) {
        let mut board = held(file);
        change(&mut board);
        put(file, &board);
    }

    /// A change made on this computer, as a later story's view would make it.
    fn change(boards: &Boards, id: &str, change: impl FnOnce(&mut Board)) {
        let mut board = boards.local(id).unwrap().unwrap();
        change(&mut board);
        boards.pull(board).unwrap();
    }

    fn title(board: &Board, number: u64) -> Option<String> {
        board
            .epics
            .iter()
            .flat_map(|epic| &epic.stories)
            .find(|story| story.number == number)
            .map(|story| story.title.clone())
    }

    fn status(board: &Board, number: u64) -> Option<String> {
        board
            .epics
            .iter()
            .flat_map(|epic| &epic.stories)
            .find(|story| story.number == number)
            .map(|story| story.status.clone())
    }

    fn here(boards: &Boards, id: &str) -> Board {
        boards.local(id).unwrap().unwrap()
    }

    /// A board with its file, both holding the made-up board.
    fn imported() -> (Boards, PathBuf, String) {
        let (boards, shared) = scratch();
        let file = shared.join("garden.json");
        put(&file, &road());
        let at = file.to_str().unwrap().to_string();
        assert_eq!(board(&boards, &["import", &at]).0, DONE);
        (boards, file, at)
    }

    /// Where the store keeps this computer's copy, for comparing bytes.
    fn copy_of(file: &Path, id: &str) -> PathBuf {
        let dir = file.parent().unwrap().parent().unwrap();
        dir.join("data").join("boards").join(format!("{id}.json"))
    }

    #[test]
    fn only_the_board_command_is_taken() {
        // Anything else is the app's own start. Neither reaches a data folder.
        assert_eq!(run(&[]), None);
        assert_eq!(run(&["chat".to_string()]), None);
        assert_eq!(run(&["--board".to_string()]), None);
        assert_eq!(run(&["list".to_string(), "board".to_string()]), None);
    }

    #[test]
    fn help_is_shown_and_a_command_that_cannot_be_read_is_refused() {
        let (boards, shared) = scratch();
        for words in [&[][..], &["help"], &["--help"], &["pull", "-h"]] {
            let (code, said) = board(&boards, words);
            assert_eq!(code, DONE, "{words:?}");
            assert_eq!(said, format!("{HELP}\n"), "{words:?}");
        }
        let wrong: [(&[&str], &str); 9] = [
            (&["frobnicate"], "That is not a board command."),
            (&["import"], "That is not a board command."),
            (&["export", "garden"], "That is not a board command."),
            (&["pull", "garden", "again"], "That is not a board command."),
            (
                &["pull", "--mine", "--theirs"],
                "Pick one of --mine and --theirs.",
            ),
            (
                &["pull", "--force"],
                "\"--force\" is not something board takes.",
            ),
            (
                &["list", "--mine"],
                "Only import, pull, and sync take --mine or --theirs.",
            ),
            (
                &["push", "--theirs"],
                "Only import, pull, and sync take --mine or --theirs.",
            ),
            (
                &["export", "garden", "garden.json", "--mine"],
                "Only import, pull, and sync take --mine or --theirs.",
            ),
        ];
        for (words, why) in wrong {
            let (code, said) = board(&boards, words);
            assert_eq!(code, REFUSED, "{words:?}");
            assert_eq!(said, format!("{why}\n\n{HELP}\n"), "{words:?}");
        }
        // None of it made a folder or a file.
        assert!(!shared.parent().unwrap().join("data").exists());
        assert_eq!(fs::read_dir(&shared).unwrap().count(), 0);
    }

    // "we can start with a flat file"
    #[test]
    fn a_first_import_makes_this_computers_copy_and_leaves_the_file_as_it_is() {
        let (boards, shared) = scratch();
        let file = shared.join("garden.json");
        put(&file, &road());
        let before = fs::read(&file).unwrap();
        let at = file.to_str().unwrap();

        let (code, said) = board(&boards, &["import", at]);
        assert_eq!(code, DONE);
        assert_eq!(
            said,
            format!("garden: this computer now has a copy of {at}.\n")
        );
        assert!(sync::same(&here(&boards, "garden"), &road()));
        assert!(sync::same(
            &boards.base("garden").unwrap().unwrap(),
            &road()
        ));
        assert_eq!(boards.source("garden").unwrap(), Some(file.clone()));
        assert_eq!(fs::read(&file).unwrap(), before);

        // Again, and from the file it now has: nothing to bring in.
        for words in [&["import", at][..], &["pull", "garden"], &["pull"]] {
            let (code, said) = board(&boards, words);
            assert_eq!(code, DONE, "{words:?}");
            assert_eq!(said, format!("garden: nothing new in {at}.\n"), "{words:?}");
        }
        assert_eq!(fs::read(&file).unwrap(), before);

        // A file that is not there, and a file that is not a board.
        let missing = shared.join("missing.json");
        let (code, said) = board(&boards, &["import", missing.to_str().unwrap()]);
        assert_eq!(code, REFUSED);
        assert!(said.contains("There is no file at"), "{said}");
        let other = shared.join("list.json");
        fs::write(&other, r#"{"milk": 2}"#).unwrap();
        let (code, _) = board(&boards, &["import", other.to_str().unwrap()]);
        assert_eq!(code, REFUSED);
        assert_eq!(fs::read(&other).unwrap(), br#"{"milk": 2}"#);
        assert_eq!(boards.sources(), ["garden"]);
    }

    // "one command option at the cli to push pull either way"
    #[test]
    fn one_command_pushes_and_one_pulls_and_neither_writes_the_other_side() {
        let (boards, file, at) = imported();
        let copy = copy_of(&file, "garden");

        // Out: a change made here goes to the file, and this copy is not
        // written.
        change(&boards, "garden", |board| {
            board.epics[0].stories[0].title = "Dig the first two beds".to_string();
        });
        let mine = fs::read(&copy).unwrap();
        let (code, said) = board(&boards, &["push"]);
        assert_eq!(code, DONE);
        assert_eq!(said, format!("garden: wrote what is here to {at}.\n"));
        assert_eq!(fs::read(&copy).unwrap(), mine);
        let sent = held(&file);
        assert_eq!(title(&sent, 1).unwrap(), "Dig the first two beds");
        assert_eq!(sent.rev.len(), 32);
        assert_eq!(sent.check(), Ok(()));
        assert!(sync::same(&boards.base("garden").unwrap().unwrap(), &sent));

        // Nothing more to send.
        let theirs = fs::read(&file).unwrap();
        let (code, said) = board(&boards, &["push", "garden"]);
        assert_eq!(code, DONE);
        assert_eq!(
            said,
            format!("garden: nothing to push. {at} already holds what is here.\n")
        );
        assert_eq!(fs::read(&file).unwrap(), theirs);

        // Back: a change made in the file comes here, and the file is not
        // written.
        edit(&file, |board| {
            board.epics[1].stories[0].status = "Building".to_string();
        });
        let theirs = fs::read(&file).unwrap();
        let (code, said) = board(&boards, &["pull"]);
        assert_eq!(code, DONE);
        assert_eq!(
            said,
            format!("garden: this computer's copy now holds the changes in {at}.\n")
        );
        assert_eq!(fs::read(&file).unwrap(), theirs);
        let now = here(&boards, "garden");
        assert_eq!(status(&now, 3).unwrap(), "Building");
        assert_eq!(title(&now, 1).unwrap(), "Dig the first two beds");
    }

    // "bi-directional sync with conflict resolution"
    #[test]
    fn sync_brings_together_what_each_side_changed() {
        let (boards, file, at) = imported();
        change(&boards, "garden", |board| {
            board.epics[0].stories[0].slices[1].lane = "written".to_string();
        });
        edit(&file, |board| {
            board.epics[0].stories[1].title = "Lay the gravel path".to_string();
        });

        let (code, said) = board(&boards, &["sync", "garden"]);
        assert_eq!(code, DONE);
        assert_eq!(
            said,
            format!(
                "garden: this computer's copy now holds the changes in {at}.\n\
                 garden: wrote what is here to {at}.\n"
            )
        );
        let (mine, theirs) = (here(&boards, "garden"), held(&file));
        assert!(sync::same(&mine, &theirs));
        assert_eq!(theirs.epics[0].stories[0].slices[1].lane, "written");
        assert_eq!(title(&theirs, 2).unwrap(), "Lay the gravel path");
        assert!(!theirs.rev.is_empty());

        // In step now, so a second sync writes nothing.
        let before = (
            fs::read(&file).unwrap(),
            fs::read(copy_of(&file, "garden")).unwrap(),
        );
        let (code, said) = board(&boards, &["sync"]);
        assert_eq!(code, DONE);
        assert_eq!(
            said,
            format!(
                "garden: nothing new in {at}.\n\
                 garden: nothing to push. {at} already holds what is here.\n"
            )
        );
        let after = (
            fs::read(&file).unwrap(),
            fs::read(copy_of(&file, "garden")).unwrap(),
        );
        assert_eq!(before, after);
    }

    // "bi-directional sync with conflict resolution"
    #[test]
    fn a_conflict_writes_nothing_until_a_side_is_picked() {
        let both_changed = || {
            let (boards, file, at) = imported();
            change(&boards, "garden", |board| {
                board.epics[0].stories[1].title = "Lay the brick path".to_string();
            });
            edit(&file, |board| {
                board.epics[0].stories[1].title = "Lay the gravel path".to_string();
            });
            (boards, file, at)
        };

        // With no side picked, each conflict is listed and nothing is written.
        let (boards, file, at) = both_changed();
        let copy = copy_of(&file, "garden");
        let synced = copy.parent().unwrap().join("synced").join("garden.json");
        let before = [&file, &copy, &synced].map(|path| fs::read(path).unwrap());
        for words in [&["sync"][..], &["pull", "garden"], &["import", &at]] {
            let (code, said) = board(&boards, words);
            assert_eq!(code, CONFLICTS, "{words:?}");
            assert!(
                said.starts_with("Conflicts in \"Garden list\" (garden): 1\n"),
                "{said}"
            );
            assert!(
                said.contains("    here:     \"Lay the brick path\"\n"),
                "{said}"
            );
            assert!(
                said.contains("    the file: \"Lay the gravel path\"\n"),
                "{said}"
            );
            assert!(said.ends_with(
                "Nothing was written. Run it again with --mine to keep what is here, or --theirs to keep what the file has.\n"
            ));
            assert_eq!(
                [&file, &copy, &synced].map(|path| fs::read(path).unwrap()),
                before
            );
        }

        // --mine keeps what is here, and the push then takes it to the file.
        let (code, said) = board(&boards, &["sync", "--mine"]);
        assert_eq!(code, DONE);
        assert_eq!(
            said,
            format!(
                "garden: nothing new in {at}.\n\
                 garden: 1 conflict settled by keeping what is on this computer.\n\
                 garden: wrote what is here to {at}.\n"
            )
        );
        assert_eq!(
            title(&here(&boards, "garden"), 2).unwrap(),
            "Lay the brick path"
        );
        assert_eq!(title(&held(&file), 2).unwrap(), "Lay the brick path");

        // --theirs keeps what the file has, and there is nothing to push.
        let (boards, file, at) = both_changed();
        let theirs = fs::read(&file).unwrap();
        let (code, said) = board(&boards, &["sync", "--theirs"]);
        assert_eq!(code, DONE);
        assert_eq!(
            said,
            format!(
                "garden: this computer's copy now holds the changes in {at}.\n\
                 garden: 1 conflict settled by keeping what the file has.\n\
                 garden: nothing to push. {at} already holds what is here.\n"
            )
        );
        assert_eq!(
            title(&here(&boards, "garden"), 2).unwrap(),
            "Lay the gravel path"
        );
        assert_eq!(fs::read(&file).unwrap(), theirs);
    }

    #[test]
    fn a_push_is_refused_while_the_file_has_changes_that_are_not_here() {
        let (boards, file, at) = imported();
        change(&boards, "garden", |board| {
            board.epics[0].stories[0].status = "On main".to_string();
        });
        edit(&file, |board| {
            board.epics[1].stories[0].status = "Building".to_string();
        });
        let before = fs::read(&file).unwrap();
        for words in [
            &["push"][..],
            &["push", "garden"],
            &["export", "garden", &at],
        ] {
            let (code, said) = board(&boards, words);
            assert_eq!(code, REFUSED, "{words:?}");
            assert_eq!(
                said,
                format!(
                    "garden: {at} has changes that are not here yet. Pull first, or run sync.\n"
                ),
                "{words:?}"
            );
            assert_eq!(fs::read(&file).unwrap(), before, "{words:?}");
        }
        // After the pull, it goes.
        assert_eq!(board(&boards, &["pull"]).0, DONE);
        assert_eq!(board(&boards, &["push"]).0, DONE);
        let sent = held(&file);
        assert_eq!(status(&sent, 1).unwrap(), "On main");
        assert_eq!(status(&sent, 3).unwrap(), "Building");
    }

    // "we can start with a flat file"
    #[test]
    fn an_export_makes_a_new_file_and_writes_over_nothing_but_its_own_board() {
        let (boards, shared) = scratch();
        boards.pull(road()).unwrap();
        let copy = copy_of(&shared.join("x.json"), "garden");
        let mine = fs::read(&copy).unwrap();

        // A new file, remembered as the board's.
        let file = shared.join("garden.json");
        let at = file.to_str().unwrap();
        let (code, said) = board(&boards, &["export", "garden", at]);
        assert_eq!(code, DONE);
        assert_eq!(said, format!("garden: wrote what is here to {at}.\n"));
        let sent = held(&file);
        assert!(sync::same(&sent, &road()));
        assert_eq!(sent.check(), Ok(()));
        assert!(!sent.rev.is_empty());
        assert_eq!(boards.source("garden").unwrap(), Some(file.clone()));
        assert_eq!(fs::read(&copy).unwrap(), mine);

        // A name that does not end in .json is not written.
        let notes = shared.join("notes.txt");
        let (code, said) = board(&boards, &["export", "garden", notes.to_str().unwrap()]);
        assert_eq!(code, REFUSED);
        assert_eq!(said, "garden: A board file's name ends in .json.\n");
        assert!(!notes.exists());

        // A file that is not a board, another board's file, and a file of
        // this board that this computer never saw are each left as they are.
        let list = shared.join("list.json");
        fs::write(&list, r#"{"milk": 2}"#).unwrap();
        let orchard = shared.join("orchard.json");
        put(&orchard, &named("orchard", "Orchard"));
        let older = shared.join("older.json");
        let mut other = road();
        other.epics[0].stories[0].title = "Dig a pond".to_string();
        put(&older, &other);
        for (path, why) in [
            (&list, "garden: This is not a \"board v1\" file.\n"),
            (
                &orchard,
                "holds another board, orchard. It was left as it is.\n",
            ),
            (
                &older,
                "has changes that are not here yet. Pull first, or run sync.\n",
            ),
        ] {
            let before = fs::read(path).unwrap();
            let (code, said) = board(&boards, &["export", "garden", path.to_str().unwrap()]);
            assert_eq!(code, REFUSED, "{said}");
            assert!(said.contains(why), "{said}");
            assert_eq!(fs::read(path).unwrap(), before);
        }
        assert_eq!(boards.source("garden").unwrap(), Some(file));

        // A board that is not here makes no file.
        let none = shared.join("none.json");
        let (code, said) = board(&boards, &["export", "meadow", none.to_str().unwrap()]);
        assert_eq!(code, REFUSED);
        assert_eq!(said, "meadow: There is no such board on this computer.\n");
        assert!(!none.exists());
    }

    #[test]
    fn with_no_id_every_board_with_a_file_is_done_and_the_worst_is_left_with() {
        let (boards, shared) = scratch();
        // Nothing has a file yet.
        boards.pull(named("pond", "Pond")).unwrap();
        for verb in ["pull", "push", "sync"] {
            let (code, said) = board(&boards, &[verb]);
            assert_eq!(code, REFUSED);
            assert_eq!(
                said,
                "No board has a file yet. Start with import or export.\n"
            );
            let (code, said) = board(&boards, &[verb, "pond"]);
            assert_eq!(code, REFUSED);
            assert_eq!(
                said,
                "pond: It has no file yet. Start with import or export.\n"
            );
        }

        // garden meets a conflict, meadow's file is not there yet, and
        // orchard has a change to send.
        let garden = shared.join("garden.json");
        let orchard = shared.join("orchard.json");
        let meadow = shared.join("meadow.json");
        put(&garden, &road());
        put(&orchard, &named("orchard", "Orchard"));
        assert_eq!(
            board(&boards, &["import", garden.to_str().unwrap()]).0,
            DONE
        );
        assert_eq!(
            board(&boards, &["import", orchard.to_str().unwrap()]).0,
            DONE
        );
        boards.pull(named("meadow", "Meadow")).unwrap();
        boards.remember("meadow", &meadow).unwrap();
        change(&boards, "garden", |board| {
            board.name = "Kitchen garden".to_string()
        });
        edit(&garden, |board| board.name = "Back garden".to_string());
        change(&boards, "orchard", |board| {
            board.epics[0].stories[0].status = "On main".to_string();
        });

        // A pull stops at the file that is not there, and goes on.
        let (code, said) = board(&boards, &["pull"]);
        assert_eq!(code, CONFLICTS);
        assert!(
            said.contains("Conflicts in \"Back garden\" (garden): 1\n"),
            "{said}"
        );
        assert!(
            said.contains(&format!(
                "meadow: There is no file at {}.\n",
                meadow.display()
            )),
            "{said}"
        );
        assert!(said.contains("orchard: nothing new in"), "{said}");

        // A sync makes the missing file, sends orchard's change, and still
        // leaves with the conflict.
        let (code, said) = board(&boards, &["sync"]);
        assert_eq!(code, CONFLICTS);
        assert!(
            said.contains("Conflicts in \"Back garden\" (garden): 1\n"),
            "{said}"
        );
        assert!(sync::same(&held(&meadow), &named("meadow", "Meadow")));
        assert_eq!(status(&held(&orchard), 1).unwrap(), "On main");
        assert_eq!(held(&garden).name, "Back garden");
        assert_eq!(here(&boards, "garden").name, "Kitchen garden");
        // pond still has no file, and was left out.
        assert!(!said.contains("pond"), "{said}");
        assert_eq!(boards.sources(), ["garden", "meadow", "orchard"]);

        // Refused is worse than done, and conflicts are worse than refused.
        change(&boards, "orchard", |board| {
            board.epics[0].stories[1].status = "Building".to_string();
        });
        edit(&orchard, |board| board.epics[1].title = "Shade".to_string());
        assert_eq!(board(&boards, &["push", "meadow"]).0, DONE);
        assert_eq!(board(&boards, &["push", "orchard"]).0, REFUSED);
        assert_eq!(board(&boards, &["push"]).0, REFUSED);
        assert_eq!(board(&boards, &["sync"]).0, CONFLICTS);
    }

    #[test]
    fn list_names_each_board_and_the_file_it_has() {
        let (boards, shared) = scratch();
        let (code, said) = board(&boards, &["list"]);
        assert_eq!(code, DONE);
        assert_eq!(
            said,
            "No board on this computer yet. import brings one in from a file.\n"
        );

        let file = shared.join("garden.json");
        put(&file, &road());
        assert_eq!(board(&boards, &["import", file.to_str().unwrap()]).0, DONE);
        let mut meadow = named("meadow", "Meadow");
        meadow.epics.truncate(1);
        meadow.epics[0].stories.truncate(1);
        boards.pull(meadow).unwrap();

        let (code, said) = board(&boards, &["list"]);
        assert_eq!(code, DONE);
        let mut lines: Vec<&str> = said.lines().collect();
        lines.sort();
        assert_eq!(
            lines,
            [
                format!("garden: \"Garden list\", 3 stories, {}", file.display()),
                "meadow: \"Meadow\", 1 story, no file yet".to_string(),
            ]
        );
    }

    #[test]
    fn what_the_file_took_away_is_taken_away_here_only_when_it_came_from_this_copy() {
        // The file came from the copy this computer last synced, so a story
        // it no longer has was removed there.
        let (boards, file, _) = imported();
        edit(&file, |board| {
            board.epics[0].stories.remove(1);
        });
        assert_eq!(board(&boards, &["pull"]).0, DONE);
        assert_eq!(title(&here(&boards, "garden"), 2), None);
        assert_eq!(
            title(&here(&boards, "garden"), 1).unwrap(),
            "Dig the first bed"
        );

        // A file that names a state this computer never saw gives nothing to
        // compare against, so nothing is taken away.
        let (boards, file, _) = imported();
        edit(&file, |board| {
            board.epics[0].stories.remove(1);
            board.rev = "somewhere-else-1".to_string();
        });
        assert_eq!(board(&boards, &["pull"]).0, DONE);
        assert_eq!(title(&here(&boards, "garden"), 2).unwrap(), "Lay the path");
    }

    /// A backend of somebody else's, written by hand to show what one has to
    /// do. It uses none of this app's code: it holds the board as plain JSON,
    /// and keeps the rev and history of the last file it exported or
    /// imported, and whether its board changed since.
    ///
    /// "any user can use the import export and build their own backend"
    /// "this keeps it loosely coupled"
    struct Backend {
        board: Value,
        last: Option<(String, Vec<String>)>,
        changed: bool,
        exports: u32,
    }

    impl Backend {
        fn new() -> Self {
            Backend {
                board: serde_json::from_str(ROAD).unwrap(),
                last: None,
                changed: false,
                exports: 0,
            }
        }

        fn change(&mut self, change: impl FnOnce(&mut Value)) {
            change(&mut self.board);
            self.changed = true;
        }

        /// A board that changed since the last file gets a new rev, with the
        /// last one at the head of its history.
        fn export(&mut self, file: &Path) {
            if self.changed || self.last.is_none() {
                let mut history = Vec::new();
                if let Some((rev, older)) = &self.last {
                    history.push(rev.clone());
                    history.extend(older.iter().cloned());
                }
                history.truncate(50);
                self.exports += 1;
                self.last = Some((format!("backend-{}", self.exports), history));
                self.changed = false;
            }
            let (rev, history) = self.last.clone().unwrap();
            let mut out = self.board.clone();
            out["rev"] = json!(rev);
            out["history"] = json!(history);
            fs::write(file, serde_json::to_string_pretty(&out).unwrap()).unwrap();
        }

        /// Takes a file whole when the file came from the last one, and
        /// refuses when its own board changed since, or the file is unrelated.
        fn import(&mut self, file: &Path) -> &'static str {
            let there: Value = serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
            let rev = there["rev"].as_str().unwrap_or_default().to_string();
            let history: Vec<String> = there["history"]
                .as_array()
                .map(|revs| {
                    revs.iter()
                        .filter_map(|rev| rev.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if self.changed {
                return "refused: export first";
            }
            if let Some((last, _)) = &self.last {
                if *last == rev {
                    return "nothing to do";
                }
                if !history.contains(last) {
                    return "refused: not from what is here";
                }
            }
            self.board = there;
            self.last = Some((rev, history));
            "taken"
        }

        fn board(&self) -> Board {
            serde_json::from_value(self.board.clone()).unwrap()
        }
    }

    // "any user can use the import export and build their own backend"
    #[test]
    fn a_backend_written_by_hand_stays_in_step_through_one_file() {
        let (boards, shared) = scratch();
        let file = shared.join("garden.json");
        let at = file.to_str().unwrap();
        let mut backend = Backend::new();

        // The backend exports, and this computer takes its first copy.
        backend.export(&file);
        assert_eq!(board(&boards, &["import", at]).0, DONE);
        assert!(sync::same(&here(&boards, "garden"), &backend.board()));
        assert_eq!(backend.import(&file), "nothing to do");

        // A change here goes out, and the backend takes the file whole.
        change(&boards, "garden", |board| {
            board.epics[0].stories[0].slices[1].lane = "written".to_string();
        });
        assert_eq!(board(&boards, &["sync"]).0, DONE);
        assert_eq!(held(&file).history, ["backend-1"]);
        assert_eq!(backend.import(&file), "taken");
        assert_eq!(
            backend.board().epics[0].stories[0].slices[1].lane,
            "written"
        );

        // A change in the backend comes back, and nothing is written out.
        backend.change(|board| board["epics"][1]["stories"][0]["status"] = json!("Building"));
        backend.export(&file);
        let exported = fs::read(&file).unwrap();
        assert_eq!(board(&boards, &["sync"]).0, DONE);
        assert_eq!(status(&here(&boards, "garden"), 3).unwrap(), "Building");
        assert_eq!(fs::read(&file).unwrap(), exported);
        assert_eq!(backend.import(&file), "nothing to do");

        // Each side changes something of its own: both changes end up on
        // both sides.
        change(&boards, "garden", |board| {
            board.epics[0].stories[0].title = "Dig the first two beds".to_string();
        });
        backend.change(|board| board["epics"][0]["stories"][1]["status"] = json!("Building"));
        backend.export(&file);
        assert_eq!(board(&boards, &["sync"]).0, DONE);
        assert_eq!(backend.import(&file), "taken");
        for side in [here(&boards, "garden"), backend.board()] {
            assert_eq!(title(&side, 1).unwrap(), "Dig the first two beds");
            assert_eq!(status(&side, 2).unwrap(), "Building");
            assert_eq!(status(&side, 3).unwrap(), "Building");
            assert_eq!(side.epics[0].stories[0].slices[1].lane, "written");
        }

        // Both change the same thing: nothing is written until a side is
        // picked.
        change(&boards, "garden", |board| {
            board.epics[1].title = "Water and shade".to_string();
        });
        backend.change(|board| board["epics"][1]["title"] = json!("Water, first"));
        backend.export(&file);
        let exported = fs::read(&file).unwrap();
        assert_eq!(board(&boards, &["sync"]).0, CONFLICTS);
        assert_eq!(fs::read(&file).unwrap(), exported);
        assert_eq!(here(&boards, "garden").epics[1].title, "Water and shade");
        assert_eq!(board(&boards, &["sync", "--theirs"]).0, DONE);
        assert_eq!(here(&boards, "garden").epics[1].title, "Water, first");
        assert_eq!(backend.import(&file), "nothing to do");
        assert!(sync::same(&here(&boards, "garden"), &backend.board()));

        // A file that does not come from what the backend holds is refused.
        let stray = shared.join("stray.json");
        let mut unrelated = road();
        unrelated.rev = "somewhere-else-1".to_string();
        put(&stray, &unrelated);
        assert_eq!(backend.import(&stray), "refused: not from what is here");
    }

    // "this keeps it loosely coupled"
    #[test]
    fn a_backend_that_changed_before_it_imported_loses_nothing() {
        let (boards, shared) = scratch();
        let file = shared.join("garden.json");
        let at = file.to_str().unwrap();
        let mut backend = Backend::new();
        backend.export(&file);
        assert_eq!(board(&boards, &["import", at]).0, DONE);

        // This computer sends a change, and the backend changes before it
        // imports the file. It refuses, and exports what it holds.
        change(&boards, "garden", |board| {
            board.epics[0].stories[0].title = "Dig the first two beds".to_string();
        });
        assert_eq!(board(&boards, &["sync"]).0, DONE);
        backend.change(|board| board["epics"][0]["stories"][1]["status"] = json!("Building"));
        assert_eq!(backend.import(&file), "refused: export first");
        backend.export(&file);

        // The file no longer comes from the copy this computer last synced,
        // so there is nothing to compare against: every difference is a
        // conflict, nothing is taken away, and nothing is written.
        let exported = fs::read(&file).unwrap();
        let (code, said) = board(&boards, &["sync"]);
        assert_eq!(code, CONFLICTS);
        assert!(said.contains("\"Dig the first two beds\""), "{said}");
        assert!(said.contains("\"Building\""), "{said}");
        assert_eq!(fs::read(&file).unwrap(), exported);
        assert_eq!(
            title(&here(&boards, "garden"), 1).unwrap(),
            "Dig the first two beds"
        );

        // Picking a side settles it, and the two are in step again.
        assert_eq!(board(&boards, &["sync", "--mine"]).0, DONE);
        assert_eq!(backend.import(&file), "taken");
        assert!(sync::same(&here(&boards, "garden"), &backend.board()));
        assert_eq!(
            title(&backend.board(), 1).unwrap(),
            "Dig the first two beds"
        );
    }
}
