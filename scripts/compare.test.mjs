// One question put to several models, as the page asks it, checked on every
// test run.
//
// It answers the ask that started it: "the next step is user can run the same
// question in 1-nth models and checks the results of each one", and "if they
// can't run asychronos then the models can iterate on the question". The
// engine on this computer holds one model at a time, so the models here take
// turns.
//
// Rust keeps the turn, and its tests in engine.rs and sessions.rs check how
// it behaves; chat.test.js checks the page's helpers. These fail when the
// page asks the models all at once, keeps asking after Stop, lets one model's
// failure stop the rest, shows one model another's answer, loses the buttons
// that pick the models or the cards that sit side by side, or stops picking a
// reopened chat's models, or when a rule loses its Rust test.
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
const ENGINE = 'apps/desktop/src-tauri/src/engine.rs';
const SESSIONS = 'apps/desktop/src-tauri/src/sessions.rs';

/** The Rust tests that keep the turn and the models a chat names. */
const RUST_TESTS = [
  [ENGINE, 'models_on_this_computer_take_turns'],
  [SESSIONS, 'the_list_names_every_model_that_answered_the_latest_question'],
  [SESSIONS, 'a_question_with_no_answer_yet_names_no_model'],
];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** One function's text in App.vue, up to the next one. */
function functionOf(app, name) {
  const start = app.search(new RegExp(`\\n(async )?function ${name}\\(`));
  assert.ok(start >= 0, `${APP} has no ${name}`);
  const end = app.slice(start + 1).search(/\n(async )?function /);
  return app.slice(start, end < 0 ? undefined : start + 1 + end);
}

test('Rust keeps the tests that keep the turn, and a chat waits for its turn', () => {
  for (const [file, name] of RUST_TESTS) {
    assert.match(read(file), new RegExp(`#\\[test\\]\\s+fn ${name}\\(\\)`), `${file} has lost ${name}`);
  }
  const engine = read(ENGINE);
  const stream = engine.slice(engine.indexOf('pub async fn chat_stream('), engine.indexOf('\n}\n', engine.indexOf('pub async fn chat_stream(')));
  assert.match(
    stream,
    /match wait_for_turn\(&chats\.turn, &mut stopped\)\.await \{\s+None => Ok\(ChatDone \{ stopped: true, \.\.ChatDone::default\(\) \}\),\s+Some\(_turn\) => \{\s+run_chat\(/,
    'a chat no longer waits for the model before it, or lets the turn go before its answer ends',
  );
  assert.match(read(SESSIONS), /models: latest_answers\(&session\.messages\),/, 'the saved chats no longer name the models that answered');
});

test('the models answer one at a time, and Stop asks no more of them', () => {
  // Brad, 2026-11-17: "if they can't run asychronos then the models can
  // iterate on the question."
  const send = functionOf(read(APP), 'send');
  assert.doesNotMatch(send, /Promise\.(all|allSettled|any|race)/, 'the models are asked all at once');
  const loop = send.slice(send.indexOf('for (const [index, entry] of asked.entries()) {'));
  assert.ok(loop.length < send.length, 'the models are no longer asked in turn');
  const body = loop.slice(0, loop.indexOf('\n  }\n'));
  assert.match(body, /^\s+if \(stopAll\) break;$/m, 'Stop does not keep the models still waiting from being asked');
  assert.ok(body.indexOf('if (stopAll) break;') < body.indexOf('await ask('), 'a model is asked before Stop is looked at');
  assert.match(body, /const error = await ask\(reply, entry, requestMessages\(history, entry\.id\)\);/, 'a model is not asked with its own side of the chat, or not waited for');
  // One model failing does not keep the next from answering.
  assert.match(body, /if \(error\) failed\.push\(`\$\{entry\.name\}: \$\{error\}`\);/, 'a failed model is not named');
  assert.doesNotMatch(body.slice(body.indexOf('const error')), /\b(return|throw)\b|if \(error\)[^;]*break/, 'one model failing stops the rest');
  assert.match(functionOf(read(APP), 'stop'), /stopAll = true;/, 'Stop no longer stops the models still waiting');
  assert.match(send, /stopAll = false;/, 'a Stop carries over to the next question');
});

test('each model reads the chat without the other models\' answers to the question', () => {
  // What each model is sent is taken before any model starts answering.
  const send = functionOf(read(APP), 'send');
  const history = send.indexOf('const history = session.messages.slice();');
  assert.ok(history >= 0, 'what each model reads is no longer kept apart');
  assert.ok(history > send.indexOf('session.messages.push({ role: "user", content: text });'), 'the models are not sent the question');
  assert.ok(history < send.indexOf('session.messages.push({ role: "assistant"'), 'a model reads the replies being written');
});

test('the models are picked with one click each, and the last one stays', () => {
  // Brad, 2026-11-17: "user can run the same question in 1-nth models".
  const app = read(APP);
  const picks = app.slice(app.indexOf('<div class="picks"'), app.indexOf('</div>', app.indexOf('<div class="picks"')));
  for (const part of [
    'role="group" aria-label="Models that answer"',
    'v-for="entry in models"',
    ':aria-pressed="picked.includes(entry.id)"',
    ':disabled="busy"',
    '@click="togglePick(entry.id)"',
  ]) {
    assert.ok(picks.includes(part), `the models' buttons have lost ${part}`);
  }
  const toggle = functionOf(app, 'togglePick');
  assert.match(toggle, /if \(busy\.value\) return;/, 'a model can be picked while the models answer');
  assert.match(toggle, /picked\.value = withPick\(picked\.value, id, !picked\.value\.includes\(id\), ids\);/, 'a click no longer picks a model or lets it go');
  assert.match(app, /const chosen = computed\(\(\) => models\.value\.filter\(\(entry\) => picked\.value\.includes\(entry\.id\)\)\);/, 'a question no longer goes to the models picked');
});

test('the answers to one question sit side by side, each under its model\'s name', () => {
  // "and checks the results of each one".
  const app = read(APP);
  assert.ok(app.includes('<div v-if="round.answers.length" class="answers" :class="{ many: round.answers.length > 1 }">'), 'the answers to a question are no longer kept together');
  assert.ok(app.includes('<h3 v-if="round.answers.length > 1" class="card-head">{{ modelName(turn.model) || "A model not on hand" }}</h3>'), 'an answer beside others no longer names its model');
  const grid = app.slice(app.indexOf('.answers.many {'), app.indexOf('}', app.indexOf('.answers.many {')));
  assert.match(grid, /display: grid;/, 'the answers are no longer side by side');
  assert.match(grid, /grid-template-columns: repeat\(auto-fit, minmax\(13rem, 1fr\)\);/, 'the answers no longer share the width');
  assert.ok(app.includes('<p v-else-if="turn.waiting" class="quiet waiting">Waits its turn.</p>'), 'a model waiting its turn no longer says so');
});

test('a reopened chat picks its models, and the meter shows the fullest window', () => {
  const app = read(APP);
  assert.match(functionOf(app, 'openSession'), /pickModels\(chatModels\(current\.value\), picked\.value\);/, 'a reopened chat no longer picks the models that answered it');
  assert.match(functionOf(app, 'sessionModels'), /entry\.models\?\.length \? entry\.models : \[entry\.model\]/, 'a saved chat no longer names the models that answered it');
  assert.match(app, /const top = computed\(\(\) => fullest\(current\.value\.messages, chosen\.value\)\);/, 'the meter no longer looks at every model picked');
  assert.ok(app.includes('{{ meterLine(top, many) }}'), 'the meter no longer names the fullest window');
});
