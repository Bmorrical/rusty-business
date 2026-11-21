// Helpers for the Workflow view, with no Vue and no Tauri in them.
//
// A board is a road: epics, the stories and bugs under them, and the slices
// each story is cut into. Rust keeps the board and checks it. These helpers
// only read one: which lanes a slice has been through, whose move it is, and
// what a story's hand looks like.

/**
 * The five lanes a slice goes through, in order. A slice's `lane` is the last
 * one it has been through. `move` is who takes it into this lane, and `rank`
 * is the card it stands for in a story's hand: ten, jack, queen, king, ace.
 */
export const LANES = [
  { id: "sliced", rank: "10", name: "Sliced", by: "You", move: "you", what: "The story is cut into slices small enough for a local model." },
  { id: "written", rank: "J", name: "Written", by: "A local model", move: "model", what: "The slice's change is written." },
  { id: "checked", rank: "Q", name: "Checked", by: "The check", move: "check", what: "The check ran on the change, and passed." },
  { id: "reviewed", rank: "K", name: "Reviewed", by: "A reviewer", move: "review", what: "A reviewer read the change, and every point is answered." },
  { id: "main", rank: "A", name: "Main", by: "You", move: "you", what: "The change is on main." },
];

const MOVES = {
  you: "Your move",
  model: "A local model's move",
  check: "The check's move",
  review: "A reviewer's move",
  "": "Done",
};

/** How many lanes a slice has been through: 0 for none, 5 when it is on main. */
export function cleared(slice) {
  return LANES.findIndex((lane) => lane.id === slice?.lane) + 1;
}

/** The lane a slice goes into next, or null when it is on main. */
export function nextLane(slice) {
  return LANES[cleared(slice)] ?? null;
}

/**
 * Whose move a slice is: what the board says, or else whoever takes it into
 * its next lane. A slice on main is nobody's move.
 */
export function sliceMove(slice) {
  const next = nextLane(slice);
  if (!next) return "";
  return slice?.move || next.move;
}

/** "Your move", "A local model's move", and so on. "Done" for nobody's. */
export function moveLabel(move) {
  return MOVES[move] ?? MOVES[""];
}

/** Each lane for one slice: "done", "next", or "later". */
export function sliceSteps(slice) {
  const through = cleared(slice);
  return LANES.map((lane, at) => ({
    lane,
    state: at < through ? "done" : at === through ? "next" : "later",
  }));
}

/**
 * A story's hand: one card for each lane, held when every slice has been
 * through it. A story with no slices holds none.
 */
export function hand(story) {
  const slices = story?.slices ?? [];
  return LANES.map((_, at) => slices.length > 0 && slices.every((slice) => cleared(slice) > at));
}

/** All five: every slice of the story is on main. */
export function royalFlush(story) {
  return hand(story).every(Boolean);
}

/** For each lane, how many of the story's slices have been through it. */
export function laneCounts(story) {
  const slices = story?.slices ?? [];
  return LANES.map((lane, at) => ({
    lane,
    done: slices.filter((slice) => cleared(slice) > at).length,
    of: slices.length,
  }));
}

/**
 * Whose move a story is. What the board says comes first. A story with no
 * slices is yours, to slice. After that it is yours when any slice is, and
 * otherwise it follows the slice furthest behind.
 */
export function storyMove(story) {
  if (royalFlush(story)) return "";
  if (story?.move) return story.move;
  const waiting = (story?.slices ?? []).filter((slice) => sliceMove(slice));
  if (!waiting.length) return "you";
  if (waiting.some((slice) => sliceMove(slice) === "you")) return "you";
  const behind = waiting.reduce((last, slice) => (cleared(slice) < cleared(last) ? slice : last));
  return sliceMove(behind);
}

/** What a story is waiting on, as a sentence. */
export function storyLine(story) {
  const slices = story?.slices ?? [];
  if (royalFlush(story)) return "A royal flush: every slice is on main.";
  if (!slices.length) {
    // Slicing is yours, unless the board gives the story to someone else.
    const move = storyMove(story);
    return move === "you" ? "Your move: cut this into slices." : `${moveLabel(move)}. No slices yet.`;
  }
  const onMain = slices.filter((slice) => !nextLane(slice)).length;
  return `${moveLabel(storyMove(story))}. ${onMain} of ${slices.length} ${slices.length === 1 ? "slice is" : "slices are"} on main.`;
}

/**
 * Opens the first board on the list that opens. `load` is asked for each in
 * turn. One that cannot be opened is passed over and named in `problem`, so
 * one file that cannot be read never hides the boards that can.
 */
export async function openFirstOf(entries, load) {
  const problems = [];
  for (const entry of entries ?? []) {
    try {
      return { local: await load(entry), problem: problems.join(" ") };
    } catch (error) {
      problems.push(`${entry?.name || entry?.id}: ${String(error)}`);
    }
  }
  return { local: null, problem: problems.join(" ") };
}

/** A story's hand in words, for someone who cannot see its five marks. */
export function handLabel(story) {
  const slices = story?.slices ?? [];
  if (!slices.length) return "No slices yet";
  if (royalFlush(story)) return "Royal flush";
  const held = hand(story).filter(Boolean).length;
  return `${held} of ${LANES.length} lanes`;
}

/** "Story" or "Bug". */
export function kindLabel(story) {
  return story?.kind === "bug" ? "Bug" : "Story";
}

/**
 * The story to open first: the first that is your move, or else the first
 * still being worked on, or else the first.
 */
export function startStory(board) {
  const stories = storiesOf(board);
  return (
    stories.find((story) => storyMove(story) === "you") ??
    stories.find((story) => !royalFlush(story)) ??
    stories[0] ??
    null
  );
}

/**
 * The slice to open first in a story: the first that is your move, or else
 * the first not yet on main, or else the first.
 */
export function startSlice(story) {
  const slices = story?.slices ?? [];
  return (
    slices.find((slice) => sliceMove(slice) === "you") ?? slices.find((slice) => nextLane(slice)) ?? slices[0] ?? null
  );
}

/** A story's slices, or none. A board may leave the list out. */
export function slicesOf(story) {
  return story?.slices ?? [];
}

/** The board's own notes on a story or a slice, or none. */
export function notesOf(entry) {
  return entry?.notes ?? [];
}

/** Every story on the board, in the road's order. */
export function storiesOf(board) {
  return (board?.epics ?? []).flatMap((epic) => epic.stories ?? []);
}

/** The road to show: every epic, or only the stories that are your move. */
export function road(board, onlyYours = false) {
  const epics = board?.epics ?? [];
  if (!onlyYours) return epics;
  return epics
    .map((epic) => ({ ...epic, stories: (epic.stories ?? []).filter((story) => storyMove(story) === "you") }))
    .filter((epic) => epic.stories.length);
}

/** How much is on the board, and how much of it is yours. */
export function counts(board) {
  const stories = storiesOf(board);
  return {
    stories: stories.length,
    yours: stories.filter((story) => storyMove(story) === "you").length,
    flushes: stories.filter(royalFlush).length,
    slices: stories.reduce((sum, story) => sum + (story.slices ?? []).length, 0),
  };
}

/** One story by its number, or null. */
export function findStory(board, number) {
  return storiesOf(board).find((story) => story.number === Number(number)) ?? null;
}

/** The note you wrote on a story, or "". */
export function noteOf(drafts, number) {
  return drafts?.stories?.[number]?.note ?? "";
}

/**
 * Your notes on stories that are no longer on the board. A pull can take a
 * story away. It never takes your note with it.
 */
export function orphanNotes(board, drafts) {
  const here = new Set(storiesOf(board).map((story) => story.number));
  return Object.entries(drafts?.stories ?? {})
    .map(([number, draft]) => ({ number: Number(number), note: draft?.note ?? "" }))
    .filter((entry) => entry.note && !here.has(entry.number))
    .sort((a, b) => a.number - b.number);
}

/** The line that says the board is a copy on this computer, and from when. */
export function pulledLine(pulledAt, now = Date.now()) {
  if (!pulledAt) return "Local copy";
  const when = new Date(pulledAt);
  const time = when.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
  const today = new Date(now).toDateString() === when.toDateString();
  const day = today ? "today" : when.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
  return `Local copy, pulled ${day} at ${time}`;
}
