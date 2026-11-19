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
 * The role of the turn that grades a question's answers and sums them up. It
 * is the chat's own record, never sent to a model as part of the chat.
 */
export const GRADE_ROLE = "grade";

/**
 * The chat in rounds: each question, the answers that came after it, one for
 * each model asked, and the grade of those answers when there is one. The
 * turns are the chat's own, not copies, so a reply that is still arriving
 * shows as it grows.
 */
export function groups(turns) {
  const rounds = [];
  for (const [index, turn] of (turns ?? []).entries()) {
    if (turn.role === "user" || !rounds.length) {
      rounds.push({ key: index, question: turn.role === "user" ? turn : null, answers: [], grade: null });
    }
    if (turn.role === GRADE_ROLE) rounds.at(-1).grade ??= turn;
    else if (turn.role !== "user") rounds.at(-1).answers.push(turn);
  }
  return rounds;
}

/**
 * The chat as one model sees it: each question with that model's own answer,
 * or with the first answer when that model was not asked, so every model
 * reads one answer to each question.
 */
function threadFor(turns, modelId) {
  const kept = [];
  for (const round of groups(turns)) {
    if (round.question) kept.push(round.question);
    const replies = round.answers.filter((turn) => turn.role === "assistant" && String(turn.content ?? "").trim());
    kept.push(...round.answers.filter((turn) => turn.role !== "assistant"));
    const own = replies.find((turn) => turn.model === modelId) ?? replies[0];
    if (own) kept.push(own);
  }
  return kept;
}

/**
 * What goes to the engine: role and answer only. Thinking is never sent
 * back, an empty turn is left out, and two turns in a row from the same
 * side (a reply that was stopped before it began) are joined, because some
 * models refuse a chat whose turns do not alternate. Given a model, each
 * question carries one answer, that model's own when it gave one.
 */
export function requestMessages(turns, modelId, system = "") {
  const messages = [];
  const instructions = String(system ?? "").trim();
  if (instructions) messages.push({ role: "system", content: instructions });
  for (const turn of modelId ? threadFor(turns, modelId) : (turns ?? [])) {
    const content = withAttachments(turn);
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

/**
 * Tokens the chat held after its latest measured reply, or null if none.
 * Given a model, only that model's replies count.
 */
export function tokensUsed(turns, modelId) {
  for (let index = (turns ?? []).length - 1; index >= 0; index -= 1) {
    const turn = turns[index];
    if (turn.role === "assistant" && Number.isFinite(turn.promptTokens) && (!modelId || turn.model === modelId)) {
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

/**
 * Of the models asked, the one whose window is fullest, with its tokens used:
 * its own latest count, or the chat's latest when it has not answered yet.
 * Null when nothing has been counted.
 */
export function fullest(turns, models) {
  let top = null;
  for (const entry of models ?? []) {
    const used = tokensUsed(turns, entry.id) ?? tokensUsed(turns);
    // A window not known, as with some outside engines' models, cannot be measured.
    if (!used || !entry.context) continue;
    const full = fullness(used, entry.context);
    if (!top || full > top.full) top = { model: entry, used, full };
  }
  return top;
}

/** "184 of 16,384 tokens", naming the model when several are asked. */
export function meterLine(top, many) {
  const count = `${formatCount(top.used)} of ${formatCount(top.model.context)} tokens`;
  return many ? `Fullest window, ${top.model.name}: ${count}` : count;
}

/** "Phi-4", "Phi-4 and gpt-oss 20B", or "Phi-4, gpt-oss 20B, and Nemotron". */
export function namesLine(names) {
  const list = [...new Set(names ?? [])];
  if (list.length <= 2) return list.join(" and ");
  return `${list.slice(0, -1).join(", ")}, and ${list.at(-1)}`;
}

/**
 * The models picked once `id` is added (`on`) or taken away, in the order
 * of `ids`, the models on hand. The last one picked stays: a question always
 * goes to at least one model.
 */
export function withPick(picks, id, on, ids) {
  const kept = (ids ?? []).filter((each) => (each === id ? on : (picks ?? []).includes(each)));
  return kept.length ? kept : (ids ?? []).filter((each) => (picks ?? []).includes(each));
}

/**
 * The first of the lists given that names a model on hand, kept to those on
 * hand and in their order. Empty when none does.
 */
export function keptPicks(ids, ...lists) {
  for (const list of lists) {
    const kept = (ids ?? []).filter((id) => (list ?? []).includes(id));
    if (kept.length) return kept;
  }
  return [];
}

/**
 * The models that answered the latest question, each once, in the order they
 * answered. Rust names the same ones in the list of saved chats.
 */
export function latestAnswers(turns) {
  const list = turns ?? [];
  const start = list.findLastIndex((turn) => turn.role === "user") + 1;
  return [...new Set(list.slice(start).filter((turn) => turn.role === "assistant" && turn.model).map((turn) => turn.model))];
}

/** The models a saved chat was last asked with: its latest answers, or its model. */
export function chatModels(session) {
  const answered = latestAnswers(session?.messages);
  return answered.length ? answered : [session?.model].filter(Boolean);
}

/** "Phi-4", or "Phi-4 and 2 more": a saved chat's models in the list's width. */
export function moreLine(names) {
  const list = [...new Set((names ?? []).filter(Boolean))];
  if (list.length <= 1) return list[0] ?? "";
  return `${list[0]} and ${list.length - 1} more`;
}

/** What a new chat says will answer: one model and its room, or each in turn. */
export function answersLine(models) {
  const list = models ?? [];
  if (!list.length) return "";
  if (list.length === 1) {
    const [only] = list;
    if (!only.context) return `${only.name} by ${only.maker} answers.`;
    return `${only.name} by ${only.maker} answers, with room for ${formatCount(only.context)} tokens.`;
  }
  return `${namesLine(list.map((entry) => entry.name))} answer in turn, side by side.`;
}

export function formatCount(count) {
  return Number(count ?? 0).toLocaleString("en-US");
}

/**
 * "16K", "200K", or "1M": a context window as its maker counts it. A window
 * in round thousands (200,000) reads in thousands; one in powers of two
 * (16,384) reads in 1,024s, as the models on this computer count theirs.
 */
export function formatWindow(context) {
  const tokens = Number(context ?? 0);
  const step = tokens % 1000 === 0 ? 1000 : 1024;
  const thousands = Math.round(tokens / step);
  return thousands >= step && thousands % step === 0 ? `${thousands / step}M` : `${thousands}K`;
}

/**
 * One sentence naming the app's models that are on this computer's disk,
 * each name once, in the order given.
 */
export function onDiskLine(models) {
  const names = (models ?? []).map((entry) => entry.name);
  if (!names.length) return "None of this app's models is on this computer yet.";
  return `On this computer: ${namesLine(names)}.`;
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
 * command: on Windows and Linux the app starts `llama-server` itself, with
 * the app's own models folder, so no one command fits and the app sends none.
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
  for (const key of ["thinking", "model", "promptTokens", "completionTokens", "seconds", "stopped", "cutShort", "tool", "toolResult", "attachments"]) {
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

/**
 * Every model the app can download, each marked `here` when it is already on
 * this computer (on the disk, or held by the engine), for the Models settings.
 */
export function modelShelf(engine) {
  const here = new Set([...(engine?.onDisk ?? []), ...(engine?.models ?? [])].map((model) => model.id));
  return (engine?.offers ?? []).map((offer) => ({ ...offer, here: here.has(offer.id) }));
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

// ---------------------------------------------------------------------------
// A project's folder: the tools the model can use in it
// ---------------------------------------------------------------------------

/** Tools that change files, which run only after the person says yes. */
export const WRITE_TOOLS = ["write", "edit"];
const TOOLS = ["list", "read", "search", ...WRITE_TOOLS];
/** Steps the model may take for one message before it has to answer. */
export const TOOL_STEPS = 16;

/** What the model is told about the folder and how to work in it. */
export function workspacePrompt(project, overview) {
  const name = String(project?.folder ?? "").split("/").filter(Boolean).pop() ?? "the folder";
  return [
    `This project has a folder of files, "${name}", that you can work in. Its top levels:`,
    `<folder>\n${String(overview ?? "").trim()}\n</folder>`,
    "To use a tool, end your message with exactly one tool block and nothing after it, then stop and wait for the result:",
    '```tool\n{"tool": "read", "path": "src/app.js"}\n```',
    "The tools, with paths relative to the folder:",
    '- {"tool": "list", "path": "src"}: the files and folders in a folder.',
    '- {"tool": "read", "path": "src/app.js", "from": 1, "to": 300}: a file\'s lines, numbered. from and to are optional.',
    '- {"tool": "search", "text": "createUser", "path": "src"}: lines that contain the text. path is optional.',
    '- {"tool": "edit", "path": "src/app.js", "find": "exact old text", "replace": "new text"}: changes text that appears exactly once in the file. Read the file first and copy the old text exactly, without line numbers.',
    '- {"tool": "write", "path": "src/new.js", "content": "the whole file"}: makes a file, or replaces all of one.',
    "When the user asks you to make, add, or change something, do it in the folder with these tools; never just show the code for them to copy. Start by listing or reading the files that matter, then write or edit.",
    "The user approves each edit and write before it happens, and may decline. Look before you change: read the files that matter instead of guessing. Use one tool per message. When you have what you need, answer normally with no tool block.",
  ].join("\n\n");
}

/** Said to a model that showed code for the folder instead of using its tools. */
export const WORK_NUDGE =
  "You showed the code instead of making the change. Do the work in the folder with the tools: start with one tool block, such as a list or a read, and nothing after it.";

/** A reply that shows code without a tool block: work described, not done. */
export function describesWork(text) {
  const source = String(text ?? "");
  return /(^|\n)[ \t]*(```|~~~)/.test(source) && !/```tool/.test(source);
}

/** The tool block at the end of a reply, if any, with the text before it. */
export function parseToolCall(text) {
  const source = String(text ?? "");
  // The closing fence stands on a line of its own, so a fence inside the
  // JSON, such as in a README the model writes, stays part of the call.
  const match = /```tool[^\n]*\n([\s\S]*?)\n[ \t]*```[ \t]*(?=\n|$)/.exec(source) ?? plainToolFence(source);
  if (!match) return null;
  const call = readCall(match[1]);
  if (call === undefined) {
    return { tool: "", args: {}, before: source.slice(0, match.index).trim(), error: "The tool block was not valid JSON." };
  }
  if (!call || typeof call !== "object" || !TOOLS.includes(call.tool)) {
    return { tool: "", args: {}, before: source.slice(0, match.index).trim(), error: `There is no tool called ${call?.tool ?? "that"}.` };
  }
  const { tool, ...args } = call;
  return { tool, args, before: source.slice(0, match.index).trim() };
}

/**
 * A tool block's JSON, or undefined when it is not JSON. Line breaks and tabs
 * a model writes raw inside a string, as small models do with code, are read
 * as the escapes it meant.
 */
function readCall(body) {
  const text = String(body ?? "").trim();
  try {
    return JSON.parse(text);
  } catch {
    let fixed = "";
    let inString = false;
    let escaped = false;
    for (const char of text) {
      if (inString && !escaped && char === "\n") fixed += "\\n";
      else if (inString && !escaped && char === "\r") fixed += "\\r";
      else if (inString && !escaped && char === "\t") fixed += "\\t";
      else fixed += char;
      if (escaped) escaped = false;
      else if (char === "\\") escaped = inString;
      else if (char === '"') inString = !inString;
    }
    try {
      return JSON.parse(fixed);
    } catch {
      return undefined;
    }
  }
}

/** Whether a fence's body is meant as a tool call, valid JSON or not. */
function looksLikeCall(body) {
  return /^\s*\{\s*"tool"\s*:/.test(body);
}

/**
 * A tool call some models put in a plain or json fence instead of a tool
 * one: the first such fence that holds a call to a tool the app has.
 */
function plainToolFence(source) {
  for (const found of source.matchAll(/```(?:json)?[ \t]*\n([\s\S]*?)\n[ \t]*```[ \t]*(?=\n|$)/g)) {
    if (looksLikeCall(found[1])) return found;
  }
  return null;
}

/** A reply's text without the plain or json fences that hold tool calls. */
export function withoutToolFences(text) {
  return String(text ?? "").replace(/```(?:json)?[ \t]*\n([\s\S]*?)\n[ \t]*```[ \t]*(?=\n|$)/g, (whole, body) =>
    looksLikeCall(body) ? "" : whole,
  );
}

/** The tool a bare set of arguments points to, from the keys it has. */
function toolFor(args) {
  if (typeof args.tool === "string") return args.tool;
  if ("find" in args && "replace" in args) return "edit";
  if ("content" in args) return "write";
  if ("text" in args) return "search";
  if ("path" in args) return "from" in args || "to" in args || /\.[\w]+$/.test(args.path) ? "read" : "list";
  return "";
}

/**
 * The tool call in a reply. A model trained on its own tool format, such as
 * gpt-oss, may answer with nothing and leave the call's arguments at the end
 * of its thinking instead of in a tool block; that call is read from there,
 * and given back as a tool block so the chat's history shows it.
 */
export function findToolCall(answer, thinking) {
  const found = parseToolCall(answer);
  if (found || shownReply(answer)) return found ? { ...found, block: "" } : null;
  const inThinking = parseToolCall(thinking);
  if (inThinking && !inThinking.error) return { ...inThinking, before: "", block: toolBlock(inThinking) };
  const tail = /(\{[^{}]*\})\s*$/.exec(String(thinking ?? ""));
  if (!tail) return null;
  let args;
  try {
    args = JSON.parse(tail[1]);
  } catch {
    return null;
  }
  const tool = toolFor(args);
  if (!TOOLS.includes(tool)) return null;
  const { tool: _named, ...rest } = args;
  const call = { tool, args: rest, before: "" };
  return { ...call, block: toolBlock(call) };
}

/** A call written out as the tool block the model is asked for. */
export function toolBlock(call) {
  return "```tool\n" + JSON.stringify({ tool: call.tool, ...call.args }) + "\n```";
}

/**
 * A reply's words without the tool block it ends with, including one still
 * streaming in. A tool block with words after it, such as an example in an
 * answer about the format, stays.
 */
export function shownReply(text) {
  const source = String(text ?? "");
  const last = [...source.matchAll(/(?:^|\n)[ \t]*```tool/g)].at(-1);
  if (!last) return source.trim();
  const rest = source.slice(last.index);
  const closed = /```tool[^\n]*\n[\s\S]*?\n[ \t]*```[ \t]*(?=\n|$)/.exec(rest);
  if (closed && rest.slice(closed.index + closed[0].length).trim()) return source.trim();
  return source.slice(0, last.index).trim();
}

/** "Read src/app.js": a step, in a few words. */
export function toolLabel(call) {
  const args = call?.args ?? {};
  const path = args.path || ".";
  switch (call?.tool) {
    case "list":
      return `Listed ${path}`;
    case "read":
      return args.from ? `Read ${path}:${args.from}-${args.to ?? ""}`.replace(/-$/, "") : `Read ${path}`;
    case "search":
      return `Searched for "${args.text ?? ""}"${args.path ? ` in ${args.path}` : ""}`;
    case "edit":
      return `Edit ${path}`;
    case "write":
      return `Write ${path}`;
    default:
      return "A tool the app does not have";
  }
}

/** What a step found, as the message that goes back to the model. */
export function toolResult(call, output, failed = false) {
  const tag = failed ? "tool_error" : "tool_result";
  return `<${tag} tool="${call?.tool ?? ""}">\n${String(output ?? "").trim()}\n</${tag}>`;
}

/** A change the model asks to make, as lines marked "-" (gone) and "+" (new). */
export function changeLines(call) {
  const args = call?.args ?? {};
  const split = (text) => String(text ?? "").replace(/\n$/, "").split("\n");
  if (call?.tool === "edit") {
    return [...split(args.find).map((text) => ({ kind: "gone", text })), ...split(args.replace).map((text) => ({ kind: "new", text }))];
  }
  if (call?.tool === "write") return split(args.content).map((text) => ({ kind: "new", text }));
  return [];
}

// ---------------------------------------------------------------------------
// Files attached to a message
// ---------------------------------------------------------------------------

/** A message's words, with the text of each file attached to it first. */
export function withAttachments(turn) {
  const text = String(turn?.content ?? "").trim();
  const files = (turn?.attachments ?? []).filter((file) => file?.text);
  if (!files.length) return text;
  const blocks = files.map((file) => {
    const cut = file.cut ? `\n(The file goes on; this is the first ${formatCount(file.text.length)} characters of ${formatCount(file.chars)}.)` : "";
    return `<attachment name="${String(file.name).replace(/"/g, "'")}">\n${file.text}${cut}\n</attachment>`;
  });
  return [...blocks, text || "The user attached the files above."].join("\n\n");
}

/**
 * Roughly how many tokens files take, at about 3.5 characters a token: close
 * enough to warn before a model's window overflows.
 */
export function attachmentTokens(files) {
  return Math.ceil((files ?? []).reduce((sum, file) => sum + String(file?.text ?? "").length, 0) / 3.5);
}

/** "PDF · 12K characters": a file's label in the chat. */
export function attachmentLine(file) {
  const size = Number(file?.chars ?? 0);
  const amount = size >= 1000 ? `${Math.round(size / 1000)}K characters` : `${size} characters`;
  return `${file?.kind ?? "File"} · ${amount}${file?.cut ? ", cut short" : ""}`;
}

/**
 * The line under the outbound gate's switch. A chat with a model on this
 * computer works either way; only what reaches past it waits for the gate.
 */
export function gateLine(open) {
  return open
    ? "Gate on. The app may go online to check for updates, download models, start the stack, and ask the outside engines you add."
    : "Gate off. Nothing leaves this computer.";
}

/** The names of the model makers picked, in the list's order. */
export function pickedMakers(makers) {
  return (makers ?? []).filter((maker) => maker.picked).map((maker) => maker.name);
}

/**
 * The makers picked once `name` is added (`on`) or taken away, in the list's
 * order. The last one picked stays: at least one maker is always picked.
 */
export function withMaker(makers, name, on) {
  const picked = (makers ?? []).filter((maker) => (maker.name === name ? on : maker.picked)).map((maker) => maker.name);
  return picked.length ? picked : pickedMakers(makers);
}

/** Whether a maker can be taken away: it is picked, and not the only one. */
export function canRemove(makers, name) {
  const picked = pickedMakers(makers);
  return picked.includes(name) && picked.length > 1;
}

/** "MIT · on this computer", "Apache 2.0 · 15.4 GB", or why it cannot download. */
export function listedLine(model) {
  if (model.onDisk) return `${model.license} · on this computer`;
  if (model.offer) return `${model.license} · ${formatSize(model.offer.bytes)}`;
  return `${model.license} · no download for this computer`;
}

/**
 * Under a model on the Installers page: the line Settings gives it, and, for
 * a model on this computer whose maker is not added, that it shows once its
 * maker is.
 */
export function installLine(maker, model) {
  const line = listedLine(model);
  return model.onDisk && !maker.picked ? `${line}, shown once ${maker.name} is added` : line;
}

/**
 * The starts of the model ids that name an outside engine's models: Grok's,
 * asked at xAI, GPT's, asked at OpenAI, Claude's, asked at Anthropic, and
 * Gemini's, asked at Google.
 */
export const OUTSIDE_PREFIXES = ["xai:", "openai:", "anthropic:", "google:"];

/** True for a model asked outside this computer. */
export function isOutside(id) {
  const text = String(id ?? "");
  return OUTSIDE_PREFIXES.some((prefix) => text.startsWith(prefix));
}

/**
 * Under a model's name on its chip: "Microsoft · 16K", or "xAI · online · 256K"
 * for one asked outside this computer, which leaves out a window not known.
 */
export function chipDetail(entry) {
  const parts = [entry.maker];
  if (isOutside(entry.id)) parts.push("online");
  if (entry.context) parts.push(formatWindow(entry.context));
  return parts.join(" · ");
}

/**
 * True when what was typed reads as a path to a file, as /, ~/, C:\, or \\
 * would start one. Anything else, such as a key pasted where the path goes,
 * is cleared from the box once Rust refuses it, so it does not stay on screen.
 */
export function looksLikePath(text) {
  return /^(?:\/|~[\/\\]|[A-Za-z]:[\/\\]|\\\\)/.test(String(text ?? "").trim());
}

/** "Grok's models: Grok 4.7 and Grok 4.7 Fast.", or that none is found yet. */
export function outsideModelsLine(status) {
  const names = (status?.models ?? []).map((entry) => entry.name);
  if (!names.length) return status?.name ? `No ${status.name} models found yet.` : "No models found yet.";
  return `${status.name}'s models: ${namesLine(names)}.`;
}

// Grading: once every answer to a question is in, one model grades each and
// sums them up.

/** The answers a round's grade numbers, in order: each answer with words in it. */
export function gradedAnswers(round) {
  return (round?.answers ?? []).filter((turn) => turn.role === "assistant" && String(turn.content ?? "").trim());
}

/** True when a round has answers from several models to grade, and no grade yet. */
export function canGrade(round) {
  return Boolean(round) && !round.grade && gradedAnswers(round).length > 1;
}

/** What the grader is told, before the question and the answers. */
export const GRADE_INSTRUCTIONS = [
  "You grade answers to one question. The answers are numbered, and the names of the models that wrote them are left out.",
  "For each answer, write one line: Answer, its number, a colon, a letter grade from A to F, a period, and one short sentence on why. For example: Answer 1: B. Right, but it leaves out the second step.",
  "Then write Summary, a colon, and a short summary: what the answers agree on, where they differ, and which is best.",
  "An answer that ends in [cut to fit] went on longer; grade what is shown.",
].join(" ");

/** What ends an answer cut short to fit the grader's window. */
export const CUT_MARK = "[cut to fit]";

// Tokens kept free in the grader's window, for its instructions and its reply.
const GRADE_RESERVE = 5120;
// Characters to a token, counted low so an answer cut to fit does fit.
const CHARS_PER_TOKEN = 3;
// The fewest characters of an answer the grader always reads.
const GRADE_FLOOR = 600;

/**
 * The characters of each answer the grader can read: all of each when its
 * window is not known, or an even share of the room the question leaves.
 */
export function answerRoom(count, context, asked = 0) {
  if (!context || !count) return Infinity;
  const room = Math.floor(((context - GRADE_RESERVE) * CHARS_PER_TOKEN - asked) / count);
  return Math.max(GRADE_FLOOR, room);
}

function cutToFit(text, room) {
  const characters = Array.from(text);
  if (characters.length <= room) return text;
  return `${characters.slice(0, room).join("").trimEnd()}\n${CUT_MARK}`;
}

/**
 * What the grader is sent: the question and each answer, numbered. Only the
 * answers' words are given, so no model's name reaches the grader, and each
 * answer is cut to fit the grader's window, `context`.
 */
export function gradeMessages(question, answers, context) {
  const asked = String(question ?? "").trim();
  const texts = (answers ?? []).map((text) => String(text ?? "").trim());
  const room = answerRoom(texts.length, context, asked.length);
  const parts = [`Question:\n${asked}`, ...texts.map((text, index) => `Answer ${index + 1}:\n${cutToFit(text, room)}`)];
  return [
    { role: "system", content: GRADE_INSTRUCTIONS },
    { role: "user", content: parts.join("\n\n") },
  ];
}

// "Answer 2: B. Why.", with or without bold, a bullet, a list number, or the
// word Grade. The letter is a capital, so "Answer 1: a clear one" is not read
// as an A, and "Answer 1: Accurate" is not read as one either.
const GRADE_LINE = /^(?:[-•*]\s*|\d+[.)]\s+)?(?:[Aa]nswer|ANSWER)\s+#?(\d+)\s*[:.()\-–—]?\s*(?:(?:[Gg]rade|GRADE)\s*:?\s*)?([A-F][+-]?)(?![A-Za-z0-9])[\s.:,)\-–—]*(.*)$/;
const SUMMARY_LINE = /^(?:#+\s*)?(?:[Ss]ummary|SUMMARY)\b\s*[:.\-–—]?\s*(.*)$/;

/**
 * The grader's reply, read: a grade and its reason for each of `count`
 * answers, null where none was given, and the summary. Everything after the
 * Summary line is the summary. A reply with no Summary line keeps whatever is
 * not a grade as its summary, so nothing the grader said is lost.
 */
export function parseGrades(text, count) {
  const grades = Array.from({ length: Math.max(0, count ?? 0) }, () => null);
  const summary = [];
  const other = [];
  let summed = false;
  for (const raw of String(text ?? "").split("\n")) {
    const line = raw.replace(/\*\*|__/g, "").trim();
    if (summed) {
      summary.push(line);
      continue;
    }
    const graded = GRADE_LINE.exec(line);
    if (graded) {
      const at = Number(graded[1]) - 1;
      // The first grade given an answer stands; a number with no answer is left out.
      if (at >= 0 && at < grades.length && !grades[at]) grades[at] = { letter: graded[2], why: graded[3].trim() };
      continue;
    }
    const heading = SUMMARY_LINE.exec(line);
    if (heading) {
      summed = true;
      summary.push(heading[1]);
    } else {
      other.push(line);
    }
  }
  const said = (summed ? summary : other).join("\n").replace(/\n{3,}/g, "\n\n").trim();
  return { grades, summary: said, summed };
}

/**
 * A round's grades, one for each of its answers in order (null for an answer
 * not graded), and its summary. While the grade is still arriving, the
 * summary shows once its Summary line has come. `nameOf` gives a model's
 * name, so where the grader wrote "Answer 2", the page shows
 * "Answer 2 (gpt-oss 20B)".
 */
export function roundGrades(round, nameOf) {
  const answers = round?.answers ?? [];
  if (!round?.grade) return { grades: answers.map(() => null), summary: "" };
  const graded = gradedAnswers(round);
  const read = parseGrades(round.grade.content, graded.length);
  const named = (text) => nameAnswers(text, graded, nameOf);
  const summary = round.grade.pending && !read.summed ? "" : named(read.summary);
  const grades = answers.map((turn) => {
    const grade = read.grades[graded.indexOf(turn)];
    return grade ? { ...grade, why: named(grade.why) } : null;
  });
  return { grades, summary };
}

// "Answer 2", not yet followed by a name in brackets.
const ANSWER_NUMBER = /\b(?:[Aa]nswer|ANSWER)\s+#?(\d+)\b(?!\s*\()/g;

/**
 * The grader's words, with each answer's number followed by the name of the
 * model that wrote it. The grader never had the names; only the page adds
 * them, once the grade is in. A number with no answer is left as it is.
 */
export function nameAnswers(text, graded, nameOf) {
  if (!nameOf) return text;
  return String(text ?? "").replace(ANSWER_NUMBER, (whole, number) => {
    const turn = graded[Number(number) - 1];
    const name = turn ? nameOf(turn.model) : "";
    return name ? `${whole} (${name})` : whole;
  });
}

/**
 * The model that grades, of the models on hand: the one picked in Settings,
 * `saved`, or, until one is picked, the first outside engine's first model, in
 * the order the Installers page lists them. "" picked means no grading. Null
 * when no model grades.
 */
export function graderFor(saved, models) {
  const list = models ?? [];
  if (saved === null || saved === undefined) return list.find((entry) => isOutside(entry.id)) ?? null;
  return list.find((entry) => entry.id === saved) ?? null;
}

/** "Grok 4.7 grades the answers once all are in." */
export function gradesLine(grader) {
  return `${grader.name} grades the answers once all are in.`;
}
