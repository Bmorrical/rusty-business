// The model makers, as the page shows them, checked on every test run.
//
// It answers the ask that started it: "user can pick google, gemini, meta from
// the app if they want those instead. I think the user can pick one or more at
// install with microsoft default, and in settings they can one click more of
// them if they want." Gemini runs only on Google's own computers, so it comes
// with the outside engines, through the outbound gate. Google's and Meta's
// models that run here are on the list now.
//
// Rust keeps the makers picked, and its tests in makers.rs check how they
// behave. These fail when the page stops asking Rust for the makers or
// leaves a refused pick ticked, when the first run loses its boxes or lets
// the last maker go, when Settings loses its one-click Add and Remove, or
// when a saved chat stops naming a model whose maker was removed.
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
const MAKERS = 'apps/desktop/src-tauri/src/makers.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';

/** The makers on the app's list, in its order. A new one is added here on purpose. */
const LISTED = ['Microsoft', 'OpenAI', 'NVIDIA', 'Google', 'Meta'];

/** The Rust tests that keep the makers, each in makers.rs or engine.rs. */
const RUST_TESTS = [
  [MAKERS, 'microsoft_is_picked_to_begin_with'],
  [MAKERS, 'a_model_already_on_this_computer_keeps_its_maker_picked'],
  [MAKERS, 'the_makers_picked_stay_as_they_were_left'],
  [MAKERS, 'at_least_one_maker_on_the_list_is_picked'],
  [MAKERS, 'makers_that_cannot_be_kept_are_not_changed'],
  [MAKERS, 'only_the_picked_makers_models_are_shown'],
  [MAKERS, 'every_maker_is_listed_with_its_models_licenses_and_downloads'],
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

test('the list holds five makers, with Microsoft picked first', () => {
  const engine = read(ENGINE);
  const catalog = engine.slice(engine.indexOf('const CATALOG'), engine.indexOf('];', engine.indexOf('const CATALOG')));
  const makers = [...new Set([...catalog.matchAll(/maker: "([^"]+)"/g)].map((found) => found[1]))];
  assert.deepEqual(makers, LISTED, 'a maker joined or left the list without this check changing');
  assert.ok(read(MAKERS).includes('pub const FIRST: &str = "Microsoft";'), 'Microsoft is no longer picked first');
  const lib = read(LIB);
  for (const command of ['makers::makers_status', 'makers::set_makers']) {
    assert.ok(lib.includes(command), `${LIB} no longer offers ${command}`);
  }
});

test('Rust keeps the tests that keep the makers', () => {
  for (const [file, name] of RUST_TESTS) {
    assert.match(read(file), new RegExp(`#\\[test\\]\\s+fn ${name}\\(\\)`), `${file} has lost ${name}`);
  }
  assert.match(
    read(ENGINE),
    /assert_eq!\(\s*names\(&shown\.set_aside\),\s+\["Phi-4"\],/,
    `${ENGINE} no longer checks that a removed maker's models are set aside`,
  );
});

test('the page asks Rust for the makers, and a refused pick is not left ticked', () => {
  const app = read(APP);
  assert.match(functionOf(app, 'readMakers'), /makers\.value = await invoke\("makers_status"\);/);
  const setMaker = functionOf(app, 'setMaker');
  assert.match(setMaker, /makers\.value = await invoke\("set_makers", \{ picked: withMaker\(makers\.value, name, on\) \}\);/);
  // Vue leaves a box alone when the pick did not change, so the box is set
  // from what Rust kept, after a refusal too.
  assert.match(setMaker, /box\.checked = pickedMakers\(makers\.value\)\.includes\(name\);/, 'a refused pick stays ticked');
  assert.match(setMaker, /finally \{\s+makersBusy\.value = false;\s+showKept\(\);/, 'the box is not set after Rust answers');
  assert.match(setMaker, /if \(makersLocked\.value\) return showKept\(\);/, 'a click while busy leaves the box ticked');
  assert.match(setMaker, /await checkEngine\(\);/, 'the models shown do not follow the makers picked');
});

test('a first run shows a box for each maker, and the last one stays', () => {
  // Brad, 2026-11-17: "I think the user can pick one or more at install with
  // microsoft default".
  const app = read(APP);
  const pick = app.slice(app.indexOf('<fieldset class="makers-pick"'), app.indexOf('</fieldset>'));
  assert.ok(pick, 'the first run has lost its makers');
  for (const part of [
    ':disabled="makersLocked"',
    'Show models from',
    'v-for="maker in makers"',
    'type="checkbox"',
    ':checked="maker.picked"',
    ':disabled="maker.picked && !canRemove(makers, maker.name)"',
    '@change="setMaker(maker.name, $event.target.checked, $event.target)"',
  ]) {
    assert.ok(pick.includes(part), `the first run's makers have lost ${part}`);
  }
  const after = app.slice(app.indexOf('</fieldset>'), app.indexOf('<p v-if="!gate" class="gate-ask">', app.indexOf('</fieldset>')));
  assert.ok(after.includes('<p v-if="makersNote" class="small" role="alert">{{ makersNote }}</p>'), 'the first run hides why a pick was refused');
});

test('Settings adds or removes a maker in one click, and is reached from the foot of the window', () => {
  // Brad, 2026-11-17: "in settings they can one click more of them if they
  // want."
  const app = read(APP);
  const start = app.indexOf('<section v-if="page === \'settings\'" class="settings"');
  assert.ok(start >= 0, 'the window has lost its Settings page');
  const settings = app.slice(start, app.indexOf('</section>', start));
  for (const part of [
    '<h3>Model makers</h3>',
    ':aria-label="`Remove ${maker.name}`"',
    ':disabled="makersLocked || !canRemove(makers, maker.name)"',
    '@click="setMaker(maker.name, false)"',
    ':aria-label="`Add ${maker.name}`"',
    '@click="setMaker(maker.name, true)"',
    '{{ listedLine(entry) }}',
    'One maker always stays picked.',
    '<p v-if="makersNote" class="small" role="alert">{{ makersNote }}</p>',
  ]) {
    assert.ok(settings.includes(part), `Settings has lost ${part}`);
  }
  assert.match(settings, /Removing a maker leaves its\s+models on this computer\./, 'Settings no longer says removing a maker deletes nothing');

  const foot = app.slice(app.indexOf('<footer class="engine"'), app.indexOf('</footer>', app.indexOf('<footer class="engine"')));
  const settingsButton = foot.indexOf('@click="togglePage(\'settings\')"');
  assert.ok(settingsButton >= 0, 'the foot of the window has lost its Settings button');
  assert.ok(settingsButton < foot.indexOf('Check for updates'), 'Settings no longer sits above Check for updates');
  assert.match(functionOf(app, 'openPage'), /await readMakers\(\);/, 'Settings opens without asking which makers are picked');
});

test('a saved chat still names a model whose maker was removed', () => {
  const name = functionOf(read(APP), 'modelName');
  assert.match(name, /\[\.\.\.models\.value, \.\.\.\(engine\.value\?\.setAside \?\? \[\]\)[,\]]/, 'a removed maker takes its name off the chats it wrote');
});
