import assert from "node:assert/strict";
import { test } from "node:test";

import { canHandOff, clock, clockLine, limitOf, ON_TIME, ranLine, runnerModels } from "./runners.js";

// Brad, 2026-11-27, on what is left to build: "The orchestrator: handing
// slices to runners, with their time limits." On the API: "direct API
// connections are refused as rule for now".

test("a slice goes only to a model here, on the LAN, or on the rig, and never one that reads pictures", () => {
  const models = [
    { id: "phi-4" },
    { id: "rig:m1" },
    { id: "phi-3.5-vision", pictures: true },
    { id: "xai:grok-4.7" },
    { id: "openai:gpt-5" },
    { id: "anthropic:claude-opus-5-5" },
  ];
  assert.deepEqual(
    runnerModels(models).map((entry) => entry.id),
    ["phi-4", "rig:m1"],
  );
  assert.deepEqual(runnerModels(undefined), []);
});

test("only a slice whose next lane is written can be handed to a model", () => {
  assert.equal(canHandOff({ lane: "sliced" }), true);
  assert.equal(canHandOff({ lane: "written" }), false);
  assert.equal(canHandOff({ lane: "checked" }), false);
  assert.equal(canHandOff({ lane: "main" }), false);
});

test("the clock reads minutes and seconds, and the limit beside it while the time rule is on", () => {
  assert.equal(clock(0), "0:00");
  assert.equal(clock(65_000), "1:05");
  assert.equal(clock(3_725_000), "1:02:05");
  const on = { rules: [{ id: ON_TIME, on: true }], limitMinutes: 30 };
  const off = { rules: [{ id: ON_TIME, on: false }], limitMinutes: 30 };
  assert.equal(limitOf(on), 1_800_000);
  assert.equal(limitOf(off), null);
  assert.equal(clockLine(65_000, limitOf(on)), "Writing, 1:05 of 30:00");
  assert.equal(clockLine(65_000, limitOf(off)), "Writing, 1:05");
});

test("what a run came to says how long it took", () => {
  assert.equal(
    ranLine({ words: "The model wrote the slice. It is in written, for the check.", tookMs: 61_000 }),
    "The model wrote the slice. It is in written, for the check. It took 1:01.",
  );
  assert.equal(ranLine({ words: "A model is already writing that slice.", tookMs: 0 }), "A model is already writing that slice.");
});
