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
export function requestMessages(turns) {
  const messages = [];
  for (const turn of turns ?? []) {
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
