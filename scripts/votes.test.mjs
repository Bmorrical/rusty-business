// Votes on answers, checked on every test run.
//
// It answers Brad's ask, 2026-11-20: "I should be able to rate answers, and
// it keeps track of which answers were asked when it got voted the best
// one." On 2026-11-22 he took both kinds, a best of several answers and a
// rating on any answer, and added: "voting is optional and it's on by
// default and user can turn it off in settings".
//
// The page tests in chat.test.js check the rules: a second click takes a vote
// back, one best per question, the tally, and that votes never reach a model.
// The Rust tests in sessions.rs check that votes are saved with their chat and
// found again. These fail when the switch starts off, leaves the Votes tab,
// stops hiding the buttons, when a saved chat drops its votes, or when the
// tally loses the questions a model won.
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
const CHAT = 'apps/desktop/src/chat.js';
const SESSIONS = 'apps/desktop/src-tauri/src/sessions.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** One function's text in a file, up to the next one. */
function functionOf(text, name) {
  const start = text.search(new RegExp(`\\n(export )?(async )?function ${name}\\(`));
  assert.ok(start >= 0, `no function ${name}`);
  const end = text.slice(start + 1).search(/\n(export )?(async )?function /);
  return text.slice(start, end < 0 ? undefined : start + 1 + end);
}

/** The Votes pane in Settings, from its opening tag to the next pane. */
function votesPane(app) {
  const start = app.indexOf(`settingsTab === 'votes'" class="settings-pane"`);
  assert.ok(start >= 0, 'Settings has no Votes pane');
  return app.slice(start, app.indexOf('class="settings-pane"', start + 40));
}

test('voting starts on, and a switch on the Votes tab turns it off for good', () => {
  const app = read(APP);
  assert.match(app, /const votesOn = ref\(stored\("votes", "on"\) !== "off"\);/, 'votes must start on');
  assert.match(app, /watch\(votesOn, \(on\) => store\("votes", on \? "on" : "off"\)\);/, 'the choice must be kept');
  assert.match(app, /@click="settingsTab = 'votes'">Votes<\/button>/, 'Settings has no Votes tab');
  assert.match(votesPane(app), /<input v-model="votesOn" type="checkbox" \/>/, 'the switch must sit on the Votes tab');
});

test('the vote buttons show only while voting is on, and only on an answer with words', () => {
  const app = read(APP);
  assert.match(app, /<span v-if="votesOn && replyText\(turn\)" class="votes">/);
  const votes = app.slice(app.indexOf('class="votes"'), app.indexOf('</span>', app.indexOf('class="votes"')));
  for (const label of ['Best answer', 'Good answer', 'Poor answer']) assert.ok(votes.includes(label), `no ${label} button`);
  assert.match(votes, /v-if="canPickBest\(round\)"/, 'a best needs several answers');
  assert.match(votes, /@click="pickBest\(round, turn\)"/);
  assert.match(votes, /@click="rate\(turn, 'up'\)"/);
  assert.match(votes, /@click="rate\(turn, 'down'\)"/);
});

test('a vote is saved with its chat, in the page and in Rust', () => {
  const saved = functionOf(read(CHAT), 'savedTurn');
  for (const key of ['"rating"', '"best"', '"votedAt"']) assert.ok(saved.includes(key), `a saved chat drops ${key}`);
  const rust = read(SESSIONS);
  assert.match(rust, /pub rating: Option<String>/);
  assert.match(rust, /pub best: Option<bool>/);
  assert.match(rust, /pub voted_at: Option<u64>/);
  assert.match(read(APP), /async function persist\(\)/);
  for (const name of ['rate', 'pickBest']) assert.match(functionOf(read(APP), name), /persist\(\);/, `${name} must save the chat`);
});

test('the tally names each model and the questions it won', () => {
  const app = read(APP);
  assert.match(read(LIB), /sessions::list_votes,/, 'Rust must answer list_votes');
  assert.match(functionOf(app, 'loadVotes'), /voteTally\(await invoke\("list_votes"\)\)/);
  const pane = votesPane(app);
  assert.ok(pane.includes('tallyLine(row)'), 'each model needs its line');
  assert.ok(pane.includes('Questions it won'), 'the questions a model won must be listed');
  assert.ok(pane.includes('openSession(win.session)'), 'a won question opens its chat');
  assert.ok(pane.includes('No votes yet.'), 'an empty tally needs words');
});

test('votes stay on this computer and never reach a model', () => {
  const sent = functionOf(read(CHAT), 'requestMessages');
  for (const word of ['rating', 'best', 'votedAt']) assert.ok(!sent.includes(word), `requestMessages must not read ${word}`);
  assert.match(sent, /messages\.push\(\{ role: turn\.role, content \}\);/);
});
