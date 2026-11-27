import assert from "node:assert/strict";
import { test } from "node:test";

import {
  CARDS_MOST,
  draftOf,
  MODELS_MOST,
  pieOf,
  pieStyle,
  planOf,
  rigModels,
  roomFor,
  shareLine,
  withCard,
  withModel,
  withoutCard,
  withoutModel,
} from "./rig.js";

// Brad, 2026-11-24: "many models at once on the LAN, a pie per card, a slider
// per model, shares can span cards".

const PLAN = {
  cards: [
    { name: "One", memoryGb: 96 },
    { name: "Two", memoryGb: 96 },
  ],
  models: [
    { id: "m1", name: "Big", address: "http://192.168.1.20:8091/v1", model: "phi-4", keyFile: null, needsGb: 100, shares: [{ card: 0, percent: 50 }, { card: 1, percent: 50 }] },
    { id: "m2", name: "Small", address: "http://192.168.1.20:8092/v1", model: "gemma-4", keyFile: "/keys/rig.key", needsGb: null, shares: [{ card: 1, percent: 30 }] },
  ],
};

test("a kept plan becomes a slider per card for each model, and back", () => {
  const draft = draftOf(PLAN);
  assert.deepEqual(draft.models.map((model) => model.percents), [[50, 50], [0, 30]]);
  assert.equal(draft.models[0].keyFile, "");
  assert.equal(draft.models[1].needsGb, "");
  assert.deepEqual(planOf(draft), PLAN);
  assert.deepEqual(draftOf(null), { cards: [], models: [] });
});

test("a slider at 0 is no share, and blank boxes are none", () => {
  const draft = draftOf(PLAN);
  draft.models[0].percents = [0, 40];
  draft.models[0].needsGb = " ";
  draft.models[1].keyFile = "  ";
  const plan = planOf(draft);
  assert.deepEqual(plan.models[0].shares, [{ card: 1, percent: 40 }]);
  assert.equal(plan.models[0].needsGb, null);
  assert.equal(plan.models[1].keyFile, null);
});

test("a share can span cards, and one card holds many shares", () => {
  const draft = draftOf(PLAN);
  assert.deepEqual(pieOf(draft, 0), { slices: [{ name: "Big", percent: 50, color: "#ff5b1f" }], free: 50, over: false });
  const two = pieOf(draft, 1);
  assert.deepEqual(two.slices.map((slice) => [slice.name, slice.percent]), [["Big", 50], ["Small", 30]]);
  assert.equal(two.free, 20);
  assert.equal(pieStyle(two), "background: conic-gradient(#ff5b1f 0% 50%, #2f7d4f 50% 80%, var(--hover) 80% 100%);");
  assert.equal(pieStyle({ slices: [], free: 100 }), "background: conic-gradient(var(--hover) 0% 100%);");
});

test("a slider goes only as far as the card has room", () => {
  const draft = draftOf(PLAN);
  assert.equal(roomFor(draft, 1, 1), 50, "Small could take the share Big holds");
  assert.equal(roomFor(draft, 0, 1), 70);
  assert.equal(roomFor(draft, 1, 0), 50);
  draft.models[1].percents = [0, 60];
  assert.equal(pieOf(draft, 1).over, true, "a card shared past 100 percent is not shown as one");
});

test("cards and models are added and taken away, with their sliders", () => {
  let draft = withCard(draftOf(PLAN));
  assert.equal(draft.cards.length, 3);
  assert.deepEqual(draft.models.map((model) => model.percents), [[50, 50, 0], [0, 30, 0]]);
  draft = withoutCard(draft, 0);
  assert.deepEqual(draft.models.map((model) => model.percents), [[50, 0], [30, 0]]);
  draft = withModel(draft);
  assert.deepEqual(draft.models.at(-1).percents, [0, 0]);
  draft = withoutModel(draft, 0);
  assert.deepEqual(draft.models.map((model) => model.name), ["Small", ""]);

  let full = { cards: [], models: [] };
  assert.equal(withModel(full), full, "a model was added with no card to share");
  for (let count = 0; count < CARDS_MOST + 1; count += 1) full = withCard(full);
  assert.equal(full.cards.length, CARDS_MOST);
  for (let count = 0; count < MODELS_MOST + 1; count += 1) full = withModel(full);
  assert.equal(full.models.length, MODELS_MOST);
});

test("what a model's shares come to reads plainly", () => {
  assert.equal(shareLine({ cards: 1.25, memoryGb: 108, room: true }), "1.25 cards, 108 GB");
  assert.equal(shareLine({ cards: 1, memoryGb: 96, room: null }), "1 card, 96 GB");
  assert.equal(shareLine({ cards: 0.3, memoryGb: 28.8, room: false }), "0.3 cards, 28.8 GB, less than it needs");
  assert.equal(shareLine(undefined), "");
});

test("the chat offers the rig's models only while the LAN gate is on", () => {
  const view = { models: [{ chatId: "rig:m1", name: "Big", listedAs: "Phi-4", context: 16384, place: "192.168.1.20:8091" }] };
  assert.deepEqual(rigModels(view, false), []);
  assert.deepEqual(rigModels(view, true), [{ id: "rig:m1", name: "Big", maker: "Phi-4", context: 16384, rig: true, place: "192.168.1.20:8091" }]);
  assert.deepEqual(rigModels(null, true), []);
});
