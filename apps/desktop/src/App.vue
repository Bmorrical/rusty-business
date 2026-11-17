<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Channel, invoke } from "@tauri-apps/api/core";

import {
  dayLabel,
  formatCount,
  formatWindow,
  fullness,
  onDiskLine,
  requestMessages,
  savedTurn,
  shouldStart,
  startLine,
  titleFrom,
  tokensPerSecond,
  tokensUsed,
} from "./chat.js";

// The page only works inside the app's own window, where Rust answers.
const inApp = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const engine = ref(null);
const checking = ref(false);
const starting = ref(false);
const startNote = ref("");
const modelId = ref("");
const sessions = ref([]);
const current = ref(blankSession());
const draft = ref("");
const busy = ref(false);
const problem = ref("");
const askTrash = ref("");
const thread = ref(null);
const composer = ref(null);

let chatId = "";
let stickToBottom = true;

function blankSession() {
  return { id: "", title: "", model: "", createdAt: 0, updatedAt: 0, messages: [] };
}

const models = computed(() => engine.value?.models ?? []);
const model = computed(() => models.value.find((entry) => entry.id === modelId.value) ?? null);
const used = computed(() => tokensUsed(current.value.messages));
const full = computed(() => fullness(used.value, model.value?.context));
const canSend = computed(() => Boolean(draft.value.trim()) && !busy.value && Boolean(model.value));
const engineAddress = computed(() => (engine.value?.url ?? "").replace(/^http:\/\//, "").replace(/\/v1$/, ""));

function remembered() {
  try {
    return window.localStorage.getItem("model") ?? "";
  } catch {
    return "";
  }
}

function remember(id) {
  try {
    window.localStorage.setItem("model", id);
  } catch {
    // The choice just is not kept for next time.
  }
}

function pickModel(preferred) {
  const ids = models.value.map((entry) => entry.id);
  const choice = [preferred, modelId.value, remembered(), ids[0]].find((id) => id && ids.includes(id));
  modelId.value = choice ?? "";
}

watch(modelId, (id) => {
  if (id) remember(id);
});

async function checkEngine() {
  if (!inApp || checking.value) return;
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
  problem.value = "";
  askTrash.value = "";
  focusComposer();
}

async function openSession(id) {
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
  try {
    await invoke("trash_session", { id });
    if (current.value.id === id) current.value = blankSession();
    await refreshSessions();
  } catch (error) {
    problem.value = String(error);
  }
}

async function send() {
  if (!canSend.value) return;
  const text = draft.value.trim();
  const session = current.value;
  const chosen = model.value;
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

  const messages = requestMessages(session.messages);
  session.messages.push({ role: "assistant", content: "", thinking: "", model: chosen.id, pending: true });
  const reply = session.messages[session.messages.length - 1];
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
}

function stop() {
  if (busy.value && chatId) invoke("stop_chat", { chatId });
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

onMounted(async () => {
  if (!inApp) return;
  window.addEventListener("focus", checkEngine);
  await Promise.all([checkEngine(), refreshSessions()]);
  focusComposer();
});

onBeforeUnmount(() => window.removeEventListener("focus", checkEngine));
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
                <span class="quiet small">{{ dayLabel(entry.updatedAt) }} · {{ modelName(entry.model) || "model not on hand" }}</span>
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
          <p v-if="!starting" class="quiet small">To start it yourself:</p>
          <code v-if="!starting" class="hint">{{ engine.startHint }}</code>
          <button v-if="!starting" class="wide" type="button" :disabled="checking" @click="checkEngine">Try again</button>
        </template>
        <p class="quiet small">Nothing you type leaves this computer.</p>
      </footer>
    </aside>

    <main class="main">
      <header class="bar">
        <h2>{{ current.title || "New chat" }}</h2>
        <label class="picker">
          <span class="quiet small">Model</span>
          <select v-model="modelId" :disabled="busy || !models.length">
            <option v-if="!models.length" value="">None on hand</option>
            <option v-for="entry in models" :key="entry.id" :value="entry.id">
              {{ entry.name }} · {{ entry.maker }} · {{ formatWindow(entry.context) }}
            </option>
          </select>
        </label>
      </header>

      <section ref="thread" class="thread" @scroll="onScroll">
        <div v-if="!current.messages.length" class="empty">
          <p class="lead">Ask anything.</p>
          <p v-if="model" class="quiet">
            {{ model.name }} by {{ model.maker }} answers, with room for {{ formatCount(model.context) }} tokens.
          </p>
          <p v-else-if="engine?.reachable" class="quiet">The engine has none of this app's models on hand.</p>
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
            rows="1"
            aria-label="Your message"
            :placeholder="model ? `Message ${model.name}` : 'No model on hand'"
            :disabled="!model"
            @input="resizeComposer"
            @keydown="onKeydown"
          ></textarea>
          <button v-if="busy" type="button" @click="stop">Stop</button>
          <button v-else class="primary" type="button" :disabled="!canSend" @click="send">Send</button>
        </div>
        <div class="meter">
          <template v-if="model && used">
            <progress :value="full" max="1" :class="{ near: full >= 0.8 }" aria-label="How full the model's window is"></progress>
            <span class="quiet small">
              {{ formatCount(used) }} of {{ formatCount(model.context) }} tokens
              <template v-if="full >= 0.8"> · nearly full, start a new chat soon</template>
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

.sessions {
  flex: 1;
  min-height: 0;
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

.engine {
  display: grid;
  gap: 0.4rem;
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
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.7rem 1.25rem;
  border-bottom: 1px solid var(--line);
}

.picker {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-shrink: 0;
}

select {
  border: 1px solid var(--line);
  background: var(--panel);
  border-radius: 8px;
  padding: 0.35rem 0.5rem;
  max-width: 19rem;
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

  .picker .quiet {
    display: none;
  }
}
</style>
