<script setup>
// The Workflow view: a board's road, a story's hand, and its slices.
//
// It reads and moves only the copy of a board kept on this computer, so it
// opens the same with both gates off. Rust keeps the board and your notes on
// it. This page asks for them when it opens and when you pick something, and
// it has no way to reach anything else. A move names only the lane: Rust
// checks it against the rules and makes it as you. A slice handed to a model
// names only the model: Rust refuses one outside this computer, keeps the
// runner to its time, and moves the slice only into written.
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Channel, invoke } from "@tauri-apps/api/core";

import {
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
  storyLine,
  storyMove,
} from "./board.js";
import { canHandOff, clock, clockLine, limitOf, ranLine, runnerModels } from "./runners.js";

const props = defineProps({
  /** The WAN gate, as the app has it. This view only says where it stands. */
  wan: { type: Boolean, default: false },
  /** The LAN gate, the same. */
  lan: { type: Boolean, default: false },
  /** Whether the side is showing, so the bar knows to offer it. */
  sideOpen: { type: Boolean, default: true },
  /** The models switched on and on hand. Only those that can be runners are offered. */
  models: { type: Array, default: () => [] },
});
const emit = defineEmits(["close", "side"]);

const SIDE_ICON = "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2ZM9 3v18";
const STEP_WORDS = { done: "Done", next: "Next", later: "Later" };
const STEP_MARKS = { done: "✓", next: "→", later: "" };

const title = ref(null);
/** The columns, the work area, and its two panes, so a pick can be brought into view. */
const cols = ref(null);
const work = ref(null);
const storyPane = ref(null);
const slicePane = ref(null);
/** Every board file in the app's folder, as Rust lists them. */
const boards = ref([]);
/** The open board and your notes on it. */
const local = ref(null);
/** False until the first answer from Rust, so nothing flashes before it. */
const ready = ref(false);
const busy = ref(false);
const problem = ref("");
const onlyYours = ref(false);
const openNumber = ref(0);
const openSliceId = ref("");

const noteText = ref("");
/** The note as it is saved, to tell whether the box has changed. */
const noteWas = ref("");
const noteLine = ref("");
const noteProblem = ref("");
/** What the last move of a slice came to, said beside that slice only. */
const moved = ref({ story: 0, slice: "", line: "", problem: "" });

const board = computed(() => local.value?.board ?? null);
const drafts = computed(() => local.value?.drafts ?? null);
const good = computed(() => boards.value.filter((entry) => !entry.problem));
const bad = computed(() => boards.value.filter((entry) => entry.problem));
const hasSample = computed(() => boards.value.some((entry) => entry.id === "sample"));
const total = computed(() => counts(board.value));
const orphans = computed(() => orphanNotes(board.value, drafts.value));

/** The road as it shows, with each story's hand worked out once. */
const rows = computed(() =>
  road(board.value, onlyYours.value).map((epic) => ({
    epic,
    stories: (epic.stories ?? []).map((entry) => ({
      story: entry,
      yours: storyMove(entry) === "you",
      hand: hand(entry),
      label: handLabel(entry),
    })),
  })),
);

const story = computed(() => findStory(board.value, openNumber.value));
/** A story that left the board, open because you have a note on it. */
const orphan = computed(() => !story.value && orphans.value.some((entry) => entry.number === openNumber.value));
/** The open story's slices, and the open slice. A board may leave a list out. */
const slices = computed(() => slicesOf(story.value));
const slice = computed(() => slices.value.find((entry) => entry.id === openSliceId.value) ?? null);
const storyNotes = computed(() => notesOf(story.value));
const sliceNotes = computed(() => notesOf(slice.value));
const cards = computed(() => {
  const held = hand(story.value);
  return laneCounts(story.value).map((card, at) => ({ ...card, held: held[at] }));
});
const noteDirty = computed(() => noteText.value.trimEnd() !== noteWas.value);

function readNote() {
  noteWas.value = noteOf(drafts.value, openNumber.value);
  noteText.value = noteWas.value;
  noteLine.value = "";
  noteProblem.value = "";
}

/** Shows a board, opened on your move. */
function show(next) {
  local.value = next;
  const first = startStory(next.board);
  openNumber.value = first?.number ?? 0;
  openSliceId.value = startSlice(first)?.id ?? "";
  readNote();
}

/** One ask of Rust at a time, and what it refused is said on the page. */
async function run(task) {
  if (busy.value) return;
  busy.value = true;
  problem.value = "";
  try {
    await task();
  } catch (error) {
    problem.value = String(error);
  } finally {
    busy.value = false;
    ready.value = true;
  }
}

function openFirst() {
  return run(async () => {
    boards.value = await invoke("list_boards");
    // A board that cannot be opened is passed over and named, so one file
    // that cannot be read never hides the boards that can.
    const opened = await openFirstOf(good.value, (entry) => invoke("load_board", { id: entry.id }));
    if (opened.local) show(opened.local);
    problem.value = opened.problem;
  });
}

async function pickBoard(event) {
  const id = event.target.value;
  await run(async () => {
    if (await saveNote()) show(await invoke("load_board", { id }));
  });
  if (board.value) event.target.value = board.value.id;
}

function openSample() {
  return run(async () => {
    if (!(await saveNote())) return;
    show(await invoke("open_sample_board"));
    boards.value = await invoke("list_boards");
  });
}

/**
 * Moves the open slice into a lane. The note is kept first, so nothing typed
 * is left behind. Rust decides: a move the rules refuse says why, and changes
 * nothing unless a runner drifted.
 */
function moveSlice(move) {
  const id = board.value?.id;
  const number = openNumber.value;
  const sliceId = openSliceId.value;
  if (!id || !sliceId) return Promise.resolve();
  return run(async () => {
    if (!(await saveNote())) return;
    moved.value = { story: number, slice: sliceId, line: "", problem: "" };
    const taken = await invoke("move_slice", { id, story: number, slice: sliceId, to: move.to });
    if (board.value?.id === id) local.value = taken.local;
    const lane = LANES.find((entry) => entry.id === move.to)?.name ?? move.to;
    moved.value = taken.verdict.allowed
      ? { story: number, slice: sliceId, line: `${move.forward ? "Moved into" : "Moved back to"} ${lane}.`, problem: "" }
      : { story: number, slice: sliceId, line: "", problem: taken.verdict.refusals.map((refusal) => refusal.words).join(" ") };
  });
}

/** The models a slice can go to, and the one picked. */
const runners = computed(() => runnerModels(props.models));
const runModel = ref("");
/** The run this page started: which slice, since when, and its limit. */
const running = ref(null);
/** The slices being written, as "board/story/slice", from Rust. */
const going = ref([]);
const now = ref(Date.now());
let ticking = 0;
/** What the last run came to, said beside that slice only. */
const ran = ref({ story: 0, slice: "", line: "", problem: "" });
/** What a model wrote for the open slice, as Rust keeps it. */
const kept = ref(null);

const runKey = computed(() => (board.value && slice.value ? `${board.value.id}/${openNumber.value}/${slice.value.id}` : ""));
const runningHere = computed(() => Boolean(runKey.value) && running.value?.key === runKey.value);
/** A model is writing the open slice, started here or before the view was opened. */
const writingHere = computed(() => runningHere.value || going.value.includes(runKey.value));
const ranHere = computed(() => (ran.value.story === openNumber.value && ran.value.slice === openSliceId.value ? ran.value : null));
const pickedRunner = computed(() => runners.value.find((entry) => entry.id === runModel.value) ?? runners.value[0] ?? null);

async function readGoing() {
  try {
    going.value = await invoke("runs_going");
  } catch {
    going.value = [];
  }
}

async function readWork() {
  const key = runKey.value;
  kept.value = null;
  if (!key) return;
  try {
    const found = await invoke("slice_work", { id: board.value.id, story: openNumber.value, slice: slice.value.id });
    if (runKey.value === key) kept.value = found;
  } catch {
    kept.value = null;
  }
}

watch(runKey, () => {
  readWork();
  readGoing();
});

/**
 * Hands the open slice to a model. The page names the model and nothing else;
 * Rust checks every rule before it asks, and again when the model is done.
 */
async function handOff() {
  const model = pickedRunner.value?.id;
  const id = board.value?.id;
  const number = openNumber.value;
  const sliceId = openSliceId.value;
  if (!model || !id || !sliceId || running.value) return;
  if (!(await saveNote())) return;
  let limit = null;
  try {
    limit = limitOf(await invoke("rules_status"));
  } catch {
    limit = null;
  }
  const key = `${id}/${number}/${sliceId}`;
  running.value = { key, story: number, slice: sliceId, started: Date.now(), limit };
  ran.value = { story: number, slice: sliceId, line: "", problem: "" };
  now.value = Date.now();
  ticking = window.setInterval(() => {
    now.value = Date.now();
  }, 1000);
  try {
    // The model's words come back as it writes; the page shows only the clock
    // and what was kept, so the channel is drained and left.
    const result = await invoke("run_slice", { id, story: number, slice: sliceId, model, onEvent: new Channel() });
    if (result.local && board.value?.id === id) local.value = result.local;
    ran.value = result.moved
      ? { story: number, slice: sliceId, line: ranLine(result), problem: "" }
      : { story: number, slice: sliceId, line: "", problem: ranLine(result) };
  } catch (error) {
    ran.value = { story: number, slice: sliceId, line: "", problem: String(error) };
  } finally {
    window.clearInterval(ticking);
    running.value = null;
    await readGoing();
    await readWork();
  }
}

async function stopRunner() {
  if (!board.value || !slice.value) return;
  await invoke("stop_run", { id: board.value.id, story: openNumber.value, slice: slice.value.id }).catch(() => false);
  await readGoing();
}

const movedHere = computed(() =>
  moved.value.story === openNumber.value && moved.value.slice === openSliceId.value ? moved.value : null,
);

let saving = null;

/**
 * Saves the note on the open story when it has changed. It answers whether
 * the note is safe, so nothing moves on and leaves it behind. What was typed
 * while an earlier save was on its way is saved after it.
 */
function saveNote() {
  if (saving) return saving.then((safe) => (safe ? saveNote() : false));
  if (!noteDirty.value || !board.value) return Promise.resolve(true);
  const id = board.value.id;
  const number = openNumber.value;
  const text = noteText.value;
  noteProblem.value = "";
  saving = invoke("save_board_note", { id, story: number, text })
    .then((saved) => {
      if (board.value?.id === id) local.value = { ...local.value, drafts: saved };
      if (openNumber.value === number) {
        noteWas.value = noteOf(saved, number);
        noteLine.value = noteWas.value ? "Saved on this computer." : "The note is cleared.";
        // A note of only spaces is a cleared one, so the box empties too.
        if (!noteWas.value && noteText.value === text) noteText.value = "";
      }
      return true;
    })
    .catch((error) => {
      noteLine.value = "";
      noteProblem.value = String(error);
      return false;
    })
    .finally(() => {
      saving = null;
    });
  return saving;
}

/** Whether a part of the page has its pieces stacked, one under the other. */
function stacked(part) {
  return Boolean(part) && getComputedStyle(part).display === "block";
}

/**
 * Brings a pane to the top and puts the keyboard on its heading. Only for a
 * pane that sits under what was just picked: side by side, it is in view.
 */
function bring(pane) {
  nextTick(() => {
    pane.value?.querySelector("h3")?.focus({ preventScroll: true });
    pane.value?.scrollIntoView({ block: "start" });
  });
}

/** What was just picked reads from its top, whichever part of the page scrolls. */
function toTop(...parts) {
  for (const part of parts) if (part.value) part.value.scrollTop = 0;
}

async function pickStory(number) {
  if (number === openNumber.value) return;
  if (!(await saveNote())) return;
  openNumber.value = number;
  openSliceId.value = startSlice(story.value)?.id ?? "";
  readNote();
  toTop(work, storyPane, slicePane);
  if (stacked(cols.value)) bring(storyPane);
}

function pickSlice(id) {
  openSliceId.value = id;
  toTop(slicePane);
  if (stacked(work.value)) bring(slicePane);
}

/** Saves, and steps back to the road's start when a cleared note was all that was open. */
async function keepNote() {
  await saveNote();
  if (!story.value && !orphan.value && board.value) {
    const first = startStory(board.value);
    openNumber.value = first?.number ?? 0;
    openSliceId.value = startSlice(first)?.id ?? "";
    readNote();
  }
}

onMounted(() => {
  nextTick(() => title.value?.focus());
  openFirst();
  readGoing();
});

/**
 * Goes back to the chat once the note is safe. A note that could not be
 * written keeps the view open, with the reason beside it.
 */
async function leave() {
  if (await saveNote()) emit("close");
}

onBeforeUnmount(() => {
  // A runner goes on when the view closes; Rust keeps what it writes.
  window.clearInterval(ticking);
  saveNote();
});
</script>

<template>
  <section class="settings workflow" aria-labelledby="workflow-title" @keydown.esc="leave">
    <header class="bar">
      <button
        v-if="!sideOpen"
        class="icon"
        type="button"
        aria-label="Show the side"
        title="Show the side"
        @click="emit('side')"
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="SIDE_ICON" /></svg>
      </button>
      <h2 id="workflow-title" ref="title" tabindex="-1">Workflow</h2>
      <button type="button" @click="leave">Back to the chat</button>
    </header>

    <div v-if="board" class="wf-strip">
      <p class="wf-board">
        <template v-if="good.length > 1">
          <label class="sr-only" for="wf-board">Board</label>
          <select id="wf-board" :value="board.id" :disabled="busy" @change="pickBoard">
            <option v-for="entry in good" :key="entry.id" :value="entry.id">{{ entry.name }}</option>
          </select>
        </template>
        <strong v-else>{{ board.name }}</strong>
        <span class="quiet small">{{ pulledLine(board.pulledAt) }}</span>
      </p>
      <p class="wf-gates small">
        <span class="quiet">This view reads and moves only that copy.</span>
        <span class="wf-gate" :class="{ on: wan }">WAN {{ wan ? "on" : "off" }}</span>
        <span class="wf-gate" :class="{ on: lan }">LAN {{ lan ? "on" : "off" }}</span>
        <button v-if="!hasSample" class="link" type="button" :disabled="busy" @click="openSample">
          Open the sample board
        </button>
      </p>
    </div>

    <p v-if="problem" class="wf-problem" role="alert">{{ problem }}</p>
    <ul v-if="bad.length" class="wf-bad small">
      <li v-for="entry in bad" :key="entry.id">
        <code>{{ entry.id }}.json</code> is in the boards folder, and was left as it is. {{ entry.problem }}
      </li>
    </ul>

    <div v-if="board" ref="cols" class="wf-cols">
      <nav class="wf-road" aria-label="The road">
        <div class="wf-road-head">
          <button class="chip small" type="button" :aria-pressed="onlyYours" @click="onlyYours = !onlyYours">
            Your move <span class="chip-mark">{{ total.yours }}</span>
          </button>
          <p class="quiet small">
            {{ total.stories }} on the road, {{ total.flushes }} with a royal flush
          </p>
        </div>

        <p v-if="!rows.length" class="quiet small wf-none">
          {{ onlyYours ? "Nothing is waiting on you." : "This board has no stories." }}
        </p>
        <section v-for="row in rows" :key="row.epic.release" class="wf-release">
          <h3>
            <span class="label">{{ row.epic.release }}</span>
            <span class="wf-release-title">{{ row.epic.title }}</span>
            <span v-if="statusName(board, row.epic, 'releases')" class="quiet small">{{
              statusName(board, row.epic, "releases")
            }}</span>
          </h3>
          <ul>
            <li v-for="entry in row.stories" :key="entry.story.number">
              <button
                class="wf-story"
                type="button"
                :aria-current="entry.story.number === openNumber ? 'true' : undefined"
                @click="pickStory(entry.story.number)"
              >
                <span class="wf-story-top">
                  <span class="wf-number">#{{ entry.story.number }}</span>
                  <span v-if="entry.story.kind === 'bug'" class="wf-tag bug">Bug</span>
                  <span v-if="entry.yours" class="wf-tag yours">Your move</span>
                  <span class="wf-pips" aria-hidden="true">
                    <span v-for="(held, at) in entry.hand" :key="at" class="wf-pip" :class="{ held }"></span>
                  </span>
                  <span class="sr-only">{{ entry.label }}</span>
                </span>
                <span class="wf-story-title">{{ entry.story.title }}</span>
              </button>
            </li>
          </ul>
        </section>

        <section v-if="orphans.length" class="wf-release">
          <h3>
            <span class="label">Your notes</span>
            <span class="wf-release-title">No longer on this board</span>
          </h3>
          <ul>
            <li v-for="entry in orphans" :key="entry.number">
              <button
                class="wf-story"
                type="button"
                :aria-current="entry.number === openNumber ? 'true' : undefined"
                @click="pickStory(entry.number)"
              >
                <span class="wf-story-top"><span class="wf-number">#{{ entry.number }}</span></span>
                <span class="wf-story-title wf-clip">{{ entry.note }}</span>
              </button>
            </li>
          </ul>
        </section>
      </nav>

      <div ref="work" class="wf-work">
        <article v-if="story || orphan" ref="storyPane" class="wf-pane" aria-labelledby="wf-story-title">
          <template v-if="story">
            <p class="label">
              {{ kindLabel(story) }} #{{ story.number
              }}<template v-if="statusName(board, story)"> · {{ statusName(board, story) }}</template>
            </p>
            <h3 id="wf-story-title" tabindex="-1">{{ story.title }}</h3>
            <p v-if="story.detail" class="wf-detail">{{ story.detail }}</p>

            <ol class="wf-hand" aria-label="This story's hand, one card for each lane">
              <li v-for="card in cards" :key="card.lane.id" class="wf-card" :class="{ held: card.held }">
                <span class="wf-rank" aria-hidden="true">{{ card.lane.rank }}</span>
                <span class="wf-card-name">{{ card.lane.name }}</span>
                <span class="wf-card-by quiet">{{ card.lane.by }}</span>
                <span class="wf-card-count">
                  {{ card.of ? `${card.done} of ${card.of}` : "No slices" }}
                  <span v-if="card.held" class="sr-only">, held</span>
                </span>
              </li>
            </ol>
            <p class="wf-line" :class="{ flush: royalFlush(story), yours: storyMove(story) === 'you' }">
              {{ storyLine(story) }}
            </p>

            <section class="wf-block" aria-labelledby="wf-slices-title">
              <h4 id="wf-slices-title" class="label">Slices</h4>
              <ul v-if="slices.length" class="wf-slices">
                <li v-for="entry in slices" :key="entry.id">
                  <button
                    class="wf-slice"
                    type="button"
                    :aria-current="entry.id === openSliceId ? 'true' : undefined"
                    @click="pickSlice(entry.id)"
                  >
                    <span class="wf-slice-id">{{ entry.id }}</span>
                    <span class="wf-slice-title">{{ entry.title }}</span>
                    <span class="wf-pips" aria-hidden="true">
                      <span
                        v-for="step in sliceSteps(entry)"
                        :key="step.lane.id"
                        class="wf-pip"
                        :class="{ held: step.state === 'done' }"
                      ></span>
                    </span>
                    <span class="wf-slice-move small" :class="{ yours: sliceMove(entry) === 'you' }">
                      {{ moveLabel(sliceMove(entry)) }}
                    </span>
                  </button>
                </li>
              </ul>
              <p v-else class="quiet">
                No slices yet. This view shows the slices a board arrives with. Cutting a story into slices here is on
                the roadmap.
              </p>
            </section>

            <section v-if="storyNotes.length" class="wf-block" aria-labelledby="wf-notes-title">
              <h4 id="wf-notes-title" class="label">Notes from the board</h4>
              <ul class="wf-notes">
                <li v-for="(entry, at) in storyNotes" :key="at">{{ entry }}</li>
              </ul>
            </section>
          </template>
          <template v-else>
            <p class="label">#{{ openNumber }}</p>
            <h3 id="wf-story-title" tabindex="-1">No longer on this board</h3>
            <p class="wf-detail">The story left the board in a newer copy. Your note on it stayed.</p>
          </template>

          <section class="wf-block">
            <label class="label" for="wf-note">My note</label>
            <textarea
              id="wf-note"
              v-model="noteText"
              rows="4"
              maxlength="4000"
              aria-describedby="wf-note-hint"
              @blur="keepNote"
            ></textarea>
            <div class="wf-note-foot">
              <p id="wf-note-hint" class="quiet small">
                Kept on this computer, apart from the board, so a newer copy of the board never replaces it.
              </p>
              <button type="button" :disabled="!noteDirty" @click="keepNote">Save</button>
            </div>
            <p class="small wf-saved" role="status">{{ noteLine }}</p>
            <p v-if="noteProblem" class="wf-problem small" role="alert">{{ noteProblem }}</p>
          </section>
        </article>

        <aside v-if="story" ref="slicePane" class="wf-pane wf-open" aria-labelledby="wf-slice-title">
          <template v-if="slice">
            <p class="label">
              Slice {{ slice.id }} of #{{ story.number
              }}<template v-if="slice.number"> · number {{ slice.number }}</template>
            </p>
            <h3 id="wf-slice-title" tabindex="-1">{{ slice.title }}</h3>
            <p class="wf-line" :class="{ yours: sliceMove(slice) === 'you' }">
              {{ moveLabel(sliceMove(slice))
              }}<template v-if="nextLane(slice)">. {{ nextLane(slice).name }} is next.</template>
              <template v-else>. It is on main.</template>
            </p>

            <section class="wf-block" aria-labelledby="wf-brief-title">
              <h4 id="wf-brief-title" class="label">The brief</h4>
              <p v-if="slice.detail" class="wf-brief">{{ slice.detail }}</p>
              <p v-else class="quiet">This slice has no brief yet.</p>
            </section>

            <section class="wf-block" aria-labelledby="wf-steps-title">
              <h4 id="wf-steps-title" class="label">Its lanes</h4>
              <ol class="wf-steps">
                <li v-for="step in sliceSteps(slice)" :key="step.lane.id" :class="step.state">
                  <span class="wf-step-mark" aria-hidden="true">{{ STEP_MARKS[step.state] }}</span>
                  <span class="wf-step-name">
                    {{ step.lane.name }}
                    <span class="sr-only">: {{ STEP_WORDS[step.state] }}.</span>
                  </span>
                  <span class="wf-step-by quiet small">{{ step.lane.by }}</span>
                  <span class="wf-step-what quiet small">{{ step.lane.what }}</span>
                </li>
              </ol>
              <div v-if="movesFor(slice).length" class="wf-moves">
                <button
                  v-for="move in movesFor(slice)"
                  :key="move.to"
                  type="button"
                  :class="{ primary: move.forward }"
                  :disabled="busy || writingHere"
                  @click="moveSlice(move)"
                >
                  {{ move.label }}
                </button>
              </div>
              <p class="small wf-saved" role="status">{{ movedHere?.line }}</p>
              <p v-if="movedHere?.problem" class="wf-problem small" role="alert">{{ movedHere.problem }}</p>
            </section>

            <section v-if="canHandOff(slice) || writingHere" class="wf-block" aria-labelledby="wf-runner-title">
              <h4 id="wf-runner-title" class="label">Hand it to a model</h4>
              <template v-if="writingHere">
                <p class="wf-clock" role="timer" aria-live="off">
                  {{ runningHere ? clockLine(now - running.started, running.limit) : "A model is writing this slice." }}
                </p>
                <div class="wf-moves">
                  <button type="button" @click="stopRunner">Stop</button>
                </div>
              </template>
              <template v-else-if="runners.length">
                <div class="wf-runner">
                  <label class="sr-only" for="wf-runner-model">Model</label>
                  <select id="wf-runner-model" :value="pickedRunner?.id" @change="runModel = $event.target.value">
                    <option v-for="entry in runners" :key="entry.id" :value="entry.id">{{ entry.name || entry.id }}</option>
                  </select>
                  <button class="primary" type="button" :disabled="busy || Boolean(running)" @click="handOff">
                    Hand it to a model
                  </button>
                </div>
                <p class="quiet small">
                  The model writes the slice on its time limit. A model on this computer or on your network only.
                </p>
              </template>
              <p v-else class="quiet small">
                No model on this computer or on your network is on hand. A slice is never sent to an outside engine.
              </p>
              <p class="small wf-saved" role="status">{{ ranHere?.line }}</p>
              <p v-if="ranHere?.problem" class="wf-problem small" role="alert">{{ ranHere.problem }}</p>
            </section>

            <section v-if="kept" class="wf-block" aria-labelledby="wf-work-title">
              <h4 id="wf-work-title" class="label">What the model wrote</h4>
              <p class="quiet small">{{ kept.model }}, in {{ clock(kept.tookMs) }}.</p>
              <p class="wf-brief">{{ kept.text }}</p>
              <p v-if="kept.cut" class="quiet small">The rest was past the size kept for a slice, and was left out.</p>
            </section>

            <section v-if="sliceNotes.length" class="wf-block" aria-labelledby="wf-slice-notes-title">
              <h4 id="wf-slice-notes-title" class="label">Notes from the board</h4>
              <ul class="wf-notes">
                <li v-for="(entry, at) in sliceNotes" :key="at">{{ entry }}</li>
              </ul>
            </section>
          </template>
          <template v-else>
            <h3 id="wf-slice-title" class="sr-only" tabindex="-1">The open slice</h3>
            <p class="quiet">
              {{ slices.length ? "Pick a slice to read its brief." : "A slice opens here once the story has one." }}
            </p>
          </template>
        </aside>
      </div>
    </div>

    <div v-else-if="ready" class="wf-empty">
      <p class="display">{{ good.length ? "No board could be opened." : "No board on this computer yet." }}</p>
      <p class="quiet">
        A board is a road: releases, the stories and bugs under them, and the slices each story is cut into. The sample
        board is made up, and shows how one reads.
      </p>
      <button class="primary" type="button" :disabled="busy" @click="openSample">Open the sample board</button>
    </div>
  </section>
</template>

<style scoped>
.workflow {
  container: workflow / inline-size;
}

.wf-strip {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 0.35rem 1.25rem;
  padding: 0.55rem 1.5rem;
  border-bottom: 1px solid var(--line);
}

.wf-board,
.wf-gates {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0.3rem 0.75rem;
}

.wf-board select {
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--page);
  padding: 0.3rem 0.5rem;
  font-weight: 600;
}

.wf-gate {
  font-family: var(--mono);
  font-size: 0.7rem;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  padding: 0.1rem 0.45rem;
  border: 1px solid var(--line);
  border-radius: 999px;
  color: var(--quiet);
}

.wf-gate.on {
  border-color: var(--accent);
  color: var(--accent-text);
}

.wf-problem {
  margin: 0.75rem 1.5rem 0;
  color: var(--bad);
}

.wf-block .wf-problem {
  margin: 0;
}

.wf-bad {
  margin: 0.6rem 1.5rem 0;
  padding: 0;
  list-style: none;
  color: var(--quiet);
}

/* The road down the left, and the work beside it. */
.wf-cols {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(13rem, 17rem) minmax(0, 1fr);
  grid-template-rows: minmax(0, 1fr);
}

/* Each pane that scrolls is also the anchor for what it holds, so text kept
   for screen readers scrolls with its pane and never stretches the page. */
.wf-road {
  position: relative;
  min-height: 0;
  overflow-y: auto;
  padding: 0.9rem 0.75rem 2rem;
  border-right: 1px solid var(--line);
}

.wf-road-head {
  display: grid;
  justify-items: start;
  gap: 0.4rem;
  padding: 0 0.4rem 0.3rem;
}

.wf-none {
  padding: 0.6rem 0.4rem;
}

.wf-release {
  margin-top: 1.1rem;
}

.wf-release h3 {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0.1rem 0.5rem;
  padding: 0 0.4rem 0.3rem;
  font-size: 0.9rem;
}

.wf-release h3 .label {
  padding: 0;
  letter-spacing: 0.12em;
}

.wf-release-title {
  font-weight: 600;
}

.wf-release h3 .quiet {
  font-weight: 400;
}

.wf-release ul,
.wf-slices,
.wf-hand,
.wf-steps,
.wf-notes {
  list-style: none;
  margin: 0;
  padding: 0;
}

.wf-release ul {
  display: grid;
  gap: 0.15rem;
}

.wf-story {
  display: grid;
  gap: 0.15rem;
  width: 100%;
  padding: 0.45rem 0.55rem;
  border-color: transparent;
  text-align: left;
}

.wf-story[aria-current="true"] {
  border-color: var(--line);
  background: var(--raised);
}

.wf-story-top {
  display: flex;
  align-items: center;
  gap: 0.45rem;
}

.wf-number,
.wf-slice-id {
  font-family: var(--mono);
  font-size: 0.72rem;
  color: var(--quiet);
}

.wf-story-title {
  line-height: 1.35;
}

.wf-clip {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--quiet);
}

.wf-tag {
  font-size: 0.66rem;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  line-height: 1.5;
}

.wf-tag.bug {
  padding: 0 0.3rem;
  border: 1px solid var(--bad);
  border-radius: 4px;
  color: var(--bad);
}

.wf-tag.yours,
.wf-slice-move.yours,
.wf-line.yours {
  color: var(--accent-text);
}

/* Five marks, one for each lane: filled when the lane is done. */
.wf-pips {
  display: inline-flex;
  gap: 3px;
  margin-left: auto;
}

.wf-pip {
  width: 0.45rem;
  height: 0.6rem;
  border: 1px solid var(--faint);
  border-radius: 2px;
}

.wf-pip.held {
  border-color: var(--accent);
  background: var(--accent);
}

/* The story, and the open slice under it. Side by side when there is room. */
.wf-work {
  position: relative;
  min-height: 0;
  overflow-y: auto;
}

.wf-pane {
  display: grid;
  align-content: start;
  gap: 0.6rem;
  padding: 1.25rem 1.5rem 2rem;
}

.wf-open {
  border-top: 1px solid var(--line);
}

.wf-pane h3:focus {
  outline: none;
}

.wf-pane h3:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 4px;
  border-radius: 4px;
}

.wf-pane h3 {
  font-family: var(--serif);
  font-size: 1.6rem;
  font-weight: 400;
  line-height: 1.2;
  letter-spacing: -0.01em;
}

.wf-open h3 {
  font-size: 1.3rem;
}

.wf-pane .label {
  padding: 0;
}

.wf-pane h4 {
  margin: 0;
  font-weight: 400;
}

.wf-detail {
  max-width: 44rem;
}

/* The hand: a card for each lane, face up once every slice is through it.
   Five in a row at any width: a card's text sizes itself to the hand. */
.wf-hand {
  container-type: inline-size;
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 0.4rem;
  margin-top: 0.5rem;
}

.wf-card {
  display: flex;
  flex-direction: column;
  gap: 0.05rem;
  min-width: 0;
  padding: 0.5rem clamp(0.4rem, 1.6cqi, 0.6rem) 0.6rem;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--raised);
  font-size: 0.8rem;
}

.wf-card.held {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 10%, var(--raised));
}

.wf-rank {
  font-family: var(--serif);
  font-size: 1.7rem;
  line-height: 1.1;
  color: var(--faint);
}

.wf-card.held .wf-rank {
  color: var(--accent-text);
}

.wf-card-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 600;
  font-size: clamp(0.74rem, 3cqi, 0.87rem);
}

.wf-card-count {
  margin-top: auto;
  padding-top: 0.3rem;
  font-family: var(--mono);
  font-size: 0.72rem;
}

.wf-line {
  font-weight: 600;
}

.wf-line.flush {
  color: var(--good);
}

.wf-block {
  display: grid;
  gap: 0.4rem;
  margin-top: 0.9rem;
}

.wf-slices {
  display: grid;
  gap: 0.3rem;
}

.wf-slice {
  display: grid;
  grid-template-columns: 1.1rem minmax(0, 1fr) auto;
  align-items: center;
  gap: 0.1rem 0.6rem;
  width: 100%;
  padding: 0.5rem 0.7rem;
  background: var(--raised);
  text-align: left;
}

.wf-slice[aria-current="true"] {
  border-color: var(--accent);
}

.wf-slice-move {
  grid-column: 2 / -1;
  color: var(--quiet);
}

.wf-notes {
  display: grid;
  gap: 0.3rem;
}

.wf-notes li {
  padding-left: 0.75rem;
  border-left: 2px solid var(--line);
}

.wf-pane textarea {
  width: 100%;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--raised);
  padding: 0.7rem 0.9rem;
  outline: none;
  resize: vertical;
}

.wf-pane textarea:focus {
  border-color: var(--faint);
}

.wf-pane textarea:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.wf-note-foot {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
}

.wf-saved {
  min-height: 1.2rem;
  color: var(--good);
}

.wf-brief {
  padding: 0.75rem 0.9rem;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--raised);
  font-family: var(--mono);
  font-size: 0.8rem;
  line-height: 1.7;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.wf-steps {
  display: grid;
  gap: 0.5rem;
}

.wf-runner {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
}

.wf-runner select {
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--page);
  padding: 0.3rem 0.5rem;
  max-width: 100%;
}

.wf-clock {
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
}

.wf-moves {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-top: 0.75rem;
}

.wf-steps li {
  display: grid;
  grid-template-columns: 1.1rem minmax(0, 1fr) auto;
  gap: 0 0.5rem;
  align-items: baseline;
}

.wf-step-mark {
  font-weight: 700;
  color: var(--accent-text);
}

.wf-steps li.done .wf-step-mark {
  color: var(--good);
}

.wf-step-name {
  font-weight: 600;
}

.wf-steps li.later .wf-step-name {
  font-weight: 400;
  color: var(--quiet);
}

.wf-step-what {
  grid-column: 2 / -1;
}

.wf-empty {
  display: grid;
  justify-items: start;
  gap: 0.9rem;
  width: min(34rem, 100%);
  margin: 0 auto;
  padding: 4rem 1.5rem;
}

.wf-empty .display {
  font-size: 2rem;
  line-height: 1.15;
}

/* Room for three: the road, the story, and the open slice. */
@container workflow (min-width: 66rem) {
  .wf-work {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(17rem, 23rem);
    grid-template-rows: minmax(0, 1fr);
    overflow: hidden;
  }

  .wf-pane {
    position: relative;
    min-height: 0;
    overflow-y: auto;
  }

  .wf-open {
    border-top: 0;
    border-left: 1px solid var(--line);
  }
}

/* A narrow window: one column, the road first. */
@container workflow (max-width: 38rem) {
  .wf-cols {
    display: block;
    overflow-y: auto;
  }

  .wf-road {
    overflow: visible;
    padding-bottom: 1rem;
    border-right: 0;
    border-bottom: 1px solid var(--line);
  }

  .wf-work {
    overflow: visible;
  }
}
</style>
