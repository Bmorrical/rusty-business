// The courier door, as the page and the docs show it, checked on every test run.
//
// It answers the asks that started it, Brad's, 2026-11-25: "no fork. LAN only
// there is an API exposed, bring your own courier app, we shape it for my
// needs to start", "that has a circuit breaker too", and "the api with the
// courier is an mcp on the RB side that any courier client can connect to".
//
// Rust keeps the door, and its tests in courier.rs check how it behaves.
// These fail when courier.rs loses those words or one of those tests, when
// the breaker leaves the foot of the window, when the Couriers pane loses a
// part, when a key or its hash can reach the page, or when SECURITY.md and
// the desktop README stop saying what the door does.
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
const COURIER = 'apps/desktop/src-tauri/src/courier.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';
const CARGO = 'apps/desktop/src-tauri/Cargo.toml';

/** The Rust tests that keep the door. */
const COURIER_TESTS = [
  'the_breaker_is_off_by_default_and_reads_only_a_plain_on',
  'the_page_is_told_where_the_door_is_only_while_it_listens',
  'only_a_private_network_may_knock',
  'a_code_pairs_once_and_the_key_is_kept_only_as_its_hash',
  'a_code_lasts_five_minutes_and_five_wrong_tries_void_it',
  'with_the_breaker_off_no_code_is_shown_and_turning_it_off_ends_one',
  'a_paired_courier_app_speaks_mcp_and_gets_the_answer',
  'the_answer_streams_back_as_it_comes',
  'an_unpaired_courier_app_or_a_web_page_is_turned_away',
  'pairing_over_the_door_gives_the_key_once',
  'requests_the_door_does_not_take',
  'the_door_listens_only_while_the_breaker_and_the_lan_gate_are_on',
  'turning_the_breaker_off_stops_an_answer_part_way',
  'a_courier_app_can_cancel_its_own_question_only',
];

/** The commands the page may call, each registered in lib.rs. */
const COMMANDS = ['courier_status', 'set_courier', 'pair_courier', 'stop_pairing_courier', 'unpair_courier'];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** Text with its line breaks and comment marks folded, so a quote can wrap. */
function flat(text) {
  return text.replace(/\n\s*(?:\/\/[!/]?)?\s*/g, ' ').replace(/\s+/g, ' ');
}

/** One function's text in App.vue. */
function functionOf(app, name) {
  const start = app.search(new RegExp(`\\n(?:async )?function ${name}\\(`));
  assert.ok(start >= 0, `${APP} has no ${name}`);
  return app.slice(start, app.indexOf('\n}', start) + 2);
}

/** One Rust struct's body. */
function structOf(source, name) {
  const start = source.indexOf(`pub struct ${name} {`);
  assert.ok(start >= 0, `${COURIER} has no ${name}`);
  return source.slice(start, source.indexOf('\n}', start) + 2);
}

test("courier.rs keeps Brad's words and every test that keeps the door", () => {
  const courier = read(COURIER);
  const words = flat(courier.slice(0, courier.indexOf('\nuse ')));
  for (const quote of [
    '"no fork. LAN only there is an API exposed, bring your own courier app, we shape it for my needs to start"',
    '"that has a circuit breaker too"',
    '"the api with the courier is an mcp on the RB side that any courier client can connect to"',
  ]) {
    assert.ok(words.includes(quote), `${COURIER} no longer quotes ${quote}`);
  }
  for (const name of COURIER_TESTS) {
    assert.match(courier, new RegExp(`#\\[test\\]\\s+fn ${name}\\(`), `${COURIER} has lost the test ${name}`);
  }
});

test('the door is written by hand, on what the app already builds', () => {
  const courier = read(COURIER);
  assert.ok(courier.includes('use tokio::net::TcpListener;'), 'the door no longer listens through tokio');
  const tokio = read(CARGO).match(/^tokio = .*$/m)?.[0] ?? '';
  for (const feature of ['"net"', '"io-util"']) {
    assert.ok(tokio.includes(feature), `${CARGO} no longer turns on tokio's ${feature}`);
  }
  for (const name of COMMANDS) {
    assert.ok(read(LIB).includes(`courier::${name},`), `${LIB} no longer registers ${name}`);
  }
  assert.match(read(LIB), /^mod courier;$/m, `${LIB} no longer builds the courier door`);
});

test('the breaker sits at the foot of the window, beside the gates', () => {
  const app = read(APP);
  const footer = app.slice(app.indexOf('<div class="gate">'), app.indexOf('</aside>'));
  assert.ok(footer.includes('Courier breaker'), 'the courier breaker is gone from the foot of the window');
  for (const part of [
    'role="switch"',
    ':aria-checked="courier.on"',
    'aria-describedby="courier-line"',
    "@click=\"changeCourier('set_courier', { open: !courier.on })\"",
    '{{ courierLine }}',
  ]) {
    assert.ok(footer.includes(part), `the courier breaker has lost ${part}`);
  }

  const mounted = app.slice(app.indexOf('onMounted(async () => {'), app.indexOf('onBeforeUnmount('));
  assert.ok(mounted.includes('readCourier();'), 'the page no longer reads the breaker when it opens');
  // A failed read shows the door closed, never open.
  assert.match(functionOf(app, 'readCourier'), /catch \{\s+courier\.value = \{ \.\.\.courier\.value, on: false,/, 'a failed read can show the door open');
});

test('the Couriers pane pairs, shows the code, and unpairs', () => {
  const app = read(APP);
  assert.ok(app.includes(`@click="openTab('settings', 'couriers')">Couriers</button>`), 'Settings has lost its Couriers tab');
  const start = app.indexOf(`settingsTab === 'couriers'" class="settings-pane">`);
  assert.ok(start >= 0, 'Settings has lost the Couriers pane');
  const pane = app.slice(start, app.indexOf('<div v-else-if="settingsTab', start + 1));
  for (const part of [
    'Pair a courier app',
    "changeCourier('pair_courier')",
    '{{ courierCode }}',
    '{{ courierCountdown }}',
    "changeCourier('stop_pairing_courier')",
    "changeCourier('unpair_courier', { id: one.id })",
    '{{ courier.place }}',
    'Turn on the breaker',
    'Turn on the LAN gate',
  ]) {
    assert.ok(pane.includes(part), `the Couriers pane has lost ${part}`);
  }
});

test('no key, and no hash of one, can reach the page', () => {
  const courier = read(COURIER);
  for (const name of ['CourierStatus', 'PairedView']) {
    const body = structOf(courier, name).replace(/^\s*\/\/\/.*$/gm, '');
    assert.doesNotMatch(body, /\b(?:key|hash|secret|token)\b/i, `${name}, which the page is sent, holds a key`);
  }
  for (const name of COMMANDS) {
    const start = courier.indexOf(`pub fn ${name}(`);
    assert.ok(start >= 0, `${COURIER} has no command ${name}`);
    const head = courier.slice(start, courier.indexOf('{', start));
    assert.match(head, /-> (?:Result<CourierStatus, String>|CourierStatus)\s*$/, `${name} sends the page more than the door's state`);
  }
});

test('SECURITY.md and the desktop README say what the door does', () => {
  const security = flat(read('SECURITY.md'));
  for (const promise of [
    'The courier door is off until you turn it on.',
    'only while both its own breaker, Courier breaker at the foot of the window, and the LAN gate are on',
    'turns away any request that carries an Origin',
    'kept only as its SHA-256',
    'It reads no chat, no file, and no setting, and reaches no engine outside this computer.',
  ]) {
    assert.ok(security.includes(promise), `SECURITY.md no longer says: ${promise}`);
  }
  const desktop = read('apps/desktop/README.md');
  assert.match(desktop, /^## Courier apps$/m, 'the desktop README has lost its Courier apps section');
  assert.ok(desktop.includes(':4710/mcp'), 'the desktop README no longer says where the door is');
  assert.ok(desktop.includes('Authorization: Bearer <key>'), 'the desktop README no longer says how a courier app sends its key');
});
