import assert from "node:assert/strict";
import { test } from "node:test";

import { LIMIT_LEAST, LIMIT_MOST, limitOf, switchWord } from "./rules.js";

// Brad, 2026-11-27: "it's a rule of the rusty business™ code, and it's enforced
// by toggle, and it's used in workflows". On the drift threshold: "Time".

test("each rule's switch says whether it is on, and the main rule is always on", () => {
  assert.equal(switchWord({ locked: false, on: true }), "On");
  assert.equal(switchWord({ locked: false, on: false }), "Off");
  assert.equal(switchWord({ locked: true, on: true }), "Always on");
  assert.equal(switchWord({ locked: true, on: false }), "Always on", "the main rule read as off");
});

test("the time limit is whole minutes in range", () => {
  assert.equal(limitOf("30"), 30);
  assert.equal(limitOf(" 45 "), 45);
  assert.equal(limitOf(LIMIT_LEAST), LIMIT_LEAST);
  assert.equal(limitOf(LIMIT_MOST), LIMIT_MOST);
  assert.equal(limitOf(LIMIT_LEAST - 1), null);
  assert.equal(limitOf(LIMIT_MOST + 1), null);
  assert.equal(limitOf("2.5"), null);
  assert.equal(limitOf(""), null);
  assert.equal(limitOf(null), null);
});
