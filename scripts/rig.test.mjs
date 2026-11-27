// Many models at once on the rig, as the page and the docs show it, checked
// on every test run.
//
// It answers Brad's ask, 2026-11-24: "many models at once on the LAN, a pie
// per card, a slider per model, shares can span cards".
//
// Rust keeps the plan and asks the rig, and its tests in rig.rs and engine.rs
// check how it behaves; rig.test.js checks the page's helpers. These fail when
// rig.rs loses those words or one of those tests, when a rig model waits for
// the turn the models here take, when the page stops asking the rig's models
// at once or Stop stops only one of them, when the Rig pane loses a pie, a
// slider, or Save, when a key can reach the page, or when SECURITY.md and the
// desktop README stop saying what the rig does.
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
const RIG = 'apps/desktop/src-tauri/src/rig.rs';
const ENGINE = 'apps/desktop/src-tauri/src/engine.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';

/** The Rust tests that keep the rig. */
const RIG_TESTS = [
  'a_plan_is_checked_before_it_is_kept',
  'each_model_keeps_its_id_and_a_new_one_takes_the_next_free',
  'each_card_is_a_pie_and_a_share_can_span_cards',
  'the_plan_is_kept_for_this_account_alone_and_read_back',
  'two_rig_models_answer_at_once',
  'one_rig_model_that_cannot_be_reached_holds_up_no_other',
  'a_rig_model_is_asked_only_while_the_lan_gate_is_on',
  'turning_the_lan_gate_off_stops_a_rig_answer',
  'the_key_goes_in_its_header_and_never_to_the_window',
  'a_picture_never_goes_to_the_rig',
];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** Text with its line breaks and comment marks folded, so a quote can wrap. */
function flat(text) {
  return text.replace(/\n\s*(?:\/\/[!/]?|\*)?\s*/g, ' ').replace(/\s+/g, ' ');
}

/** One function's text in App.vue. */
function functionOf(app, name) {
  const start = app.search(new RegExp(`\\n(?:async )?function ${name}\\(`));
  assert.ok(start >= 0, `${APP} has no ${name}`);
  return app.slice(start, app.indexOf('\n}', start) + 2);
}

/** One Rust struct's body, without its comments. */
function structOf(source, name) {
  const start = source.indexOf(`pub struct ${name} {`);
  assert.ok(start >= 0, `${RIG} has no ${name}`);
  return source.slice(start, source.indexOf('\n}', start) + 2).replace(/^\s*\/\/\/.*$/gm, '');
}

test("rig.rs keeps Brad's words and every test that keeps the rig", () => {
  const rig = read(RIG);
  const quote = '"many models at once on the LAN, a pie per card, a slider per model, shares can span cards"';
  assert.ok(flat(rig.slice(0, rig.indexOf('\nuse '))).includes(quote), `${RIG} no longer quotes the ask`);
  for (const name of RIG_TESTS) {
    assert.match(rig, new RegExp(`#\\[test\\]\\s+fn ${name}\\(`), `${RIG} has lost the test ${name}`);
  }
  assert.match(read(LIB), /^mod rig;$/m, `${LIB} no longer builds the rig`);
  for (const name of ['rig_status', 'save_rig']) {
    assert.ok(read(LIB).includes(`rig::${name},`), `${LIB} no longer registers ${name}`);
  }
  assert.ok(read(LIB).includes('app.manage(rig::Rig::new(data.join("rig.json")));'), 'the plan is no longer kept in the app folder');
});

test("a rig model takes no turn from the models here, and builds no client of its own", () => {
  const engine = read(ENGINE);
  assert.match(engine, /#\[test\]\s+fn a_rig_model_answers_while_a_model_here_holds_the_turn\(/, `${ENGINE} has lost the test that keeps the rig out of the turn`);
  const route = engine.slice(engine.indexOf('async fn route('), engine.indexOf('\n}', engine.indexOf('async fn route(')));
  const rigAt = route.indexOf('rig::is_rig(');
  assert.ok(rigAt >= 0, 'the chat no longer sends a rig model to the rig');
  assert.ok(rigAt < route.indexOf('chat_on_engine('), 'a rig model now waits for the turn on this computer');
  assert.doesNotMatch(read(RIG), /Client::builder\(\)/, `${RIG} builds a client of its own, past the gate`);
});

test('the page asks the rig at once, the models here in turn, and Stop stops every one', () => {
  const app = read(APP);
  const send = functionOf(app, 'send');
  assert.ok(send.includes('const onRig = askRig(asked, replies, history, failed);'), 'the rig is no longer asked beside the models here');
  assert.ok(send.indexOf('const onRig = askRig(') < send.indexOf('for (const [index, entry] of asked.entries()) {'), 'the rig is asked only after the models here');
  assert.match(send, /if \(stopAll\) break;\s+if \(entry\.rig\) continue;/, 'a rig model is asked twice, or waits its turn');
  assert.ok(send.includes('await onRig;'), 'the question ends before the rig has answered');
  const rig = functionOf(app, 'askRig');
  assert.ok(rig.includes('Promise.all('), 'the rig models are no longer asked at once');
  assert.ok(rig.includes('if (!entry.rig) return;'), 'a model here is asked at once with the rig');
  assert.ok(rig.includes('if (error) failed.push(`${entry.name}: ${error}`);'), "one rig model's failure is lost, or stops the rest");
  assert.ok(rig.includes('requestMessages(history, entry.id, "", false)'), 'a picture goes to the rig');

  assert.ok(app.includes('const liveChats = new Set();'), 'the page no longer knows every chat under way');
  const ask = functionOf(app, 'ask');
  assert.ok(ask.includes('liveChats.add(chatId);') && ask.includes('liveChats.delete(chatId);'), 'a chat is not kept while it runs, or kept after');
  assert.ok(functionOf(app, 'stop').includes('for (const chatId of liveChats) invoke("stop_chat", { chatId });'), 'Stop stops only one of the models answering');
});

test("the chat offers the rig's models only while the LAN gate is on, and names them after", () => {
  const app = read(APP);
  assert.ok(app.includes('const rigModels = computed(() => rigOffered(rig.value, lanGate.value));'), "the rig's models are offered with the LAN gate off");
  assert.ok(functionOf(app, 'modelName').includes('...rigOffered(rig.value, true)]'), 'a saved chat loses the name of the rig model that answered it');
  assert.ok(app.includes('<span v-if="placeOf(turn.model)" class="quiet"> on {{ placeOf(turn.model) }}</span>'), 'an answer from the rig no longer says where it came from');
  const mounted = app.slice(app.indexOf('onMounted(async () => {'), app.indexOf('onBeforeUnmount('));
  assert.ok(mounted.includes('readRig();'), 'the page no longer reads the plan when it opens');
});

test('the Rig pane shows a pie per card and a slider per model, and saves', () => {
  const app = read(APP);
  assert.ok(app.includes(`@click="openTab('settings', 'rig')">Rig</button>`), 'Settings has lost its Rig tab');
  const start = app.indexOf(`settingsTab === 'rig'" class="settings-pane">`);
  assert.ok(start >= 0, 'Settings has lost the Rig pane');
  const pane = app.slice(start, app.indexOf('<div v-else-if="settingsTab', start + 1));
  for (const part of [
    'class="rig-pie"',
    ':style="pieStyle(pieOf(rigDraft, place))"',
    'role="img"',
    'v-for="(card, place) in rigDraft.cards" :key="place" class="rig-share"',
    'type="range"',
    ':max="roomFor(rigDraft, index, place)"',
    ':step="SHARE_STEP"',
    'rigDraft = withCard(rigDraft)',
    'rigDraft = withModel(rigDraft)',
    '@click="saveRig"',
    '{{ rigShare(model.id) }}',
    'Turn on the LAN gate',
    'A picture never goes to the rig.',
  ]) {
    assert.ok(pane.includes(part), `the Rig pane has lost ${part}`);
  }
  assert.ok(functionOf(app, 'saveRig').includes('invoke("save_rig", { plan: planOf(rigDraft.value) })'), 'Save no longer sends the plan to Rust to check');
  assert.match(app, /\.rig-pie \{[^}]*border-radius: 50%;/, 'a card no longer shows as a round pie');
});

test('no key can reach the page: the plan holds only the path to one', () => {
  const rig = read(RIG);
  for (const name of ['Card', 'Share', 'RigModel', 'Plan', 'Slice', 'CardView', 'ModelView', 'RigView']) {
    const body = structOf(rig, name).replace(/\bkey_file\b/g, '');
    assert.doesNotMatch(body, /\b(?:key|secret|token|password)\b/i, `${name}, which the page is sent, holds a key`);
  }
  assert.match(structOf(rig, 'RigModel'), /pub key_file: Option<String>,/, 'the plan no longer names the file a key is read from');
  for (const name of ['rig_status', 'save_rig']) {
    const start = rig.indexOf(`pub fn ${name}(`);
    assert.ok(start >= 0, `${RIG} has no command ${name}`);
    assert.match(rig.slice(start, rig.indexOf('{', start)), /-> (?:Result<RigView, String>|RigView)\s*$/, `${name} sends the page more than the plan and its view`);
  }
});

test('SECURITY.md and the desktop README say what the rig does', () => {
  const security = flat(read('SECURITY.md'));
  for (const promise of [
    'A model on the rig is asked only while the LAN gate is on',
    'at an address on this computer or a private network, over plain http',
    'A picture never goes to the rig.',
    'The plan is kept in a file only you can read, and holds the path to a key, never the key.',
  ]) {
    assert.ok(security.includes(promise), `SECURITY.md no longer says: ${promise}`);
  }
  const desktop = read('apps/desktop/README.md');
  assert.match(desktop, /^## Rig$/m, 'the desktop README has lost its Rig section');
  assert.ok(desktop.includes('node scripts/fake-rig.mjs'), 'the desktop README no longer says how to try the rig with no cards');
  assert.ok(desktop.includes('--tensor-split'), 'the desktop README no longer says what the split is for');
});
