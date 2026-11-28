// The orchestrator's rules, as the code, the page, and the docs keep them,
// checked on every test run.
//
// It answers Brad's ask, 2026-11-27: "it's a rule of the rusty business™ code,
// and it's enforced by toggle, and it's used in workflows". On the drift
// threshold: "Time".
//
// Rust holds the rules and checks each move, and its tests in rules.rs check
// how they behave; rules.test.js checks the page's helpers. These fail when
// rules.rs stops quoting the ask or loses a test that keeps the rules, when a
// rule leaves the list or the main rule gains a toggle, when the rules' lanes
// and parties stop matching the Workflow's, when the Rules tab loses a switch
// or the time limit, when SECURITY.md, BOARD.md, and the desktop README
// stop saying that only you take a slice into main, or when the Workflow's
// move of a slice could be made as anyone but you, or from a lane the page
// names.
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
const RULES = 'apps/desktop/src-tauri/src/rules.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';
const BOARD_JS = 'apps/desktop/src/board.js';
const VIEW = 'apps/desktop/src/Workflow.vue';

/** The Rust tests that keep the rules. */
const RULES_TESTS = [
  'every_rule_starts_on',
  'a_slice_goes_through_the_lanes_in_order',
  'each_lane_is_worked_by_its_own_party_or_by_you',
  'only_you_take_a_slice_into_main_whatever_is_off',
  'a_runner_past_its_time_has_drifted_and_the_slice_is_cut_smaller',
  'a_move_the_app_does_not_know_is_refused',
  'the_toggles_are_kept_for_this_account_alone_and_read_back',
  'the_rules_know_the_boards_lanes_and_moves',
  'a_move_you_make_is_written_with_whose_move_is_next',
  'a_refused_move_writes_nothing',
  'a_runner_that_drifted_leaves_the_lane_and_hands_the_slice_back',
  'a_slice_you_send_back_goes_to_that_lanes_party_next',
  'a_story_or_slice_not_on_the_board_is_said_and_nothing_is_written',
];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** Text with its line breaks and comment marks folded, so a quote can wrap. */
function flat(text) {
  return text.replace(/\n\s*(?:\/\/[!/]?|\*)?\s*/g, ' ').replace(/\s+/g, ' ');
}

test("rules.rs keeps Brad's words and every test that keeps the rules", () => {
  const rules = read(RULES);
  const quote = `"it's a rule of the rusty business™ code, and it's enforced by toggle, and it's used in workflows"`;
  assert.ok(flat(rules.slice(0, rules.indexOf('\nuse '))).includes(quote), `${RULES} no longer quotes the ask`);
  for (const name of RULES_TESTS) {
    assert.match(rules, new RegExp(`#\\[test\\]\\s+fn ${name}\\(`), `${RULES} has lost the test ${name}`);
  }
  const lib = read(LIB);
  assert.match(lib, /^mod rules;$/m, `${LIB} no longer builds the rules`);
  for (const name of ['rules_status', 'set_rule', 'set_rule_limit', 'check_move', 'move_slice']) {
    assert.ok(lib.includes(`rules::${name},`), `${LIB} no longer registers ${name}`);
  }
  assert.ok(lib.includes('app.manage(rules::Rules::new(data.join("rules.json")));'), 'the toggles are no longer kept in the app folder');
});

test('the four rules stay, and only the main rule has no toggle', () => {
  const rules = read(RULES);
  const list = rules.slice(rules.indexOf('const RULES:'), rules.indexOf('];', rules.indexOf('const RULES:')));
  const ids = [...list.matchAll(/\(\s*([A-Z_]+),\s*"[^"]+",\s*"[^"]+",\s*(true|false),\s*\)/g)].map((match) => [match[1], match[2]]);
  assert.deepEqual(ids, [['IN_ORDER', 'false'], ['OWN_PARTY', 'false'], ['MAIN_IS_YOURS', 'true'], ['ON_TIME', 'false']], 'a rule left the list, or the main rule gained a toggle');
  // The main rule is checked outside every toggle.
  const judge = rules.slice(rules.indexOf('fn judge('), rules.indexOf('\n}', rules.indexOf('fn judge(')));
  assert.match(judge, /\/\/ Locked on: no toggle reaches it\.\s+if LANES\[to\] == "main" && step\.by != "you" \{/, 'the main rule sits behind a toggle');
  assert.ok(rules.includes('"Only you take a slice into main. That rule has no toggle."'), 'the main rule can be turned off');
  assert.doesNotMatch(rules, /Client::builder\(\)|reqwest::/, `${RULES} reaches the network`);
});

test("the rules' lanes and parties are the Workflow's", () => {
  const board = read(BOARD_JS);
  const lanes = [...board.matchAll(/\{ id: "(\w+)", rank: "\w+", name: "\w+", by: "[^"]+", move: "(\w+)"/g)].map((match) => [match[1], match[2]]);
  assert.equal(lanes.length, 5, `${BOARD_JS} no longer lists the five lanes`);
  const rules = read(RULES);
  const party = rules.slice(rules.indexOf('pub fn party('), rules.indexOf('\n}', rules.indexOf('pub fn party(')));
  for (const [lane, move] of lanes) {
    assert.match(party, new RegExp(`"${lane}"[^=]*=> Some\\("${move}"\\)`), `the rules give ${lane} to someone other than the Workflow's ${move}`);
  }
});

test('the Rules tab shows a switch for each rule and the time limit', () => {
  const app = read(APP);
  assert.ok(app.includes(`@click="openTab('settings', 'rules')">Rules</button>`), 'Settings has lost its Rules tab');
  const start = app.indexOf(`settingsTab === 'rules'" class="settings-pane">`);
  assert.ok(start >= 0, 'Settings has lost the Rules pane');
  const pane = app.slice(start, app.indexOf('<div v-else-if="settingsTab', start + 1));
  for (const part of [
    'v-for="rule in rulesView.rules"',
    'role="switch"',
    ':aria-checked="rule.on"',
    ':disabled="rule.locked || rulesBusy"',
    `@click="changeRules('set_rule', { id: rule.id, on: !rule.on })"`,
    '{{ switchWord(rule) }}',
    'v-model="limitDraft"',
    '@click="saveLimit"',
  ]) {
    assert.ok(pane.includes(part), `the Rules pane has lost ${part}`);
  }
  const mounted = app.slice(app.indexOf('onMounted(async () => {'), app.indexOf('onBeforeUnmount('));
  assert.ok(mounted.includes('readRules();'), 'the page no longer reads the rules when it opens');
});

test('SECURITY.md, BOARD.md, and the desktop README say that only you take a slice into main', () => {
  const security = flat(read('SECURITY.md'));
  for (const promise of [
    'no model, check, or reviewer moves a slice into main, and the app never merges.',
    'The switches are kept in a file only you can read.',
  ]) {
    assert.ok(security.includes(promise), `SECURITY.md no longer says: ${promise}`);
  }
  assert.ok(flat(read('BOARD.md')).includes('Only you take a slice into `main`: that rule has no switch, and the app never merges.'), 'BOARD.md no longer says only you take a slice into main');
  const desktop = read('apps/desktop/README.md');
  assert.match(desktop, /^## Rules$/m, 'the desktop README has lost its Rules section');
  assert.ok(flat(desktop).includes('**Only you take a slice into main.** This rule has no switch.'), 'the desktop README no longer says the main rule has no switch');
});

test('a move from the Workflow is made as you, from the lane the board says', () => {
  const rules = read(RULES);
  const start = rules.indexOf('pub fn move_slice(');
  assert.ok(start >= 0, `${RULES} has lost move_slice`);
  const taken = rules
    .slice(start + 'pub fn move_slice('.length, rules.indexOf(') ->', start))
    .split('\n')
    .map((each) => each.trim().replace(/,$/, ''))
    .filter(Boolean);
  assert.deepEqual(
    taken,
    ["boards: tauri::State<'_, Boards>", "rules: tauri::State<'_, Rules>", 'id: String', 'story: u64', 'slice: String', 'to: String'],
    'move_slice takes more from the page than the slice and its lane',
  );
  const body = rules.slice(start, rules.indexOf('\n}', start));
  assert.match(body, /by: "you"\.to_string\(\),\s+\.\.Move::default\(\)/, 'a move from the page is not made as you');
  const take = rules.slice(rules.indexOf('pub fn take('), rules.indexOf('\n}', rules.indexOf('pub fn take(')));
  assert.match(take, /\|held\| \{\s+step\.from = held\.lane\.clone\(\);/, 'where the slice is comes from the caller');
  assert.ok(take.includes('let verdict = rules.check(&step)?;'), 'a move is written without the rules');

  const view = read(VIEW);
  const asks = [...view.matchAll(/invoke\("move_slice", \{([^}]*)\}\)/g)].map((match) => match[1].split(',').map((each) => each.trim().split(':')[0]));
  assert.deepEqual(asks, [['id', 'story', 'slice', 'to']], 'the page sends more than the slice and its lane');

  const security = flat(read('SECURITY.md'));
  assert.ok(
    security.includes('is made as you: the page names only the lane, so it cannot say a model, the check, or a reviewer made it.'),
    'SECURITY.md no longer says a move from the page is made as you',
  );
  assert.ok(flat(read('BOARD.md')).includes('`board sync` then carries them to the shared file'), 'BOARD.md no longer says a move reaches the shared file');
});
