<script setup>
import { renderMarkdown } from "./markdown.js";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";

import {
  answersLine,
  arrangeSessions,
  canGrade,
  canRemove,
  chatModels,
  checkedLine,
  chipDetail,
  downloaded,
  downloadLine,
  firstRunOffers,
  changeLines,
  findToolCall,
  attachmentLine,
  attachmentTokens,
  shownReply,
  withAttachments,
  describesWork,
  withoutToolFences,
  WORK_NUDGE,
  TOOL_STEPS,
  toolLabel,
  toolResult,
  WRITE_TOOLS,
  workspacePrompt,
  formatCount,
  formatSize,
  fullest,
  gateLine,
  GRADE_ROLE,
  gradedAnswers,
  gradeMessages,
  graderFor,
  gradesLine,
  greeting,
  groups,
  installLine,
  isOn,
  isOutside,
  keptPicks,
  listedLine,
  looksLikePath,
  memoryRequest,
  meterLine,
  modelLine,
  moreLine,
  namesLine,
  onDiskLine,
  outsideModelsLine,
  parseFacts,
  pickedMakers,
  quickest,
  requestMessages,
  roundGrades,
  savedTurn,
  shouldStart,
  showsStartHint,
  showsUpdate,
  stackLine,
  startLine,
  switchedOn,
  systemPrompt,
  titleFrom,
  tokensPerSecond,
  uninstallAsk,
  uninstalledLine,
  UPDATE_EVERY,
  updateDetail,
  updateLine,
  withMaker,
  withPick,
  webPrompt,
  WEB_TOOL,
} from "./chat.js";

// The page only works inside the app's own window, where Rust answers.
const inApp = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// Line icons, drawn at 24 by 24 with a round stroke.
const ICONS = {
  clip: "M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48",
  file: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8ZM14 2v6h6",
  close: "M18 6 6 18M6 6l12 12",
  compose: "M12 20h9M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z",
  search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14Zm10 17-4.35-4.35",
  folder:
    "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
  plus: "M12 5v14M5 12h14",
  side: "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2ZM9 3v18",
  settings: "M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6",
  more: "M5 12h.01M12 12h.01M19 12h.01",
  up: "M12 19V5M5 12l7-7 7 7",
  chevron: "m6 9 6 6 6-6",
  back: "m15 18-6-6 6-6",
  copy: "M9 9h11v11H9zM5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1",
  check: "M20 6 9 17l-5-5",
};

const SUGGESTIONS = [
  { verb: "Write", rest: "a cover letter that does not sound like AI" },
  { verb: "Learn", rest: "how interest rates actually work" },
  { verb: "Code", rest: "a script that renames photos by date" },
  { verb: "Plan", rest: "a week of dinners under $60" },
];

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
// "I should be able to deactivate and uninstall a model." The ids Rust keeps
// switched off, where an uninstalled model goes, and what the page is doing.
const off = ref([]);
const switching = ref(false);
const modelNote = ref("");
const outsideOffNote = ref("");
const uninstallPlace = ref(null);
const askUninstall = ref("");
const uninstalling = ref("");
const freeing = ref(false);
const freeNote = ref("");
// The outside engines, as Rust keeps them, in the Installers page's order.
const outside = ref([]);
// The change under way, as its command and the engine's id, or "".
const outsideBusy = ref("");
// Why the engines could not be read, and what each engine's last change said.
const outsideNote = ref("");
const outsideNotes = ref({});
// The path typed for each engine's key, by the engine's id.
const outsidePaths = ref({});
// Web search: off to start. A search sends its words to Brave or DuckDuckGo.
const webOn = ref(stored("web", "off") === "on");
const web = ref({ engine: "duckduckgo", keyPath: null, warning: null });
const webPath = ref("");
const webBusy = ref(false);
const webNote = ref("");
// The model picked in Settings to grade the answers: its id, "" for no
// grading, or null until one is picked.
const graderPick = ref(rememberedGrader());
// What the main pane shows: "chat", "installers", or "settings".
const page = ref("chat");
// The open page's title, which takes the focus when the page opens.
const pageTitle = ref(null);
let updateTimer = 0;

// Projects, memory, search, and the side's own state.
const projects = ref([]);
const memory = ref("");
const memoryDraft = ref("");
const memorySaved = ref("");
const learnOn = ref(stored("learn", "on") !== "off");
// Off to start: learning from an outside engine sends your message and memory to its maker once more.
const learnOutside = ref(stored("learnOutside", "off") === "on");
const autoApply = ref(stored("autoApply", "off") === "on");
const learnedNote = ref("");
const theme = ref(stored("theme", "system"));
const sideOpen = ref(stored("side", "open") !== "closed");
const closedProjects = ref({});
const query = ref("");
const found = ref([]);
const searchBox = ref(null);
const menuFor = ref("");
const settingsTab = ref("memory");
const projectDialog = ref(null);
const editing = ref(null);
let editingFrom = {};
const askTrashProject = ref(false);
const now = ref(Date.now());
let searchTimer = 0;
let learnedTimer = 0;
let clockTimer = 0;
let learning = Promise.resolve();

let chatId = "";
let learnId = "";
/** True once Stop is pressed: the models still waiting, and the steps in a project's folder, end too. */
let stopAll = false;
const TOOL_STATUS = { running: "Running", asked: "Waiting for you", done: "Done", failed: "Failed", skipped: "Skipped" };
/** Settles the change waiting for Apply or Skip. */
let approve = null;
let checkAgain = false;
let stickToBottom = true;

function blankSession(project = null) {
  return { id: "", title: "", model: "", createdAt: 0, updatedAt: 0, messages: [], project, pinned: false };
}

// An outside engine's models are offered only while the gate is on and the
// engine is added.
const outsideListed = computed(() => outside.value.filter((entry) => entry.added).flatMap((entry) => entry.models ?? []));
const outsideModels = computed(() => (gate.value ? outsideListed.value : []));
// Every model on hand, switched on or off.
const onHand = computed(() => [...(engine.value?.models ?? []), ...outsideModels.value]);
// A model switched off is left out of the models a question can go to, the
// grader's place, and every comparison, since each of those reads this list.
const models = computed(() => switchedOn(onHand.value, off.value));
// Models are on hand, and every one of them is switched off.
const allOff = computed(() => onHand.value.length > 0 && !models.value.length);
// The models a question goes to, in the list's order.
const chosen = computed(() => models.value.filter((entry) => picked.value.includes(entry.id)));
const many = computed(() => chosen.value.length > 1);
const top = computed(() => fullest(current.value.messages, chosen.value));
const rounds = computed(() => groups(current.value.messages));
// Each round's grades and summary, by the round's key.
const gradesFor = computed(() => new Map(rounds.value.map((round) => [round.key, roundGrades(round, modelName)])));
// The model that grades the answers, of the models on hand, or null.
const grader = computed(() => graderFor(graderPick.value, models.value));
// What the list in Settings shows picked.
const graderChoice = computed({
  get: () => graderPick.value ?? grader.value?.id ?? "",
  set: (id) => {
    graderPick.value = id;
    rememberGrader(id);
  },
});
// A grader picked that is not on hand now, such as an outside engine with the
// gate off.
const graderAway = computed(() => (graderPick.value && !grader.value ? graderPick.value : ""));
/** Files read and waiting to go with the next message. */
const attached = ref([]);
/** How many files are still being read. */
const attaching = ref(0);
const dropping = ref(false);
let stopDropping = null;
const canSend = computed(
  () => Boolean(draft.value.trim() || attached.value.length) && !busy.value && chosen.value.length > 0 && !attaching.value,
);
/** The model picked with the smallest window, which files must fit. */
const tightest = computed(() =>
  chosen.value.filter((entry) => entry.context).reduce((least, entry) => (!least || entry.context < least.context ? entry : least), null),
);
/** True when the waiting files would nearly fill a picked model's window. */
const tooBig = computed(
  () =>
    Boolean(tightest.value) &&
    attached.value.length > 0 &&
    attachmentTokens(attached.value) + (top.value?.used ?? 0) > tightest.value.context * 0.85,
);
// With an outside engine to ask, the first run's downloads wait on the
// Installers page.
const offers = computed(() => (outsideModels.value.length ? [] : firstRunOffers(engine.value)));
const quick = computed(() => quickest(offers.value));
const handStart = computed(() => showsStartHint(engine.value, starting.value));
const makersLocked = computed(() => makersBusy.value || Boolean(downloading.value));
const engineAddress = computed(() => (engine.value?.url ?? "").replace(/^http:\/\//, "").replace(/\/v1$/, ""));
const side = computed(() => arrangeSessions(sessions.value, projects.value, now.value));
const searching = computed(() => Boolean(query.value.trim()));
const currentProject = computed(() => projects.value.find((project) => project.id === current.value.project) ?? null);
const hello = computed(() => greeting(new Date(now.value).getHours()));
const engineOn = computed(() => Boolean(engine.value?.reachable));

function stored(key, fallback) {
  try {
    return window.localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}

function store(key, value) {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // The choice just is not kept for next time.
  }
}

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

function rememberedGrader() {
  try {
    return window.localStorage.getItem("grader");
  } catch {
    return null;
  }
}

function rememberGrader(id) {
  try {
    window.localStorage.setItem("grader", id);
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

function applyTheme() {
  if (theme.value === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme.value;
}

watch(theme, (value) => {
  store("theme", value);
  applyTheme();
});

watch(learnOn, (on) => store("learn", on ? "on" : "off"));
watch(learnOutside, (on) => store("learnOutside", on ? "on" : "off"));
watch(autoApply, (on) => store("autoApply", on ? "on" : "off"));
watch(webOn, (on) => store("web", on ? "on" : "off"));
watch(sideOpen, (open) => store("side", open ? "open" : "closed"));

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
  // The outside engines' models come and go with the gate.
  pickModels(picked.value);
  // With the gate on, what waited for it goes ahead, and a line saying the
  // gate is off is gone.
  if (gate.value) {
    updateChecked.value = "";
    checkStack();
    checkUpdate();
    // Each engine added before its models were found finds them now, one at
    // a time.
    for (const entry of outside.value.filter((each) => each.added && !each.models?.length)) {
      await refreshOutside(entry.id);
    }
  }
}

/**
 * "user can add their outside inference, start with Grok and Grok ai. user
 * adds it manually, they will need to provide or specify a path to a key".
 * And of the engines after Grok: "and claude and codex/gpt and gemmini". Rust
 * keeps the path to each engine's key, reads the key from it each time the
 * engine is asked, and never hands the key to the page. The page sends only
 * the path typed.
 */
/**
 * Web search's key: Rust keeps the path to the person's Brave key, as for an
 * outside engine, and never hands the key to the page. With no key, a search
 * goes to DuckDuckGo.
 */
async function readWeb() {
  try {
    web.value = await invoke("web_status");
  } catch (error) {
    webNote.value = String(error);
  }
}

async function changeWeb(command, args = {}) {
  if (webBusy.value) return false;
  webBusy.value = true;
  webNote.value = "";
  try {
    web.value = await invoke(command, args);
    return true;
  } catch (error) {
    webNote.value = String(error);
    return false;
  } finally {
    webBusy.value = false;
  }
}

async function setWebKey() {
  const given = webPath.value.trim();
  if (!given) return;
  const added = await changeWeb("set_web_key", { keyPath: given });
  // A key pasted where the path goes does not stay on screen.
  if (added || !looksLikePath(given)) webPath.value = "";
}

/** "Friday, November 20, 2026": the day, so the model knows what "now" is. */
function todayLine() {
  return new Date().toLocaleDateString(undefined, { weekday: "long", year: "numeric", month: "long", day: "numeric" });
}

async function readOutside() {
  try {
    const listed = await invoke("outside_status");
    outside.value = Array.isArray(listed) ? listed : [];
    outsideNote.value = "";
  } catch (error) {
    outsideNote.value = String(error);
  }
}

/**
 * Asks Rust to change one outside engine, by its id. Gives back whether Rust
 * took the change.
 */
async function changeOutside(command, id, args = {}) {
  if (outsideBusy.value) return false;
  outsideBusy.value = `${command} ${id}`;
  outsideNotes.value = { ...outsideNotes.value, [id]: "" };
  let taken = false;
  let note = "";
  try {
    const changed = await invoke(command, { engine: id, ...args });
    outside.value = outside.value.map((entry) => (entry.id === changed.id ? changed : entry));
    // With the gate off, the line above the engines already says what waits
    // for it.
    note = gate.value ? (changed.error ?? "") : "";
    taken = true;
  } catch (error) {
    // Nothing changed; Rust says why.
    note = String(error);
  } finally {
    outsideBusy.value = "";
  }
  outsideNotes.value = { ...outsideNotes.value, [id]: note };
  // The models a question goes to follow the engines'.
  pickModels(picked.value);
  return taken;
}

async function addOutside(id) {
  const given = (outsidePaths.value[id] ?? "").trim();
  if (!given) return;
  const added = await changeOutside("add_outside", id, { keyPath: given });
  // A key pasted where the path goes does not stay on screen.
  if (added || !looksLikePath(given)) outsidePaths.value = { ...outsidePaths.value, [id]: "" };
}

function refreshOutside(id) {
  return changeOutside("refresh_outside", id);
}

function removeOutside(id) {
  return changeOutside("remove_outside", id);
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

/** The models switched off, as Rust keeps them. */
async function readOff() {
  try {
    off.value = await invoke("deactivated_models");
  } catch (error) {
    modelNote.value = String(error);
  }
}

/**
 * Switches a model on or off: every id it has here, or an outside engine's
 * one. The models a question goes to follow the switch.
 */
async function setActive(ids, active, outsideModel = false) {
  if (switching.value || busy.value) return;
  const note = outsideModel ? outsideOffNote : modelNote;
  switching.value = true;
  note.value = "";
  try {
    for (const id of ids) off.value = await invoke("set_model_active", { id, active });
  } catch (error) {
    // Rust kept what it could; it says why the rest was not.
    note.value = String(error);
    await readOff();
  } finally {
    switching.value = false;
    pickModels(picked.value);
  }
}

/**
 * Uninstall: Rust moves each of the model's ids, its folder or file, to the
 * Trash, where the person can take it back. Nothing is deleted.
 */
async function uninstall(entry) {
  askUninstall.value = "";
  if (uninstalling.value || busy.value || downloading.value) return;
  uninstalling.value = entry.name;
  modelNote.value = "";
  try {
    for (const id of entry.ids ?? []) await invoke("uninstall_model", { id });
    modelNote.value = uninstalledLine(entry.name, uninstallPlace.value);
  } catch (error) {
    modelNote.value = String(error);
  } finally {
    uninstalling.value = "";
    await Promise.all([readMakers(), readOff()]);
    await checkEngine();
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

/**
 * Opens Installers or Settings in place of the chat. Both show the makers, and
 * Installers shows the outside engines, as Rust keeps them, so each asks Rust
 * afresh.
 */
async function openPage(name) {
  page.value = name;
  menuFor.value = "";
  if (name === "settings") {
    memoryDraft.value = memory.value;
    memorySaved.value = "";
  }
  nextTick(() => pageTitle.value?.focus());
  await readMakers();
  await readOutside();
  await readOff();
}

/** Back to the chat. */
function closePage() {
  page.value = "chat";
  focusComposer();
}

/** A button in the side bar opens its page, or closes it when it is open. */
function togglePage(name) {
  return page.value === name ? closePage() : openPage(name);
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
 * Lets go of the memory a loaded model takes. Rust restarts the engine the
 * app started, with nothing loaded; one the app did not start is left alone.
 */
async function freeMemory() {
  if (freeing.value || busy.value) return;
  freeing.value = true;
  freeNote.value = "";
  try {
    const report = await invoke("free_memory");
    if (report.freed) freeNote.value = "Memory freed. The next message loads its model again.";
    else if (!engine.value?.reachable) freeNote.value = "No engine is running, so no model is in memory.";
    else freeNote.value = "The app did not start this engine, so it cannot free its memory. Quit that engine to free it.";
    await checkEngine();
  } catch (error) {
    freeNote.value = String(error);
  } finally {
    freeing.value = false;
  }
}

/**
 * Downloads one model from the app's list, then starts the engine with it.
 * Rust checks every file as it arrives and moves the model into place only
 * when all of it is there. Gives back whether the whole model is in place.
 */
async function download(offer) {
  if (downloading.value) return false;
  downloading.value = offer.id;
  progress.value = { done: 0, total: offer.bytes };
  stoppedNote.value = "";
  problem.value = "";
  const channel = new Channel();
  channel.onmessage = (update) => {
    progress.value = update;
  };
  let whole = false;
  try {
    const report = await invoke("download_model", { id: offer.id, onProgress: channel });
    if (report.stopped) stoppedNote.value = `The download of ${offer.name} stopped, and none of it was kept.`;
    // With the model in place, the engine can start with it.
    if (report.whole) await Promise.all([checkEngine(), readMakers()]);
    whole = report.whole === true;
  } catch (error) {
    problem.value = String(error);
  } finally {
    downloading.value = "";
    progress.value = null;
    stopping.value = false;
  }
  return whole;
}

/**
 * "one click from an installers page would be good". Install downloads the
 * model, then adds its maker if it was not added, so the model shows at the
 * top of the window. A download that stopped or failed leaves the makers as
 * they were.
 */
async function install(maker, offer) {
  if (!(await download(offer))) return;
  if (!pickedMakers(makers.value).includes(maker)) await setMaker(maker, true);
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
  if (searching.value) runSearch();
}

async function refreshProjects() {
  try {
    projects.value = await invoke("list_projects");
  } catch {
    // Without projects the side shows chats alone.
  }
}

async function loadMemory() {
  const before = memory.value;
  try {
    memory.value = await invoke("load_memory");
  } catch {
    memory.value = "";
  }
  // An edit under way in Settings is not written over.
  if (memoryDraft.value === before) memoryDraft.value = memory.value;
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

function newChat(project = null) {
  if (busy.value) return;
  current.value = blankSession(project);
  page.value = "chat";
  problem.value = "";
  askTrash.value = "";
  menuFor.value = "";
  if (project) closedProjects.value = { ...closedProjects.value, [project]: false };
  focusComposer();
}

async function openSession(id) {
  if (busy.value) return;
  page.value = "chat";
  menuFor.value = "";
  if (id === current.value.id) return;
  problem.value = "";
  askTrash.value = "";
  try {
    current.value = await invoke("load_session", { id });
    // A step left waiting or running when the app closed never ran.
    for (const turn of current.value.messages) {
      if (turn.tool && ["asked", "running"].includes(turn.tool.status)) turn.tool.status = "skipped";
    }
    pickModels(chatModels(current.value), picked.value);
    stickToBottom = true;
    scrollDown();
    focusComposer();
  } catch (error) {
    problem.value = String(error);
  }
}

async function trashSession(id) {
  // A chat answering now is saved as it goes, so it stays until it is done.
  if (busy.value) return;
  askTrash.value = "";
  menuFor.value = "";
  try {
    await invoke("trash_session", { id });
    if (current.value.id === id) current.value = blankSession();
    await refreshSessions();
  } catch (error) {
    problem.value = String(error);
  }
}

/** Pins a chat, or files it in a project, without changing when it was last used. */
async function arrange(entry, pinned, project) {
  menuFor.value = "";
  try {
    const summary = await invoke("arrange_session", { id: entry.id, pinned, project: project ?? null });
    if (current.value.id === entry.id) {
      current.value.pinned = summary.pinned;
      current.value.project = summary.project ?? null;
    }
    await refreshSessions();
  } catch (error) {
    problem.value = String(error);
  }
}

function toggleMenu(id) {
  askTrash.value = "";
  menuFor.value = menuFor.value === id ? "" : id;
}

function onSearchInput() {
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(runSearch, 180);
}

async function runSearch() {
  const asked = query.value;
  if (!asked.trim()) {
    found.value = [];
    return;
  }
  try {
    const results = await invoke("search_sessions", { query: asked });
    if (asked === query.value) found.value = results;
  } catch (error) {
    problem.value = String(error);
  }
}

function clearSearch() {
  query.value = "";
  found.value = [];
}

function toggleProject(id) {
  closedProjects.value = { ...closedProjects.value, [id]: !closedProjects.value[id] };
}

function projectName(id) {
  return projects.value.find((project) => project.id === id)?.name ?? "";
}

function editProject(project) {
  menuFor.value = "";
  askTrashProject.value = false;
  editing.value = project
    ? { ...project }
    : { id: "", name: "", instructions: "", memory: "", folder: "", createdAt: 0, updatedAt: 0 };
  editingFrom = { ...editing.value };
  nextTick(() => projectDialog.value?.showModal());
}

async function chooseFolder() {
  try {
    const folder = await invoke("pick_folder");
    if (folder && editing.value) editing.value.folder = folder;
  } catch (error) {
    problem.value = String(error);
  }
}

function closeProject() {
  projectDialog.value?.close();
  editing.value = null;
}

async function saveProject() {
  if (!editing.value) return;
  try {
    // Memories learned while the dialog was open are kept: only the fields
    // changed here go over the project as it is now.
    await refreshProjects();
    const latest = projects.value.find((project) => project.id === editing.value.id);
    const project = { ...(latest ?? editing.value) };
    for (const [key, value] of Object.entries(editing.value)) {
      if (value !== editingFrom[key]) project[key] = value;
    }
    const saved = await invoke("save_project", { project });
    await refreshProjects();
    closedProjects.value = { ...closedProjects.value, [saved.id]: false };
    closeProject();
  } catch (error) {
    problem.value = String(error);
  }
}

async function trashProject() {
  const id = editing.value?.id;
  if (!id) return;
  try {
    await invoke("trash_project", { id });
    if (current.value.project === id) current.value.project = null;
    closeProject();
    await Promise.all([refreshProjects(), refreshSessions()]);
  } catch (error) {
    problem.value = String(error);
  }
}

async function saveMemory() {
  try {
    memory.value = await invoke("save_memory", { text: memoryDraft.value });
    memoryDraft.value = memory.value;
    memorySaved.value = "Saved.";
  } catch (error) {
    memorySaved.value = String(error);
  }
}

/**
 * After a reply, asks the same model whether the person's message holds
 * anything worth remembering, and adds it to the global memory or the
 * project's. A model that strays or fails saves nothing.
 */
async function learn(message, chosenId, project) {
  if (!learnOn.value || !chosenId) return;
  const id = `memory-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  learnId = id;
  try {
    const done = await invoke("chat_stream", {
      chatId: id,
      request: { model: chosenId, messages: memoryRequest(message, memory.value, project), maxTokens: 1024 },
      onEvent: new Channel(),
    });
    const facts = parseFacts(done.answer, Boolean(project));
    if (facts.user.length) await invoke("remember", { facts: facts.user, project: null });
    if (facts.project.length && project) await invoke("remember", { facts: facts.project, project: project.id });
    const saved = [...facts.user, ...facts.project];
    if (!saved.length) return;
    await Promise.all([loadMemory(), refreshProjects()]);
    learnedNote.value = `Remembered: ${saved.join(" · ")}`;
    window.clearTimeout(learnedTimer);
    learnedTimer = window.setTimeout(() => (learnedNote.value = ""), 8000);
  } catch {
    // Memory is a nicety; a failed look saves nothing and says nothing.
  } finally {
    if (learnId === id) learnId = "";
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
  const project = currentProject.value;
  problem.value = "";
  const files = attached.value;
  if (!session.messages.length) session.title = titleFrom(text || files[0]?.name);
  session.model = asked[0].id;
  if (files.length) session.messages.push({ role: "user", content: text, attachments: files });
  else session.messages.push({ role: "user", content: text });
  // What each model reads: the chat up to this question, and no other
  // model's answer to it.
  const history = session.messages.slice();
  draft.value = "";
  attached.value = [];
  busy.value = true;
  stopAll = false;
  stickToBottom = true;
  resizeComposer();
  scrollDown();
  // Saved before the models answer, so the question survives a crash.
  await persist();
  // The engine answers one at a time, so a look for memories still under way
  // finishes first.
  await learning;

  let system = systemPrompt(memory.value, project);
  // A project's folder, and the web, are worked in by one model at a time.
  let tools = false;
  let folder = false;
  if (project?.folder && asked.length === 1) {
    try {
      const overview = await invoke("workspace_overview", { project: project.id });
      system = [system, workspacePrompt(project, overview)].filter(Boolean).join("\n\n");
      tools = folder = true;
    } catch (error) {
      problem.value = String(error);
    }
  }
  // The web is searched only while Settings allows it and the gate is on.
  if (webOn.value && gate.value && asked.length === 1) {
    system = [system, webPrompt(todayLine(), folder)].filter(Boolean).join("\n\n");
    tools = true;
  }
  if (system) history.unshift({ role: "system", content: system });

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
  let answered = !stopAll && !failed.length;
  // In a project with a folder, or with web search on, the model may take steps.
  if (tools && answered && session.messages.includes(replies[0])) {
    answered = await work(session, asked[0], project, system, replies[0]);
  }
  // Once every answer is in, the grader grades them. After Stop, it does not.
  const judge = grader.value;
  if (!stopAll && judge) {
    const round = groups(session.messages).at(-1);
    if (canGrade(round)) {
      const error = await grade(session, round, judge);
      if (error) failed.push(error);
    }
  }
  busy.value = false;
  if (failed.length) {
    problem.value = failed.join("\n");
    checkEngine();
  }
  await persist();
  scrollDown();
  focusComposer();
  // An outside engine looks for memories only when Settings allows it.
  if (answered && text && (learnOutside.value || !isOutside(asked[0].id))) learning = learn(text, asked[0].id, project);
}

/** The chat with what the model is told first, when there is anything to tell. */
function withSystem(turns, system) {
  return system ? [{ role: "system", content: system }, ...turns] : turns;
}

/**
 * In a project with a folder, the model may take steps there: each reply
 * that ends in a tool block runs that tool, and what it found goes back to
 * the model, until it answers without one. Gives back whether it answered.
 */
async function work(session, entry, project, system, first) {
  let reply = first;
  let nudged = false;
  for (let step = 0; ; step += 1) {
    if (reply.stopped || stopAll) return false;
    const call = findToolCall(reply.content, reply.thinking);
    if (call?.block) reply.content = call.block;
    let result;
    if (!call) {
      // A model that shows code instead of working in the folder is told
      // once to use its tools.
      if (nudged || !project?.folder || !describesWork(reply.content)) return true;
      nudged = true;
      result = WORK_NUDGE;
    } else {
      if (step >= TOOL_STEPS) {
        problem.value = `The model took ${TOOL_STEPS} steps without answering, so the app stopped it. Ask it to carry on if you like.`;
        return false;
      }
      result = await useTool(project, call, reply);
      await persist();
      if (result === null) return false;
    }
    session.messages.push({ role: "user", content: result, toolResult: true });
    // Stop pressed while the tool ran asks the model nothing more.
    if (stopAll) {
      await persist();
      return false;
    }
    const messages = requestMessages(withSystem(session.messages.slice(), system), entry.id);
    session.messages.push({ role: "assistant", content: "", thinking: "", model: entry.id, pending: true });
    reply = session.messages.at(-1);
    await persist();
    if (stopAll) {
      session.messages = session.messages.filter((turn) => turn !== reply);
      return false;
    }
    scrollDown();
    const error = await ask(reply, entry, messages);
    if (error || (!reply.content && !reply.thinking)) {
      session.messages = session.messages.filter((turn) => turn !== reply);
      if (error) problem.value = `${entry.name}: ${error}`;
      return false;
    }
    await persist();
  }
}

/** Reads files into text, to go with the next message. */
async function attachPaths(paths) {
  for (const path of paths ?? []) {
    attaching.value += 1;
    try {
      const file = await invoke("read_attachment", { path });
      if (!attached.value.some((known) => known.name === file.name && known.text === file.text)) {
        attached.value = [...attached.value, file];
      }
    } catch (error) {
      problem.value = String(error);
    } finally {
      attaching.value -= 1;
    }
  }
  focusComposer();
}

async function pickFiles() {
  try {
    await attachPaths(await invoke("pick_files"));
  } catch (error) {
    problem.value = String(error);
  }
}

function detach(index) {
  attached.value = attached.value.filter((_, at) => at !== index);
}

/** Files dropped on the window are attached, while a chat is open. */
function onDrop(event) {
  const kind = event.payload?.type;
  const chatting = page.value === "chat";
  if (kind === "enter" || kind === "over") dropping.value = chatting;
  else if (kind === "leave") dropping.value = false;
  else if (kind === "drop") {
    dropping.value = false;
    if (chatting) attachPaths(event.payload.paths);
  }
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

/**
 * "The results are graded and summarized by grok after all the results are
 * in", and "I should be able to pick which engine does the grading". The
 * grader reads the question and the answers, numbered and without the models'
 * names, and its grade is kept with them. A grade that failed or was stopped
 * part way, or one that never began, is not kept. Gives back why it failed,
 * or nothing.
 */
async function grade(session, round, judge) {
  const answers = gradedAnswers(round).map((turn) => turn.content);
  const messages = gradeMessages(withAttachments(round.question), answers, judge.context);
  const at = session.messages.indexOf(round.answers.at(-1)) + 1;
  session.messages.splice(at, 0, { role: GRADE_ROLE, content: "", thinking: "", model: judge.id, pending: true });
  const reply = session.messages[at];
  scrollDown();
  const error = await ask(reply, judge, messages);
  if (error || reply.stopped || !reply.content.trim()) {
    session.messages = session.messages.filter((turn) => turn !== reply);
  }
  return error ? `${judge.name} could not grade the answers: ${error}` : "";
}

/** Grades a round that has none yet, such as one asked before a grader was picked. */
async function gradeRound(round) {
  const judge = grader.value;
  if (busy.value || !judge || !canGrade(round)) return;
  const session = current.value;
  problem.value = "";
  busy.value = true;
  stopAll = false;
  const error = await grade(session, round, judge);
  busy.value = false;
  if (error) {
    problem.value = error;
    checkEngine();
  }
  await persist();
}

/**
 * Runs the step a reply asked for in the project's folder, and answers with
 * what goes back to the model. A change waits for Apply or Skip. Null when
 * the person pressed Stop.
 */
async function useTool(project, call, reply) {
  reply.tool = { tool: call.tool, args: call.args, status: "running" };
  if (call.error) {
    Object.assign(reply.tool, { status: "failed", output: call.error });
    return toolResult(call, call.error, true);
  }
  if (call.tool === WEB_TOOL) {
    try {
      if (!webOn.value) throw new Error("Web search is off in Settings. Answer without it.");
      const output = await invoke("web_search", { query: String(call.args.query ?? "") });
      Object.assign(reply.tool, { status: "done", output: output.slice(0, 6000) });
      return toolResult(call, output);
    } catch (error) {
      const said = String(error?.message ?? error);
      Object.assign(reply.tool, { status: "failed", output: said });
      return toolResult(call, said, true);
    }
  }
  if (!project?.folder) {
    const said = "This chat has no folder to work in. Answer without that tool.";
    Object.assign(reply.tool, { status: "failed", output: said });
    return toolResult(call, said, true);
  }
  if (WRITE_TOOLS.includes(call.tool) && !autoApply.value) {
    reply.tool.status = "asked";
    scrollDown();
    const yes = await new Promise((resolve) => (approve = resolve));
    approve = null;
    if (stopAll) {
      reply.tool.status = "skipped";
      return null;
    }
    if (!yes) {
      reply.tool.status = "skipped";
      return toolResult(call, "The user declined this change. Ask what they would like instead, or try another way.", true);
    }
  }
  try {
    const output = await invoke("workspace_tool", { project: project.id, call: { ...call.args, tool: call.tool } });
    Object.assign(reply.tool, { status: "done", output: output.slice(0, 6000) });
    return toolResult(call, output);
  } catch (error) {
    Object.assign(reply.tool, { status: "failed", output: String(error) });
    return toolResult(call, String(error), true);
  }
}

/** Links in a reply stay put: the window never leaves the chat. */
/** An answer's words. A tool block it ends with is shown as its step instead. */
function replyText(turn) {
  return turn.tool || turn.pending ? shownReply(withoutToolFences(turn.content)) : String(turn.content ?? "").trim();
}

const copied = ref(null);
let copiedTimer;

/** Puts a message's text on the clipboard, and marks its button for a moment. */
async function copyText(turn, text) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const box = document.createElement("textarea");
    box.value = text;
    document.body.append(box);
    box.select();
    document.execCommand("copy");
    box.remove();
  }
  copied.value = turn;
  clearTimeout(copiedTimer);
  copiedTimer = setTimeout(() => (copied.value = null), 1500);
}

function keepLinks(event) {
  if (event.target.closest("a")) event.preventDefault();
}

function decide(yes) {
  approve?.(yes);
}

function stop() {
  if (!busy.value) return;
  stopAll = true;
  if (chatId) invoke("stop_chat", { chatId });
  // A look for memories the new question waits on stops too.
  if (learnId) invoke("stop_chat", { chatId: learnId });
  approve?.(false);
}

function suggest(entry) {
  draft.value = `${entry.verb} ${entry.rest}`;
  resizeComposer();
  focusComposer();
}

function onKeydown(event) {
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    send();
  }
}

/** Command-N starts a chat, Command-K searches, and Escape closes a menu. */
function onWindowKey(event) {
  const command = event.metaKey || event.ctrlKey;
  if (command && event.key.toLowerCase() === "n") {
    event.preventDefault();
    newChat();
  } else if (command && event.key.toLowerCase() === "k") {
    event.preventDefault();
    sideOpen.value = true;
    nextTick(() => searchBox.value?.focus());
  } else if (event.key === "Escape" && menuFor.value) {
    menuFor.value = "";
  }
}

function onWindowClick(event) {
  if (menuFor.value && !event.target.closest?.(".menu, .more")) menuFor.value = "";
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
  // So does a chat an outside engine answered after the engine is removed.
  // A model switched off still names the chats it wrote.
  const known = [...onHand.value, ...(engine.value?.setAside ?? []), ...outside.value.flatMap((entry) => entry.models ?? [])];
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

/** The grade an answer got, or null. */
function gradeOf(round, index) {
  return gradesFor.value.get(round.key)?.grades[index] ?? null;
}

/** Under a grade: that it was graded without names, and its counts. */
function gradeNote(turn) {
  return ["Graded without the models' names", replyNote(turn, false)].filter(Boolean).join(" · ");
}

applyTheme();

onMounted(async () => {
  if (!inApp) return;
  window.addEventListener("focus", checkEngine);
  window.addEventListener("keydown", onWindowKey);
  window.addEventListener("click", onWindowClick);
  clockTimer = window.setInterval(() => (now.value = Date.now()), 60_000);
  // While the gate is off, Rust starts no stack and asks for no new version.
  await readGate();
  // So is the path to each outside engine's key, and the models found with it.
  await readOutside();
  await readWeb();
  // The makers picked are kept on this computer, so they need no gate.
  await readMakers();
  // So are the models switched off, and where Uninstall moves a model.
  await readOff();
  try {
    uninstallPlace.value = await invoke("uninstall_place");
  } catch {
    // The question then says "the Trash".
  }
  // The stack can take minutes, so the page does not wait for it.
  checkStack();
  // So does the check for a new version, which asks GitHub.
  checkUpdate();
  updateTimer = window.setInterval(checkUpdate, UPDATE_EVERY);
  // Dropping files is a nicety; the page works without it.
  try {
    getCurrentWebview()
      .onDragDropEvent(onDrop)
      .then((stop) => (stopDropping = stop))
      .catch(() => {});
  } catch {
    // No webview to listen to.
  }
  await Promise.all([checkEngine(), refreshSessions(), refreshProjects(), loadMemory()]);
  focusComposer();
});

onBeforeUnmount(() => {
  window.removeEventListener("focus", checkEngine);
  window.removeEventListener("keydown", onWindowKey);
  window.removeEventListener("click", onWindowClick);
  window.clearInterval(updateTimer);
  window.clearInterval(clockTimer);
  stopDropping?.();
});
</script>

<template>
  <div v-if="!inApp" class="outside">
    <h1>Rusty Business™</h1>
    <p>This page runs inside the Rusty Business™ window. Start it with <code>npm run tauri dev</code>.</p>
  </div>

  <div v-else class="app" :class="{ 'side-closed': !sideOpen }">
    <aside v-show="sideOpen" class="side">
      <div class="side-head">
        <div class="brand-row">
          <h1 class="brand"><span class="mark" aria-hidden="true"></span>Rusty Business™</h1>
          <button class="icon" type="button" aria-label="Hide the side" title="Hide the side" @click="sideOpen = false">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.side" /></svg>
          </button>
        </div>
        <button class="new-chat" type="button" :disabled="busy" @click="newChat()">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.compose" /></svg>
          <span>New conversation</span>
          <kbd>⌘ N</kbd>
        </button>
        <label class="search">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.search" /></svg>
          <input
            ref="searchBox"
            v-model="query"
            type="search"
            placeholder="Search conversations"
            aria-label="Search conversations"
            @input="onSearchInput"
            @keydown.escape="clearSearch"
          />
        </label>
      </div>

      <nav class="sessions" aria-label="Saved chats">
        <template v-if="searching">
          <p class="label">Results</p>
          <p v-if="!found.length" class="quiet pad">No chat holds “{{ query.trim() }}”.</p>
          <ul v-else>
            <li v-for="entry in found" :key="entry.id" :class="{ open: entry.id === current.id }">
              <button class="session" type="button" :disabled="busy" @click="openSession(entry.id)">
                <span class="session-title">{{ entry.title }}</span>
                <span v-if="entry.snippet" class="snippet">{{ entry.snippet }}</span>
                <span v-else-if="entry.project" class="snippet">{{ projectName(entry.project) }}</span>
              </button>
            </li>
          </ul>
        </template>

        <template v-else>
          <div class="label-row">
            <p class="label">Projects</p>
            <button class="icon small-icon" type="button" aria-label="New project" title="New project" @click="editProject(null)">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.plus" /></svg>
            </button>
          </div>
          <p v-if="!projects.length" class="quiet pad small">Group chats, and give them their own instructions and memory.</p>
          <div v-for="project in projects" :key="project.id" class="project">
            <div class="project-row" :class="{ here: current.project === project.id }">
              <button class="project-name" type="button" :aria-expanded="!closedProjects[project.id]" @click="toggleProject(project.id)">
                <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.folder" /></svg>
                <span>{{ project.name }}</span>
              </button>
              <button
                class="icon small-icon row-action"
                type="button"
                :aria-label="`Settings for ${project.name}`"
                title="Project settings"
                @click="editProject(project)"
              >
                <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.settings" /></svg>
              </button>
            </div>
            <ul v-if="!closedProjects[project.id]" class="nested">
              <li>
                <button class="session start" type="button" :disabled="busy" @click="newChat(project.id)">
                  <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.compose" /></svg>
                  <span>New chat</span>
                </button>
              </li>
              <li
                v-for="entry in side.byProject[project.id]"
                :key="entry.id"
                :class="{ open: entry.id === current.id }"
              >
                <template v-if="askTrash === entry.id">
                  <span class="ask">Move to the trash?</span>
                  <button class="link" type="button" :disabled="busy" @click="trashSession(entry.id)">Yes</button>
                  <button class="link" type="button" @click="askTrash = ''">No</button>
                </template>
                <template v-else>
                  <button class="session" type="button" :disabled="busy" @click="openSession(entry.id)">
                    <span class="session-title">{{ entry.title }}</span>
                  </button>
                  <button class="icon small-icon more" type="button" :aria-label="`More for ${entry.title}`" @click.stop="toggleMenu(entry.id)">
                    <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.more" /></svg>
                  </button>
                  <div v-if="menuFor === entry.id" class="menu" role="menu">
                    <button type="button" role="menuitem" @click="arrange(entry, !entry.pinned, entry.project)">
                      {{ entry.pinned ? "Unpin" : "Pin" }}
                    </button>
                    <button type="button" role="menuitem" @click="arrange(entry, entry.pinned, null)">Take out of {{ project.name }}</button>
                    <button
                      v-for="other in projects.filter((each) => each.id !== project.id)"
                      :key="other.id"
                      type="button"
                      role="menuitem"
                      @click="arrange(entry, entry.pinned, other.id)"
                    >
                      Move to {{ other.name }}
                    </button>
                    <button type="button" role="menuitem" class="danger" :disabled="busy" @click="(askTrash = entry.id), (menuFor = '')">Move to the trash</button>
                  </div>
                </template>
              </li>
            </ul>
          </div>

          <template v-for="group in [{ label: 'Pinned', items: side.pinned }, ...side.ages]" :key="group.label">
            <p v-if="group.items.length" class="label">{{ group.label }}</p>
            <ul v-if="group.items.length">
              <li v-for="entry in group.items" :key="entry.id" :class="{ open: entry.id === current.id }">
                <template v-if="askTrash === entry.id">
                  <span class="ask">Move to the trash?</span>
                  <button class="link" type="button" :disabled="busy" @click="trashSession(entry.id)">Yes</button>
                  <button class="link" type="button" @click="askTrash = ''">No</button>
                </template>
                <template v-else>
                  <button class="session" type="button" :disabled="busy" @click="openSession(entry.id)">
                    <span class="session-title">{{ entry.title }}</span>
                  </button>
                  <button class="icon small-icon more" type="button" :aria-label="`More for ${entry.title}`" @click.stop="toggleMenu(entry.id)">
                    <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.more" /></svg>
                  </button>
                  <div v-if="menuFor === entry.id" class="menu" role="menu">
                    <button type="button" role="menuitem" @click="arrange(entry, !entry.pinned, null)">
                      {{ entry.pinned ? "Unpin" : "Pin" }}
                    </button>
                    <button
                      v-for="other in projects"
                      :key="other.id"
                      type="button"
                      role="menuitem"
                      @click="arrange(entry, entry.pinned, other.id)"
                    >
                      Move to {{ other.name }}
                    </button>
                    <button type="button" role="menuitem" class="danger" :disabled="busy" @click="(askTrash = entry.id), (menuFor = '')">Move to the trash</button>
                  </div>
                </template>
              </li>
            </ul>
          </template>
          <p v-if="!sessions.length" class="quiet pad small">Chats are saved here as you go.</p>
        </template>
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
          :aria-current="page === 'installers' ? 'page' : undefined"
          @click="togglePage('installers')"
        >
          Installers
        </button>
        <button
          class="wide"
          type="button"
          :aria-current="page === 'settings' ? 'page' : undefined"
          @click="togglePage('settings')"
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
      <div v-if="dropping" class="drop" aria-hidden="true">
        <p class="display">Drop to attach</p>
        <p class="quiet small">PDF, Word, PowerPoint, spreadsheets, CSV, and text or code files</p>
      </div>
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

      <section v-if="page === 'installers'" class="settings" aria-labelledby="installers-title" @keydown.esc="closePage">
        <header class="bar">
          <button v-if="!sideOpen" class="icon" type="button" aria-label="Show the side" title="Show the side" @click="sideOpen = true">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.side" /></svg>
          </button>
          <h2 id="installers-title" ref="pageTitle" tabindex="-1">Installers</h2>
          <button type="button" @click="closePage">Back to the chat</button>
        </header>
        <div class="settings-body">
          <h3>Models to run here</h3>
          <p class="quiet">
            Install downloads a model from Hugging Face to this computer, then adds its maker, so the model shows at the
            top of the window. Nothing downloads until you press Install.
          </p>
          <p v-if="!gate" class="gate-ask">
            <span>The outbound gate is off, so nothing can install yet.</span>
            <button type="button" :disabled="gateBusy" @click="setGate(true)">Turn on the gate</button>
          </p>
          <ul class="makers" aria-label="Models to install">
            <li v-for="maker in makers" :key="maker.name" class="maker">
              <div class="maker-head">
                <span class="maker-name">{{ maker.name }}</span>
                <span v-if="!maker.picked" class="quiet small">Not added</span>
              </div>
              <ul class="maker-models">
                <li v-for="entry in maker.models" :key="entry.name" class="offer">
                  <span class="offer-text">
                    <span class="offer-name">{{ entry.name }}</span>
                    <span class="quiet small">{{ installLine(maker, entry) }}</span>
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
                    v-else-if="entry.onDisk && !maker.picked"
                    type="button"
                    :aria-label="`Show ${entry.name}`"
                    :disabled="makersLocked"
                    @click="setMaker(maker.name, true)"
                  >
                    Show
                  </button>
                  <button
                    v-else-if="entry.offer && !entry.onDisk"
                    type="button"
                    :aria-label="`Install ${entry.name}`"
                    :disabled="makersLocked || !gate"
                    @click="install(maker.name, entry.offer)"
                  >
                    Install
                  </button>
                </li>
              </ul>
            </li>
          </ul>
          <p v-if="makersNote" class="small" role="alert">{{ makersNote }}</p>
          <p v-if="stoppedNote" class="quiet small">{{ stoppedNote }}</p>

          <h3 class="gap-above">Outside engines</h3>
          <p class="quiet">
            Each engine here answers from its maker's computers, not this one. Give the path to a file that holds your
            key from that maker. Rusty Business™ reads the key from that file when it asks the engine and sends it only
            to that maker's API, while the gate is on. The key never shows here.
          </p>
          <p v-if="!gate" class="gate-ask">
            <span>The outbound gate is off, so no outside engine can be asked.</span>
            <button type="button" :disabled="gateBusy" @click="setGate(true)">Turn on the gate</button>
          </p>
          <div
            v-for="entry in outside"
            :key="entry.id"
            class="engine-card"
            :aria-labelledby="`${entry.id}-name`"
            role="group"
          >
            <div class="maker-head">
              <span :id="`${entry.id}-name`" class="maker-name">
                {{ entry.name }} <span class="quiet small">by {{ entry.maker }}</span>
              </span>
              <span v-if="entry.added" class="engine-buttons">
                <button type="button" :disabled="Boolean(outsideBusy) || !gate" @click="refreshOutside(entry.id)">
                  {{ outsideBusy === `refresh_outside ${entry.id}` ? `Asking ${entry.maker}...` : "Refresh models" }}
                </button>
                <button
                  type="button"
                  :aria-label="`Remove ${entry.name}`"
                  :disabled="Boolean(outsideBusy)"
                  @click="removeOutside(entry.id)"
                >
                  Remove
                </button>
              </span>
            </div>
            <template v-if="entry.added">
              <p class="small key-file">Key file: <code>{{ entry.keyPath }}</code></p>
              <p v-if="entry.warning" class="small" role="alert">{{ entry.warning }}</p>
              <p class="quiet small">{{ outsideModelsLine(entry) }}</p>
            </template>
            <form v-else class="key-path" @submit.prevent="addOutside(entry.id)">
              <label :for="`${entry.id}-key-path`" class="small">Path to your {{ entry.maker }} key file</label>
              <span class="key-path-row">
                <input
                  :id="`${entry.id}-key-path`"
                  v-model="outsidePaths[entry.id]"
                  type="text"
                  :placeholder="`~/.config/${entry.id}/key`"
                  spellcheck="false"
                  autocomplete="off"
                  autocapitalize="off"
                  autocorrect="off"
                  :disabled="Boolean(outsideBusy)"
                />
                <button
                  class="primary"
                  type="submit"
                  :disabled="Boolean(outsideBusy) || !(outsidePaths[entry.id] ?? '').trim()"
                >
                  {{ outsideBusy === `add_outside ${entry.id}` ? "Adding..." : "Add" }}
                </button>
              </span>
            </form>
            <p v-if="outsideNotes[entry.id]" class="small" role="alert">{{ outsideNotes[entry.id] }}</p>
          </div>
          <p v-if="outsideNote" class="small" role="alert">{{ outsideNote }}</p>
        </div>
      </section>
      <section v-if="page === 'settings'" class="settings" aria-labelledby="settings-title" @keydown.esc="closePage">
        <header class="bar">
          <button v-if="!sideOpen" class="icon" type="button" aria-label="Show the side" title="Show the side" @click="sideOpen = true">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.side" /></svg>
          </button>
          <h2 id="settings-title" ref="pageTitle" tabindex="-1">Settings</h2>
          <button type="button" @click="closePage">Back to the chat</button>
        </header>
        <div class="settings-body">
          <nav class="settings-tabs" aria-label="Settings">
            <button type="button" :class="{ here: settingsTab === 'memory' }" @click="settingsTab = 'memory'">Memory</button>
            <button type="button" :class="{ here: settingsTab === 'models' }" @click="settingsTab = 'models'">Models</button>
            <button type="button" :class="{ here: settingsTab === 'folders' }" @click="settingsTab = 'folders'">Folders</button>
            <button type="button" :class="{ here: settingsTab === 'web' }" @click="settingsTab = 'web'">Web</button>
            <button type="button" :class="{ here: settingsTab === 'appearance' }" @click="settingsTab = 'appearance'">Appearance</button>
          </nav>
          <div v-if="settingsTab === 'memory'" class="settings-pane">
            <h3>Memory</h3>
            <p class="quiet">
              What the assistant knows about you in every chat. Write it however you like (Markdown works). The assistant also
              adds to it as you talk. Project memory lives in each project's settings.
            </p>
            <textarea v-model="memoryDraft" class="memory" rows="16" aria-label="Memory" spellcheck="true"></textarea>
            <div class="settings-actions">
              <button class="solid" type="button" :disabled="memoryDraft === memory" @click="saveMemory">Save</button>
              <span v-if="memorySaved" class="quiet small">{{ memorySaved }}</span>
            </div>
            <label class="check">
              <input v-model="learnOn" type="checkbox" />
              <span>
                Learn from my chats
                <span class="quiet small block">After each reply, the same model looks for lasting facts in your message and adds them here.</span>
              </span>
            </label>
            <label class="check">
              <input v-model="learnOutside" type="checkbox" :disabled="!learnOn" />
              <span>
                Let outside engines learn too
                <span class="quiet small block">When Grok, GPT, Claude, or Gemini answers, it also looks for facts. That sends your message and this memory to its maker a second time, and uses your API credits.</span>
              </span>
            </label>
          </div>
          <div v-else-if="settingsTab === 'folders'" class="settings-pane">
            <h3>Folders</h3>
            <p class="quiet">A project with a folder lets the model list, read, and search its files, and edit or write them.</p>
            <label class="check">
              <input v-model="autoApply" type="checkbox" />
              <span>
                Apply changes without asking
                <span class="quiet small block">The model's edits and new files go straight into the folder, with no Apply or Skip. Stop still ends the work.</span>
              </span>
            </label>
          </div>
          <div v-else-if="settingsTab === 'web'" class="settings-pane">
            <h3>Web search</h3>
            <p class="quiet">A model can look things up on the web while it answers, such as a plumber near you or today's news.</p>
            <label class="check">
              <input v-model="webOn" type="checkbox" />
              <span>
                Let models search the web
                <span class="quiet small block">The model writes the search; only its words leave this computer, to {{ web.engine === "brave" ? "Brave Search" : "DuckDuckGo" }}. Works while the outbound gate is on, with one model asked at a time.</span>
              </span>
            </label>
            <p v-if="webOn && !gate" class="gate-ask">
              <span>The outbound gate is off, so nothing can be searched.</span>
              <button type="button" :disabled="gateBusy" @click="setGate(true)">Turn on the gate</button>
            </p>
            <h3 class="gap-above">Brave Search key</h3>
            <p class="quiet">
              Optional. With a key from Brave's Search API, results are steadier than DuckDuckGo's. Give the path to a file
              that holds the key; Rusty Business™ reads it for each search and sends it only to Brave. The key never shows here.
            </p>
            <template v-if="web.keyPath">
              <p class="small key-file">Key file: <code>{{ web.keyPath }}</code></p>
              <p v-if="web.warning" class="small" role="alert">{{ web.warning }}</p>
              <div class="settings-actions">
                <button type="button" :disabled="webBusy" @click="changeWeb('remove_web_key')">Remove key</button>
              </div>
            </template>
            <form v-else class="key-path" @submit.prevent="setWebKey">
              <label for="web-key-path" class="small">Path to your Brave Search key file</label>
              <span class="key-path-row">
                <input
                  id="web-key-path"
                  v-model="webPath"
                  type="text"
                  placeholder="~/.config/brave/key"
                  spellcheck="false"
                  autocomplete="off"
                  autocapitalize="off"
                  autocorrect="off"
                  :disabled="webBusy"
                />
                <button class="primary" type="submit" :disabled="webBusy || !webPath.trim()">Add</button>
              </span>
            </form>
            <p v-if="webNote" class="small" role="alert">{{ webNote }}</p>
          </div>
          <div v-else-if="settingsTab === 'models'" class="settings-pane">
            <h3>Model makers</h3>
            <p class="quiet">
              The app shows the models of the makers you pick. Removing a maker leaves its models on this computer.
            </p>
            <p class="quiet">
              Switch a model off to leave it out of questions, grading, and comparisons. Its files stay. Uninstall moves
              a model's files to {{ uninstallPlace?.trash || "the Trash" }}, where you can take them back.
            </p>
            <p class="quiet small">
              To install a model or add an outside engine, go to the
              <button class="link in-text" type="button" @click="openPage('installers')">Installers</button> page.
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
                      <span class="quiet small">{{ modelLine(entry, off) }}</span>
                    </span>
                    <template v-if="entry.ids?.length && askUninstall === entry.name">
                      <span class="ask-uninstall small">{{ uninstallAsk(entry.name, uninstallPlace) }}</span>
                      <button
                        class="link"
                        type="button"
                        :aria-label="`Yes, move ${entry.name}`"
                        :disabled="busy || Boolean(downloading)"
                        @click="uninstall(entry)"
                      >
                        Yes
                      </button>
                      <button class="link" type="button" @click="askUninstall = ''">No</button>
                    </template>
                    <span v-else-if="entry.ids?.length" class="model-buttons">
                      <button
                        class="switch model-switch"
                        type="button"
                        role="switch"
                        :aria-checked="isOn(entry.ids, off)"
                        :aria-label="`Use ${entry.name}`"
                        :disabled="switching || busy || Boolean(uninstalling)"
                        @click="setActive(entry.ids, !isOn(entry.ids, off))"
                      >
                        <span class="track" aria-hidden="true"><span class="knob"></span></span>
                        <span class="quiet small">{{ isOn(entry.ids, off) ? "On" : "Off" }}</span>
                      </button>
                      <button
                        type="button"
                        :aria-label="`Uninstall ${entry.name}`"
                        :disabled="busy || Boolean(downloading) || Boolean(uninstalling)"
                        @click="askUninstall = entry.name"
                      >
                        {{ uninstalling === entry.name ? "Moving..." : "Uninstall" }}
                      </button>
                    </span>
                  </li>
                </ul>
              </li>
            </ul>
            <p v-if="pickedMakers(makers).length === 1" class="quiet small">One maker always stays picked.</p>
            <p v-if="makersNote" class="small" role="alert">{{ makersNote }}</p>
            <p v-if="modelNote" class="small" role="alert">{{ modelNote }}</p>

            <h3 class="gap-above">Outside models</h3>
            <p class="quiet">A model switched off stays with its maker, and is left out here the same way.</p>
            <ul v-if="outsideListed.length" class="maker-models" aria-label="Outside models">
              <li v-for="entry in outsideListed" :key="entry.id" class="offer">
                <span class="offer-text">
                  <span class="offer-name">{{ entry.name }}</span>
                  <span class="quiet small">
                    {{ chipDetail(entry) }}{{ isOn([entry.id], off) ? "" : ", switched off" }}
                  </span>
                </span>
                <button
                  class="switch model-switch"
                  type="button"
                  role="switch"
                  :aria-checked="isOn([entry.id], off)"
                  :aria-label="`Use ${entry.name}`"
                  :disabled="switching || busy"
                  @click="setActive([entry.id], !isOn([entry.id], off), true)"
                >
                  <span class="track" aria-hidden="true"><span class="knob"></span></span>
                  <span class="quiet small">{{ isOn([entry.id], off) ? "On" : "Off" }}</span>
                </button>
              </li>
            </ul>
            <p v-else class="quiet small">
              No outside engine is added. Add one on the
              <button class="link in-text" type="button" @click="openPage('installers')">Installers</button> page.
            </p>
            <p v-if="outsideOffNote" class="small" role="alert">{{ outsideOffNote }}</p>

            <h3 class="gap-above">Loaded models</h3>
            <p class="quiet">
              A model stays in this computer's memory after it answers, until the app quits. Free memory lets it go now.
              The next message loads it again, which takes a moment.
            </p>
            <div class="settings-actions">
              <button type="button" :disabled="freeing || busy || Boolean(downloading)" @click="freeMemory">
                {{ freeing ? "Freeing..." : "Free memory" }}
              </button>
              <span v-if="freeNote" class="quiet small" role="status">{{ freeNote }}</span>
            </div>

            <h3 class="gap-above">Grading</h3>
            <p class="quiet">
              When several models answer, one model grades each answer from A to F once all are in, and sums them up. It
              reads the question and the answers, numbered and without the models' names.
            </p>
            <div class="grader-pick">
              <label for="grader" class="small">Grades the answers</label>
              <select id="grader" v-model="graderChoice" :disabled="busy">
                <option value="">No grading</option>
                <option v-for="entry in models" :key="entry.id" :value="entry.id">
                  {{ entry.name }} · {{ chipDetail(entry) }}
                </option>
                <option v-if="graderAway" :value="graderAway" disabled>
                  {{ modelName(graderAway) || "A model not on hand" }} · {{ off.includes(graderAway) ? "switched off" : "not on hand" }}
                </option>
              </select>
            </div>
            <p v-if="graderPick === null" class="quiet small">
              Until you pick one, the first outside engine on hand grades, in the order the Installers page lists them.
            </p>
            <p v-if="graderAway && off.includes(graderAway)" class="quiet small">
              The model picked is switched off, so the answers are not graded.
            </p>
            <p v-else-if="graderAway" class="quiet small">
              The model picked is not on hand, so the answers are not graded.
              <template v-if="isOutside(graderAway)">Outside engines are on hand only while the gate is on.</template>
            </p>
            <p v-if="grader && isOutside(grader.id)" class="small">
              {{ grader.name }} grades on {{ grader.maker }}'s computers, so the question and every answer, the answers
              from this computer too, go to {{ grader.maker }} each time it grades. That happens only while the gate is on.
            </p>
          </div>
          <div v-else class="settings-pane">
            <h3>Appearance</h3>
            <p class="quiet">Follow the system, or keep one look.</p>
            <div class="segmented" role="radiogroup" aria-label="Theme">
              <button
                v-for="choice in ['system', 'light', 'dark']"
                :key="choice"
                type="button"
                role="radio"
                :aria-checked="theme === choice"
                :class="{ here: theme === choice }"
                @click="theme = choice"
              >
                {{ choice[0].toUpperCase() + choice.slice(1) }}
              </button>
            </div>
          </div>
        </div>
      </section>

      <header v-show="page === 'chat'" class="bar">
        <button v-if="!sideOpen" class="icon" type="button" aria-label="Show the side" title="Show the side" @click="sideOpen = true">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.side" /></svg>
        </button>
        <div class="picks" role="group" aria-label="Models that answer">
          <span v-if="models.length" class="quiet small picks-label">
            <span class="dot" :class="engineOn ? 'live' : 'off'" aria-hidden="true"></span>Ask
          </span>
          <span v-else class="quiet small">{{ allOff ? "Every model is switched off" : "No model on hand" }}</span>
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
        <h2 class="bar-title">
          <span v-if="currentProject" class="crumb">{{ currentProject.name }} /</span>
          {{ current.title || "New chat" }}
        </h2>
      </header>

      <section v-show="page === 'chat'" ref="thread" class="thread" @scroll="onScroll">
        <div v-if="!current.messages.length" class="empty">
          <div v-if="offers.length" class="first-run">
            <p class="display lead">Pick a model to start.</p>
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
            <p class="quiet small">
              More models, and the outside engines, are on the
              <button class="link in-text" type="button" @click="openPage('installers')">Installers</button> page.
            </p>
          </div>
          <template v-else>
            <div class="hello">
              <p class="display lead">{{ currentProject ? currentProject.name : hello }}.</p>
              <p v-if="chosen.length" class="quiet">{{ answersLine(chosen) }}</p>
              <p v-else-if="allOff" class="quiet">
                Every model on hand is switched off. Switch one on in
                <button class="link in-text" type="button" @click="
                    settingsTab = 'models';
                    openPage('settings');
                  ">Settings, Models</button>.
              </p>
              <p v-else-if="starting" class="quiet">Starting the engine...</p>
              <p v-else-if="engine?.reachable" class="quiet">The engine has none of this app's models on hand.</p>
              <p v-if="many && grader" class="quiet">{{ gradesLine(grader) }}</p>
              <div v-if="chosen.length && !currentProject" class="pills">
                <button v-for="entry in SUGGESTIONS" :key="entry.verb" class="pill" type="button" @click="suggest(entry)">
                  <strong>{{ entry.verb }}</strong> {{ entry.rest }}
                </button>
              </div>
            </div>
          </template>
        </div>

        <article v-for="round in rounds" :key="round.key" class="round">
          <div v-if="round.question && !round.question.toolResult" class="turn user">
            <ul v-if="round.question.attachments?.length" class="files sent" aria-label="Attached files">
              <li v-for="(file, at) in round.question.attachments" :key="at" class="file" :title="file.name">
                <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.file" /></svg>
                <span class="file-text">
                  <span class="file-name">{{ file.name }}</span>
                  <span class="file-line">{{ attachmentLine(file) }}</span>
                </span>
              </li>
            </ul>
            <p v-if="round.question.content" class="bubble">{{ round.question.content }}</p>
            <div v-if="round.question.content" class="turn-foot">
              <button class="copy" type="button" :aria-label="copied === round.question ? 'Copied' : 'Copy'" :title="copied === round.question ? 'Copied' : 'Copy'" @click="copyText(round.question, round.question.content)">
                <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="copied === round.question ? ICONS.check : ICONS.copy" /></svg>
              </button>
            </div>
          </div>
          <div v-if="round.answers.length" class="answers" :class="{ many: round.answers.length > 1 }">
            <section v-for="(turn, index) in round.answers" :key="index" class="turn assistant">
              <h3 v-if="round.answers.length > 1" class="card-head">{{ modelName(turn.model) || "A model not on hand" }}</h3>
              <p v-if="gradeOf(round, index)" class="grade small">
                <span class="grade-letter"><span class="sr-only">Grade </span>{{ gradeOf(round, index).letter }}</span>
                <span>{{ gradeOf(round, index).why }}</span>
              </p>
              <details v-if="turn.thinking" class="thinking">
                <summary>{{ turn.pending && !turn.content ? "Thinking..." : "Thinking" }}</summary>
                <p>{{ turn.thinking }}</p>
              </details>
              <!-- eslint-disable-next-line vue/no-v-html -- markdown-it escapes raw HTML in the reply -->
              <div v-if="replyText(turn)" class="answer prose" @click="keepLinks" v-html="renderMarkdown(replyText(turn))"></div>
              <p v-else-if="turn.waiting" class="quiet waiting">Waits its turn.</p>
              <p v-else-if="turn.pending && !turn.thinking" class="quiet waiting">Waiting for the model...</p>
              <div v-if="turn.tool" class="tool" :class="turn.tool.status">
                <div class="tool-head">
                  <span class="tool-label">{{ toolLabel(turn.tool) }}</span>
                  <span class="tool-status">{{ TOOL_STATUS[turn.tool.status] }}</span>
                </div>
                <pre v-if="turn.tool.status === 'asked'" class="change"><span
                  v-for="(line, at) in changeLines(turn.tool)"
                  :key="at"
                  :class="line.kind"
                >{{ line.kind === "gone" ? "- " : "+ " }}{{ line.text }}
</span></pre>
                <div v-if="turn.tool.status === 'asked'" class="tool-actions">
                  <button class="solid" type="button" @click="decide(true)">Apply</button>
                  <button type="button" @click="decide(false)">Skip</button>
                  <button class="link" type="button" @click="(autoApply = true), decide(true)">Apply all from now on</button>
                </div>
                <details v-else-if="turn.tool.output || changeLines(turn.tool).length" class="tool-more">
                  <summary>Details</summary>
                  <pre v-if="changeLines(turn.tool).length" class="change"><span
                    v-for="(line, at) in changeLines(turn.tool)"
                    :key="at"
                    :class="line.kind"
                  >{{ line.kind === "gone" ? "- " : "+ " }}{{ line.text }}
</span></pre>
                  <pre v-if="turn.tool.output">{{ turn.tool.output }}</pre>
                </details>
              </div>
              <div v-if="!turn.pending" class="turn-foot">
                <button v-if="replyText(turn)" class="copy" type="button" :aria-label="copied === turn ? 'Copied' : 'Copy'" :title="copied === turn ? 'Copied' : 'Copy'" @click="copyText(turn, replyText(turn))">
                  <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="copied === turn ? ICONS.check : ICONS.copy" /></svg>
                </button>
                <p class="quiet small note">{{ replyNote(turn, round.answers.length === 1) }}</p>
              </div>
            </section>
          </div>
          <section v-if="round.grade" class="grade-card">
            <h3 class="card-head">Summary, by {{ modelName(round.grade.model) || "a model not on hand" }}</h3>
            <details v-if="round.grade.thinking" class="thinking">
              <summary>{{ round.grade.pending && !round.grade.content ? "Thinking..." : "Thinking" }}</summary>
              <p>{{ round.grade.thinking }}</p>
            </details>
            <!-- eslint-disable-next-line vue/no-v-html -- markdown-it escapes raw HTML in the reply -->
            <div v-if="gradesFor.get(round.key)?.summary" class="answer prose" @click="keepLinks" v-html="renderMarkdown(gradesFor.get(round.key).summary)"></div>
            <p v-else-if="round.grade.pending" class="quiet waiting">Grading the answers...</p>
            <p v-if="!round.grade.pending" class="quiet small note">{{ gradeNote(round.grade) }}</p>
          </section>
          <p v-else-if="!busy && grader && canGrade(round)" class="grade-ask">
            <button type="button" @click="gradeRound(round)">Grade with {{ grader.name }}</button>
          </p>
        </article>
      </section>

      <!-- Shown on the Installers page too, where a download can fail. -->
      <p v-if="problem" class="problem" role="alert">
        {{ problem }}
        <button class="link" type="button" @click="problem = ''">Dismiss</button>
      </p>

      <footer v-show="page === 'chat'" class="compose">
        <div class="box">
          <ul v-if="attached.length || attaching" class="files" aria-label="Files to send">
            <li v-for="(file, at) in attached" :key="at" class="file" :title="file.name">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.file" /></svg>
              <span class="file-text">
                <span class="file-name">{{ file.name }}</span>
                <span class="file-line">{{ attachmentLine(file) }}</span>
              </span>
              <button class="file-x" type="button" :aria-label="`Remove ${file.name}`" @click="detach(at)">
                <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.close" /></svg>
              </button>
            </li>
            <li v-if="attaching" class="file reading quiet small">Reading...</li>
          </ul>
          <p v-if="tooBig" class="too-big small">These files may not fit in {{ tightest.name }}'s window. Try fewer, or a model with a bigger one.</p>
          <textarea
            ref="composer"
            v-model="draft"
            rows="2"
            aria-label="Your message"
            :placeholder="chosen.length ? `Message ${namesLine(chosen.map((entry) => entry.name))}` : allOff ? 'Every model is switched off' : 'No model on hand'"
            :disabled="!chosen.length"
            @input="resizeComposer"
            @keydown="onKeydown"
          ></textarea>
          <div class="box-row">
            <button class="icon attach" type="button" aria-label="Attach files" title="Attach files" :disabled="busy" @click="pickFiles">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.clip" /></svg>
            </button>
            <span class="quiet small model-note">{{ namesLine(chosen.map((entry) => entry.name)) }}</span>
            <button v-if="busy" class="send stop" type="button" aria-label="Stop" title="Stop" @click="stop">
              <span class="stop-square" aria-hidden="true"></span>
            </button>
            <button v-else class="send" type="button" :disabled="!canSend" aria-label="Send" title="Send" @click="send">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.up" /></svg>
            </button>
          </div>
        </div>
        <div class="meter">
          <span v-if="learnedNote" class="learned small">{{ learnedNote }}</span>
          <template v-else-if="top">
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
          <span v-else class="quiet small">Can make mistakes. Check what matters.</span>
        </div>
      </footer>
    </main>

    <dialog ref="projectDialog" class="dialog" @close="editing = null">
      <form v-if="editing" method="dialog" @submit.prevent="saveProject">
        <h2 class="display">{{ editing.id ? "Project settings" : "New project" }}</h2>
        <label class="field">
          <span>Name</span>
          <input v-model="editing.name" type="text" maxlength="80" placeholder="Gledit" required />
        </label>
        <label class="field">
          <span>Instructions</span>
          <span class="quiet small">Given to the model in every chat in this project.</span>
          <textarea v-model="editing.instructions" rows="4" placeholder="Answer as a product marketer. Keep it short."></textarea>
        </label>
        <div class="field">
          <span>Folder</span>
          <span class="quiet small">A folder on this computer the model can read for coding work. It changes files only when you press Apply.</span>
          <div class="folder-row">
            <input v-model="editing.folder" type="text" placeholder="/Users/you/code/site" aria-label="Folder" spellcheck="false" />
            <button type="button" @click="chooseFolder">Choose...</button>
          </div>
        </div>
        <label class="field">
          <span>Memory</span>
          <span class="quiet small">What the assistant knows about this project. It adds to it as you talk.</span>
          <textarea v-model="editing.memory" class="memory" rows="8"></textarea>
        </label>
        <div class="dialog-actions">
          <template v-if="editing.id">
            <template v-if="askTrashProject">
              <span class="ask">Move the project to the trash? Its chats stay.</span>
              <button class="link danger" type="button" @click="trashProject">Yes</button>
              <button class="link" type="button" @click="askTrashProject = false">No</button>
            </template>
            <button v-else class="link danger" type="button" @click="askTrashProject = true">Move to the trash</button>
          </template>
          <span class="spacer"></span>
          <button type="button" @click="closeProject">Cancel</button>
          <button class="solid" type="submit">Save</button>
        </div>
      </form>
    </dialog>
  </div>
</template>

<style>
:root {
  --page: #f3eee3;
  --side: #ebe5d8;
  --panel: #f3eee3;
  --raised: #faf7f0;
  --ink: #0e0d0b;
  --quiet: #6f695e;
  --faint: #9a9284;
  --line: #d9d0be;
  --hover: #e3dccd;
  --accent: #ff5b1f;
  --accent-text: #c2410c;
  --accent-ink: #0e0d0b;
  --solid: #0e0d0b;
  --solid-ink: #f3eee3;
  --bubble: #e6dfd0;
  --good: #2f7d4f;
  --bad: #b3261e;
  --shadow: 0 1px 0 rgba(14, 13, 11, 0.04), 0 8px 28px rgba(14, 13, 11, 0.08);
  --sans: "Instrument Sans Variable", "Instrument Sans", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  --serif: "Instrument Serif", ui-serif, Georgia, serif;
  --mono: "JetBrains Mono Variable", "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace;
  font-family: var(--sans);
  font-size: 15px;
  line-height: 1.55;
  color: var(--ink);
  background: var(--page);
  color-scheme: light;
  -webkit-font-smoothing: antialiased;
}

@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --page: #0e0d0b;
    --side: #121110;
    --panel: #0e0d0b;
    --raised: #171614;
    --ink: #f3eee3;
    --quiet: #8f897e;
    --faint: #5f5a52;
    --line: #2a2723;
    --hover: #1e1c19;
    --accent-text: #ff7a45;
    --solid: #f3eee3;
    --solid-ink: #0e0d0b;
    --bubble: #1e1c19;
    --good: #5cba82;
    --bad: #f2867f;
    --shadow: 0 1px 0 rgba(0, 0, 0, 0.3), 0 12px 40px rgba(0, 0, 0, 0.45);
    color-scheme: dark;
  }
}

:root[data-theme="dark"] {
  --page: #0e0d0b;
  --side: #121110;
  --panel: #0e0d0b;
  --raised: #171614;
  --ink: #f3eee3;
  --quiet: #8f897e;
  --faint: #5f5a52;
  --line: #2a2723;
  --hover: #1e1c19;
  --accent-text: #ff7a45;
  --solid: #f3eee3;
  --solid-ink: #0e0d0b;
  --bubble: #1e1c19;
  --good: #5cba82;
  --bad: #f2867f;
  --shadow: 0 1px 0 rgba(0, 0, 0, 0.3), 0 12px 40px rgba(0, 0, 0, 0.45);
  color-scheme: dark;
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
h3,
p,
ul {
  margin: 0;
}

h1 {
  font-size: 1rem;
  font-weight: 600;
}

h2 {
  font-size: 1rem;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

h3 {
  font-size: 1.15rem;
  font-weight: 600;
}

button,
select,
textarea,
input {
  font: inherit;
  color: inherit;
}

button {
  border: 1px solid var(--line);
  background: transparent;
  border-radius: 10px;
  padding: 0.4rem 0.9rem;
  cursor: pointer;
  transition:
    background-color 0.15s,
    border-color 0.15s,
    color 0.15s;
}

button:hover:not(:disabled) {
  background: var(--hover);
}

button:disabled {
  opacity: 0.45;
  cursor: default;
}

button.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-ink);
  font-weight: 600;
}

button.primary:hover:not(:disabled) {
  background: #ff7a45;
}

button.solid {
  background: var(--solid);
  border-color: var(--solid);
  color: var(--solid-ink);
  font-weight: 500;
  padding: 0.5rem 1.2rem;
}

button.solid:hover:not(:disabled) {
  background: var(--solid);
  opacity: 0.88;
}

button.wide {
  width: 100%;
}

button.link {
  border: 0;
  background: none;
  padding: 0 0.25rem;
  color: var(--accent-text);
  text-decoration: underline;
  text-underline-offset: 2px;
}

button.link:hover:not(:disabled) {
  background: none;
}

.danger {
  color: var(--bad) !important;
}

button.icon {
  display: inline-grid;
  place-items: center;
  border: 0;
  padding: 0.35rem;
  border-radius: 8px;
  color: var(--quiet);
}

button.icon:hover:not(:disabled) {
  color: var(--ink);
}

svg {
  width: 1.15rem;
  height: 1.15rem;
  flex-shrink: 0;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.6;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.small-icon svg {
  width: 0.95rem;
  height: 0.95rem;
}

.more svg {
  stroke-width: 3;
}

/* A link inside a sentence sits in it like a word. */
button.link.in-text {
  padding: 0;
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

.block {
  display: block;
}

.pad {
  padding: 0.5rem 0.75rem;
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}

code,
kbd {
  font-family: var(--mono);
  font-size: 0.82em;
}

.display {
  font-family: var(--serif);
  font-weight: 400;
  letter-spacing: -0.01em;
}

.outside {
  max-width: 32rem;
  margin: 4rem auto;
  padding: 0 1rem;
}

.app {
  display: grid;
  grid-template-columns: 18rem minmax(0, 1fr);
  height: 100%;
}

.app.side-closed {
  grid-template-columns: minmax(0, 1fr);
}

/* The side: brand, new chat, search, projects, chats, the engine. */
.side {
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--side);
  border-right: 1px solid var(--line);
}

.side-head {
  display: grid;
  gap: 0.6rem;
  padding: 0.9rem 0.75rem 0.5rem;
}

.brand-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 0.25rem 0.35rem;
}

.brand {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  font-family: var(--serif);
  font-weight: 400;
  font-size: 1.55rem;
  letter-spacing: -0.02em;
  line-height: 1;
}

.mark {
  width: 1.35rem;
  height: 1.35rem;
  border-radius: 50% 50% 50% 12%;
  background: var(--accent);
  box-shadow: inset -4px -4px 0 rgba(0, 0, 0, 0.12);
}

.new-chat {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  width: 100%;
  padding: 0.6rem 0.8rem;
  border-radius: 12px;
  background: var(--raised);
  text-align: left;
}

.new-chat span {
  flex: 1;
}

.new-chat kbd {
  color: var(--faint);
  font-size: 0.7rem;
}

.search {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  padding: 0.35rem 0.8rem;
  color: var(--quiet);
  border-radius: 10px;
}

.search:focus-within {
  background: var(--hover);
}

.search input {
  flex: 1;
  min-width: 0;
  border: 0;
  background: none;
  outline: none;
  padding: 0.2rem 0;
  color: var(--ink);
}

.search input::placeholder {
  color: var(--quiet);
}

.search input::-webkit-search-cancel-button {
  filter: grayscale(1);
}

/* The saved chats keep room for one and a half chats, however tall the
   footer under them grows. */
.sessions {
  flex: 1;
  min-height: 5rem;
  overflow-y: auto;
  padding: 0 0.5rem 0.75rem;
}

.label,
.label-row {
  font-family: var(--mono);
  font-size: 0.66rem;
  letter-spacing: 0.22em;
  text-transform: uppercase;
  color: var(--faint);
}

.label {
  padding: 1.1rem 0.75rem 0.4rem;
}

.label-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-right: 0.3rem;
}

.label-row .label {
  padding-right: 0;
}

.sessions ul {
  list-style: none;
  padding: 0;
}

.sessions li {
  position: relative;
  display: flex;
  align-items: center;
  gap: 0.1rem;
  border-radius: 10px;
  min-height: 2.3rem;
}

.sessions li:hover,
.project-row:hover {
  background: var(--hover);
}

.sessions li.open {
  background: var(--hover);
}

.sessions li.open .session-title {
  color: var(--ink);
}

.session {
  flex: 1;
  min-width: 0;
  display: grid;
  text-align: left;
  border: 0;
  background: none !important;
  padding: 0.4rem 0.75rem;
}

.session-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: color-mix(in srgb, var(--ink) 82%, transparent);
}

.snippet {
  font-size: 0.78rem;
  color: var(--quiet);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.session.start {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  color: var(--quiet);
}

.session.start svg {
  width: 0.9rem;
  height: 0.9rem;
}

/* Faded rather than hidden, so the keyboard still reaches them. */
.more,
.row-action {
  opacity: 0;
}

.sessions li:hover .more,
.sessions li:focus-within .more,
.more:focus-visible,
.project-row:hover .row-action,
.project-row:focus-within .row-action,
.row-action:focus-visible {
  opacity: 1;
}

.project-row {
  display: flex;
  align-items: center;
  border-radius: 10px;
}

.project-name {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 0.65rem;
  border: 0;
  background: none !important;
  padding: 0.45rem 0.75rem;
  text-align: left;
}

.project-name span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.project-row.here .project-name svg {
  color: var(--accent);
}

.nested {
  margin-left: 1.15rem;
}

.menu {
  position: absolute;
  right: 0.25rem;
  top: calc(100% - 0.2rem);
  z-index: 5;
  display: grid;
  min-width: 12rem;
  max-height: 18rem;
  overflow-y: auto;
  padding: 0.3rem;
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: 12px;
  box-shadow: var(--shadow);
}

.menu button {
  border: 0;
  text-align: left;
  padding: 0.4rem 0.6rem;
  border-radius: 8px;
  font-size: 0.9rem;
}

.ask {
  flex: 1;
  padding-left: 0.75rem;
  font-size: 0.88rem;
}

/* On a short window the footer scrolls, so the gate's switch at its foot can
   always be reached. */
.engine {
  display: grid;
  gap: 0.35rem;
  min-height: 0;
  overflow-y: auto;
  padding: 0.8rem 1rem 0.9rem;
  border-top: 1px solid var(--line);
  font-size: 0.82rem;
}

.engine > p:first-child,
.engine #stack-reason,
.engine p:has(> .dot) {
  font-family: var(--mono);
  font-size: 0.7rem;
  letter-spacing: 0.04em;
}

.engine button.wide {
  padding: 0.3rem 0.7rem;
  font-size: 0.8rem;
}

.dot {
  display: inline-block;
  width: 0.45rem;
  height: 0.45rem;
  border-radius: 50%;
  margin-right: 0.5rem;
  vertical-align: 0.08em;
}

.dot.on {
  background: var(--good);
}

.dot.off {
  background: var(--bad);
}

.dot.live {
  background: var(--accent);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent);
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
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: 8px;
  user-select: all;
  overflow-wrap: anywhere;
}

/* The chat. */
.main {
  position: relative;
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
  gap: 0.5rem 1rem;
  min-height: 3.6rem;
  padding: 0.6rem 1.5rem;
  border-bottom: 1px solid var(--line);
}

.bar-title {
  margin-left: auto;
  max-width: 50%;
  color: var(--quiet);
  font-size: 0.88rem;
}

.crumb {
  color: var(--faint);
}

.back {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  border: 0;
  padding: 0.35rem 0.6rem 0.35rem 0.35rem;
  color: var(--quiet);
}

/* The notice of a new version, above the chat's title. */
.update {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 0.6rem 1rem;
  padding: 0.7rem 1.5rem;
  border-bottom: 1px solid var(--line);
  border-left: 3px solid var(--accent);
  background: var(--raised);
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
  justify-content: flex-start;
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
  padding: 2rem 1.5rem 1rem;
}

.empty {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 100%;
}

.hello {
  width: min(56rem, 100%);
  display: grid;
  gap: 0.6rem;
}

.lead {
  font-size: clamp(2.6rem, 5.6vw, 4.6rem);
  line-height: 1.02;
}

.sub {
  font-size: 1.3rem;
  color: var(--quiet);
}

.pills {
  display: flex;
  flex-wrap: wrap;
  gap: 0.6rem;
  margin-top: 2.2rem;
}

.pill {
  border-radius: 999px;
  padding: 0.6rem 1.1rem;
  color: var(--quiet);
  text-align: left;
}

.pill strong {
  color: var(--ink);
  font-weight: 500;
}

.pill:hover:not(:disabled) {
  border-color: var(--faint);
}

.first-run {
  display: grid;
  gap: 0.6rem;
  justify-self: center;
  width: min(36rem, 100%);
}

.first-run .lead {
  font-size: 2.6rem;
}

.offers {
  list-style: none;
  margin: 0.6rem 0 0;
  padding: 0;
  display: grid;
  gap: 0.55rem;
}

.offer {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.8rem 1rem;
  border: 1px solid var(--line);
  border-radius: 14px;
  background: var(--raised);
}

.offer-text {
  display: grid;
  flex: 1;
  min-width: 0;
}

.offer-name {
  font-weight: 600;
}

.have {
  font-family: var(--mono);
  font-size: 0.66rem;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: var(--faint);
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

.settings .bar {
  justify-content: space-between;
}

.settings-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: grid;
  align-content: start;
  gap: 0.6rem;
  width: min(44rem, 100%);
  margin: 0 auto;
  padding: 2rem 1.5rem 3rem;
}

/* Settings: its tabs down the side, and the open one beside them. */
.settings-body:has(> .settings-tabs) {
  width: min(60rem, 100%);
  grid-template-columns: 12rem minmax(0, 1fr);
  gap: 2.5rem;
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

/* A model's switch and Uninstall, in Settings. */
.model-buttons {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

button.switch.model-switch {
  width: auto;
  padding: 0.3rem 0.2rem;
  font-weight: 400;
}

.offer:has(.ask-uninstall) {
  flex-wrap: wrap;
}

/* While Uninstall asks, the model keeps its line and the question sits below. */
.offer:has(.ask-uninstall) .offer-text {
  flex-basis: 100%;
}

.ask-uninstall {
  flex: 1 1 14rem;
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

.grader-pick {
  display: grid;
  gap: 0.3rem;
  max-width: 26rem;
}

.grader-pick select {
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--page);
  padding: 0.4rem 0.6rem;
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
  margin: 0 auto 1.6rem;
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 0.4rem;
  user-select: text;
  -webkit-user-select: text;
}

.turn.user {
  justify-items: end;
}

.answers {
  min-width: 0;
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

/* An answer's grade, under its model's name. */
.grade {
  display: flex;
  align-items: baseline;
  gap: 0.45rem;
  margin: 0;
}

.grade-letter {
  flex: none;
  min-width: 1.7rem;
  padding: 0 0.35rem;
  border-radius: 6px;
  background: var(--accent);
  color: var(--accent-ink);
  font-weight: 700;
  text-align: center;
}

/* The grader's summary, under the answers it graded. */
.grade-card {
  display: grid;
  gap: 0.35rem;
  max-width: 72rem;
  margin: 0 auto 1.1rem;
  padding: 0.7rem 0.85rem;
  border: 1px solid var(--line);
  border-left: 3px solid var(--accent);
  border-radius: 10px;
  background: var(--panel);
  user-select: text;
  -webkit-user-select: text;
}

.grade-card .note {
  margin: 0;
}

.grade-ask {
  max-width: 72rem;
  margin: -0.4rem auto 1.1rem;
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

.bubble,
.thinking p {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.answer {
  overflow-wrap: anywhere;
}

.bubble {
  background: var(--bubble);
  border-radius: 18px 18px 6px 18px;
  padding: 0.6rem 1rem;
  max-width: 85%;
}

.answer {
  font-size: 1.02rem;
  line-height: 1.65;
}

.turn-foot {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.turn-foot .note {
  margin: 0;
}

.copy {
  display: inline-grid;
  place-items: center;
  width: 1.7rem;
  height: 1.7rem;
  padding: 0;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--faint);
  cursor: pointer;
}

.copy:hover,
.copy:focus-visible {
  background: var(--bubble);
  color: inherit;
}

.copy svg {
  width: 0.95rem;
  height: 0.95rem;
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.note {
  font-family: var(--mono);
  font-size: 0.68rem;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--faint);
}

.thinking {
  border-left: 2px solid var(--line);
  padding-left: 0.85rem;
  color: var(--quiet);
  font-size: 0.9rem;
}

.thinking summary {
  cursor: pointer;
  width: fit-content;
  font-family: var(--mono);
  font-size: 0.72rem;
  letter-spacing: 0.12em;
  text-transform: uppercase;
}

.thinking p {
  margin-top: 0.4rem;
}

.waiting {
  font-style: italic;
}

.problem {
  max-width: 46rem;
  margin: 0 auto 0.5rem;
  width: calc(100% - 3rem);
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--bad);
  border-radius: 10px;
  color: var(--bad);
  overflow-wrap: anywhere;
  white-space: pre-line;
}

.compose {
  padding: 0.5rem 1.5rem 0.9rem;
}

.box {
  display: grid;
  gap: 0.4rem;
  max-width: 54rem;
  margin: 0 auto;
  padding: 0.9rem 0.9rem 0.7rem 1.15rem;
  border: 1px solid var(--line);
  border-radius: 20px;
  background: var(--raised);
  box-shadow: var(--shadow);
  transition: border-color 0.15s;
}

.box:focus-within {
  border-color: var(--faint);
}

.box textarea {
  resize: none;
  border: 0;
  outline: none;
  background: none;
  padding: 0.15rem 0;
  max-height: 220px;
  font-size: 1.02rem;
}

.box textarea::placeholder {
  color: var(--quiet);
}

.box-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.model-note {
  font-family: var(--mono);
  font-size: 0.66rem;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--faint);
}

.send {
  display: grid;
  place-items: center;
  width: 2.4rem;
  height: 2.4rem;
  padding: 0;
  border: 0;
  border-radius: 12px;
  background: var(--accent);
  color: var(--accent-ink);
}

.send:hover:not(:disabled) {
  background: #ff7a45;
}

.send:disabled {
  background: var(--hover);
  color: var(--quiet);
  opacity: 1;
}

.send svg {
  stroke-width: 2;
}

.send.stop {
  background: var(--solid);
}

.stop-square {
  width: 0.7rem;
  height: 0.7rem;
  border-radius: 2px;
  background: var(--solid-ink);
}

.meter {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.6rem;
  max-width: 54rem;
  margin: 0.55rem auto 0;
  min-height: 1.2rem;
}

.meter .quiet,
.learned {
  font-family: var(--mono);
  font-size: 0.66rem;
  letter-spacing: 0.16em;
  text-transform: uppercase;
}

.meter .quiet {
  color: var(--faint);
}

.learned {
  color: var(--accent-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

progress {
  width: 7rem;
  height: 0.35rem;
  accent-color: var(--accent);
}

progress.near {
  accent-color: var(--bad);
}

/* Settings. */
.settings-tabs {
  display: grid;
  align-content: start;
  gap: 0.2rem;
}

.settings-tabs button {
  border: 0;
  text-align: left;
  padding: 0.55rem 0.9rem;
}

.settings-tabs button.here {
  background: var(--hover);
}

.settings-pane {
  display: grid;
  gap: 0.8rem;
  align-content: start;
}

textarea.memory,
.field input,
.field textarea {
  width: 100%;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--raised);
  padding: 0.8rem 1rem;
  outline: none;
  resize: vertical;
}

textarea.memory {
  font-family: var(--mono);
  font-size: 0.85rem;
  line-height: 1.7;
}

textarea.memory:focus,
.field input:focus,
.field textarea:focus {
  border-color: var(--faint);
}

.settings-actions {
  display: flex;
  align-items: center;
  gap: 0.8rem;
}

.check {
  display: flex;
  gap: 0.7rem;
  align-items: flex-start;
  margin-top: 0.8rem;
  padding-top: 1.2rem;
  border-top: 1px solid var(--line);
  cursor: pointer;
}

.check input {
  margin-top: 0.3rem;
  accent-color: var(--accent);
}

.segmented {
  display: inline-flex;
  width: fit-content;
  padding: 0.2rem;
  border: 1px solid var(--line);
  border-radius: 12px;
}

.segmented button {
  border: 0;
  padding: 0.4rem 1rem;
}

.segmented button.here {
  background: var(--solid);
  color: var(--solid-ink);
}

/* The project dialog. */
.dialog {
  width: min(36rem, calc(100% - 2rem));
  max-height: calc(100% - 3rem);
  padding: 1.6rem;
  border: 1px solid var(--line);
  border-radius: 18px;
  background: var(--panel);
  color: var(--ink);
  box-shadow: var(--shadow);
}

.dialog::backdrop {
  background: rgba(14, 13, 11, 0.55);
  backdrop-filter: blur(2px);
}

.dialog form {
  display: grid;
  gap: 1rem;
}

.dialog h2 {
  font-size: 2rem;
  white-space: normal;
}

.field {
  display: grid;
  gap: 0.3rem;
}

.field > span:first-child {
  font-weight: 500;
}

.dialog-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-top: 0.3rem;
}

.dialog-actions .ask {
  flex: 0 1 auto;
  padding: 0;
}

.spacer {
  flex: 1;
}

@media (max-width: 760px) {
  .app {
    grid-template-columns: 14rem minmax(0, 1fr);
  }

  .bar-title,
  .picks-label,
  .chip-detail {
    display: none;
  }

  .settings-body,
  .settings-body:has(> .settings-tabs) {
    grid-template-columns: minmax(0, 1fr);
    gap: 1rem;
  }

  .settings-tabs {
    display: flex;
  }
}

.tool {
  margin: 0.6rem 0 0.2rem;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--raised);
  padding: 0.55rem 0.75rem;
}

.tool.asked {
  border-color: var(--accent);
}

.tool-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
}

.tool-label {
  font-family: var(--mono);
  font-size: 0.78rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tool-status {
  font-family: var(--mono);
  font-size: 0.62rem;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: var(--faint);
  flex: none;
}

.tool.asked .tool-status {
  color: var(--accent);
}

.tool.failed .tool-status {
  color: var(--danger, #c0392b);
}

.tool pre {
  margin: 0.5rem 0 0;
  max-height: 18rem;
  overflow: auto;
  font-family: var(--mono);
  font-size: 0.74rem;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}

.change .gone {
  color: #b4391f;
  background: color-mix(in srgb, #b4391f 10%, transparent);
}

.change .new {
  color: #2f7d4a;
  background: color-mix(in srgb, #2f7d4a 10%, transparent);
}

.tool-actions {
  display: flex;
  gap: 0.5rem;
  margin-top: 0.6rem;
}

.tool-more summary {
  margin-top: 0.35rem;
  cursor: pointer;
  font-size: 0.78rem;
  color: var(--quiet);
}

.folder-row {
  display: flex;
  gap: 0.5rem;
}

.folder-row input {
  flex: 1;
  min-width: 0;
  font-family: var(--mono);
  font-size: 0.8rem;
}

.prose > :first-child {
  margin-top: 0;
}

.prose > :last-child {
  margin-bottom: 0;
}

.prose p,
.prose ul,
.prose ol,
.prose pre,
.prose blockquote,
.prose table,
.prose .math {
  margin: 0 0 0.9em;
}

.prose h1,
.prose h2,
.prose h3,
.prose h4 {
  font-family: var(--sans);
  font-weight: 600;
  line-height: 1.3;
  margin: 1.4em 0 0.5em;
}

.prose h1 {
  font-size: 1.35rem;
}

.prose h2 {
  font-size: 1.2rem;
}

.prose h3,
.prose h4 {
  font-size: 1.05rem;
}

.prose ul,
.prose ol {
  padding-left: 1.4em;
}

.prose li {
  margin: 0.3em 0;
}

.prose li > p {
  margin: 0.3em 0;
}

.prose strong {
  font-weight: 600;
}

.prose a {
  color: var(--accent);
  text-decoration: underline;
  text-underline-offset: 2px;
  cursor: default;
}

.prose code {
  font-family: var(--mono);
  font-size: 0.86em;
  background: var(--bubble);
  border-radius: 5px;
  padding: 0.1em 0.35em;
}

.prose pre {
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 0.8rem 1rem;
  overflow-x: auto;
  line-height: 1.5;
}

.prose pre code {
  background: none;
  padding: 0;
  font-size: 0.82rem;
  overflow-wrap: normal;
  white-space: pre;
}

.prose blockquote {
  border-left: 3px solid var(--line);
  padding-left: 0.9em;
  color: var(--quiet);
}

.prose hr {
  border: 0;
  border-top: 1px solid var(--line);
  margin: 1.5em 0;
}

.prose table {
  border-collapse: collapse;
  display: block;
  overflow-x: auto;
  font-size: 0.94rem;
}

.prose th,
.prose td {
  border: 1px solid var(--line);
  padding: 0.4em 0.7em;
  text-align: left;
}

.prose th {
  font-weight: 600;
  background: var(--raised);
}

.prose .math {
  overflow-x: auto;
  overflow-y: hidden;
}

.prose .katex-display {
  margin: 0;
}

.drop {
  position: absolute;
  inset: 0.75rem;
  z-index: 20;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.4rem;
  border: 2px dashed var(--accent);
  border-radius: 18px;
  background: color-mix(in srgb, var(--panel) 88%, transparent);
  pointer-events: none;
}

.drop .display {
  font-size: 2rem;
  margin: 0;
}

.box-row .model-note {
  flex: 1;
  margin-left: 0.4rem;
}

.attach svg {
  width: 1.05rem;
  height: 1.05rem;
}

.files {
  list-style: none;
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin: 0 0 0.6rem;
  padding: 0;
}

.files.sent {
  justify-content: flex-end;
  margin: 0 0 0.4rem;
}

.file {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  max-width: 16rem;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--raised);
  padding: 0.45rem 0.6rem;
}

.file > svg {
  color: var(--accent);
}

.file.reading {
  padding: 0.6rem 0.8rem;
}

.file-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.file-name {
  font-size: 0.86rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-line {
  font-family: var(--mono);
  font-size: 0.62rem;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--faint);
}

.file-x {
  display: inline-grid;
  place-items: center;
  border: 0;
  background: none;
  padding: 0.15rem;
  color: var(--quiet);
  border-radius: 6px;
}

.file-x:hover {
  color: var(--ink);
}

.file-x svg {
  width: 0.85rem;
  height: 0.85rem;
}

.too-big {
  margin: 0 0 0.5rem;
  color: var(--bad);
}
</style>
