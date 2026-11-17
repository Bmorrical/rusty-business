import assert from "node:assert/strict";
import { test } from "node:test";

import {
  dayLabel,
  formatCount,
  formatWindow,
  fullness,
  requestMessages,
  savedTurn,
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
