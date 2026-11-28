// The runners, as the code, the page, and the docs keep them, checked on
// every test run.
//
// It answers Brad's ask, 2026-11-27, on what is left to build: "The
// orchestrator: handing slices to runners, with their time limits." On the
// API: "direct API connections are refused as rule for now".
//
// Rust hands a slice to a model and its tests in runners.rs check how a run
// behaves; runners.test.js checks the page's helpers. These fail when
// runners.rs stops quoting the ask or loses a test that keeps a runner, when
// a model outside this computer could be a runner, when the rules are not
// asked before the model and again after it, when the clock stops cutting a
// runner off at its limit, when a runner's work could be kept before the
// rules allow it or inside the board's own file, when a runner could move a
// slice anywhere but written or as anyone but a model, when the page could
// send more than the slice and the model's name, or when SECURITY.md,
// BOARD.md, and the desktop README stop saying so.
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const RUNNERS = 'apps/desktop/src-tauri/src/runners.rs';
const ENGINE = 'apps/desktop/src-tauri/src/engine.rs';
const BOARD = 'apps/desktop/src-tauri/src/board.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';
const VIEW = 'apps/desktop/src/Workflow.vue';
const HELPERS = 'apps/desktop/src/runners.js';
const APP = 'apps/desktop/src/App.vue';

/** The Rust tests that keep a runner. */
const RUNNER_TESTS = [
  'a_runner_in_time_writes_the_slice_and_its_work_is_kept',
  'a_runner_stopped_at_its_limit_has_drifted_and_nothing_is_kept',
  'the_cut_off_is_the_limit_and_a_moment_more',
  'the_clock_stops_an_ask_at_the_cut_off',
  'a_run_you_stop_or_a_model_that_writes_nothing_moves_nothing',
  'a_model_outside_this_computer_is_never_a_runner',
  'a_slice_not_the_models_to_write_is_refused_before_it_is_asked',
  'one_runner_at_a_time_on_a_slice_and_it_can_be_stopped',
  'the_work_is_kept_off_the_board_and_cut_at_its_limit',
  'the_work_is_readable_by_its_owner_only',
];

const COMMANDS = ['run_slice', 'stop_run', 'runs_going', 'slice_work'];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** Text with its line breaks and comment marks folded, so a quote can wrap. */
function flat(text) {
  return text.replace(/\n\s*(?:\/\/[!/]?|\*)?\s*/g, ' ').replace(/\s+/g, ' ');
}

/** The body of a Rust function, from its name to the brace that closes it at its own depth. */
function body(text, head) {
  const start = text.indexOf(head);
  assert.ok(start >= 0, `no ${head}`);
  const indent = text.slice(text.lastIndexOf('\n', start) + 1, start).match(/^\s*/)[0];
  return text.slice(start, text.indexOf(`\n${indent}}`, start));
}

test("runners.rs keeps Brad's words and every test that keeps a runner", () => {
  const runners = read(RUNNERS);
  const tests = flat(runners.slice(runners.indexOf('mod tests {')));
  assert.ok(tests.includes('"The orchestrator: handing slices to runners, with their time limits."'), `${RUNNERS} no longer quotes the ask`);
  assert.ok(tests.includes('"direct API connections are refused as rule for now"'), `${RUNNERS} no longer quotes the rule on the API`);
  for (const name of RUNNER_TESTS) {
    assert.match(runners, new RegExp(`#\\[test\\]\\s+fn ${name}\\(`), `${RUNNERS} has lost the test ${name}`);
  }
  const lib = read(LIB);
  assert.match(lib, /^mod runners;$/m, `${LIB} no longer builds the runners`);
  for (const name of COMMANDS) assert.ok(lib.includes(`runners::${name},`), `${LIB} no longer registers ${name}`);
  assert.ok(lib.includes('app.manage(runners::Runs::default());'), 'the runs are no longer kept');
});

test('a model outside this computer is never a runner, nor one that reads pictures', () => {
  const runners = read(RUNNERS);
  const runnable = body(runners, 'pub fn runnable(');
  assert.match(runnable, /^\s*if outside::is_outside\(model\) \{\s+Err\(OUTSIDE\.to_string\(\)\)/m, 'an outside model is not refused first');
  assert.match(runnable, /\} else if engine::reads_pictures\(model\) \{\s+Err\(WORDS_ONLY\.to_string\(\)\)/, 'a model that reads pictures could be a runner');
  assert.ok(
    runners.includes('"A slice goes only to a model on this computer or on your network. Direct API connections are refused."'),
    'the refusal of an outside model is reworded',
  );
  const run = body(runners, 'pub(crate) async fn run<');
  assert.ok(run.indexOf('runnable(model)?;') >= 0, 'a run does not check its model');
  assert.ok(run.indexOf('runnable(model)?;') < run.indexOf('ask('), 'the model is asked before it is checked');
  // The runners never reach out themselves: an ask goes through the engine or the rig, behind the LAN gate.
  for (const never of ['reqwest', 'Client::builder', 'outside::ask', 'run_outside', 'https://']) {
    assert.ok(!runners.includes(never), `${RUNNERS} holds ${never}`);
  }
  const ask = body(read(ENGINE), 'pub(crate) async fn runner_ask(');
  assert.match(ask, /if is_remote\(&base\) \{\s+lan\.run\(ask\)\.await\s+\} else \{\s+ask\.await\s+\}/, 'a runner on the LAN goes past the LAN gate');
  assert.ok(ask.indexOf('wait_for_turn(') < ask.indexOf('timed('), "the runner's clock starts before it holds its turn");
});

test('the rules are asked before the model and again after it, and only a model writes into written', () => {
  const run = body(read(RUNNERS), 'pub(crate) async fn run<');
  const before = run.indexOf('let before = rules.check(');
  const asked = run.indexOf('let timed = ask(');
  const after = run.indexOf('let after = rules.check(');
  assert.ok(before >= 0 && before < asked, 'the model is asked before the rules');
  assert.match(run.slice(before, asked), /if !before\.allowed \{\s+return Ok\(refused\(before\)\);/, 'a refused slice still goes to the model');
  assert.ok(after > asked, 'the rules are not asked again once the model is done');
  const move = body(read(RUNNERS), 'fn by_model(');
  assert.match(move, /to: "written"\.to_string\(\),\s+by: "model"\.to_string\(\),/, 'a runner moves a slice somewhere other than written, or as someone other than a model');
  assert.ok(!read(RUNNERS).includes('"main"'), 'a runner names main');
  // Its work is kept only when the move is allowed, and before the slice moves.
  const kept = run.indexOf('boards.keep_work(');
  assert.ok(kept > after, 'work is kept before the rules allow it');
  assert.match(run.slice(after, kept), /let work = if after\.allowed \{/, 'work is kept whatever the rules say');
  assert.ok(kept < run.lastIndexOf('rules::take('), 'the slice moves before its work is kept');
});

test('a runner is stopped at its limit, and a runner stopped there has drifted', () => {
  const runners = read(RUNNERS);
  assert.match(runners, /pub const GRACE: Duration = Duration::from_secs\(1\);/, 'the grace past the limit changed');
  const run = body(runners, 'pub(crate) async fn run<');
  assert.match(run, /let cut_off = on_time\s*\.then\(\|\| Duration::from_secs\(u64::from\(view\.limit_minutes\) \* 60\) \+ GRACE\);/, 'the cut-off is not the limit');
  const timed = body(runners, 'pub(crate) async fn timed(');
  assert.ok(timed.includes('tokio::time::timeout(cut_off, ask)'), 'the clock no longer stops the ask');
  assert.match(timed, /Err\(_\) => Ok\(Timed \{\s+done: None,/, 'what a runner cut off wrote is kept');
  // A runner cut off goes through the rules, which say it drifted.
  const cut = run.slice(run.indexOf('let Some(done) = timed.done else {'), run.indexOf('if done.stopped {'));
  assert.ok(cut.includes('rules::take(boards, rules, id, story, slice, step)?'), 'a drifted runner is not written to the board');
  assert.ok(!cut.includes('keep_work'), "a drifted runner's work is kept");
});

test("a runner's work is kept beside the board, never in the board's own file", () => {
  const board = read(BOARD);
  assert.match(board, /pub const WORK_FORMAT: &str = "board work v1";/, 'the work file lost its format');
  assert.match(board, /pub\(crate\) const WORK_LIMIT: usize = 64 \* 1024;/, "a runner's work is no longer capped");
  const keep = body(board, 'pub fn keep_work(');
  assert.ok(keep.includes('let _held = self.writing.lock()') || keep.includes('self.writing'), 'work is kept without the write lock');
  assert.ok(keep.includes('self.put(Kind::Work,'), 'work is kept somewhere other than its own place');
  assert.ok(!keep.includes('Kind::Snapshot'), "work is kept in the board's own file");
});

test('the page sends the slice and the model, and offers only models here or on your network', () => {
  const view = read(VIEW);
  const asks = [...view.matchAll(/invoke\("run_slice", \{([^}]*)\}\)/g)].map((match) => match[1].split(',').map((each) => each.trim().split(':')[0]));
  assert.deepEqual(asks, [['id', 'story', 'slice', 'model', 'onEvent']], 'the page sends more than the slice and the model');
  assert.ok(view.includes('const runners = computed(() => runnerModels(props.models));'), 'the page offers models the runners do not check');
  const helpers = read(HELPERS);
  assert.match(helpers, /return \(models \?\? \[\]\)\.filter\(\(entry\) => entry && !entry\.pictures && !isOutside\(entry\.id\)\);/, 'the page offers an outside model or one that reads pictures');
  assert.ok(read(APP).includes(':models="models"'), 'the Workflow view is not handed the models switched on');
  const pane = view.slice(view.indexOf('aria-labelledby="wf-runner-title"'), view.indexOf('aria-labelledby="wf-work-title"'));
  for (const part of ['@click="handOff"', '@click="stopRunner"', 'role="timer"', 'clockLine(now - running.started, running.limit)']) {
    assert.ok(pane.includes(part), `the runner pane has lost ${part}`);
  }
});

test('SECURITY.md, BOARD.md, and the desktop README say where a slice goes and how long it runs', () => {
  const security = flat(read('SECURITY.md'));
  for (const promise of [
    'A slice is handed only to a model on this computer, on your network, or on your rig. Direct API connections are refused.',
    'A runner is stopped at its time limit, and nothing it wrote is kept.',
  ]) {
    assert.ok(security.includes(promise), `SECURITY.md no longer says: ${promise}`);
  }
  assert.ok(
    flat(read('BOARD.md')).includes("A runner's work is kept in the app's own store, beside the board, and never in the board's file."),
    "BOARD.md no longer says where a runner's work is kept",
  );
  const desktop = read('apps/desktop/README.md');
  assert.match(desktop, /^## Runners$/m, 'the desktop README has lost its Runners section');
});
