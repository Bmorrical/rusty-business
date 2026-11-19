<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { Channel, invoke } from "@tauri-apps/api/core";

import {
  arrangeSessions,
  checkedLine,
  downloaded,
  downloadLine,
  firstRunOffers,
  formatCount,
  formatSize,
  formatWindow,
  fullness,
  greeting,
  memoryRequest,
  onDiskLine,
  parseFacts,
  quickest,
  requestMessages,
  savedTurn,
  shouldStart,
  showsStartHint,
  showsUpdate,
  stackLine,
  startLine,
  systemPrompt,
  titleFrom,
  tokensPerSecond,
  tokensUsed,
  UPDATE_EVERY,
  updateDetail,
  updateLine,
} from "./chat.js";

// The page only works inside the app's own window, where Rust answers.
const inApp = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// Line icons, drawn at 24 by 24 with a round stroke.
const ICONS = {
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
const modelId = ref("");
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
let updateTimer = 0;

// Projects, memory, search, and the side's own state.
const projects = ref([]);
const memory = ref("");
const memoryDraft = ref("");
const memorySaved = ref("");
const learnOn = ref(stored("learn", "on") !== "off");
const learnedNote = ref("");
const theme = ref(stored("theme", "system"));
const sideOpen = ref(stored("side", "open") !== "closed");
const closedProjects = ref({});
const query = ref("");
const found = ref([]);
const searchBox = ref(null);
const menuFor = ref("");
const view = ref("chat");
const settingsTab = ref("memory");
const projectDialog = ref(null);
const editing = ref(null);
const askTrashProject = ref(false);
const now = ref(Date.now());
let searchTimer = 0;
let learnedTimer = 0;
let clockTimer = 0;
let learning = Promise.resolve();

let chatId = "";
let checkAgain = false;
let stickToBottom = true;

function blankSession(project = null) {
  return { id: "", title: "", model: "", createdAt: 0, updatedAt: 0, messages: [], project, pinned: false };
}

const models = computed(() => engine.value?.models ?? []);
const model = computed(() => models.value.find((entry) => entry.id === modelId.value) ?? null);
const used = computed(() => tokensUsed(current.value.messages));
const full = computed(() => fullness(used.value, model.value?.context));
const canSend = computed(() => Boolean(draft.value.trim()) && !busy.value && Boolean(model.value));
const offers = computed(() => firstRunOffers(engine.value));
const quick = computed(() => quickest(offers.value));
const handStart = computed(() => showsStartHint(engine.value, starting.value));
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
  return stored("model", "");
}

function remember(id) {
  store("model", id);
}

function pickModel(preferred) {
  const ids = models.value.map((entry) => entry.id);
  const choice = [preferred, modelId.value, remembered(), ids[0]].find((id) => id && ids.includes(id));
  modelId.value = choice ?? "";
}

watch(modelId, (id) => {
  if (id) remember(id);
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
watch(sideOpen, (open) => store("side", open ? "open" : "closed"));

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
    pickModel(current.value.model);
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
    if (report.whole) await checkEngine();
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
  try {
    memory.value = await invoke("load_memory");
  } catch {
    memory.value = "";
  }
  memoryDraft.value = memory.value;
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
  view.value = "chat";
  problem.value = "";
  askTrash.value = "";
  menuFor.value = "";
  if (project) closedProjects.value = { ...closedProjects.value, [project]: false };
  focusComposer();
}

async function openSession(id) {
  view.value = "chat";
  menuFor.value = "";
  if (busy.value || id === current.value.id) return;
  problem.value = "";
  askTrash.value = "";
  try {
    current.value = await invoke("load_session", { id });
    pickModel(current.value.model);
    stickToBottom = true;
    scrollDown();
    focusComposer();
  } catch (error) {
    problem.value = String(error);
  }
}

async function trashSession(id) {
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
    : { id: "", name: "", instructions: "", memory: "", createdAt: 0, updatedAt: 0 };
  nextTick(() => projectDialog.value?.showModal());
}

function closeProject() {
  projectDialog.value?.close();
  editing.value = null;
}

async function saveProject() {
  if (!editing.value) return;
  try {
    const saved = await invoke("save_project", { project: editing.value });
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

function openSettings(tab = "memory") {
  menuFor.value = "";
  settingsTab.value = tab;
  memoryDraft.value = memory.value;
  memorySaved.value = "";
  view.value = "settings";
}

/**
 * After a reply, asks the same model whether the person's message holds
 * anything worth remembering, and adds it to the global memory or the
 * project's. A model that strays or fails saves nothing.
 */
async function learn(message, chosenId, project) {
  if (!learnOn.value || !chosenId) return;
  try {
    const done = await invoke("chat_stream", {
      chatId: `memory-${Date.now()}-${Math.random().toString(36).slice(2)}`,
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
  }
}

async function send() {
  if (!canSend.value) return;
  const text = draft.value.trim();
  const session = current.value;
  const chosen = model.value;
  const project = currentProject.value;
  problem.value = "";
  if (!session.messages.length) session.title = titleFrom(text);
  session.model = chosen.id;
  session.messages.push({ role: "user", content: text });
  draft.value = "";
  busy.value = true;
  stickToBottom = true;
  resizeComposer();
  scrollDown();
  // Saved before the model answers, so the question survives a crash.
  await persist();

  const messages = requestMessages(session.messages, systemPrompt(memory.value, project));
  session.messages.push({ role: "assistant", content: "", thinking: "", model: chosen.id, pending: true });
  const reply = session.messages[session.messages.length - 1];
  // The engine answers one at a time, so a look for memories still under way
  // finishes first.
  await learning;
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
  let answered = false;
  try {
    const done = await invoke("chat_stream", {
      chatId,
      request: { model: chosen.id, messages, maxTokens: null },
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
    answered = !done.stopped;
  } catch (error) {
    finished = true;
    problem.value = String(error);
    checkEngine();
  } finally {
    reply.pending = false;
    if (!reply.content && !reply.thinking) session.messages.pop();
    chatId = "";
    busy.value = false;
    await persist();
    scrollDown();
    focusComposer();
  }
  if (answered) learning = learn(text, chosen.id, project);
}

function stop() {
  if (busy.value && chatId) invoke("stop_chat", { chatId });
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
  return models.value.find((entry) => entry.id === id)?.name ?? "";
}

function replyNote(turn) {
  const parts = [modelName(turn.model)];
  if (turn.completionTokens) parts.push(`${formatCount(turn.completionTokens)} tokens`);
  const speed = tokensPerSecond(turn);
  if (speed) parts.push(`${formatCount(speed)} a second`);
  if (turn.cutShort) parts.push("reached the length limit");
  else if (turn.stopped) parts.push("stopped");
  return parts.filter(Boolean).join(" · ");
}

applyTheme();

onMounted(async () => {
  if (!inApp) return;
  window.addEventListener("focus", checkEngine);
  window.addEventListener("keydown", onWindowKey);
  window.addEventListener("click", onWindowClick);
  clockTimer = window.setInterval(() => (now.value = Date.now()), 60_000);
  // The stack can take minutes, so the page does not wait for it.
  checkStack();
  // So does the check for a new version, which asks GitHub.
  checkUpdate();
  updateTimer = window.setInterval(checkUpdate, UPDATE_EVERY);
  await Promise.all([checkEngine(), refreshSessions(), refreshProjects(), loadMemory()]);
  focusComposer();
});

onBeforeUnmount(() => {
  window.removeEventListener("focus", checkEngine);
  window.removeEventListener("keydown", onWindowKey);
  window.removeEventListener("click", onWindowClick);
  window.clearInterval(updateTimer);
  window.clearInterval(clockTimer);
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
                  <button class="link" type="button" @click="trashSession(entry.id)">Yes</button>
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
                    <button type="button" role="menuitem" class="danger" @click="(askTrash = entry.id), (menuFor = '')">Move to the trash</button>
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
                  <button class="link" type="button" @click="trashSession(entry.id)">Yes</button>
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
                    <button type="button" role="menuitem" class="danger" @click="(askTrash = entry.id), (menuFor = '')">Move to the trash</button>
                  </div>
                </template>
              </li>
            </ul>
          </template>
          <p v-if="!sessions.length" class="quiet pad small">Chats are saved here as you go.</p>
        </template>
      </nav>

      <button class="settings-link" type="button" :class="{ here: view === 'settings' }" @click="openSettings()">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.settings" /></svg>
        <span>Settings &amp; memory</span>
      </button>

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
          <button class="wide" type="button" aria-describedby="stack-reason" @click="checkStack">Try again</button>
        </template>
        <button class="wide" type="button" :disabled="updateChecking" @click="checkNow">
          {{ updateChecking ? "Checking..." : "Check for updates" }}
        </button>
        <p v-if="updateChecked" class="small">{{ updateChecked }}</p>
        <p class="quiet small">Nothing you type leaves this computer.</p>
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

      <template v-if="view === 'settings'">
        <header class="bar">
          <button v-if="!sideOpen" class="icon" type="button" aria-label="Show the side" title="Show the side" @click="sideOpen = true">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.side" /></svg>
          </button>
          <button class="back" type="button" @click="view = 'chat'">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.back" /></svg>
            Back to chat
          </button>
        </header>
        <section class="settings">
          <div class="settings-head">
            <h2 class="display">Settings</h2>
            <p class="quiet">What the assistant remembers, and how the window looks.</p>
          </div>
          <div class="settings-body">
            <nav class="settings-tabs" aria-label="Settings">
              <button type="button" :class="{ here: settingsTab === 'memory' }" @click="settingsTab = 'memory'">Memory</button>
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
                  <span class="quiet small block">After each reply, the same model looks for lasting facts in your message and adds them here. It runs on this computer, like everything else.</span>
                </span>
              </label>
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
      </template>

      <template v-else>
        <header class="bar">
          <button v-if="!sideOpen" class="icon" type="button" aria-label="Show the side" title="Show the side" @click="sideOpen = true">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.side" /></svg>
          </button>
          <label class="picker">
            <span class="dot" :class="engineOn ? 'live' : 'off'" aria-hidden="true"></span>
            <span class="sr-only">Model</span>
            <select v-model="modelId" :disabled="busy || !models.length">
              <option v-if="!models.length" value="">None on hand</option>
              <option v-for="entry in models" :key="entry.id" :value="entry.id">
                {{ entry.name }} · {{ entry.maker }} · {{ formatWindow(entry.context) }}
              </option>
            </select>
            <svg class="chevron" viewBox="0 0 24 24" aria-hidden="true"><path :d="ICONS.chevron" /></svg>
          </label>
          <h2 class="bar-title">
            <span v-if="currentProject" class="crumb">{{ currentProject.name }} /</span>
            {{ current.title || "New chat" }}
          </h2>
        </header>

        <section ref="thread" class="thread" @scroll="onScroll">
          <div v-if="!current.messages.length" class="empty">
            <div v-if="offers.length" class="first-run">
              <p class="display lead">Pick a model to start.</p>
              <p class="quiet">Each one is a single download from Hugging Face. Nothing downloads until you press Download.</p>
              <ul class="offers" aria-label="Models to download">
                <li v-for="offer in offers" :key="offer.id" class="offer">
                  <span class="offer-text">
                    <span class="offer-name">{{ offer.name }}</span>
                    <span class="quiet small">
                      by {{ offer.maker }} · {{ formatSize(offer.bytes) }}{{ offer.id === quick ? " · quickest to start" : "" }}
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
                    :class="{ primary: offer.id === quick }"
                    type="button"
                    :disabled="Boolean(downloading)"
                    @click="download(offer)"
                  >
                    Download
                  </button>
                </li>
              </ul>
              <p v-if="stoppedNote" class="quiet small">{{ stoppedNote }}</p>
            </div>
            <div v-else class="hello">
              <p class="display lead">{{ currentProject ? currentProject.name : hello }}.</p>
              <p v-if="model" class="sub">
                {{ currentProject ? "A new chat in this project." : "What are we thinking about?" }}
              </p>
              <p v-else-if="starting" class="sub">Starting the engine...</p>
              <p v-else-if="engine?.reachable" class="sub">The engine has none of this app's models on hand.</p>
              <div v-if="model && !currentProject" class="pills">
                <button v-for="entry in SUGGESTIONS" :key="entry.verb" class="pill" type="button" @click="suggest(entry)">
                  <strong>{{ entry.verb }}</strong> {{ entry.rest }}
                </button>
              </div>
            </div>
          </div>

          <article v-for="(turn, index) in current.messages" :key="index" class="turn" :class="turn.role">
            <template v-if="turn.role === 'user'">
              <p class="bubble">{{ turn.content }}</p>
            </template>
            <template v-else>
              <details v-if="turn.thinking" class="thinking">
                <summary>{{ turn.pending && !turn.content ? "Thinking..." : "Thinking" }}</summary>
                <p>{{ turn.thinking }}</p>
              </details>
              <p v-if="turn.content" class="answer">{{ turn.content }}</p>
              <p v-else-if="turn.pending && !turn.thinking" class="quiet waiting">Waiting for the model...</p>
              <p v-if="!turn.pending" class="quiet small note">{{ replyNote(turn) }}</p>
            </template>
          </article>
        </section>

        <p v-if="problem" class="problem" role="alert">
          {{ problem }}
          <button class="link" type="button" @click="problem = ''">Dismiss</button>
        </p>

        <footer class="compose">
          <div class="box">
            <textarea
              ref="composer"
              v-model="draft"
              rows="2"
              aria-label="Your message"
              :placeholder="model ? 'Ask anything...' : 'No model on hand'"
              :disabled="!model"
              @input="resizeComposer"
              @keydown="onKeydown"
            ></textarea>
            <div class="box-row">
              <span class="quiet small model-note">{{ model ? model.name : "" }}</span>
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
            <template v-else-if="model && used">
              <progress :value="full" max="1" :class="{ near: full >= 0.8 }" aria-label="How full the model's window is"></progress>
              <span class="quiet small">
                {{ formatCount(used) }} of {{ formatCount(model.context) }} tokens
                <template v-if="full >= 0.8"> · nearly full, start a new chat soon</template>
              </span>
            </template>
            <span v-else class="quiet small">Can make mistakes. Check what matters.</span>
          </div>
        </footer>
      </template>
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

.sessions {
  flex: 1;
  min-height: 0;
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

.more,
.row-action {
  visibility: hidden;
}

.sessions li:hover .more,
.more:focus-visible,
.project-row:hover .row-action,
.row-action:focus-visible {
  visibility: visible;
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

.settings-link {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  margin: 0 0.5rem 0.4rem;
  padding: 0.5rem 0.75rem;
  border: 0;
  text-align: left;
  color: var(--quiet);
}

.settings-link.here,
.settings-link:hover {
  color: var(--ink);
}

.engine {
  display: grid;
  gap: 0.35rem;
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
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--panel);
}

.bar {
  display: flex;
  align-items: center;
  gap: 1rem;
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

.picker {
  display: flex;
  align-items: center;
  flex-shrink: 0;
  min-width: 0;
}

.picker .chevron {
  width: 0.85rem;
  height: 0.85rem;
  margin-left: -1.35rem;
  color: var(--quiet);
  pointer-events: none;
}

.picker .dot {
  margin-right: 0.6rem;
}

select {
  appearance: none;
  -webkit-appearance: none;
  border: 0;
  background: transparent;
  border-radius: 8px;
  padding: 0.35rem 1.6rem 0.35rem 0.35rem;
  max-width: 24rem;
  font-family: var(--mono);
  font-size: 0.78rem;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  cursor: pointer;
}

select:hover:not(:disabled) {
  background-color: var(--hover);
}

option {
  background: var(--raised);
  color: var(--ink);
  text-transform: none;
  letter-spacing: 0;
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

.offer-progress {
  display: grid;
  gap: 0.2rem;
  justify-items: end;
}

.turn {
  max-width: 46rem;
  margin: 0 auto 1.6rem;
  display: grid;
  gap: 0.4rem;
  user-select: text;
  -webkit-user-select: text;
}

.turn.user {
  justify-items: end;
}

.bubble,
.answer,
.thinking p {
  white-space: pre-wrap;
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
.settings {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 2.5rem 2rem 3rem;
}

.settings-head {
  max-width: 60rem;
  margin: 0 auto 2rem;
  display: grid;
  gap: 0.3rem;
}

.settings-head h2 {
  font-size: 3rem;
  line-height: 1;
  white-space: normal;
}

.settings-body {
  max-width: 60rem;
  margin: 0 auto;
  display: grid;
  grid-template-columns: 12rem minmax(0, 1fr);
  gap: 2.5rem;
}

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

  .bar-title {
    display: none;
  }

  .settings-body {
    grid-template-columns: minmax(0, 1fr);
    gap: 1rem;
  }

  .settings-tabs {
    display: flex;
  }
}
</style>
