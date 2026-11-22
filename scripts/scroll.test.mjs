// Where the chat's view goes while the models answer, checked on every test
// run.
//
// It answers Brad's ask, 2026-11-22: "put in a bug fix, when model 2 is
// thinking, I can't keep my scroll position on question 1, the browser is
// forced to the bottom following whoever is answering, instead the reader
// should choose the viewport position."
//
// The page tests in chat.test.js check the rules: a scroll up stops the view,
// a scroll to the end follows again, and the view only ever moves down. These
// fail when the old pull to the bottom comes back, when the view is moved
// anywhere but its one function, when a streamed word moves the view without
// asking the reader's state, or when the page stops hearing the wheel.
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

// Brad, 2026-11-22: "the browser is forced to the bottom following whoever is
// answering"
test('the old pull to the bottom is gone', () => {
  const app = read(APP);
  assert.doesNotMatch(app, /stickToBottom/, 'the view no longer sticks to the bottom on its own');
  assert.doesNotMatch(app, /function scrollDown\(/);
  assert.doesNotMatch(app, /scrollDown\(\)/);
});

// Brad, 2026-11-22: "instead the reader should choose the viewport position."
test('the view moves in one place, and only while the reader lets it', () => {
  const app = read(APP);
  const writes = app.match(/\.scrollTop\s*=(?!=)/g) ?? [];
  assert.equal(writes.length, 2, 'the view is moved by followAnswer, and once by jumpToEnd as a saved chat opens');
  const follow = functionOf(app, 'followAnswer');
  assert.match(follow, /viewAfterMove\(view, viewTop, pane\)/, 'a scroll not yet reported still counts');
  assert.match(follow, /if \(view === VIEW_STAY\) return;/, 'a view the reader placed never moves');
  assert.match(follow, /viewTarget\(view, pane,/);
  assert.match(functionOf(app, 'jumpToEnd'), /pane\.scrollTop = pane\.scrollHeight;/);
  assert.match(functionOf(app, 'onScroll'), /view = viewAfterMove\(view, viewTop, pane\);/);
});

// Brad, 2026-11-22: "when model 2 is thinking, I can't keep my scroll position
// on question 1"
test('every streamed word and every new answer asks the reader first', () => {
  const app = read(APP);
  assert.match(app, /@scroll="onScroll" @wheel\.passive="onWheel"/, 'the chat hears the wheel before the scroll lands');
  assert.match(functionOf(app, 'onWheel'), /if \(event\.deltaY < 0\) view = VIEW_STAY;/);
  const ask = functionOf(app, 'ask');
  assert.match(ask, /channel\.onmessage[\s\S]*followAnswer\(\);[\s\S]*\};/, 'a streamed word follows only through followAnswer');
  assert.match(functionOf(app, 'send'), /view = VIEW_START;/, 'a question sent starts by following its answers');
  assert.match(app, /followAnswer\("\.tool\.asked"\)/, 'a step that waits on the reader is brought into view unless the reader placed it');
  for (const name of ['viewAfterMove', 'viewTarget', 'VIEW_START', 'VIEW_END', 'VIEW_STAY']) {
    assert.match(app, new RegExp(`\\n  ${name},\\n`), `App.vue takes ${name} from chat.js`);
  }
});
