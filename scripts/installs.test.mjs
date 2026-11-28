// One Rusty Business™ install asking another, as the code, the page, and the
// docs keep it, checked on every test run.
//
// It answers Brad's ask, 2026-11-25: "one [Rusty Business™] can talk to
// another [Rusty Business™] on the same LAN, it has it's own connection
// handshake", and "anything bubbled back ... goes to rb to rb back to me".
// Then, 2026-11-27: "The handshake".
//
// handshake.rs signs and proves, courier.rs is the door that checks, and
// installs.rs is the side that asks; their Rust tests check how a pairing,
// a question, and a stop behave. These fail when those files stop quoting
// the ask or lose a test that keeps the handshake, when a command of it goes
// unregistered or a package is added for it, when a request goes unsigned,
// an answer is read before its proof is checked, or an install is asked with
// the Courier breaker off, when a picture could go to another install or its
// secret reach the page, when the page offers an install's models with the
// breaker or the LAN gate off, when the Installs pane loses the way to pair
// or unpair, when the door could ask a third install, or when SECURITY.md and
// the desktop README stop saying what the handshake does.
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const HANDSHAKE = 'apps/desktop/src-tauri/src/handshake.rs';
const INSTALLS = 'apps/desktop/src-tauri/src/installs.rs';
const COURIER = 'apps/desktop/src-tauri/src/courier.rs';
const ENGINE = 'apps/desktop/src-tauri/src/engine.rs';
const RUNNERS = 'apps/desktop/src-tauri/src/runners.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';
const CARGO = 'apps/desktop/src-tauri/Cargo.toml';
const APP = 'apps/desktop/src/App.vue';
const HELPERS = 'apps/desktop/src/installs.js';

const ASK = '"one [Rusty Business™] can talk to another [Rusty Business™] on the same LAN, it has it\'s own connection handshake"';

/** The Rust tests that keep the handshake, by file. */
const TESTS = new Map([
  [HANDSHAKE, [
    'hmac_is_the_one_rfc_4231_gives',
    'a_signature_holds_only_for_its_secret_its_body_and_its_time',
    'a_signed_header_is_read_only_in_its_exact_shape',
    'an_answer_is_proven_only_by_the_secret_and_the_askers_number',
  ]],
  [INSTALLS, [
    'an_install_pairs_with_the_code_keeps_only_the_secret_and_asks_through_the_door',
    'a_wrong_code_pairs_nothing_and_nothing_is_kept',
    'an_installs_key_is_never_taken_as_a_bearer_token',
    'a_request_copied_off_the_network_changed_late_or_unsigned_is_turned_away',
    'an_answer_that_does_not_prove_itself_is_never_shown',
    'stop_reaches_the_other_install_and_keeps_what_came',
    'nothing_goes_with_the_breaker_or_the_lan_gate_off_or_with_a_picture',
    'an_address_is_numbers_on_the_private_network',
  ]],
  [COURIER, [
    'an_install_signs_each_request_and_each_answer_carries_its_proof',
    'the_turns_before_a_question_are_a_short_list_of_roles_and_words',
  ]],
]);

const COMMANDS = ['installs_status', 'pair_install', 'unpair_install', 'install_models'];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** Text with its line breaks and comment marks folded, so a quote can wrap. */
function flat(text) {
  return text.replace(/\n\s*(?:\/\/[!/]?|\*)?\s*/g, ' ').replace(/\s+/g, ' ');
}

/** The body of a function, from its name to the brace that closes it at its own depth. */
function body(text, head) {
  const start = text.indexOf(head);
  assert.ok(start >= 0, `no ${head}`);
  const indent = text.slice(text.lastIndexOf('\n', start) + 1, start).match(/^\s*/)[0];
  return text.slice(start, text.indexOf(`\n${indent}}`, start));
}

test("the handshake keeps Brad's words and every test that keeps it", () => {
  for (const file of [HANDSHAKE, INSTALLS, COURIER]) {
    assert.ok(flat(read(file)).includes(ASK), `${file} no longer quotes the ask`);
  }
  assert.ok(flat(read(HELPERS)).includes(ASK), `${HELPERS} no longer quotes the ask`);
  for (const [file, names] of TESTS) {
    const text = read(file);
    for (const name of names) {
      assert.match(text, new RegExp(`#\\[(?:tokio::)?test\\]\\s+(?:async )?fn ${name}\\(`), `${file} has lost the test ${name}`);
    }
  }
  const lib = read(LIB);
  assert.match(lib, /^mod handshake;$/m, `${LIB} no longer builds the handshake`);
  assert.match(lib, /^mod installs;$/m, `${LIB} no longer builds the installs`);
  assert.ok(lib.includes('app.manage(installs::Installs::new(data.join("installs.json")));'), `${LIB} no longer keeps the installs paired`);
  for (const name of COMMANDS) assert.ok(lib.includes(`installs::${name},`), `${LIB} no longer registers ${name}`);
});

test('the handshake is HMAC-SHA256 on the package the app already builds, with a two-minute clock', () => {
  const handshake = read(HANDSHAKE);
  assert.ok(handshake.includes('use sha2::{Digest, Sha256};'), `${HANDSHAKE} is no longer written on sha2`);
  assert.ok(handshake.includes('pub const CLOCK_SLACK: u64 = 120;'), 'the clocks may be more than two minutes apart');
  assert.match(body(handshake, 'pub fn holds('), /same\(expected\.as_bytes\(\), signed\.mac\.as_bytes\(\)\) && now\.abs_diff\(signed\.time\) <= CLOCK_SLACK/, 'a signature is checked in a time that tells how soon it differs, or its time is not checked');
  assert.doesNotMatch(read(CARGO), /^\s*(?:hmac|ring|aws-lc-rs|hkdf)\s*=/m, `${CARGO} adds a package for the handshake`);
});

test('the door checks each signature, refuses a number seen before, and proves its answer', () => {
  const courier = read(COURIER);
  const who = body(courier, 'fn install_who(');
  assert.ok(who.includes('entry.kind == Kind::Install'), "a courier app's key is taken as an install's secret");
  assert.ok(who.includes('handshake::holds(&signed, &secret, body, now)'), 'the door no longer checks the signature');
  assert.ok(who.includes('seen.contains_key(&signed.nonce)'), 'a request copied off the network can be sent again');
  assert.ok(who.includes('handshake::proof(&secret, &signed.nonce)'), "the door's answer no longer proves itself");
  assert.ok(courier.includes(`"That install's handshake did not hold."`), 'a signature that does not hold is no longer turned away');
});

test('the asking side signs every request and reads nothing before its proof', () => {
  const installs = read(INSTALLS);
  const signed = body(installs, 'async fn signed(');
  assert.ok(signed.includes('.header(handshake::SIGNED, said)'), 'a request to another install goes unsigned');
  const proven = signed.indexOf('handshake::proven(&install.secret, &nonce, proof)');
  assert.ok(proven >= 0, "an answer's proof is no longer checked");
  assert.ok(!/\.(?:json|text|bytes|bytes_stream|chunk)\(\)/.test(signed.slice(0, proven)), 'an answer is read before its proof is checked');
  assert.equal((installs.match(/\.post\(/g) ?? []).length, 2, 'another install is asked some way that is neither the pairing nor a signed request');
  const guarded = body(installs, 'async fn guarded<');
  assert.ok(guarded.includes('if !breaker.is_open() {'), 'another install is asked with the Courier breaker off');
  assert.ok(guarded.includes('() = breaker.closed() => Err(BREAKER_CLOSED.to_string()),'), 'turning the breaker off no longer stops an answer under way');
  assert.ok(guarded.includes('lan.run('), 'another install is asked past the LAN gate');
  const chat = body(installs, 'pub(crate) async fn run_chat(');
  assert.ok(chat.includes('return Err(vision::STAYS_HERE.to_string());'), 'a picture can go to another install');
  assert.ok(chat.includes('guarded(lan, breaker,'), 'a chat asks another install outside the guard');
  const view = body(installs, 'pub struct InstallView {');
  assert.doesNotMatch(view, /secret|key/, "an install's secret can reach the page");
});

test('a chat and a runner reach another install only through installs.rs, and the door never asks a third', () => {
  assert.match(read(ENGINE), /else if installs::is_install\(/, 'the chat no longer sends an install model to its install');
  assert.match(read(ENGINE), /installs::run_chat\(\s*lan,\s*&others\.breaker,\s*others\.installs,/, 'the chat asks another install past its breaker');
  assert.ok(read(RUNNERS).includes('installs::run_chat(lan, breaker, installs, &request, &mut stopped, emit)'), 'a runner asks another install past its breaker');
  for (const head of ['pub(crate) async fn courier_models(', 'pub(crate) async fn courier_ask(']) {
    assert.doesNotMatch(body(read(ENGINE), head), /installs::/, 'an install asked through the door can ask a third');
  }
});

test("the page offers an install's models only with the breaker and the LAN gate on, and pairs and unpairs", () => {
  const app = read(APP);
  assert.ok(app.includes('const installsReady = computed(() => installs.value.ready && lanGate.value && courier.value.on);'), "an install's models are offered with the breaker or the LAN gate off");
  assert.ok(app.includes('const otherModels = computed(() => installOffered(installFound.value, installsReady.value));'), "an install's models are offered whether or not it can be asked");
  assert.ok(app.includes('...rigModels.value, ...otherModels.value]'), "an install's models are no longer offered in the chat");
  assert.ok(app.includes(`@click="openTab('settings', 'installs')">Installs</button>`), 'Settings has lost its Installs tab');
  assert.ok(app.includes('<form class="rig-fields" @submit.prevent="pairInstall">'), 'the Installs pane has lost the way to pair');
  assert.ok(app.includes('@click="unpairInstall(one.id)">Unpair</button>'), 'the Installs pane has lost the way to unpair');
  assert.ok(app.includes('invoke("pair_install", {'), 'the page no longer pairs through Rust');
  assert.doesNotMatch(app, /installs?\.(?:secret|key)\b|one\.secret/, "an install's secret is read on the page");
});

test('SECURITY.md and the desktop README say what the handshake does', () => {
  const security = flat(read('SECURITY.md'));
  for (const words of [
    'Another install is asked only through its handshake.',
    'kept on both sides only as its SHA-256, in a file only you can read',
    'cannot be sent again or changed, and one more than two minutes off is turned away',
    'an answer without it is never shown',
    'a picture never goes to it',
    'the words are not sealed',
  ]) {
    assert.ok(security.includes(words), `SECURITY.md no longer says: ${words}`);
  }
  const readme = read('apps/desktop/README.md');
  assert.match(readme, /^## Installs$/m, 'the desktop README has lost its Installs section');
  const installs = flat(readme.slice(readme.indexOf('## Installs'), readme.indexOf('## Rules')));
  assert.ok(installs.includes('each answer is shown only once it proves it came from the install you paired'), 'the desktop README no longer says an answer proves itself');
  assert.ok(installs.includes('A picture never goes to another install'), 'the desktop README no longer says a picture stays here');
});
