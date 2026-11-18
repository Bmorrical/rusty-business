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
 * The chat in rounds: each question, and the answers that came after it, one
 * for each model asked. The turns are the chat's own, not copies, so a reply
 * that is still arriving shows as it grows.
 */
export function groups(turns) {
  const rounds = [];
  for (const [index, turn] of (turns ?? []).entries()) {
    if (turn.role === "user" || !rounds.length) {
      rounds.push({ key: index, question: turn.role === "user" ? turn : null, answers: [] });
    }
    if (turn.role !== "user") rounds.at(-1).answers.push(turn);
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
export function requestMessages(turns, modelId) {
  const messages = [];
  for (const turn of modelId ? threadFor(turns, modelId) : (turns ?? [])) {
    const content = String(turn.content ?? "").trim();
    if (!content || !["system", "user", "assistant"].includes(turn.role)) continue;
    const last = messages[messages.length - 1];
    if (last && last.role === turn.role) {
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
    // A window not known, as with some of Grok's, cannot be measured.
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

/** "16K" or "262K": a context window in thousands of tokens. */
export function formatWindow(context) {
  return `${Math.round(Number(context ?? 0) / 1024)}K`;
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
 * The line under the outbound gate's switch. A chat with a model on this
 * computer works either way; only what reaches past it waits for the gate.
 */
export function gateLine(open) {
  return open
    ? "Gate on. The app may go online to check for updates, download models, start the stack, and ask Grok."
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

/** The start of a model id that names one of Grok's models, asked at xAI. */
export const OUTSIDE_PREFIX = "xai:";

/** True for a model asked outside this computer. */
export function isOutside(id) {
  return String(id ?? "").startsWith(OUTSIDE_PREFIX);
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
  if (!names.length) return `No ${status?.name ?? "Grok"} models found yet.`;
  return `${status.name}'s models: ${namesLine(names)}.`;
}
