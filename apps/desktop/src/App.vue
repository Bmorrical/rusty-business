<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { Channel, invoke } from "@tauri-apps/api/core";

import {
  answersLine,
  canRemove,
  chatModels,
  checkedLine,
  chipDetail,
  dayLabel,
  downloaded,
  downloadLine,
  firstRunOffers,
  formatCount,
  formatSize,
  fullest,
  gateLine,
  groups,
  keptPicks,
  listedLine,
  looksLikePath,
  meterLine,
  moreLine,
  namesLine,
  onDiskLine,
  outsideModelsLine,
  pickedMakers,
  quickest,
  requestMessages,
  savedTurn,
  shouldStart,
  showsStartHint,
  showsUpdate,
  stackLine,
  startLine,
  titleFrom,
  tokensPerSecond,
  UPDATE_EVERY,
  updateDetail,
  updateLine,
  withMaker,
  withPick,
} from "./chat.js";

// The page only works inside the app's own window, where Rust answers.
const inApp = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const engine = ref(null);
const checking = ref(false);
const starting = ref(false);
const startNote = ref("");
const stack = ref(null);
const stackStarting = ref(false);
const picked = ref([]);
const sessions = ref([]);
const current = ref(blankSession());
const draft = ref("");
const busy = ref(false);
const problem = ref("");
const askTrash = ref("");
const downloading = ref("");
const progress = ref(null);
const stopping = ref(false);
const stoppedNote = ref("");
const thread = ref(null);
const composer = ref(null);
const update = ref(null);
const updateLater = ref("");
const updateOpened = ref("");
const updateProblem = ref("");
const updateChecking = ref(false);
const updateChecked = ref("");
const gate = ref(false);
const gateBusy = ref(false);
const gateNote = ref("");
const makers = ref([]);
const makersBusy = ref(false);
const makersNote = ref("");
const outside = ref(null);
// The change to Grok under way, by the name of its command, or "".
const outsideBusy = ref("");
const outsideNote = ref("");
const outsidePath = ref("");
const settingsOpen = ref(false);
const settingsTitle = ref(null);
let updateTimer = 0;

let chatId = "";
let stopAll = false;
let checkAgain = false;
let stickToBottom = true;

function blankSession() {
  return { id: "", title: "", model: "", createdAt: 0, updatedAt: 0, messages: [] };
}

// Grok's models are offered only while the gate is on and Grok is added.
const outsideModels = computed(() => (gate.value && outside.value?.added ? (outside.value.models ?? []) : []));
const models = computed(() => [...(engine.value?.models ?? []), ...outsideModels.value]);
// The models a question goes to, in the list's order.
const chosen = computed(() => models.value.filter((entry) => picked.value.includes(entry.id)));
const many = computed(() => chosen.value.length > 1);
const top = computed(() => fullest(current.value.messages, chosen.value));
const rounds = computed(() => groups(current.value.messages));
const canSend = computed(() => Boolean(draft.value.trim()) && !busy.value && chosen.value.length > 0);
// With Grok to ask, the first run's downloads wait in Settings.
const offers = computed(() => (outsideModels.value.length ? [] : firstRunOffers(engine.value)));
const quick = computed(() => quickest(offers.value));
const handStart = computed(() => showsStartHint(engine.value, starting.value));
const makersLocked = computed(() => makersBusy.value || Boolean(downloading.value));
const engineAddress = computed(() => (engine.value?.url ?? "").replace(/^http:\/\//, "").replace(/\/v1$/, ""));

function remembered() {
  try {
    const kept = JSON.parse(window.localStorage.getItem("models") ?? "null");
    if (Array.isArray(kept)) return kept;
    // Before several models could be asked, one was kept on its own.
    const one = window.localStorage.getItem("model");
    return one ? [one] : [];
  } catch {
    return [];
  }
}

function remember(ids) {
  try {
    window.localStorage.setItem("models", JSON.stringify(ids));
  } catch {
    // The choice just is not kept for next time.
  }
}

/**
 * Picks the models a question goes to: the first of the lists given that
 * names a model on hand, then the models picked last time, then the first
 * model on hand.
 */
function pickModels(...preferred) {
  const ids = models.value.map((entry) => entry.id);
  picked.value = keptPicks(ids, ...preferred, remembered(), ids.slice(0, 1));
}

watch(picked, (ids) => {
  if (ids.length) remember(ids);
});

/**
 * "the next step is user can run the same question in 1-nth models and checks
 * the results of each one". One click adds a model to the question or takes
 * it away; the last one stays.
 */
function togglePick(id) {
  if (busy.value) return;
  const ids = models.value.map((entry) => entry.id);
  picked.value = withPick(picked.value, id, !picked.value.includes(id), ids);
}

async function checkEngine() {
  if (!inApp) return;
  if (checking.value) {
    // The check under way may have looked before whatever asked for this
    // one, such as a download that just finished, so one more follows it.
    checkAgain = true;
    return;
  }
  checking.value = true;
  try {
    engine.value = await invoke("engine_status");
    if (shouldStart(engine.value)) {
      // No engine answers, so the app starts one. Rust starts only what is
      // missing, so asking again while one is on its way up changes nothing.
      starting.value = true;
      const start = await invoke("start_engine");
      if (start.answering) engine.value = await invoke("engine_status");
      startNote.value = startLine(start, engine.value);
    } else {
      startNote.value = "";
    }
    pickModels(picked.value, chatModels(current.value));
  } catch (error) {
    problem.value = String(error);
  } finally {
    starting.value = false;
    checking.value = false;
    if (checkAgain) {
      checkAgain = false;
      checkEngine();
    }
  }
}

/**
 * "there is a hard switch / circuit breaker to hard stop all outbound
 * communication, user can turn it on to use their inference", and "off by
 * default, everything stops". Rust keeps the gate and holds every connection
 * past this computer to it; the page only shows it and turns it on or off.
 */
async function readGate() {
  try {
    gate.value = (await invoke("gate_status")) === true;
  } catch {
    gate.value = false;
  }
}

async function setGate(open) {
  if (gateBusy.value) return;
  gateBusy.value = true;
  gateNote.value = "";
  try {
    gate.value = (await invoke("set_gate", { open })) === true;
  } catch (error) {
    // The gate changed but could not be kept for next time; Rust says so.
    gateNote.value = String(error);
    await readGate();
  } finally {
    gateBusy.value = false;
  }
  // Grok's models come and go with the gate.
  pickModels(picked.value);
  // With the gate on, what waited for it goes ahead, and a line saying the
  // gate is off is gone.
  if (gate.value) {
    updateChecked.value = "";
    checkStack();
    checkUpdate();
    if (outside.value?.added && !outside.value.models?.length) refreshOutside();
  }
}

/**
 * "user can add their outside inference, start with Grok and Grok ai. user
 * adds it manually, they will need to provide or specify a path to a key".
 * Rust keeps the path, reads the key from it each time Grok is asked, and
 * never hands the key to the page. The page sends only the path typed.
 */
async function readOutside() {
  try {
    outside.value = await invoke("outside_status");
  } catch (error) {
    outsideNote.value = String(error);
  }
}

/** Asks Rust to change Grok. Gives back whether Rust took the change. */
async function changeOutside(command, args) {
  if (outsideBusy.value) return false;
  outsideBusy.value = command;
  outsideNote.value = "";
  let taken = false;
  try {
    outside.value = await invoke(command, args);
    // With the gate off, the line above Grok already says what waits for it.
    outsideNote.value = gate.value ? (outside.value?.error ?? "") : "";
    taken = true;
  } catch (error) {
    // Nothing changed; Rust says why.
    outsideNote.value = String(error);
  } finally {
    outsideBusy.value = "";
  }
  // The models a question goes to follow Grok's.
  pickModels(picked.value);
  return taken;
}

async function addOutside() {
  const given = outsidePath.value.trim();
  if (!given) return;
  const added = await changeOutside("add_outside", { keyPath: given });
  // A key pasted where the path goes does not stay on screen.
  if (added || !looksLikePath(given)) outsidePath.value = "";
}

function refreshOutside() {
  return changeOutside("refresh_outside");
}

function removeOutside() {
  return changeOutside("remove_outside");
}

/**
 * "user can pick google, gemini, meta from the app if they want those instead.
 * I think the user can pick one or more at install with microsoft default, and
 * in settings they can one click more of them if they want." Rust keeps the
 * makers picked and reports only their models; the page shows the list and
 * asks Rust to change it. None of this leaves the computer.
 */
async function readMakers() {
  try {
    makers.value = await invoke("makers_status");
  } catch (error) {
    makersNote.value = String(error);
  }
}

async function setMaker(name, on, box) {
  // A checkbox shows what Rust kept. Vue leaves a box alone when the pick did
  // not change, so a refused click would otherwise stay ticked.
  const showKept = () => {
    if (box) box.checked = pickedMakers(makers.value).includes(name);
  };
  if (makersLocked.value) return showKept();
  makersBusy.value = true;
  makersNote.value = "";
  try {
    makers.value = await invoke("set_makers", { picked: withMaker(makers.value, name, on) });
  } catch (error) {
    // Nothing changed; Rust says why.
    makersNote.value = String(error);
    await readMakers();
  } finally {
    makersBusy.value = false;
    showKept();
  }
  // The models shown follow the makers picked.
  await checkEngine();
}

async function openSettings() {
  settingsOpen.value = true;
  nextTick(() => settingsTitle.value?.focus());
  await readMakers();
  await readOutside();
}

function closeSettings() {
  settingsOpen.value = false;
  focusComposer();
}

/**
 * Asks the app to start the stack. Rust starts only what is missing and stops
 * only what it started, when the app quits, so this runs when the window
 * opens and from Try again, never on focus: a first start downloads the
 * stack, which takes a few minutes.
 */
async function checkStack() {
  if (!inApp || stackStarting.value) return;
  stackStarting.value = true;
  try {
    stack.value = await invoke("start_stack");
  } catch (error) {
    stack.value = { running: false, started: false, address: "", reason: String(error) };
  } finally {
    stackStarting.value = false;
  }
}

/**
 * "when a new build is ready, the user is notified in app, and it gives them
 * the download". Rust asks GitHub for the newest release and answers with the
 * new version, or none. A check that fails shows nothing: the next one, a day
 * later or when the window opens again, tries again.
 */
async function checkUpdate() {
  if (!inApp) return;
  try {
    update.value = await invoke("check_update");
  } catch {
    // Nothing to show; the notice stays as it was.
  }
}

/**
 * "is the user able to click a refresh button? If not, I want that.", then
 * "check for updates I imply". The Check for updates button asks now and says
 * what it found: the new version, which brings back a notice put off with
 * Later, that this is the newest, or why the check failed.
 */
async function checkNow() {
  if (!inApp || updateChecking.value) return;
  updateChecking.value = true;
  updateChecked.value = "";
  try {
    update.value = await invoke("check_update");
    updateLater.value = "";
    updateChecked.value = checkedLine(update.value, await getVersion());
  } catch (error) {
    updateChecked.value = String(error);
  } finally {
    updateChecking.value = false;
  }
}

/**
 * Opens the new version's download, or its notes, in the browser. The page
 * names only which: Rust builds the address from what it found.
 */
async function openUpdate(part) {
  updateProblem.value = "";
  try {
    await invoke("open_update", { part });
    if (part === "download") updateOpened.value = update.value?.version ?? "";
  } catch (error) {
    updateProblem.value = String(error);
  }
}

/**
 * Downloads one model from the app's list, then starts the engine with it.
 * Rust checks every file as it arrives and moves the model into place only
 * when all of it is there.
 */
async function download(offer) {
  if (downloading.value) return;
  downloading.value = offer.id;
  progress.value = { done: 0, total: offer.bytes };
  stoppedNote.value = "";
  problem.value = "";
  const channel = new Channel();
  channel.onmessage = (update) => {
    progress.value = update;
  };
  try {
    const report = await invoke("download_model", { id: offer.id, onProgress: channel });
    if (report.stopped) stoppedNote.value = `The download of ${offer.name} stopped, and none of it was kept.`;
    // With the model in place, the engine can start with it.
    if (report.whole) await Promise.all([checkEngine(), readMakers()]);
  } catch (error) {
    problem.value = String(error);
  } finally {
    downloading.value = "";
    progress.value = null;
    stopping.value = false;
  }
}

async function cancelDownload() {
  stopping.value = true;
  try {
    await invoke("stop_download");
  } catch (error) {
    problem.value = String(error);
    stopping.value = false;
  }
}

async function refreshSessions() {
  try {
    sessions.value = await invoke("list_sessions");
  } catch (error) {
    problem.value = String(error);
  }
}

/** Saves the open chat. Only the fields Rust sets come back into it. */
async function persist() {
  const session = current.value;
  try {
    const saved = await invoke("save_session", {
      session: { ...session, messages: session.messages.map(savedTurn) },
    });
    session.id = saved.id;
    session.title = saved.title;
    session.createdAt = saved.createdAt;
    session.updatedAt = saved.updatedAt;
    await refreshSessions();
  } catch (error) {
    problem.value = String(error);
  }
}

function newChat() {
  if (busy.value) return;
  current.value = blankSession();
  settingsOpen.value = false;
  problem.value = "";
  askTrash.value = "";
  focusComposer();
}

async function openSession(id) {
  if (busy.value) return;
  settingsOpen.value = false;
  if (id === current.value.id) return;
  problem.value = "";
  askTrash.value = "";
  try {
    current.value = await invoke("load_session", { id });
    pickModels(chatModels(current.value), picked.value);
    stickToBottom = true;
    scrollDown();
    focusComposer();
  } catch (error) {
    problem.value = String(error);
  }
}

async function trashSession(id) {
  askTrash.value = "";
  try {
    await invoke("trash_session", { id });
    if (current.value.id === id) current.value = blankSession();
    await refreshSessions();
  } catch (error) {
    problem.value = String(error);
  }
}

/**
 * Asks each model picked the same question, in the list's order. "if they
 * can't run asychronos then the models can iterate on the question": the
 * engine holds one model at a time, so the models here take turns, each
 * after the one before it, and Rust keeps any two from running at once. Each
 * model reads the chat with its own earlier answers.
 */
async function send() {
  if (!canSend.value) return;
  const text = draft.value.trim();
  const session = current.value;
  const asked = chosen.value;
  problem.value = "";
  if (!session.messages.length) session.title = titleFrom(text);
  session.model = asked[0].id;
  session.messages.push({ role: "user", content: text });
  // What each model reads: the chat up to this question, and no other
  // model's answer to it.
  const history = session.messages.slice();
  draft.value = "";
  busy.value = true;
  stopAll = false;
  stickToBottom = true;
  resizeComposer();
  scrollDown();
  // Saved before the models answer, so the question survives a crash.
  await persist();

  const start = session.messages.length;
  for (const entry of asked) {
    session.messages.push({ role: "assistant", content: "", thinking: "", model: entry.id, pending: true, waiting: true });
  }
  const replies = session.messages.slice(start);
  const failed = [];
  for (const [index, entry] of asked.entries()) {
    // Stop ends the reply under way, and the models still waiting are not asked.
    if (stopAll) break;
    const reply = replies[index];
    reply.waiting = false;
    const error = await ask(reply, entry, requestMessages(history, entry.id));
    // One model failing does not keep the next from answering.
    if (error) failed.push(`${entry.name}: ${error}`);
  }
  for (const reply of replies) {
    reply.pending = false;
    reply.waiting = false;
  }
  // A reply that never began is not kept.
  session.messages = session.messages.filter((turn) => !replies.includes(turn) || turn.content || turn.thinking);
  busy.value = false;
  if (failed.length) {
    problem.value = failed.join("\n");
    checkEngine();
  }
  await persist();
  scrollDown();
  focusComposer();
}

/** Streams one model's reply into `reply`. Gives back why it failed, or nothing. */
async function ask(reply, entry, messages) {
  let finished = false;
  const channel = new Channel();
  channel.onmessage = (event) => {
    if (finished) return;
    if (event.kind === "thinking") reply.thinking += event.text;
    else if (event.kind === "answer") reply.content += event.text;
    else if (event.kind === "reset") {
      reply.thinking = event.thinking;
      reply.content = event.answer;
    }
    scrollDown();
  };
  chatId = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  try {
    const done = await invoke("chat_stream", {
      chatId,
      request: { model: entry.id, messages, maxTokens: null },
      onEvent: channel,
    });
    finished = true;
    reply.content = done.answer;
    reply.thinking = done.thinking;
    reply.promptTokens = done.promptTokens ?? undefined;
    reply.completionTokens = done.completionTokens ?? undefined;
    reply.seconds = done.seconds || undefined;
    reply.stopped = done.stopped;
    reply.cutShort = done.finishReason === "length";
    return "";
  } catch (error) {
    finished = true;
    return String(error);
  } finally {
    reply.pending = false;
    chatId = "";
  }
}

function stop() {
  if (!busy.value) return;
  stopAll = true;
  if (chatId) invoke("stop_chat", { chatId });
}

function onKeydown(event) {
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    send();
  }
}

function resizeComposer() {
  nextTick(() => {
    const box = composer.value;
    if (!box) return;
    box.style.height = "auto";
    box.style.height = `${Math.min(box.scrollHeight, 220)}px`;
  });
}

function focusComposer() {
  nextTick(() => composer.value?.focus());
}

function onScroll() {
  const pane = thread.value;
  if (pane) stickToBottom = pane.scrollHeight - pane.scrollTop - pane.clientHeight < 60;
}

function scrollDown() {
  if (!stickToBottom) return;
  nextTick(() => {
    const pane = thread.value;
    if (pane) pane.scrollTop = pane.scrollHeight;
  });
}

function modelName(id) {
  // A model whose maker was removed is set aside, not offered, but a saved
  // chat still names the model that wrote it.
  // So does a chat Grok answered after Grok is removed.
  const known = [...models.value, ...(engine.value?.setAside ?? []), ...(outside.value?.models ?? [])];
  return known.find((entry) => entry.id === id)?.name ?? "";
}

/** A saved chat's models: "Phi-4", or "Phi-4 and 2 more". */
function sessionModels(entry) {
  const ids = entry.models?.length ? entry.models : [entry.model];
  return moreLine(ids.map(modelName)) || "model not on hand";
}

/** Under a reply: its model, unless a card above it names it, and its counts. */
function replyNote(turn, named = true) {
  const parts = [named ? modelName(turn.model) : ""];
  if (turn.completionTokens) parts.push(`${formatCount(turn.completionTokens)} tokens`);
  const speed = tokensPerSecond(turn);
  if (speed) parts.push(`${formatCount(speed)} a second`);
  if (turn.cutShort) parts.push("reached the length limit");
  else if (turn.stopped) parts.push("stopped");
  return parts.filter(Boolean).join(" · ");
}

onMounted(async () => {
  if (!inApp) return;
  window.addEventListener("focus", checkEngine);
  // While the gate is off, Rust starts no stack and asks for no new version.
  await readGate();
  // So is the path to Grok's key, and the models found with it.
  await readOutside();
  // The makers picked are kept on this computer, so they need no gate.
  await readMakers();
  // The stack can take minutes, so the page does not wait for it.
  checkStack();
  // So does the check for a new version, which asks GitHub.
  checkUpdate();
  updateTimer = window.setInterval(checkUpdate, UPDATE_EVERY);
  await Promise.all([checkEngine(), refreshSessions()]);
  focusComposer();
});

onBeforeUnmount(() => {
  window.removeEventListener("focus", checkEngine);
  window.clearInterval(updateTimer);
});
</script>

<template>
  <div v-if="!inApp" class="outside">
    <h1>Rusty Business™</h1>
    <p>This page runs inside the Rusty Business™ window. Start it with <code>npm run tauri dev</code>.</p>
  </div>

  <div v-else class="app">
    <aside class="side">
      <div class="side-head">
        <h1>Rusty Business™</h1>
        <button class="primary wide" type="button" :disabled="busy" @click="newChat">New chat</button>
      </div>

      <nav class="sessions" aria-label="Saved chats">
        <p v-if="!sessions.length" class="quiet pad">Chats are saved here as you go.</p>
        <ul v-else>
          <li v-for="entry in sessions" :key="entry.id" :class="{ open: entry.id === current.id }">
            <template v-if="askTrash === entry.id">
              <span class="ask">Move to the trash?</span>
              <button class="link" type="button" @click="trashSession(entry.id)">Yes</button>
              <button class="link" type="button" @click="askTrash = ''">No</button>
            </template>
            <template v-else>
              <button class="session" type="button" :disabled="busy" @click="openSession(entry.id)">
                <span class="session-title">{{ entry.title }}</span>
                <span class="quiet small">{{ dayLabel(entry.updatedAt) }} · {{ sessionModels(entry) }}</span>
              </button>
              <button
                class="trash"
                type="button"
                :disabled="busy"
                :aria-label="`Move ${entry.title} to the trash`"
                title="Move to the trash"
                @click="askTrash = entry.id"
              >
                ×
              </button>
            </template>
          </li>
        </ul>
      </nav>

      <footer class="engine" aria-live="polite">
        <template v-if="!engine">
          <p class="quiet">Looking for the engine...</p>
        </template>
        <template v-else-if="engine.reachable">
          <p><span class="dot on" aria-hidden="true"></span>{{ engine.name }} on {{ engineAddress }}</p>
          <p v-if="engine.hidden" class="quiet small">
            {{ engine.hidden }} other {{ engine.hidden === 1 ? "model" : "models" }} on this engine
            {{ engine.hidden === 1 ? "is" : "are" }} not on this app's list.
          </p>
        </template>
        <template v-else>
          <p><span class="dot off" aria-hidden="true"></span>{{ engine.error }}</p>
          <p class="quiet small">{{ onDiskLine(engine.onDisk) }}</p>
          <p v-if="starting" class="quiet small">Starting the engine...</p>
          <p v-if="!starting && startNote" class="small">{{ startNote }}</p>
          <p v-if="handStart" class="quiet small">To start it yourself:</p>
          <code v-if="handStart" class="hint">{{ engine.startHint }}</code>
          <button v-if="!starting" class="wide" type="button" :disabled="checking" @click="checkEngine">Try again</button>
        </template>
        <p v-if="stackStarting" class="quiet small">{{ stackLine(stack, true) }}</p>
        <p v-else-if="stack?.running"><span class="dot on" aria-hidden="true"></span>{{ stackLine(stack, false) }}</p>
        <template v-else-if="stack">
          <p id="stack-reason"><span class="dot off" aria-hidden="true"></span>{{ stackLine(stack, false) }}</p>
          <button v-if="gate" class="wide" type="button" aria-describedby="stack-reason" @click="checkStack">Try again</button>
        </template>
        <button
          class="wide"
          type="button"
          :aria-current="settingsOpen ? 'page' : undefined"
          @click="settingsOpen ? closeSettings() : openSettings()"
        >
          Settings
        </button>
        <button class="wide" type="button" :disabled="updateChecking" @click="checkNow">
          {{ updateChecking ? "Checking..." : "Check for updates" }}
        </button>
        <p v-if="updateChecked" class="small">{{ updateChecked }}</p>
        <div class="gate">
          <button
            class="switch"
            type="button"
            role="switch"
            :aria-checked="gate"
            aria-describedby="gate-line"
            :disabled="gateBusy"
            @click="setGate(!gate)"
          >
            <span class="track" aria-hidden="true"><span class="knob"></span></span>
            Outbound gate
          </button>
          <p id="gate-line" class="quiet small">{{ gateLine(gate) }}</p>
          <p v-if="gateNote" class="small" role="alert">{{ gateNote }}</p>
        </div>
      </footer>
    </aside>

    <main class="main">
      <section v-if="showsUpdate(update, updateLater)" class="update" role="status" aria-label="New version">
        <div class="update-words">
          <p>
            <strong>{{ updateLine(update) }}</strong>
            <span class="quiet small">{{ updateDetail(update) }}</span>
          </p>
          <template v-if="updateOpened === update.version">
            <p class="small">{{ update.steps }}</p>
            <code v-if="update.command" class="hint small">{{ update.command }}</code>
            <p class="quiet small">If no download started, it is on {{ update.page }}.</p>
          </template>
          <p v-if="updateProblem" class="update-problem small" role="alert">{{ updateProblem }}</p>
        </div>
        <div class="update-buttons">
          <button class="primary" type="button" @click="openUpdate('download')">Download</button>
          <button type="button" @click="openUpdate('notes')">What changed</button>
          <button class="link" type="button" @click="updateLater = update.version">Later</button>
        </div>
      </section>
      <section v-if="settingsOpen" class="settings" aria-labelledby="settings-title" @keydown.esc="closeSettings">
        <header class="bar">
          <h2 id="settings-title" ref="settingsTitle" tabindex="-1">Settings</h2>
          <button type="button" @click="closeSettings">Back to the chat</button>
        </header>
        <div class="settings-body">
          <h3>Model makers</h3>
          <p class="quiet">
            The app shows the models of the makers you pick. Removing a maker leaves its models on this computer.
          </p>
          <p v-if="!gate" class="gate-ask">
            <span>The outbound gate is off, so nothing can download yet.</span>
            <button type="button" :disabled="gateBusy" @click="setGate(true)">Turn on the gate</button>
          </p>
          <ul class="makers" aria-label="Model makers">
            <li v-for="maker in makers" :key="maker.name" class="maker">
              <div class="maker-head">
                <span class="maker-name">{{ maker.name }}</span>
                <button
                  v-if="maker.picked"
                  type="button"
                  :aria-label="`Remove ${maker.name}`"
                  :disabled="makersLocked || !canRemove(makers, maker.name)"
                  @click="setMaker(maker.name, false)"
                >
                  Remove
                </button>
                <button
                  v-else
                  class="primary"
                  type="button"
                  :aria-label="`Add ${maker.name}`"
                  :disabled="makersLocked"
                  @click="setMaker(maker.name, true)"
                >
                  Add
                </button>
              </div>
              <ul class="maker-models">
                <li v-for="entry in maker.models" :key="entry.name" class="offer">
                  <span class="offer-text">
                    <span class="offer-name">{{ entry.name }}</span>
                    <span class="quiet small">{{ listedLine(entry) }}</span>
                  </span>
                  <template v-if="entry.offer && downloading === entry.offer.id">
                    <span class="offer-progress">
                      <progress
                        :value="downloaded(progress)"
                        max="1"
                        :aria-label="`How much of ${entry.name} has downloaded`"
                      ></progress>
                      <span class="quiet small">{{ stopping ? "Stopping..." : downloadLine(progress) }}</span>
                    </span>
                    <button type="button" :disabled="stopping" @click="cancelDownload">Cancel</button>
                  </template>
                  <button
                    v-else-if="maker.picked && entry.offer && !entry.onDisk"
                    type="button"
                    :aria-label="`Download ${entry.name}`"
                    :disabled="Boolean(downloading) || !gate"
                    @click="download(entry.offer)"
                  >
                    Download
                  </button>
                </li>
              </ul>
            </li>
          </ul>
          <p v-if="pickedMakers(makers).length === 1" class="quiet small">One maker always stays picked.</p>
          <p v-if="makersNote" class="small" role="alert">{{ makersNote }}</p>
          <p v-if="stoppedNote" class="quiet small">{{ stoppedNote }}</p>

          <h3 class="gap-above">Outside engines</h3>
          <p class="quiet">
            Grok, by xAI, answers from xAI's computers, not this one. Give the path to a file that holds your xAI key.
            Rusty Business™ reads the key from that file when it asks Grok and sends it only to xAI's API, while the gate
            is on. The key never shows here.
          </p>
          <p v-if="!gate" class="gate-ask">
            <span>The outbound gate is off, so Grok cannot be asked.</span>
            <button type="button" :disabled="gateBusy" @click="setGate(true)">Turn on the gate</button>
          </p>
          <div class="engine-card" aria-labelledby="grok-name" role="group">
            <div class="maker-head">
              <span id="grok-name" class="maker-name">Grok</span>
              <span v-if="outside?.added" class="engine-buttons">
                <button type="button" :disabled="Boolean(outsideBusy) || !gate" @click="refreshOutside">
                  {{ outsideBusy === "refresh_outside" ? "Asking xAI..." : "Refresh models" }}
                </button>
                <button type="button" aria-label="Remove Grok" :disabled="Boolean(outsideBusy)" @click="removeOutside">
                  Remove
                </button>
              </span>
            </div>
            <template v-if="outside?.added">
              <p class="small key-file">Key file: <code>{{ outside.keyPath }}</code></p>
              <p v-if="outside.warning" class="small" role="alert">{{ outside.warning }}</p>
              <p class="quiet small">{{ outsideModelsLine(outside) }}</p>
            </template>
            <form v-else class="key-path" @submit.prevent="addOutside">
              <label for="xai-key-path" class="small">Path to your xAI key file</label>
              <span class="key-path-row">
                <input
                  id="xai-key-path"
                  v-model="outsidePath"
                  type="text"
                  placeholder="~/.config/xai/key"
                  spellcheck="false"
                  autocomplete="off"
                  autocapitalize="off"
                  autocorrect="off"
                  :disabled="Boolean(outsideBusy)"
                />
                <button class="primary" type="submit" :disabled="Boolean(outsideBusy) || !outsidePath.trim()">
                  {{ outsideBusy === "add_outside" ? "Adding..." : "Add" }}
                </button>
              </span>
            </form>
          </div>
          <p v-if="outsideNote" class="small" role="alert">{{ outsideNote }}</p>
        </div>
      </section>
      <header v-show="!settingsOpen" class="bar">
        <h2>{{ current.title || "New chat" }}</h2>
        <div class="picks" role="group" aria-label="Models that answer">
          <span v-if="models.length" class="quiet small picks-label">Ask</span>
          <span v-else class="quiet small">No model on hand</span>
          <button
            v-for="entry in models"
            :key="entry.id"
            class="chip"
            type="button"
            :aria-pressed="picked.includes(entry.id)"
            :disabled="busy"
            :title="picked.length === 1 && picked[0] === entry.id ? 'One model always stays picked.' : undefined"
            @click="togglePick(entry.id)"
          >
            <span class="chip-mark" aria-hidden="true">{{ picked.includes(entry.id) ? "✓" : "+" }}</span>
            {{ entry.name }}
            <span class="quiet small chip-detail">{{ chipDetail(entry) }}</span>
          </button>
        </div>
      </header>

      <section v-show="!settingsOpen" ref="thread" class="thread" @scroll="onScroll">
        <div v-if="!current.messages.length" class="empty">
          <div v-if="offers.length" class="first-run">
            <p class="lead">Pick a model to start.</p>
            <p class="quiet">Each one is a single download from Hugging Face. Nothing downloads until you press Download.</p>
            <fieldset class="makers-pick" :disabled="makersLocked">
              <legend class="quiet small">Show models from</legend>
              <label v-for="maker in makers" :key="maker.name" class="maker-check">
                <input
                  type="checkbox"
                  :checked="maker.picked"
                  :disabled="maker.picked && !canRemove(makers, maker.name)"
                  @change="setMaker(maker.name, $event.target.checked, $event.target)"
                />
                {{ maker.name }}
              </label>
            </fieldset>
            <p v-if="makersNote" class="small" role="alert">{{ makersNote }}</p>
            <p v-if="!gate" class="gate-ask">
              <span>The outbound gate is off, so nothing can download yet.</span>
              <button class="primary" type="button" :disabled="gateBusy" @click="setGate(true)">Turn on the gate</button>
            </p>
            <ul class="offers" aria-label="Models to download">
              <li v-for="offer in offers" :key="offer.id" class="offer">
                <span class="offer-text">
                  <span class="offer-name">{{ offer.name }}</span>
                  <span class="quiet small">
                    by {{ offer.maker }} · {{ offer.license }} · {{ formatSize(offer.bytes) }}{{
                      offer.id === quick ? " · quickest to start" : ""
                    }}
                  </span>
                </span>
                <template v-if="downloading === offer.id">
                  <span class="offer-progress">
                    <progress
                      :value="downloaded(progress)"
                      max="1"
                      :aria-label="`How much of ${offer.name} has downloaded`"
                    ></progress>
                    <span class="quiet small">{{ stopping ? "Stopping..." : downloadLine(progress) }}</span>
                  </span>
                  <button type="button" :disabled="stopping" @click="cancelDownload">Cancel</button>
                </template>
                <button
                  v-else
                  :class="{ primary: gate && offer.id === quick }"
                  type="button"
                  :disabled="Boolean(downloading) || !gate"
                  @click="download(offer)"
                >
                  Download
                </button>
              </li>
            </ul>
            <p v-if="stoppedNote" class="quiet small">{{ stoppedNote }}</p>
          </div>
          <template v-else>
            <p class="lead">Ask anything.</p>
            <p v-if="chosen.length" class="quiet">{{ answersLine(chosen) }}</p>
            <p v-else-if="starting" class="quiet">Starting the engine...</p>
            <p v-else-if="engine?.reachable" class="quiet">The engine has none of this app's models on hand.</p>
          </template>
        </div>

        <article v-for="round in rounds" :key="round.key" class="round">
          <div v-if="round.question" class="turn user">
            <p class="bubble">{{ round.question.content }}</p>
          </div>
          <div v-if="round.answers.length" class="answers" :class="{ many: round.answers.length > 1 }">
            <section v-for="(turn, index) in round.answers" :key="index" class="turn assistant">
              <h3 v-if="round.answers.length > 1" class="card-head">{{ modelName(turn.model) || "A model not on hand" }}</h3>
              <details v-if="turn.thinking" class="thinking">
                <summary>{{ turn.pending && !turn.content ? "Thinking..." : "Thinking" }}</summary>
                <p>{{ turn.thinking }}</p>
              </details>
              <p v-if="turn.content" class="answer">{{ turn.content }}</p>
              <p v-else-if="turn.waiting" class="quiet waiting">Waits its turn.</p>
              <p v-else-if="turn.pending && !turn.thinking" class="quiet waiting">Waiting for the model...</p>
              <p v-if="!turn.pending" class="quiet small note">{{ replyNote(turn, round.answers.length === 1) }}</p>
            </section>
          </div>
        </article>
      </section>

      <!-- Shown under Settings too, where a download can fail. -->
      <p v-if="problem" class="problem" role="alert">
        {{ problem }}
        <button class="link" type="button" @click="problem = ''">Dismiss</button>
      </p>

      <footer v-show="!settingsOpen" class="compose">
        <div class="box">
          <textarea
            ref="composer"
            v-model="draft"
            rows="1"
            aria-label="Your message"
            :placeholder="chosen.length ? `Message ${namesLine(chosen.map((entry) => entry.name))}` : 'No model on hand'"
            :disabled="!chosen.length"
            @input="resizeComposer"
            @keydown="onKeydown"
          ></textarea>
          <button v-if="busy" type="button" @click="stop">Stop</button>
          <button v-else class="primary" type="button" :disabled="!canSend" @click="send">Send</button>
        </div>
        <div class="meter">
          <template v-if="top">
            <progress
              :value="top.full"
              max="1"
              :class="{ near: top.full >= 0.8 }"
              :aria-label="many ? 'How full the fullest window is' : 'How full the model\'s window is'"
            ></progress>
            <span class="quiet small">
              {{ meterLine(top, many) }}
              <template v-if="top.full >= 0.8"> · nearly full, start a new chat soon</template>
            </span>
          </template>
          <span v-else class="quiet small">Return sends. Shift and Return starts a new line.</span>
        </div>
      </footer>
    </main>
  </div>
</template>

<style>
:root {
  --page: #f6f5f2;
  --panel: #ffffff;
  --side: #eeece7;
  --ink: #1f1d1a;
  --quiet: #6b665e;
  --line: #dcd8d0;
  --accent: #a4441c;
  --accent-ink: #ffffff;
  --bubble: #e9e5dc;
  --good: #2f7d4f;
  --bad: #b3261e;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  font-size: 15px;
  line-height: 1.5;
  color: var(--ink);
  background: var(--page);
  color-scheme: light dark;
}

@media (prefers-color-scheme: dark) {
  :root {
    --page: #1b1a18;
    --panel: #232220;
    --side: #151413;
    --ink: #ece9e3;
    --quiet: #a09a90;
    --line: #37342f;
    --accent: #e0733f;
    --accent-ink: #1b1a18;
    --bubble: #33302b;
    --good: #5cba82;
    --bad: #f2867f;
  }
}

* {
  box-sizing: border-box;
}

html,
body,
#app {
  height: 100%;
  margin: 0;
}

body {
  background: var(--page);
  overflow: hidden;
}

h1,
h2,
p,
ul {
  margin: 0;
}

h1 {
  font-size: 1rem;
  font-weight: 650;
}

h2 {
  font-size: 1rem;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

button,
select,
textarea {
  font: inherit;
  color: inherit;
}

button {
  border: 1px solid var(--line);
  background: var(--panel);
  border-radius: 8px;
  padding: 0.4rem 0.9rem;
  cursor: pointer;
}

button:disabled {
  opacity: 0.5;
  cursor: default;
}

button.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-ink);
  font-weight: 600;
}

button.wide {
  width: 100%;
}

button.link {
  border: 0;
  background: none;
  padding: 0 0.25rem;
  color: var(--accent);
  text-decoration: underline;
}

:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.quiet {
  color: var(--quiet);
}

.small {
  font-size: 0.8rem;
}

.pad {
  padding: 0.75rem 1rem;
}

code {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.85em;
}

.outside {
  max-width: 32rem;
  margin: 4rem auto;
  padding: 0 1rem;
}

.app {
  display: grid;
  grid-template-columns: 16.5rem minmax(0, 1fr);
  height: 100%;
}

/* The side: new chat, saved chats, the engine. */
.side {
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--side);
  border-right: 1px solid var(--line);
}

.side-head {
  display: grid;
  gap: 0.75rem;
  padding: 1rem;
}

/* The saved chats keep room for one and a half chats, however tall the
   footer under them grows. */
.sessions {
  flex: 1;
  min-height: 5rem;
  overflow-y: auto;
}

.sessions ul {
  list-style: none;
  padding: 0 0.5rem;
}

.sessions li {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  border-radius: 8px;
  min-height: 3.1rem;
}

.sessions li.open {
  background: var(--bubble);
}

.session {
  flex: 1;
  min-width: 0;
  display: grid;
  text-align: left;
  border: 0;
  background: none;
  padding: 0.4rem 0.5rem;
}

.session-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.trash {
  border: 0;
  background: none;
  padding: 0.2rem 0.55rem;
  color: var(--quiet);
  font-size: 1.1rem;
  line-height: 1;
  visibility: hidden;
}

.sessions li:hover .trash,
.trash:focus-visible {
  visibility: visible;
}

.ask {
  flex: 1;
  padding-left: 0.5rem;
  font-size: 0.9rem;
}

/* On a short window the footer scrolls, so the gate's switch at its foot can
   always be reached. */
.engine {
  display: grid;
  gap: 0.4rem;
  min-height: 0;
  overflow-y: auto;
  padding: 0.9rem 1rem;
  border-top: 1px solid var(--line);
  font-size: 0.9rem;
}

.dot {
  display: inline-block;
  width: 0.55rem;
  height: 0.55rem;
  border-radius: 50%;
  margin-right: 0.45rem;
}

.dot.on {
  background: var(--good);
}

.dot.off {
  background: var(--bad);
}

/* The outbound gate: a switch, off until the person turns it on. */
.gate {
  display: grid;
  gap: 0.25rem;
  padding-top: 0.5rem;
  border-top: 1px solid var(--line);
}

button.switch {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  width: 100%;
  padding: 0.3rem 0;
  border: 0;
  background: none;
  font-weight: 600;
  text-align: left;
}

.track {
  position: relative;
  flex: none;
  width: 2.2rem;
  height: 1.25rem;
  border-radius: 999px;
  background: var(--line);
  transition: background-color 0.15s;
}

.knob {
  position: absolute;
  top: 0.15rem;
  left: 0.15rem;
  width: 0.95rem;
  height: 0.95rem;
  border-radius: 50%;
  background: var(--panel);
  box-shadow: 0 1px 2px rgb(0 0 0 / 0.3);
  transition: transform 0.15s;
}

.switch[aria-checked="true"] .track {
  background: var(--good);
}

.switch[aria-checked="true"] .knob {
  transform: translateX(0.95rem);
}

@media (prefers-reduced-motion: reduce) {
  .track,
  .knob {
    transition: none;
  }
}

.gate-ask {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.5rem 0.75rem;
  margin: 0;
}

.hint {
  display: block;
  padding: 0.4rem 0.5rem;
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 6px;
  user-select: all;
  overflow-wrap: anywhere;
}

/* The chat. */
.main {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--panel);
}

.bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem 1rem;
  padding: 0.7rem 1.25rem;
  border-bottom: 1px solid var(--line);
}

/* The notice of a new version, above the chat's title. */
.update {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 0.6rem 1rem;
  padding: 0.7rem 1.25rem;
  border-bottom: 1px solid var(--line);
  border-left: 3px solid var(--accent);
  background: var(--page);
}

.update-words {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  min-width: 0;
  flex: 1 1 18rem;
}

.update-words p {
  margin: 0;
}

.update-words strong {
  margin-right: 0.4rem;
}

.update-buttons {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.update-problem {
  color: var(--bad);
}

/* The models a question goes to: one click picks one or lets it go. */
.picks {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: 0.35rem;
  min-width: 0;
}

.chip {
  display: inline-flex;
  align-items: baseline;
  gap: 0.35rem;
  padding: 0.25rem 0.65rem;
  border-radius: 999px;
}

.chip[aria-pressed="true"] {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, var(--panel));
}

.chip-mark {
  color: var(--accent);
  font-weight: 700;
}

.chip[aria-pressed="false"] .chip-mark {
  color: var(--quiet);
}

.thread {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 1.25rem;
}

.empty {
  display: grid;
  gap: 0.35rem;
  place-content: center;
  text-align: center;
  height: 100%;
}

.lead {
  font-size: 1.4rem;
  font-weight: 600;
}

.first-run {
  display: grid;
  gap: 0.5rem;
  justify-self: center;
  width: min(34rem, 100%);
  text-align: left;
}

.offers {
  list-style: none;
  margin: 0.4rem 0 0;
  padding: 0;
  display: grid;
  gap: 0.5rem;
}

.offer {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.65rem 0.85rem;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--panel);
}

.offer-text {
  display: grid;
  flex: 1;
  min-width: 0;
}

.offer-name {
  font-weight: 600;
}

.offer-progress {
  display: grid;
  gap: 0.2rem;
  justify-items: end;
}

/* Settings, in place of the chat. */
.settings {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.settings h2:focus {
  outline: none;
}

.settings-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: grid;
  align-content: start;
  gap: 0.6rem;
  width: min(40rem, 100%);
  margin: 0 auto;
  padding: 1.25rem;
}

.settings-body h3,
.settings-body p {
  margin: 0;
}

.makers,
.maker-models {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 0.5rem;
}

.makers {
  gap: 1rem;
  margin-top: 0.4rem;
}

.maker-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  margin-bottom: 0.4rem;
}

.maker-name {
  font-weight: 600;
}

.settings-body .gap-above {
  margin-top: 1rem;
}

.engine-card {
  display: grid;
  gap: 0.4rem;
  padding: 0.65rem 0.85rem;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--panel);
}

.engine-card .maker-head {
  margin-bottom: 0;
}

.engine-buttons {
  display: flex;
  gap: 0.5rem;
}

.key-file code {
  overflow-wrap: anywhere;
}

.key-path {
  display: grid;
  gap: 0.3rem;
}

.key-path-row {
  display: flex;
  gap: 0.5rem;
}

.key-path input {
  flex: 1;
  min-width: 0;
  font: inherit;
  color: inherit;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--page);
  padding: 0.4rem 0.6rem;
}

.makers-pick {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem 1rem;
  margin: 0.2rem 0 0;
  padding: 0;
  border: 0;
}

.makers-pick legend {
  float: left;
  margin-right: 0.25rem;
}

.maker-check {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
}

.maker-check input {
  accent-color: var(--accent);
  margin: 0;
}

.turn {
  max-width: 46rem;
  margin: 0 auto 1.1rem;
  display: grid;
  gap: 0.35rem;
  user-select: text;
  -webkit-user-select: text;
}

.turn.user {
  justify-items: end;
}

.answers {
  max-width: 46rem;
  margin: 0 auto 1.1rem;
}

.answers > .turn {
  margin: 0;
  max-width: none;
}

/* Several models' answers to one question, side by side. */
.answers.many {
  max-width: 72rem;
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr));
  gap: 0.75rem;
  align-items: start;
}

.answers.many > .turn {
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 0.7rem 0.85rem;
  background: var(--page);
}

.card-head {
  margin: 0;
  font-size: 0.95rem;
  font-weight: 600;
}

.bubble,
.answer,
.thinking p {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.bubble {
  background: var(--bubble);
  border-radius: 14px 14px 4px 14px;
  padding: 0.55rem 0.85rem;
  max-width: 85%;
}

.thinking {
  border-left: 3px solid var(--line);
  padding-left: 0.75rem;
  color: var(--quiet);
  font-size: 0.9rem;
}

.thinking summary {
  cursor: pointer;
  width: fit-content;
}

.thinking p {
  margin-top: 0.35rem;
}

.waiting {
  font-style: italic;
}

.problem {
  margin: 0 1.25rem 0.5rem;
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--bad);
  border-radius: 8px;
  color: var(--bad);
  overflow-wrap: anywhere;
  white-space: pre-line;
}

.compose {
  padding: 0.75rem 1.25rem 0.9rem;
  border-top: 1px solid var(--line);
}

.box {
  display: flex;
  align-items: flex-end;
  gap: 0.6rem;
  max-width: 46rem;
  margin: 0 auto;
}

textarea {
  flex: 1;
  resize: none;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--page);
  padding: 0.55rem 0.75rem;
  max-height: 220px;
}

.meter {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  max-width: 46rem;
  margin: 0.45rem auto 0;
  min-height: 1.2rem;
}

progress {
  width: 7rem;
  height: 0.4rem;
  accent-color: var(--accent);
}

progress.near {
  accent-color: var(--bad);
}

@media (max-width: 760px) {
  .app {
    grid-template-columns: 12.5rem minmax(0, 1fr);
  }

  .picks-label,
  .chip-detail {
    display: none;
  }
}
</style>
