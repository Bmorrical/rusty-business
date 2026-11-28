import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  cleared,
  counts,
  findStory,
  hand,
  handLabel,
  kindLabel,
  laneCounts,
  LANES,
  moveLabel,
  movesFor,
  nextLane,
  noteOf,
  notesOf,
  openFirstOf,
  orphanNotes,
  pulledLine,
  road,
  royalFlush,
  sliceMove,
  slicesOf,
  sliceSteps,
  startSlice,
  startStory,
  statusName,
  storiesOf,
  storyLine,
  storyMove,
} from "./board.js";

// The same made-up board the app carries, so the page's reading of it and
// Rust's never drift apart. The file has a `meta` block and a `data` block.
// Rust hands the page one flat board, the same for both lines of the format:
// what `meta` holds, and the releases as `epics`.
const file = JSON.parse(readFileSync(new URL("../src-tauri/sample-board.json", import.meta.url), "utf8"));
const sample = { ...file.meta, epics: file.data.releases };
const story = (number) => findStory(sample, number);

test("there are five lanes, in the order a slice goes through them", () => {
  assert.deepEqual(
    LANES.map((lane) => lane.id),
    ["sliced", "written", "checked", "reviewed", "main"],
  );
  assert.deepEqual(
    LANES.map((lane) => lane.by),
    ["You", "A local model", "The check", "A reviewer", "You"],
  );
  // The hand a story is dealt: a royal flush is ten, jack, queen, king, ace.
  assert.deepEqual(
    LANES.map((lane) => lane.rank),
    ["10", "J", "Q", "K", "A"],
  );
});

test("a slice's lane is the last one it has been through", () => {
  assert.equal(cleared({ lane: "" }), 0);
  assert.equal(cleared({}), 0);
  assert.equal(cleared({ lane: "sliced" }), 1);
  assert.equal(cleared({ lane: "main" }), 5);
  assert.equal(cleared({ lane: "shipped" }), 0);
  assert.equal(nextLane({ lane: "written" }).id, "checked");
  assert.equal(nextLane({ lane: "main" }), null);
});

test("whose move a slice is follows its next lane, unless the board says otherwise", () => {
  assert.equal(sliceMove({ lane: "" }), "you");
  assert.equal(sliceMove({ lane: "sliced" }), "model");
  assert.equal(sliceMove({ lane: "written" }), "check");
  assert.equal(sliceMove({ lane: "checked" }), "review");
  assert.equal(sliceMove({ lane: "reviewed" }), "you");
  assert.equal(sliceMove({ lane: "main" }), "");
  assert.equal(sliceMove({ lane: "sliced", move: "you" }), "you");
  // A slice on main is nobody's move, whatever the board says.
  assert.equal(sliceMove({ lane: "main", move: "you" }), "");

  assert.equal(moveLabel("you"), "Your move");
  assert.equal(moveLabel("model"), "A local model's move");
  assert.equal(moveLabel("check"), "The check's move");
  assert.equal(moveLabel("review"), "A reviewer's move");
  assert.equal(moveLabel(""), "Done");
});

test("a slice's five steps say which are done, which is next, and which come later", () => {
  assert.deepEqual(
    sliceSteps({ lane: "written" }).map((step) => step.state),
    ["done", "done", "next", "later", "later"],
  );
  assert.deepEqual(
    sliceSteps({ lane: "main" }).map((step) => step.state),
    ["done", "done", "done", "done", "done"],
  );
  assert.equal(sliceSteps({})[0].state, "next");
});

test("a slice offers its next lane, and a way back to each lane it has been through", () => {
  const moves = (lane) => movesFor({ lane }).map((move) => [move.to, move.label, move.forward]);
  assert.deepEqual(moves(""), [["sliced", "Mark it sliced", true]]);
  assert.deepEqual(moves("sliced"), [["written", "Mark it written", true]]);
  assert.deepEqual(moves("checked"), [
    ["reviewed", "Mark it reviewed", true],
    ["sliced", "Write it again", false],
    ["written", "Check it again", false],
  ]);
  assert.deepEqual(moves("reviewed").map(([to, label]) => [to, label])[0], ["main", "Take it into main"]);
  assert.equal(moves("reviewed").length, 4);
  assert.deepEqual(movesFor({ lane: "main" }), [], "a slice on main can be moved");
  assert.deepEqual(movesFor(undefined), [["sliced", "Mark it sliced", true]].map(([to, label, forward]) => ({ to, label, forward })));
});

test("a story holds a lane when every slice has been through it", () => {
  // #412: main, checked, written, sliced.
  assert.deepEqual(hand(story(412)), [true, false, false, false, false]);
  assert.deepEqual(
    laneCounts(story(412)).map((lane) => `${lane.done}/${lane.of}`),
    ["4/4", "3/4", "2/4", "1/4", "1/4"],
  );
  assert.equal(handLabel(story(412)), "1 of 5 lanes");
  assert.equal(royalFlush(story(412)), false);
});

test("a story holds a royal flush when all five are done", () => {
  assert.deepEqual(hand(story(409)), [true, true, true, true, true]);
  assert.equal(royalFlush(story(409)), true);
  assert.equal(handLabel(story(409)), "Royal flush");
  assert.equal(storyMove(story(409)), "");
  assert.equal(storyLine(story(409)), "A royal flush: every slice is on main.");
});

test("a story with no slices holds nothing, and slicing it is your move", () => {
  assert.deepEqual(hand(story(413)), [false, false, false, false, false]);
  assert.equal(royalFlush(story(413)), false);
  assert.equal(handLabel(story(413)), "No slices yet");
  assert.equal(storyMove(story(413)), "you");
  assert.equal(storyLine(story(413)), "Your move: cut this into slices.");
  assert.equal(royalFlush({}), false);
  assert.equal(storyMove(undefined), "you");
});

test("a story with no slices that the board gives to someone else is not called yours", () => {
  const given = { ...story(413), move: "model" };
  assert.equal(storyMove(given), "model");
  assert.equal(storyLine(given), "A local model's move. No slices yet.");
  assert.equal(storyLine({ move: "review", slices: [] }), "A reviewer's move. No slices yet.");
  // Given to you by the board, it reads as it does when nobody is named.
  assert.equal(storyLine({ ...story(413), move: "you" }), "Your move: cut this into slices.");
});

test("the first board that opens is shown, and one that cannot be opened is named and passed over", async () => {
  const entries = [
    { id: "torn", name: "Torn" },
    { id: "road", name: "Road" },
    { id: "later", name: "Later" },
  ];
  const asked = [];
  const load = async (entry) => {
    asked.push(entry.id);
    if (entry.id === "torn") throw "Could not read your notes for this board: no such luck.";
    return { board: { id: entry.id }, drafts: {} };
  };

  const opened = await openFirstOf(entries, load);
  assert.equal(opened.local.board.id, "road");
  assert.equal(opened.problem, "Torn: Could not read your notes for this board: no such luck.");
  // It stops at the first that opens.
  assert.deepEqual(asked, ["torn", "road"]);

  // The first on the list opens: nothing to say.
  assert.deepEqual(await openFirstOf(entries.slice(1), load), { local: { board: { id: "road" }, drafts: {} }, problem: "" });

  // None opens: every one is named, by its id when it has no name.
  const none = await openFirstOf([{ id: "torn", name: "Torn" }, { id: "torn" }], load);
  assert.equal(none.local, null);
  assert.equal(
    none.problem,
    "Torn: Could not read your notes for this board: no such luck. torn: Could not read your notes for this board: no such luck.",
  );

  // No boards at all, or no list.
  assert.deepEqual(await openFirstOf([], load), { local: null, problem: "" });
  assert.deepEqual(await openFirstOf(undefined, load), { local: null, problem: "" });
});

test("a story is your move when any slice is, and otherwise follows the slice furthest behind", () => {
  // #415: one slice reviewed (yours, to main), one the board marks as yours.
  assert.equal(storyMove(story(415)), "you");
  assert.equal(storyLine(story(415)), "Your move. 0 of 2 slices are on main.");
  // Yours even when another slice is further behind: one waits on a model, one on you to merge.
  assert.equal(storyMove({ slices: [{ lane: "sliced" }, { lane: "reviewed" }] }), "you");
  // #412: nothing of yours; the slice furthest behind waits on a model.
  assert.equal(storyMove(story(412)), "model");
  assert.equal(storyLine(story(412)), "A local model's move. 1 of 4 slices are on main.");
  assert.equal(storyMove(story(420)), "model");
  // What the board says about the story comes first.
  assert.equal(storyMove({ ...story(412), move: "review" }), "review");
  assert.equal(storyLine({ slices: [{ lane: "written" }] }), "The check's move. 0 of 1 slice is on main.");
});

test('the "your move" filter keeps only your stories, and the releases that have one', () => {
  const numbers = (epics) => epics.map((epic) => epic.stories.map((entry) => entry.number));
  assert.deepEqual(numbers(road(sample)), [
    [409, 412, 413, 415],
    [420, 421],
  ]);
  assert.deepEqual(numbers(road(sample, true)), [[413, 415], [421]]);
  assert.equal(road(sample), sample.epics);

  const done = { epics: [{ release: "1", stories: [story(409)] }] };
  assert.deepEqual(road(done, true), []);
  assert.deepEqual(road(null, true), []);
  assert.deepEqual(road(undefined), []);
});

test("the counts say how much is on the board and how much of it is yours", () => {
  assert.deepEqual(counts(sample), { stories: 6, yours: 3, flushes: 1, slices: 10 });
  assert.deepEqual(counts(null), { stories: 0, yours: 0, flushes: 0, slices: 0 });
  assert.equal(storiesOf(sample).length, 6);
});

test("a story is found by its number, as a number or as text", () => {
  assert.equal(findStory(sample, 415).title, "The date filter skips the last day");
  assert.equal(findStory(sample, "415").number, 415);
  assert.equal(findStory(sample, 9), null);
  assert.equal(kindLabel(story(415)), "Bug");
  assert.equal(kindLabel(story(412)), "Story");
});

test("the view opens on your move: the first story that is yours, and its first slice that is", () => {
  assert.equal(startStory(sample).number, 413);
  assert.equal(startStory({ epics: [{ stories: [story(409), story(412)] }] }).number, 412);
  assert.equal(startStory({ epics: [{ stories: [story(409)] }] }).number, 409);
  assert.equal(startStory(null), null);

  assert.equal(startSlice(story(415)).id, "a");
  assert.equal(startSlice({ slices: [{ id: "a", lane: "sliced" }, { id: "b", lane: "reviewed" }] }).id, "b");
  assert.equal(startSlice(story(412)).id, "b");
  assert.equal(startSlice(story(409)).id, "a");
  assert.equal(startSlice(story(413)), null);
});

test("a story or a slice with a list left out reads as having none", () => {
  // The sample leaves `notes` off most slices, as a board from another tool may.
  const bare = story(412).slices[0];
  assert.equal("notes" in bare, false);
  assert.deepEqual(notesOf(bare), []);
  assert.deepEqual(notesOf(story(415).slices[1]), ["The wording needs a decision first."]);
  assert.deepEqual(notesOf(null), []);
  assert.deepEqual(slicesOf({ number: 1 }), []);
  assert.deepEqual(slicesOf(undefined), []);
  assert.equal(slicesOf(story(412)).length, 4);
  assert.deepEqual(road({ epics: [{ release: "1" }] }), [{ release: "1" }]);
});

test("your note on a story is read from the drafts, not from the board", () => {
  const drafts = { format: "board drafts v1", stories: { 412: { note: "Ask about dates too.", notedAt: 5 } } };
  assert.equal(noteOf(drafts, 412), "Ask about dates too.");
  assert.equal(noteOf(drafts, 413), "");
  assert.equal(noteOf(null, 412), "");
});

test("a note on a story that left the board is still shown", () => {
  const drafts = {
    stories: {
      999: { note: "It was here last week." },
      412: { note: "Still on the board." },
      7: { note: "An older one." },
      8: { note: "" },
    },
  };
  assert.deepEqual(orphanNotes(sample, drafts), [
    { number: 7, note: "An older one." },
    { number: 999, note: "It was here last week." },
  ]);
  assert.deepEqual(orphanNotes(sample, null), []);
});

test("the board says it is a copy on this computer, and from when", () => {
  const now = new Date(2026, 10, 20, 15, 30).getTime();
  assert.equal(pulledLine(0, now), "Local copy");
  assert.equal(pulledLine(new Date(2026, 10, 20, 9, 5).getTime(), now), "Local copy, pulled today at 9:05 AM");
  assert.equal(pulledLine(new Date(2026, 10, 18, 14, 0).getTime(), now), "Local copy, pulled Nov 18, 2026 at 2:00 PM");
});

test("the sample board names nobody and no address", () => {
  const text = JSON.stringify(file);
  assert.ok(!/https?:|www\.|@/.test(text), "the sample board holds an address");
  assert.deepEqual(Object.keys(file), ["meta", "data"]);
  assert.equal(sample.format, "board");
  assert.equal(sample.version, "2.0");
  assert.equal(sample.id, "sample");
  for (const lane of LANES) {
    assert.ok(
      storiesOf(sample).some((entry) => entry.slices.some((slice) => slice.lane === lane.id)),
      `the sample shows no slice at ${lane.id}`,
    );
  }
});

// "dynamic would be good rather than static, so myself and others can do more
// flexibility with less code changes."
test("a status reads as the board's own list names it", () => {
  // The sample lists its statuses, and every row points at one by id.
  assert.equal(statusName(sample, story(413)), "Ready to slice");
  assert.equal(statusName(sample, story(409)), "On main");
  assert.equal(statusName(sample, sample.epics[0], "releases"), "Building");
  assert.equal(statusName(sample, sample.epics[1], "releases"), "Scheduled");
  for (const entry of storiesOf(sample)) {
    assert.ok(sample.statuses.stories.some((status) => status.id === entry.statusId), `#${entry.number}`);
    assert.equal(statusName(sample, entry), entry.status);
  }

  // Rename a status in the list, and every row that points at it reads the
  // new name, with no row changed.
  const renamed = {
    ...sample,
    statuses: {
      ...sample.statuses,
      stories: sample.statuses.stories.map((status) =>
        status.id === "building" ? { ...status, name: "In progress" } : status,
      ),
    },
  };
  assert.equal(story(412).status, "Building");
  assert.equal(statusName(renamed, story(412)), "In progress");
  assert.equal(statusName(renamed, story(413)), "Ready to slice");

  // The two lists are apart: a story's id is never looked up among releases.
  assert.equal(statusName(sample, { status: "As written", statusId: "on-main" }, "releases"), "As written");
  assert.equal(statusName(sample, { status: "As written", statusId: "out" }, "releases"), "Out");

  // An id the list does not have, no id, or no list: the row's own words.
  assert.equal(statusName(sample, { status: "Parked", statusId: "parked" }), "Parked");
  assert.equal(statusName(sample, { status: "Parked" }), "Parked");
  assert.equal(statusName({ epics: [] }, { status: "Parked", statusId: "building" }), "Parked");
  assert.equal(statusName(null, { status: "Parked", statusId: "building" }), "Parked");
  assert.equal(statusName(sample, { statusId: "nowhere" }), "");
  assert.equal(statusName(sample, null), "");
});
