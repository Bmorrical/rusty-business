// The outbound gate, as the page shows it, checked on every test run.
//
// It answers the asks that started it: "there is a hard switch / circuit
// breaker to hard stop all outbound communication, user can turn it on to use
// their inference", and "off by default, everything stops".
//
// Rust keeps the gate, and its tests in gate.rs check how it behaves. These
// fail when the page stops reading the gate as Rust keeps it before it asks
// for anything, reads anything but a plain yes as on, loses the switch at the
// foot of the window, or lets a short window hide it, or when the comparison
// stops naming the Rust tests that keep the gate.
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const APP = 'apps/desktop/src/App.vue';
const COMPARISON = 'scripts/comparison.test.mjs';

/** The Rust tests that keep the gate, each named in the comparison's list. */
const GATE_TESTS = [
  ['GATE', 'the_gate_is_off_by_default'],
  ['GATE', 'a_closed_gate_makes_no_connection'],
  ['GATE', 'every_connection_past_this_computer_goes_through_the_gate'],
  ['UPDATE', 'with_the_gate_off_nothing_is_asked_and_the_check_says_to_turn_it_on'],
  ['DOWNLOADS', 'with_the_gate_off_nothing_is_downloaded'],
  ['STACK', 'with_the_gate_off_the_stack_is_not_started'],
];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** One function's text in App.vue, up to the next one. */
function functionOf(app, name) {
  const start = app.indexOf(`async function ${name}(`);
  assert.ok(start >= 0, `${APP} has no ${name}`);
  const end = app.indexOf('\nasync function ', start + 1);
  return app.slice(start, end < 0 ? undefined : end);
}

test('the page reads the gate as Rust keeps it, before it asks for anything', () => {
  const app = read(APP);
  const readGate = functionOf(app, 'readGate');
  assert.match(readGate, /gate\.value = \(await invoke\("gate_status"\)\) === true;/, 'the page reads more than a plain yes as on');
  assert.match(readGate, /catch \{\s+gate\.value = false;/, 'the page reads a failed answer as on');

  const mounted = app.slice(app.indexOf('onMounted(async () => {'), app.indexOf('onBeforeUnmount('));
  const first = mounted.indexOf('await readGate();');
  assert.ok(first >= 0, 'the page does not read the gate when it opens');
  for (const ask of ['checkStack();', 'checkUpdate();', 'checkEngine()']) {
    assert.ok(mounted.indexOf(ask) > first, `the page runs ${ask} before it knows whether the gate is on`);
  }
});

test('a switch at the foot of the window turns the gate on and off', () => {
  const app = read(APP);
  const footer = app.slice(app.indexOf('<div class="gate">'), app.indexOf('</footer>'));
  assert.ok(footer, 'the switch is gone from the foot of the window');
  for (const part of ['role="switch"', ':aria-checked="gate"', 'aria-describedby="gate-line"', ':disabled="gateBusy"', '@click="setGate(!gate)"', 'Outbound gate']) {
    assert.ok(footer.includes(part), `the gate's switch has lost ${part}`);
  }
  assert.ok(footer.includes('{{ gateLine(gate) }}'), "the switch no longer says what the gate's state means");

  const setGate = functionOf(app, 'setGate');
  assert.match(setGate, /gate\.value = \(await invoke\("set_gate", \{ open \}\)\) === true;/, 'the switch does not ask Rust to change the gate');

  // On a short window, such as a first run at 680 by 480, the footer is
  // taller than the room under the saved chats. It scrolls, so the switch can
  // still be reached, and the saved chats keep some room of their own.
  const rule = (selector) => new RegExp(`\\n${selector.replace('.', '\\.')} \\{([^}]*)\\}`).exec(app)?.[1] ?? '';
  assert.match(rule('.engine'), /min-height: 0;[\s\S]*overflow-y: auto;/, 'the footer cannot scroll, so a short window hides the switch');
  assert.match(rule('.sessions'), /min-height: 5rem;/, 'the footer can squeeze the saved chats out of sight');
});

test('the comparison names every Rust test that keeps the gate', () => {
  const comparison = read(COMPARISON);
  const kept = comparison.slice(comparison.indexOf('const KEPT_BY = ['), comparison.indexOf('];', comparison.indexOf('const KEPT_BY = [')));
  for (const [file, name] of GATE_TESTS) {
    assert.ok(kept.includes(`{ file: ${file}, test: '${name}' }`), `the comparison no longer names ${name}`);
  }
});
