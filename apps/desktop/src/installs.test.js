import assert from "node:assert/strict";
import { test } from "node:test";

import { canPair, installModels, installsLine, isInstall, troubleOf } from "./installs.js";

// Brad, 2026-11-25: "one [Rusty Business™] can talk to another [Rusty
// Business™] on the same LAN, it has it's own connection handshake", and
// "anything bubbled back ... goes to rb to rb back to me". Then, 2026-11-27:
// "The handshake".

const found = [
  { id: "ab-1", name: "Upstairs", models: ["phi-4", "maker/model-2"], error: null },
  { id: "cd-2", name: "Garage", models: [], error: "Garage is not answering." },
];

test("an install's models are offered only while the breaker and the LAN gate are on", () => {
  assert.deepEqual(installModels(found, false), []);
  assert.deepEqual(installModels(found, true), [
    { id: "install:ab-1/phi-4", name: "phi-4 on Upstairs", maker: "Upstairs", install: true, place: "Upstairs" },
    { id: "install:ab-1/maker/model-2", name: "maker/model-2 on Upstairs", maker: "Upstairs", install: true, place: "Upstairs" },
  ]);
  assert.deepEqual(installModels(null, true), []);
});

test("an install's chat id is told apart from the rest", () => {
  assert.ok(isInstall("install:ab-1/phi-4"));
  assert.ok(!isInstall("rig:m1"));
  assert.ok(!isInstall("phi-4"));
  assert.ok(!isInstall(undefined));
});

test("the page says why no install is asked, and what each one's trouble is", () => {
  assert.match(installsLine(false, true), /circuit breaker is off/);
  assert.match(installsLine(true, false), /LAN gate is off/);
  assert.match(installsLine(true, true), /offered with your own/);
  assert.equal(troubleOf(found, "cd-2"), "Garage is not answering.");
  assert.equal(troubleOf(found, "ab-1"), "");
});

test("pairing needs an address, the six-digit code, and a name", () => {
  assert.ok(canPair({ address: "192.168.1.20", code: "123 456", name: "Upstairs" }));
  assert.ok(!canPair({ address: "", code: "123456", name: "Upstairs" }));
  assert.ok(!canPair({ address: "192.168.1.20", code: "12345", name: "Upstairs" }));
  assert.ok(!canPair({ address: "192.168.1.20", code: "123456", name: " " }));
  assert.ok(!canPair(null));
});
