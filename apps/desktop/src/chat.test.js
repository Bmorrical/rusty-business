import assert from "node:assert/strict";
import { test } from "node:test";

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

test("a title is the first line with text, cut to fit", () => {
  assert.equal(titleFrom("\n\n  What   is 2+2?  \nSecond line"), "What is 2+2?");
  assert.equal(titleFrom("   "), "New chat");
  const long = titleFrom("word ".repeat(40));
  assert.equal(Array.from(long).length <= 60, true);
  assert.equal(long.endsWith("..."), true);
});

test("thinking never goes back to the engine", () => {
  const messages = requestMessages([
    { role: "user", content: "What is 2+2?" },
    { role: "assistant", content: "4.", thinking: "Add them.", promptTokens: 12 },
    { role: "user", content: "And 3+3?" },
  ]);
  assert.deepEqual(messages, [
    { role: "user", content: "What is 2+2?" },
    { role: "assistant", content: "4." },
    { role: "user", content: "And 3+3?" },
  ]);
});

test("a reply stopped before it began does not break the turn order", () => {
  const messages = requestMessages([
    { role: "user", content: "First" },
    { role: "assistant", content: "", stopped: true },
    { role: "user", content: "Second" },
  ]);
  assert.deepEqual(messages, [{ role: "user", content: "First\n\nSecond" }]);
});

test("a turn with a role the engine does not take is left out", () => {
  assert.deepEqual(requestMessages([{ role: "tool", content: "x" }]), []);
  assert.deepEqual(requestMessages(undefined), []);
});

test("tokens used come from the latest measured reply", () => {
  const turns = [
    { role: "user", content: "a" },
    { role: "assistant", content: "b", promptTokens: 10, completionTokens: 5 },
    { role: "user", content: "c" },
    { role: "assistant", content: "d", promptTokens: 30, completionTokens: 7 },
    { role: "user", content: "e" },
  ];
  assert.equal(tokensUsed(turns), 37);
  assert.equal(tokensUsed([{ role: "user", content: "a" }]), null);
});

test("fullness stays between nothing and full", () => {
  assert.equal(fullness(null, 16384), 0);
  assert.equal(fullness(8192, 16384), 0.5);
  assert.equal(fullness(99999, 16384), 1);
});

test("numbers read the way a person writes them", () => {
  assert.equal(formatCount(16384), "16,384");
  assert.equal(formatWindow(16384), "16K");
  assert.equal(formatWindow(262144), "256K");
});

test("speed needs both a count and a time", () => {
  assert.equal(tokensPerSecond({ completionTokens: 100, seconds: 2 }), 50);
  assert.equal(tokensPerSecond({ completionTokens: 100, seconds: 0 }), null);
  assert.equal(tokensPerSecond({ seconds: 2 }), null);
});

test("days are named from the calendar, not from hours", () => {
  const now = new Date(2026, 10, 16, 0, 30).getTime();
  assert.equal(dayLabel(new Date(2026, 10, 16, 0, 5).getTime(), now), "Today");
  assert.equal(dayLabel(new Date(2026, 10, 15, 23, 55).getTime(), now), "Yesterday");
  assert.equal(dayLabel(new Date(2026, 10, 2, 12, 0).getTime(), now), "Nov 2");
  assert.equal(dayLabel(0, now), "");
});

test("a saved turn drops window-only and empty fields", () => {
  const saved = savedTurn({
    role: "assistant",
    content: "4.",
    thinking: "",
    model: "phi",
    promptTokens: 12,
    completionTokens: 3,
    seconds: 0.4,
    stopped: false,
    cutShort: false,
    pending: true,
  });
  assert.deepEqual(saved, {
    role: "assistant",
    content: "4.",
    model: "phi",
    promptTokens: 12,
    completionTokens: 3,
    seconds: 0.4,
  });
});

test("a reply that ran into the length limit is saved that way", () => {
  const saved = savedTurn({ role: "assistant", content: "One, two", cutShort: true, stopped: false });
  assert.deepEqual(saved, { role: "assistant", content: "One, two", cutShort: true });
});

// Brad, on what the app does by itself: "it should search and up the local
// models and stack." With the engine off, the app says which of its models it
// found on the disk.
test("the models on this computer are named in one sentence", () => {
  const phi = { id: "mlx-community/phi-4-4bit", name: "Phi-4" };
  const gptOss = { id: "mlx-community/gpt-oss-20b-MXFP4-Q8", name: "gpt-oss 20B" };
  const nemotron = { id: "mlx-community/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-4bit", name: "Nemotron 3.5 Lightning" };

  assert.equal(onDiskLine([phi]), "On this computer: Phi-4.");
  assert.equal(onDiskLine([phi, gptOss]), "On this computer: Phi-4 and gpt-oss 20B.");
  assert.equal(
    onDiskLine([phi, gptOss, nemotron]),
    "On this computer: Phi-4, gpt-oss 20B, and Nemotron 3.5 Lightning.",
  );
});

test("a model kept in two sizes is named once", () => {
  const models = [
    { id: "mlx-community/phi-4-4bit", name: "Phi-4" },
    { id: "mlx-community/phi-4-8bit", name: "Phi-4" },
    { id: "mlx-community/gpt-oss-20b-MXFP4-Q8", name: "gpt-oss 20B" },
  ];
  assert.equal(onDiskLine(models), "On this computer: Phi-4 and gpt-oss 20B.");
});

test("with none of its models on the disk, the app says so", () => {
  const none = "None of this app's models is on this computer yet.";
  assert.equal(onDiskLine([]), none);
  // An older report has no list at all.
  assert.equal(onDiskLine(undefined), none);
  assert.equal(onDiskLine(null), none);
});

// Brad, on what the app does by itself: "it should search and up the local
// models and stack." and "install and go, one click ready". The page asks for
// the engine to be started only when no engine answers.
test("the engine is asked for only when none answers", () => {
  assert.equal(shouldStart({ reachable: false, error: "The engine is not answering." }), true);
  // One that answers is left alone.
  assert.equal(shouldStart({ reachable: true }), false);
  // Before the first report there is nothing to go on.
  assert.equal(shouldStart(null), false);
  assert.equal(shouldStart(undefined), false);
  // A report that does not say is not a reason to start anything.
  assert.equal(shouldStart({}), false);
});

test("when the app cannot start the engine, it says why, once", () => {
  const off = { reachable: false, error: "The engine is not answering." };
  const notFound = "The app looked for mlx_lm.server on this computer and did not find it.";

  assert.equal(startLine({ answering: false, started: false, reason: notFound }, off), notFound);
  assert.equal(startLine({ answering: false, started: true, reason: ` ${notFound}\n` }, off), notFound);

  // An engine that answers leaves nothing to explain.
  assert.equal(startLine({ answering: true, started: true, reason: null }, { reachable: true }), "");
  assert.equal(startLine({ answering: true, started: false, reason: notFound }, off), "");
  assert.equal(startLine({ answering: false, started: false, reason: notFound }, { reachable: true }), "");

  // No reason, or no report at all.
  assert.equal(startLine({ answering: false, started: false, reason: null }, off), "");
  assert.equal(startLine({ answering: false, started: false }, off), "");
  assert.equal(startLine(undefined, off), "");

  // The same sentence is not said twice, one line under the other.
  const badAddress = "The engine address must be on this computer (for example http://127.0.0.1:8080/v1).";
  assert.equal(startLine({ answering: false, started: false, reason: badAddress }, { reachable: false, error: badAddress }), "");
});
