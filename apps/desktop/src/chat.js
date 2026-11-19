// Small pure helpers for the chat window. No Tauri and no Vue in here, so
// `npm test` can run them with Node alone.

const TITLE_LIMIT = 60;

/** A session's title: the first line of its first message, cut to fit. */
export function titleFrom(text) {
  const line =
    String(text ?? "")
      .split("\n")
      .map((part) => part.replace(/\s+/g, " ").trim())
      .find((part) => part) ?? "";
  if (!line) return "New chat";
  const characters = Array.from(line);
  if (characters.length <= TITLE_LIMIT) return line;
  return `${characters.slice(0, TITLE_LIMIT - 3).join("").trimEnd()}...`;
}

/**
 * What goes to the engine: role and answer only. Thinking is never sent
 * back, an empty turn is left out, and two turns in a row from the same
 * side (a reply that was stopped before it began) are joined, because some
 * models refuse a chat whose turns do not alternate.
 */
export function requestMessages(turns, system = "") {
  const messages = [];
  const instructions = String(system ?? "").trim();
  if (instructions) messages.push({ role: "system", content: instructions });
  for (const turn of turns ?? []) {
    const content = String(turn.content ?? "").trim();
    if (!content || !["system", "user", "assistant"].includes(turn.role)) continue;
    const last = messages[messages.length - 1];
    if (last && last.role === turn.role && turn.role !== "system") {
      last.content = `${last.content}\n\n${content}`;
    } else {
      messages.push({ role: turn.role, content });
    }
  }
  return messages;
}

/** Tokens the chat held after its latest measured reply, or null if none. */
export function tokensUsed(turns) {
  for (let index = (turns ?? []).length - 1; index >= 0; index -= 1) {
    const turn = turns[index];
    if (turn.role === "assistant" && Number.isFinite(turn.promptTokens)) {
      return turn.promptTokens + (turn.completionTokens ?? 0);
    }
  }
  return null;
}

/** How full the model's window is, from 0 to 1. */
export function fullness(used, context) {
  if (!used || !context) return 0;
  return Math.min(1, used / context);
}

export function formatCount(count) {
  return Number(count ?? 0).toLocaleString("en-US");
}

/** "16K" or "262K": a context window in thousands of tokens. */
export function formatWindow(context) {
  return `${Math.round(Number(context ?? 0) / 1024)}K`;
}

/**
 * One sentence naming the app's models that are on this computer's disk,
 * each name once, in the order given.
 */
export function onDiskLine(models) {
  const names = [...new Set((models ?? []).map((entry) => entry.name))];
  if (!names.length) return "None of this app's models is on this computer yet.";
  if (names.length === 1) return `On this computer: ${names[0]}.`;
  if (names.length === 2) return `On this computer: ${names[0]} and ${names[1]}.`;
  return `On this computer: ${names.slice(0, -1).join(", ")}, and ${names.at(-1)}.`;
}

/**
 * True when the app's report says no engine answers, which is when the app
 * starts one. An engine that answers is never started again.
 */
export function shouldStart(engine) {
  return Boolean(engine) && engine.reachable === false;
}

/**
 * The line under the engine's own after the app tried to start one: why it
 * could not, in the app's words. Nothing when an engine answers, when no
 * reason came back, or when the reason is the line already shown above it.
 */
export function startLine(start, engine) {
  const reason = (start?.reason ?? "").trim();
  if (!reason || start?.answering || engine?.reachable || reason === engine?.error) return "";
  return reason;
}

/**
 * True when the footer should show the command that starts the engine by
 * hand. Not while the app starts one, and not where there is no such
 * command: Windows and Linux have no engine yet, so the app sends no
 * command for them.
 */
export function showsStartHint(engine, starting) {
  return !starting && Boolean((engine?.startHint ?? "").trim());
}

/** What the footer says while the app starts the stack. */
export const STACK_STARTING = "Starting the stack. The first start downloads it, which takes a few minutes.";

/**
 * The footer's line about the stack: that it is starting, where its front
 * door answers once it runs, or why it is not running, in the app's words.
 * Nothing before the app has asked.
 */
export function stackLine(stack, starting) {
  if (starting) return STACK_STARTING;
  if (!stack) return "";
  if (stack.running) return `Stack on ${stack.address}`;
  return (stack.reason ?? "").trim() || "The stack is not running.";
}

/** Tokens a second for one reply, or null when it cannot be worked out. */
export function tokensPerSecond(turn) {
  if (!turn?.completionTokens || !turn?.seconds || turn.seconds < 0.05) return null;
  return Math.round(turn.completionTokens / turn.seconds);
}

/** "Today", "Yesterday", or a short date. */
export function dayLabel(milliseconds, now = Date.now()) {
  if (!milliseconds) return "";
  const startOfDay = (value) => {
    const date = new Date(value);
    return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  };
  const days = Math.round((startOfDay(now) - startOfDay(milliseconds)) / 86_400_000);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  return new Date(milliseconds).toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

/** A turn as it is saved: no window-only fields, no empty ones. */
export function savedTurn(turn) {
  const saved = { role: turn.role, content: turn.content ?? "" };
  for (const key of ["thinking", "model", "promptTokens", "completionTokens", "seconds", "stopped", "cutShort"]) {
    const value = turn[key];
    if (value !== undefined && value !== null && value !== "" && value !== false) saved[key] = value;
  }
  return saved;
}

/** "8.3 GB": a download's size, the way the app's own sentences give it. */
export function formatSize(bytes) {
  return `${(Number(bytes ?? 0) / 1e9).toFixed(1)} GB`;
}

/**
 * The models the page offers to download, in the app's order. Only on a first
 * run: when none of the app's models is on this computer's disk and the engine
 * serves none of them either. Otherwise nothing is offered.
 */
export function firstRunOffers(engine) {
  if (!engine || engine.onDisk?.length || engine.models?.length) return [];
  return engine.offers ?? [];
}

/** The offer that gets talking soonest: the smallest download. */
export function quickest(offers) {
  let best = null;
  for (const offer of offers ?? []) {
    if (!best || offer.bytes < best.bytes) best = offer;
  }
  return best?.id ?? "";
}

/** How far a download has come, from 0 to 1. */
export function downloaded(progress) {
  if (!progress?.total) return 0;
  return Math.min(1, Math.max(0, progress.done / progress.total));
}

/** "3.1 of 8.3 GB": how far a download has come, in words. */
export function downloadLine(progress) {
  const total = Number(progress?.total ?? 0);
  const done = Math.min(Number(progress?.done ?? 0), total);
  return `${(done / 1e9).toFixed(1)} of ${formatSize(total)}`;
}

/** How often the page asks whether a new version is out, while it stays open. */
export const UPDATE_EVERY = 24 * 60 * 60 * 1000;

/**
 * Whether to show the notice of a new version: when one was found, and the
 * person has not chosen Later for that same version.
 */
export function showsUpdate(update, later) {
  return Boolean(update?.version) && update.version !== later;
}

/** "Rusty Business™ 0.5.0 is ready.": the notice's first line. */
export function updateLine(update) {
  return `Rusty Business™ ${update.version} is ready.`;
}

/** "This is 0.4.0. Download opens Rusty-Business-mac-arm64.dmg in your browser." */
export function updateDetail(update) {
  return `This is ${update.current}. Download opens ${update.file} in your browser.`;
}

/**
 * What the Check for updates button found: the new version, or that this one,
 * `current`, is the newest.
 */
export function checkedLine(update, current) {
  if (update?.version) return updateLine(update);
  return current ? `Rusty Business™ ${current} is the newest version.` : "This is the newest version.";
}

/**
 * What the model is told before a chat: what it remembers about the person,
 * and, in a project, the project's name, instructions, and memory. Empty when
 * there is nothing to tell.
 */
export function systemPrompt(memory, project) {
  const parts = [];
  const section = (heading, text) => {
    const body = String(text ?? "").trim();
    if (body) parts.push(`${heading}:\n<memory>\n${body}\n</memory>`);
  };
  section("What you remember about the user", memory);
  if (project) {
    parts.push(`This conversation is part of the user's "${project.name}" project.`);
    const instructions = String(project.instructions ?? "").trim();
    if (instructions) parts.push(`<project_instructions>\n${instructions}\n</project_instructions>`);
    section("What you remember about this project", project.memory);
  }
  if (!parts.length) return "";
  return [
    "You have a long-term memory. Use what you remember naturally, without announcing it. When the user asks you to remember something, acknowledge it briefly.",
    ...parts,
  ].join("\n\n");
}

/**
 * The request that asks the model what, in the person's latest message, is
 * worth remembering. It answers with JSON alone.
 */
export function memoryRequest(message, memory, project) {
  const scopes = project
    ? `The chat is part of the "${project.name}" project. Use "project" for facts that only matter to it, and "user" for facts about the user in general.`
    : 'Use "user" for every fact.';
  const known = [memory, project?.memory].map((text) => String(text ?? "").trim()).filter(Boolean).join("\n");
  const system = [
    "You keep a chat assistant's long-term memory about the user. Read the user's message and list new facts worth remembering in later chats.",
    "Save a fact when the user asks you to remember something, or shares a lasting detail about themselves, their preferences, their work, or their plans. Do not save questions, one-off requests, temporary details, or anything already remembered. Most messages hold nothing worth saving, so an empty list is the usual answer.",
    'Write each fact as a short sentence, like "Lives in Denver". Answer with JSON only, in this shape: {"memories":[{"content":"...","scope":"user"}]}',
    scopes,
    known ? `Already remembered:\n<memory>\n${known}\n</memory>` : "",
  ]
    .filter(Boolean)
    .join("\n\n");
  return [
    { role: "system", content: system },
    { role: "user", content: String(message ?? "") },
  ];
}

/**
 * The facts in the model's answer to memoryRequest, split by where they go.
 * An answer that is not the JSON asked for gives none: a small model that
 * strays saves nothing rather than something wrong.
 */
export function parseFacts(answer, inProject) {
  const facts = { user: [], project: [] };
  const text = String(answer ?? "");
  const start = text.indexOf("{");
  const end = text.lastIndexOf("}");
  if (start < 0 || end <= start) return facts;
  let parsed;
  try {
    parsed = JSON.parse(text.slice(start, end + 1));
  } catch {
    return facts;
  }
  const list = Array.isArray(parsed?.memories) ? parsed.memories : [];
  for (const entry of list.slice(0, 5)) {
    const content = typeof entry?.content === "string" ? entry.content.trim() : "";
    if (!content) continue;
    if (inProject && entry.scope === "project") facts.project.push(content);
    else facts.user.push(content);
  }
  return facts;
}

/**
 * The side's lists: each project's chats, the pinned chats, and the rest by
 * age. A chat in a project that no longer exists counts as in none. Each
 * list keeps the order given, which is newest first.
 */
export function arrangeSessions(sessions, projects, now = Date.now()) {
  const known = new Set((projects ?? []).map((project) => project.id));
  const byProject = {};
  for (const project of projects ?? []) byProject[project.id] = [];
  const pinned = [];
  const ages = [
    { label: "Today", items: [] },
    { label: "Previous 7 days", items: [] },
    { label: "Previous 30 days", items: [] },
    { label: "Older", items: [] },
  ];
  const today = new Date(now);
  const startOfToday = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
  for (const entry of sessions ?? []) {
    if (entry.project && known.has(entry.project)) byProject[entry.project].push(entry);
    else if (entry.pinned) pinned.push(entry);
    else {
      const days = (startOfToday - entry.updatedAt) / 86_400_000;
      const age = entry.updatedAt >= startOfToday ? 0 : days <= 7 ? 1 : days <= 30 ? 2 : 3;
      ages[age].items.push(entry);
    }
  }
  return { byProject, pinned, ages: ages.filter((age) => age.items.length) };
}

/** "Good morning", "Good afternoon", or "Good evening", by the hour. */
export function greeting(hour) {
  if (hour >= 5 && hour < 12) return "Good morning";
  if (hour >= 12 && hour < 17) return "Good afternoon";
  return "Good evening";
}
