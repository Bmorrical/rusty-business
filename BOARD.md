# The board file

A board is a road: its releases, the stories and bugs under each, and the
slices each story is cut into. Rusty Business™ shows one in its Workflow
view, and keeps it in step with one file kept somewhere else.

This page is the ruleset for that file: what it holds, each rule, and why
the rule is there. Anyone can read it and bring in a road of their own. The
program that writes the other side of the file is yours to build, in any
language, and it needs none of this app's code.

- [The rule above the rest](#the-rule-above-the-rest)
- [The file](#the-file)
- [The meta block](#the-meta-block)
- [The data block](#the-data-block)
- [Statuses](#statuses)
- [Lanes and moves](#lanes-and-moves)
- [Versions](#versions)
- [Reading a board v1 file](#reading-a-board-v1-file)
- [When both sides changed](#when-both-sides-changed)
- [Limits](#limits)
- [A backend of your own](#a-backend-of-your-own)

## The rule above the rest

**No direct API connection to a board's backend, for now.** The app holds no
address, no token, and no key for one, and a change that adds one is
refused. A board moves as a file: exported on one side, and imported on the
other.

Why: the app is offline first, and at most LAN. A file crosses only when a
person moves it, so nothing about a road leaves a computer by itself, and
the app has no door for a backend to be reached through. It also keeps the
two sides loosely coupled: either can be replaced, and neither has to be
running for the other to work.

"For now" is meant. The rule is the project's owner's, and it stands until
a pull request changes this page. [AGENTS.md](AGENTS.md) sets it beside
every other rule of the project.

## The file

One board is one JSON file, in UTF-8, of at most 4 MB. It has two blocks:

```json
{
  "meta": {
    "format": "board",
    "version": "2.0",
    "id": "list-app",
    "name": "A list app",
    "writtenBy": "my-tracker 3.1",
    "writtenAt": "2026-11-21T15:04:05Z",
    "rev": "export-12",
    "history": ["export-11", "export-10"],
    "statuses": {
      "stories": [
        { "id": "planned", "name": "Planned", "finished": false },
        { "id": "building", "name": "Building", "finished": false, "moves": ["planned", "done"] },
        { "id": "done", "name": "Done", "finished": true, "moves": [] }
      ],
      "releases": [
        { "id": "ahead", "name": "Ahead", "finished": false },
        { "id": "out", "name": "Out", "finished": true }
      ]
    }
  },
  "data": {
    "releases": [
      {
        "release": "1.2.0",
        "title": "Lists that sort",
        "detail": "",
        "status": "Ahead",
        "statusId": "ahead",
        "stories": [
          {
            "number": 412,
            "kind": "story",
            "title": "Sortable column headers",
            "detail": "Click a column's header to sort the list by it.",
            "status": "Building",
            "statusId": "building",
            "notes": ["Numbers sort as numbers, not as text."],
            "slices": [
              {
                "id": "a",
                "number": 9001,
                "title": "A sort helper for text and numbers",
                "detail": "Goal: one function that sorts rows by a column.",
                "lane": "written"
              }
            ]
          }
        ]
      }
    ]
  }
}
```

- **`meta`** says what the file is, who wrote it, which state of the board
  it holds, and what the board's own words mean.
- **`data`** is the board.

Why two blocks: a program can tell whether it can read a file, and how, from
`meta` alone, before it reads one row. What describes the board and what the
board holds also change at different speeds, and keeping them apart lets
either grow without touching the other.

## The meta block

| Key | What it is | Why |
| --- | --- | --- |
| `format` | `board`. | One word that says this is a board file and nothing else. |
| `version` | The version of this ruleset the file follows, as `2.0`: a line, a dot, and a step within the line. | A reader knows what it is reading before it reads it. See [Versions](#versions). |
| `id` | Which board this is: up to 64 letters, digits, and hyphens. | A file is never taken for another board's. It is letters, digits, and hyphens only so that it can name a file and never a path. |
| `name` | The board's name, for people. Up to 120 characters. | The id is for programs. |
| `writtenBy` | The program that wrote the file, and its version. Up to 120 characters. Optional. | When a file is wrong, this says whom to ask. |
| `writtenAt` | When it was written, in UTC, as `2026-11-21T15:04:05Z`. Up to 40 characters. Optional. | A person can tell an old export from a new one. No program decides anything by it: clocks disagree, and `rev` does that work. |
| `rev` | The name of the state this file holds: up to 64 letters, digits, and hyphens. Optional. | Two copies can be told apart, or told to be the same, without comparing them row by row. |
| `history` | The revs this state came from, the newest first, at most 50. Optional. | A program can tell whether one copy came from another, which is the difference between taking a file whole and merging it. |
| `statuses` | The board's own statuses: one list for stories and bugs, and one for releases. Optional. | See [Statuses](#statuses). |

A board that is only read needs no `rev` and no `history`.

The app adds one key of its own to the copy it keeps, `pulledAt`: when that
copy was taken, in milliseconds. A backend ignores it.

## The data block

`data.releases` is the list of releases, in the board's order.

A **release** holds:

| Key | What it is |
| --- | --- |
| `release` | Its name, such as `1.2.0`. Up to 120 characters, and no two releases on a board share one. |
| `title`, `detail` | What it is for. Up to 300 and 20,000 characters. |
| `status`, `statusId` | See [Statuses](#statuses). |
| `stories` | Its stories and bugs, in order. |

A **story** holds:

| Key | What it is |
| --- | --- |
| `number` | Its number, from 1 to 999,999,999. No two stories on a board share one. |
| `kind` | `story` or `bug`. A story with no `kind` is a story. |
| `title`, `detail` | Up to 300 and 20,000 characters. |
| `status`, `statusId` | See [Statuses](#statuses). |
| `move` | Whose move it is. See [Lanes and moves](#lanes-and-moves). Optional. |
| `notes` | The board's own notes: up to 200, each up to 4,000 characters. |
| `slices` | The slices it is cut into, in order, up to 100. |

A **slice** holds:

| Key | What it is |
| --- | --- |
| `id` | Up to 16 letters, digits, and hyphens. No two slices of a story share one. |
| `number` | The backend's own number for the slice, from 1 to 999,999,999. Optional. |
| `title` | Up to 300 characters. |
| `detail` | Its brief: what to do, in which files, and how to tell it is done. Up to 20,000 characters. |
| `lane`, `move` | See [Lanes and moves](#lanes-and-moves). |
| `notes` | Up to 200, each up to 4,000 characters. |

Each thing has one key it is known by, and that key never changes: a release
its `release`, a story its `number`, and a slice its `id` within its story.

Why: both sides change the same board, and the only way to say "this is the
slice you had" is a key neither side rewrites. A title is not one. People
fix titles.

A slice's `number` is the backend's to give, once it has filed the slice.
The app keeps it, shows it, and writes it back. It is never what a slice is
matched by, because a slice cut on this side has none until the backend has
seen it.

## Statuses

A status is a word a board uses for where a story, a bug, or a release
stands. Every board has its own, so the words are data in the file, not a
list in this app's code.

Why: a board that wants a status called "Waiting on a reviewer" should get
it by changing its own file, not by asking for a change to a program.

`meta.statuses.stories` is the list for stories and bugs, and
`meta.statuses.releases` is the list for releases. A status in either holds:

| Key | What it is |
| --- | --- |
| `id` | Its stable id: up to 64 letters, digits, hyphens, and underscores, and no two in a list share one. It never changes. |
| `name` | What people read. Up to 120 characters. It may change. |
| `finished` | `true` when a thing in this status is done with. |
| `moves` | The ids of the statuses this one may move to. With no `moves`, it may move to any. An empty list means none. |
| `archived` | `true` on a status that is retired, and left out on every other. A row that holds a retired status keeps it, and no row is newly given it. |

A list's order is the order of its statuses. There is no number to keep
beside it, so the two can never disagree.

Why a status's id may hold an underscore: a backend's ids are its own, and
it should not have to rename one to write a board file. A status's id names
a row's status and never a file. The board's `id`, a `rev`, and a slice's
`id` stay letters, digits, and hyphens.

A retired status stays on its list. Why: rows still hold it, and a row whose
`statusId` is not on the list has only its words left. The app keeps
`archived` and writes it back as it arrived, and it shows a retired status's
name like any other.

A row says where it stands with two keys, side by side:

- **`statusId`** is the id of a status in its list.
- **`status`** is that status's name, as text.

Why both: the id is what lasts. Rename "Building" to "In progress" and every
row still points at the same status, with nothing to remap. The name beside
it is for a person reading the file, and for a reader that has no list.

What a reader does:

- When `statusId` names a status in the list, that is the row's status, and
  the list's name is what is shown.
- When there is no `statusId`, or the list does not have it, `status` is
  shown as it is written.
- Neither case is an error. A status is never held to a fixed list.

A writer that has a list writes both keys. A writer with no list writes
`status` alone, and that is a whole file.

## Lanes and moves

A slice goes through five lanes, in this order: `sliced`, `written`,
`checked`, `reviewed`, and `main`. A slice's `lane` is the last one it has
been through. A slice with no `lane` has been through none.

`move`, on a story or a slice, says whose move it is: `you`, `model`,
`check`, or `review`. Leave it out, and the move goes to whoever takes the
slice into its next lane.

The app checks every move of a slice against its rules, which are coded
into it, each with a switch in Settings, under Rules. Only you take a slice
into `main`: that rule has no switch, and the app never merges.

When you move a slice in the Workflow view, the app writes its `lane` and
`move` into the copy on this computer, and nothing else on the board.
`board sync` then carries them to the shared file like any other change.

Lanes and moves are this app's own. A backend that has no use for them keeps
them and writes them back as they arrived.

Why: they record how a slice is being worked on this side. A backend that
dropped them would take that away on the next export.

`kind`, `lane`, and `move` are the three closed lists in the file. A value
outside one is refused, because a slice in a lane the reader has never heard
of has no place to be shown.

## Versions

`meta.version` is `line.step`: two whole numbers with a dot between them,
at most 16 characters. This page describes line 2, so a file that follows it
says `2.0`, or a later step such as `2.3`. A file whose `meta.format` is
`board` and whose version is not written this way, or is below line 2, is
not a board file.

**Within a line, a file only gains.** A step may add a key. It never
renames one, moves one, removes one, changes what one means, or adds a value
to one of the three closed lists.

**A reader keeps what it does not know.** A key this page does not name, at
any place in the file, is never a reason to refuse it. A reader built for
`2.0` reads `2.7`. When that reader writes the file back, the keys it did
not know go back where they were, with what they held, and the file keeps
the higher version.

**A change that is not an addition starts a new line**, and this page then
states the mapping from the line before, key by key, as it does below for
line 1.

**A reader says so when the line is above its own.** It names the line the
file is on and the lines it reads, and changes nothing. This is the one
refusal a version brings. The app's words for it:

```text
This board file is on line 3 of the format, and this app reads lines 1 and 2. Nothing was changed.
```

Why: the next party should be able to move its own logic ahead without
making everyone else remap. An addition costs the others nothing, so it is
free. A rename costs every reader, so it has to be announced, and the
mapping written down once, here, in place of being worked out by each of
them.

## Reading a board v1 file

Line 1 is board v1: one flat block, with `"format": "board v1"` at the top.
The app reads it, and a program that still writes it has nothing to change.

The mapping from line 1 to line 2:

| Board v1 | Line 2 |
| --- | --- |
| `format`: `board v1` | `meta.format`: `board`, and `meta.version`: `2.0` |
| `id`, `name`, `rev`, `history` | The same keys, under `meta` |
| `epics` | `data.releases` |
| Everything inside a release, a story, and a slice | The same keys, in the same places |

Board v1 has no `meta.statuses`, no `statusId`, no `writtenBy`, and no
`writtenAt`. A `status` in a v1 file is shown as it is written.

A copy of a board keeps the line its file arrived on. A board that came in
as v1 is written back as v1, so the program on the other side keeps reading
it. When that program starts to write line 2, the next pull moves the copy
to line 2, and it is written as line 2 from then on. A copy is never
written back down a line.

Why never down: line 2 holds what line 1 has no place for, and writing down
would drop it without a word. The cost is one thing to know before a backend
moves up: once it has exported a line 2 file, the app's pushes are line 2
files, so it has to go on reading them.

On the pull that moves a copy up, the two sides are on different lines for
one merge. The copy on the lower line is raised first, so that what the
lower line cannot hold is not read as a change:

- The format, the version, and the status lists come from the file on the
  higher line.
- A key this app does not know, on the board, a release, a story, or a
  slice, comes from the higher line wherever the lower copy has none.
- A row's `statusId` comes from the higher line while the row's `status`
  reads the same on both sides. A row whose `status` was changed on the
  lower line keeps its words and has no `statusId`.

## When both sides changed

`board sync` merges three ways: this computer's copy, the file, and the last
copy the two sides agreed on. A thing changed on one side is taken. The same
thing changed two ways is a conflict: it is listed, and nothing is written
until a side is picked. Three parts of a line 2 file have a rule of their
own.

- **A key this app does not know** is merged by itself, one key at a time.
  A step ahead on one side and a note on the other do not collide unless
  both changed the same key.
- **A slice's `number`** comes from the side that has one. Only when both
  sides have one, and they differ, is it merged like any other key.
- **Each status list** is merged whole. A list changed on one side is taken
  as it is. A list changed on both sides is one conflict, for the list, and
  not one for each status in it.

Why a list goes whole: its order is its meaning. Two lists merged status by
status could come out in an order neither side wrote.

With no copy to compare against, a key or a list that only one side has
stays. The merged board is on the higher of the two versions.

## Limits

| What | At most |
| --- | --- |
| A file | 4 MB |
| Releases on a board | 500 |
| Stories and bugs on a board | 5,000 |
| Slices in a story | 100 |
| Notes on a story or a slice | 200, each up to 4,000 characters |
| Statuses in a list | 100 |
| Revs in `history` | 50 |

Why: a file is read whole, and checked whole, before any of it is used. A
limit on each part keeps a file that is wrong, or is not a board at all,
from being costly to turn away.

## A backend of your own

The file is the whole of the join. A backend keeps the `rev` and `history`
of the last file it exported or imported, and whether its board changed
since.

- **To export**, when its board changed since that file, or it has never
  exported, it gives the board a new `rev`, one it has never used, and
  writes `history` as the last `rev` followed by the last `history`, at
  most 50. When nothing changed, it writes the same `rev` and `history`
  again.
- **To import**, when its own board changed since that file, it refuses:
  export first, run `board sync` on the app's side, then import. When the
  file's `rev` is the last one, there is nothing to do. When the last `rev`
  is in the file's `history`, it takes the file whole, and the file's `rev`
  and `history` become its last. Otherwise it refuses, because the file
  does not come from what it holds.
- It keeps every key it does not use, and writes it back as it arrived.
- A release keeps its name, a story its number, and a slice its id.

Why a `rev` is never used twice: it is the only thing that tells two states
apart. Two states under one name would look like no change at all.

**What "changed" means.** A backend tells whether its board changed by
comparing what it would write now with what the last file held, not by
watching its own tables. Four things stay out of that comparison:
`writtenBy`, `writtenAt`, the app's `pulledAt`, and every key the backend
only keeps.

Why: how a backend stores its board is its own, and a stored value can
change while the board stays the same. A key that only the app uses is not
the backend's change, so it never makes a new `rev` there.

**An import may apply part of a file.** A backend may take some of what a
file holds and leave the rest, when it lists every difference it did not
apply for the person importing. The file's `rev` and `history` still become
its last. Its board now holds a state that is not the file's, so its next
export has a new `rev`, with the file's `rev` at the head of `history`.

Why list them: without the list, the person importing believes the two
sides agree. Why a new `rev`: on the app's side, the next `board sync` then
reads each thing not applied as a change the backend made, and takes it. A
thing that was changed again on the app's side is a conflict to settle.

A backend that cannot compare may give a new `rev` after any import that
took anything. A new `rev` over the same board costs one line of `history`
and nothing else.

Three things an import may always leave not applied:

- **A status.** An import need not add, change, or retire a status. A
  difference in `meta.statuses` is listed like any other. Why: on a
  backend, a status may stand for more than its entry in the file.
- **A row the file does not have.** To an import, a row's absence from a
  file means nothing by itself, and nothing is removed for it.
- **A text the backend left out of its own export**, below. The file is
  empty there because the export was.

**What a writer may leave out.**

- **A text over its limit.** A `detail` or a note longer than
  [Limits](#limits) allows is left out whole, and the writer tells the
  person exporting how many it left out. It is never cut short. Why: a cut
  text reads as the whole text, and would come back on the next import as
  a change nobody made.
- **Rows that are not for the other side.** A backend may keep rows out of
  the files it writes, so a file is not a list of everything a board
  holds. It keeps such a row out of every file. Why: the app's copy is a
  copy of the file, so `board sync` reads a row that was in the last file
  and is not in this one as removed on the backend's side.
- **`moves`, where no list was set.** No `moves` means any, and an empty
  list means none. A writer keeps "not set" and "set to none" apart, and
  writes `moves` only for a status that has a list.

The merging is done on the app's side, by `board sync`. A backend that
changes its board before it imports a file pushed from the app has gone its
own way: when it next exports, nothing can say what each side changed, so
nothing is taken away, and every difference is listed as a conflict to
settle for one side.

The `board` command, and where the app keeps its copy, are in the
[desktop app's README](apps/desktop/README.md#boards).
