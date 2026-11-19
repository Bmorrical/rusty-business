// Switching a model off, and uninstalling one, checked on every test run.
//
// It answers the ask that started it: "next story, I should be able to
// deactivate and uninstall a model. I can't do that currently from what I can
// see." (Brad, 2026-11-19.)
//
// Rust keeps the models switched off and moves a model to the Trash, and its
// tests in deactivated.rs and uninstall.rs check how they behave. These fail
// when one of those tests goes, when Uninstall could delete or reach past the
// models folder, when its tests could touch the real Trash or the real
// Hugging Face cache, when the app stops offering the commands, or when the
// page loses the switch, the Uninstall button, or the question before it, or
// stops leaving a model switched off out of the questions, grading, and
// comparisons.
//
// The roadmap lists the switch and Uninstall as done, and the Settings
// picture shows them.
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

import { SCENES } from './screenshots.mjs';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const APP = 'apps/desktop/src/App.vue';
const DEACTIVATED = 'apps/desktop/src-tauri/src/deactivated.rs';
const UNINSTALL = 'apps/desktop/src-tauri/src/uninstall.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';
const CARGO = 'apps/desktop/src-tauri/Cargo.toml';

/** The Rust tests that keep the switch and Uninstall. */
const RUST_TESTS = [
  [DEACTIVATED, 'a_model_switched_off_stays_off_until_it_is_switched_on'],
  [DEACTIVATED, 'only_models_the_app_knows_are_kept_off'],
  [DEACTIVATED, 'models_that_cannot_be_kept_off_are_left_as_they_were'],
  [UNINSTALL, 'uninstall_moves_only_that_model_out_of_the_cache'],
  [UNINSTALL, 'uninstall_moves_a_gguf_folder_or_a_gguf_file_on_its_own'],
  [UNINSTALL, 'a_model_not_on_the_list_or_not_here_is_never_moved'],
  [UNINSTALL, 'a_model_the_trash_refuses_stays_where_it_was'],
];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** A Rust file's code before its tests, and its tests. */
function halves(file) {
  const text = read(file);
  const at = text.indexOf('#[cfg(test)]');
  assert.ok(at > 0, `${file} has no tests`);
  return [text.slice(0, at), text.slice(at)];
}

/** One function's text in App.vue, up to the next one. */
function functionOf(app, name) {
  const start = app.search(new RegExp(`\\n(async )?function ${name}\\(`));
  assert.ok(start >= 0, `${APP} has no ${name}`);
  const end = app.slice(start + 1).search(/\n(async )?function /);
  return app.slice(start, end < 0 ? undefined : start + 1 + end);
}

/** The Models tab of Settings, as the page writes it. */
function modelsTab(app) {
  const start = app.indexOf(`settingsTab === 'models'" class="settings-pane"`);
  assert.ok(start > 0, `${APP} has no Models tab in Settings`);
  return app.slice(start, app.indexOf('<div v-else class="settings-pane">', start));
}

test('Rust keeps the tests that keep the switch and Uninstall', () => {
  for (const [file, name] of RUST_TESTS) {
    assert.match(read(file), new RegExp(`#\\[test\\]\\s+fn ${name}\\(\\)`), `${file} has lost ${name}`);
  }
});

test('the app offers the switch and Uninstall, and keeps the switch in its own folder', () => {
  const lib = read(LIB);
  for (const command of [
    'deactivated::deactivated_models',
    'deactivated::set_model_active',
    'uninstall::uninstall_model',
    'uninstall::uninstall_place',
  ]) {
    assert.ok(lib.includes(command), `${LIB} no longer offers ${command}`);
  }
  assert.ok(
    lib.includes('app.manage(deactivated::Deactivated::new(data.join("deactivated")));'),
    `${LIB} no longer keeps the models switched off in the app's own folder`,
  );
});

test('Uninstall moves to the Trash and never deletes', () => {
  const [code, tests] = halves(UNINSTALL);
  assert.match(read(CARGO), /^trash = "5\.\d+"$/m, `${CARGO} no longer has the trash crate`);
  assert.ok(code.includes('context.delete(place)'), `${UNINSTALL} no longer moves a model with the trash crate`);
  // On a Mac, the file manager moves it, so the app never asks to control Finder.
  assert.ok(code.includes('DeleteMethod::NsFileManager'), `${UNINSTALL} no longer moves a model through the file manager on a Mac`);
  for (const removal of ['remove_dir_all', 'remove_dir(', 'remove_file', 'fs::rename', 'Command::new']) {
    assert.ok(!code.includes(removal), `${UNINSTALL} now uses ${removal} outside its tests`);
  }
  // Only a model on the list, and only a plain name in the models folder.
  assert.ok(code.includes('engine::allowed_models(&[id.to_string()])'), `${UNINSTALL} no longer checks the list`);
  assert.ok(code.includes('.filter(|name| models::plain_name(name))'), `${UNINSTALL} no longer refuses a name that steps out`);
  assert.ok(code.includes('if downloads.running()'), `${UNINSTALL} no longer waits for a download`);
  // Its tests move into a folder of their own, never the real Trash or cache.
  assert.ok(tests.includes('std::env::temp_dir()'), `${UNINSTALL}'s tests no longer use a temporary folder`);
  for (const real of ['to_trash)', 'TrashContext', 'models_folder()', 'uninstall_model(', 'home_dir', '.cache/huggingface']) {
    assert.ok(!tests.includes(real), `${UNINSTALL}'s tests now reach ${real}`);
  }
});

test('the page leaves a model switched off out of questions, grading, and comparisons', () => {
  const app = read(APP);
  assert.ok(
    app.includes('const models = computed(() => switchedOn(onHand.value, off.value));'),
    `${APP} no longer leaves switched-off models out of the list`,
  );
  // Every place a model is picked from reads that list.
  for (const [what, line] of [
    ['the models a question goes to', 'const chosen = computed(() => models.value.filter('],
    ['the grader', 'const grader = computed(() => graderFor(graderPick.value, models.value));'],
    ['a pick', 'const ids = models.value.map((entry) => entry.id);\n  picked.value = keptPicks('],
  ]) {
    assert.ok(app.includes(line), `${APP} no longer picks ${what} from the models switched on`);
  }
  // The chips at the top, and the grader's list, show only models switched on.
  assert.equal((app.match(/v-for="entry in models"/g) ?? []).length, 2, `${APP} lists models from elsewhere`);
  // The switch is read before the engine, so a model switched off is never picked at the start.
  const mounted = app.slice(app.indexOf('onMounted(async () => {'));
  assert.ok(mounted.indexOf('await readOff();') < mounted.indexOf('checkEngine()'), `${APP} reads the switch too late`);
  // A chat a model switched off wrote still names it.
  assert.ok(functionOf(app, 'modelName').includes('...onHand.value'), `${APP} forgets the names of models switched off`);
});

test('Settings, Models has the switch, and Uninstall asks first', () => {
  const app = read(APP);
  const tab = modelsTab(app);
  assert.equal((tab.match(/role="switch"/g) ?? []).length, 2, 'Settings, Models lost a switch: one for models here, one for outside models');
  assert.equal((tab.match(/:aria-label="`Use \$\{entry\.name\}`"/g) ?? []).length, 2, 'a switch lost its name');
  assert.ok(tab.includes('@click="setActive(entry.ids, !isOn(entry.ids, off))"'), 'the switch for a model here changed');
  assert.ok(tab.includes('@click="setActive([entry.id], !isOn([entry.id], off), true)"'), 'the switch for an outside model changed');
  // Uninstall only asks; Yes moves it.
  assert.ok(tab.includes(':aria-label="`Uninstall ${entry.name}`"'), 'Settings, Models lost Uninstall');
  assert.ok(tab.includes('@click="askUninstall = entry.name"'), 'Uninstall no longer asks first');
  assert.ok(tab.includes('{{ uninstallAsk(entry.name, uninstallPlace) }}'), 'the question before Uninstall is gone');
  assert.match(tab, /@click="uninstall\(entry\)"\s*>\s*Yes\s*</, 'Yes no longer uninstalls');
  assert.ok(tab.includes(`@click="askUninstall = ''">No</button>`), 'No is gone');
  // What the buttons ask Rust.
  assert.ok(
    functionOf(app, 'setActive').includes('await invoke("set_model_active", { id, active })'),
    'the switch no longer asks Rust',
  );
  const uninstall = functionOf(app, 'uninstall');
  assert.ok(uninstall.includes('await invoke("uninstall_model", { id })'), 'Uninstall no longer asks Rust');
  assert.ok(uninstall.includes('await checkEngine();'), 'the models on hand are not read again after Uninstall');
});

test('the roadmap and the Settings picture show the switch and Uninstall', () => {
  const roadmap = read('ROADMAP.md');
  const done = roadmap.slice(roadmap.indexOf('\n## Done\n'));
  const start = done.indexOf('\n- **Switching a model off, and uninstalling one.**');
  assert.ok(start >= 0, 'the roadmap does not list the switch and Uninstall as done');
  const next = done.indexOf('\n- ', start + 1);
  const row = done.slice(start, next < 0 ? undefined : next);
  for (const words of ['a switch', 'the outside engines', 'left out of questions, grading, and comparisons', 'the Trash', 'Nothing is deleted.']) {
    assert.ok(row.replace(/\s+/g, ' ').includes(words), `the roadmap's row no longer says "${words}"`);
  }
  // The picture opens Settings on its Models tab, where the switches are.
  const scene = SCENES.find((each) => each.file === 'settings.png');
  assert.deepEqual([scene?.page, scene?.tab], ['settings', 'Models'], 'settings.png no longer shows Settings, Models');
  for (const file of ['README.md', 'docs/screenshots/README.md']) {
    const alt = read(file).match(/\[!\[([^\]]+)\]\((?:docs\/screenshots\/)?settings\.png\)/)?.[1] ?? '';
    assert.match(alt, /switch on and an Uninstall button/, `${file} no longer says the Settings picture shows the switch and Uninstall`);
  }
});
