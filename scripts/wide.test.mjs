// Opening one answer wide and closing it again, checked on every test run.
//
// It answers Brad's ask, 2026-11-22: "ld like claude, I should be able to
// open and close view, like split view to read more of one."
//
// The page tests in chat.test.js check the rule: opening an answer closes any
// other in its round, and a second click closes it. These fail when the
// button leaves the answers that sit side by side, when an opened answer stops
// taking the round's whole width, when the choice is saved or carried to
// another chat, or when opening one moves the view.
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

/** The button that opens an answer wide, from its tag to its end. */
function button(app) {
  const start = app.indexOf('class="copy widen"');
  assert.ok(start >= 0, 'an answer beside others has no button to open it wide');
  return app.slice(app.lastIndexOf('<button', start), app.indexOf('</button>', start));
}

// Brad, 2026-11-22: "I should be able to open and close view"
test('each answer beside others has one button that opens it and closes it', () => {
  const app = read(APP);
  const widen = button(app);
  assert.match(widen, /v-if="round\.answers\.length > 1"/, 'the button shows only where answers sit side by side');
  assert.match(widen, /@click="wide = withWide\(wide, round\.answers, turn\)"/, 'a click no longer opens or closes the answer');
  assert.match(widen, /:aria-pressed="wide\.has\(turn\)"/, 'a screen reader is no longer told whether the answer is open');
  assert.match(widen, /:aria-label="wide\.has\(turn\) \? 'Back to side by side' : 'Open wide'"/);
  assert.match(app, /class="turn assistant" :class="\{ opened: wide\.has\(turn\)[ ,]/);
  assert.match(app, /\n {2}withWide,\n/, 'App.vue takes withWide from chat.js');
});

// Brad, 2026-11-22: "like split view to read more of one."
test('an answer opened wide takes the whole width, and the others stay side by side', () => {
  const app = read(APP);
  const rule = app.slice(app.indexOf('.answers.many > .turn.opened {'));
  const opened = rule.slice(0, rule.indexOf('}'));
  assert.match(opened, /grid-column: 1 \/ -1;/, 'an opened answer no longer takes the round\'s width');
  assert.match(opened, /order: -1;/, 'an opened answer no longer moves to the top of its question, so it can drop out of view');
});

test('what is open is never saved, and a chat opens side by side', () => {
  const app = read(APP);
  assert.match(app, /const wide = shallowRef\(new Set\(\)\);/);
  const writes = app.match(/\bwide\.value = /g) ?? [];
  assert.equal(writes.length, 2, 'only a new chat and an opened chat set what is open, both to nothing');
  assert.match(functionOf(app, 'newChat'), /wide\.value = new Set\(\);/);
  assert.match(functionOf(app, 'openSession'), /wide\.value = new Set\(\);/);
  assert.doesNotMatch(app, /localStorage[^\n]*\bwide\b|\bwide\b[^\n]*localStorage/, 'what is open went to the browser\'s storage');
  assert.doesNotMatch(read('apps/desktop/src/chat.js'), /function savedTurn[^}]*wide/, 'what is open went into a saved chat');
});

test('opening an answer never moves the view', () => {
  const widen = button(read(APP));
  assert.doesNotMatch(widen, /scroll|followAnswer|jumpToEnd|focus/, 'opening an answer moved the view');
});
