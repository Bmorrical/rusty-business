// Grok, the first engine outside this computer, checked on every test run.
//
// It answers the ask that started it: "user can add their outside inference,
// start with Grok and Grok ai. user adds it manually, they will need to
// provide or specify a path to a key, now, the Rusty Business™ can query Grok
// for the same question", and "you can use grok by it's self too."
//
// Rust keeps the path and reads the key, and its tests in outside.rs check
// how it behaves; chat.test.js checks the page's helpers. These fail when the
// key could reach the page, a log, or anywhere but xAI's API, when xAI's
// address can be changed, when the key is read before the gate is checked,
// when Grok waits for the turn the models on this computer take, when the
// page keeps the path, leaves a pasted key on screen, offers Grok with the
// gate off, or loses the way to add, refresh, and remove it, or when a rule
// loses its Rust test.
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
const OUTSIDE = 'apps/desktop/src-tauri/src/outside.rs';
const ENGINE = 'apps/desktop/src-tauri/src/engine.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';

/** The Rust tests that keep Grok's rules. */
const RUST_TESTS = [
  'the_key_never_leaves_rust',
  'a_key_pasted_where_the_path_goes_is_refused_and_not_kept',
  'a_file_that_is_not_just_a_key_is_refused',
  'a_key_file_others_can_read_is_warned_of',
  'with_the_gate_off_grok_is_not_asked_and_the_key_is_not_read',
  'only_grok_models_found_are_asked',
  'grok_is_asked_at_xais_api_alone_over_https',
  'xais_list_gives_grok_models_for_text_with_their_names',
  'xais_refusals_are_said_plainly',
  'turning_the_gate_off_stops_grok_part_way_through',
  'removing_grok_forgets_the_path_and_keeps_the_models',
  'a_kept_file_is_read_with_care',
];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** A Rust file's own code, without the tests below it. */
function code(file) {
  return read(file).split('#[cfg(test)]\nmod tests')[0];
}

/** One Rust function's text, at the top level or in an impl, to its last brace. */
function rustFunction(source, name) {
  const found = new RegExp(`\\n( *)(?:pub(?:\\(crate\\))? )?(?:async )?fn ${name}[<(]`).exec(source);
  assert.ok(found, `there is no fn ${name}`);
  const end = source.indexOf(`\n${found[1]}}\n`, found.index);
  return source.slice(found.index, end);
}

/** One function's text in App.vue, up to the next one. */
function pageFunction(app, name) {
  const start = app.search(new RegExp(`\\n(?:async )?function ${name}\\(`));
  assert.ok(start >= 0, `${APP} has no ${name}`);
  const end = app.slice(start + 1).search(/\n(?:async )?function |\n\/\*\*|\nonMounted\(/);
  return app.slice(start, end < 0 ? undefined : start + 1 + end);
}

test('the key stays in Rust: never sent to the page, never logged, never kept by the app', () => {
  const source = code(OUTSIDE);
  // The key has no Debug, Display, Clone, or Serialize to leak it through.
  assert.match(source, /\n\/\/\/[^\n]*\n(?:\/\/\/[^\n]*\n)*struct Key\(String\);/, 'the key is no longer a type of its own');
  assert.doesNotMatch(source, /#\[derive[^\]]*\]\s*(?:\/\/\/[^\n]*\s*)*struct Key\b/, 'the key can now be printed, copied, or sent');
  for (const way of ['println!', 'eprintln!', 'print!', 'dbg!', 'log::', 'tracing::']) {
    assert.ok(!source.includes(way), `${OUTSIDE} writes with ${way}`);
  }
  // What the page gets names the path and the models, never the key.
  const status = source.slice(source.indexOf('pub struct OutsideStatus'), source.indexOf('}', source.indexOf('pub struct OutsideStatus')));
  assert.ok(status.includes('key_path: Option<String>'), 'the page no longer learns which file holds the key');
  assert.doesNotMatch(status.replace(/key_path/g, ''), /key/i, 'what the page gets has a field for the key');
  // What the app keeps is the path, and the models found.
  const kept = source.slice(source.indexOf('struct Kept {'), source.indexOf('}', source.indexOf('struct Kept {')));
  assert.ok(kept.includes('key_path: Option<PathBuf>'), 'the app no longer keeps the path');
  assert.doesNotMatch(kept.replace(/key_path/g, ''), /key/i, 'the app keeps the key');
  // The key goes out only as the request's own sign-in, which reqwest marks
  // as sensitive, and an answer's words are never read back into an error.
  assert.equal(source.match(/\.bearer_auth\(&key\.0\)/g)?.length, 2, 'the key is used somewhere new');
  assert.equal(source.match(/key\.0/g)?.length, 2, 'the key is read somewhere new');
  assert.ok(!source.includes('.text()'), "an error could now repeat xAI's own words");
});

test("Grok is asked at xAI's API alone, over https, and only while the gate is on", () => {
  const source = code(OUTSIDE);
  assert.ok(source.includes('const XAI_BASE: &str = "https://api.x.ai/v1";'), "xAI's address has moved");
  assert.ok(source.includes('pub const XAI_HOST: &str = "api.x.ai";'), "xAI's host has moved");
  // Nothing outside the code can point Grok somewhere else.
  const reads = source.match(/env::var(?:_os)?\("[A-Z_]+"\)/g) ?? [];
  assert.deepEqual(reads, ['env::var_os("HOME")', 'env::var_os("USERPROFILE")'], `${OUTSIDE} reads a setting that could move Grok`);
  assert.equal(source.match(/\{XAI_BASE\}\//g)?.length, 2, 'Grok is asked at an address not built on XAI_BASE');

  const client = rustFunction(source, 'client');
  assert.match(client, /\{\s+gate\.client\(GATE_OFF,/, "Grok's client does not go through the gate");
  for (const rule of ['.no_proxy()', '.https_only(true)', 'redirect::Policy::none()', '.referer(false)']) {
    assert.ok(client.includes(rule), `Grok's client has lost ${rule}`);
  }
  // An answer from anywhere else is refused, both for the list and for a chat.
  assert.equal(source.match(/if !from_xai\(response\.url\(\)\)/g)?.length, 2, "an answer is no longer checked to come from xAI's API");

  // The gate is checked before the key is read, and turning it off stops
  // Grok, whether it is finding models or answering.
  for (const name of ['run_chat', 'refresh']) {
    const body = rustFunction(source, name);
    const gate = body.indexOf('client(gate)?');
    assert.ok(gate >= 0, `${name} does not ask the gate first`);
    assert.ok(body.indexOf('read_key(') > gate, `${name} reads the key before it checks the gate`);
    assert.ok(body.includes('gate.closed()'), `${name} goes on when the gate is turned off`);
  }
});

test('Grok waits for no turn, and the app knows its commands', () => {
  const engine = code(ENGINE);
  const stream = rustFunction(engine, 'chat_stream');
  const outside = stream.indexOf('let result = if outside::is_outside(&request.model) {');
  assert.ok(outside >= 0, "the chat no longer sends Grok's models to Grok");
  assert.ok(stream.indexOf('wait_for_turn(') > outside, 'Grok waits for the turn the models on this computer take');
  assert.ok(stream.includes('outside::run_chat(&gate, &outside,'), 'Grok is asked without the gate');

  const lib = read(LIB);
  assert.ok(lib.includes('app.manage(outside::Outside::new(data.join("outside.json")));'), "the app no longer keeps Grok's path in its own folder");
  for (const command of ['outside_status', 'add_outside', 'refresh_outside', 'remove_outside']) {
    assert.ok(lib.includes(`outside::${command},`), `the page can no longer ask for ${command}`);
  }
  // Remove forgets the path. The key file is the person's, and the app never
  // removes or changes it.
  const source = code(OUTSIDE);
  for (const way of ['remove_file', 'remove_dir', 'set_permissions', 'OpenOptions']) {
    assert.ok(!source.includes(way), `${OUTSIDE} can now change a file with ${way}`);
  }
});

test('the page sends only the path, keeps none of it, and offers Grok only with the gate on', () => {
  const app = read(APP);
  assert.match(
    app,
    /const outsideModels = computed\(\(\) => \(gate\.value && outside\.value\?\.added \? \(outside\.value\.models \?\? \[\]\) : \[\]\)\);/,
    "Grok's models are offered with the gate off, or before Grok is added",
  );
  assert.match(app, /const models = computed\(\(\) => \[\.\.\.\(engine\.value\?\.models \?\? \[\]\), \.\.\.outsideModels\.value\]\);/, "Grok can no longer answer alone or as one of several");

  const add = pageFunction(app, 'addOutside');
  assert.ok(add.includes('invoke') === false && add.includes('changeOutside("add_outside", { keyPath: given })'), 'adding Grok sends more than the path');
  assert.ok(add.includes('if (added || !looksLikePath(given)) outsidePath.value = "";'), 'a key pasted where the path goes stays on screen');
  for (const name of ['refreshOutside', 'removeOutside']) {
    assert.match(pageFunction(app, name), new RegExp(`changeOutside\\("${name.replace('Outside', '_outside')}"\\)`), `${name} no longer asks Rust`);
  }
  const change = pageFunction(app, 'changeOutside');
  assert.ok(change.includes('pickModels(picked.value);'), "the models picked no longer follow Grok's");
  assert.ok(pageFunction(app, 'setGate').includes('pickModels(picked.value);'), "Grok's models stay picked once the gate is off");

  // Nothing about the key or its path is kept by the page: only the models
  // picked to answer, and the one picked to grade.
  const storage = app.match(/localStorage\.setItem\("[^"]*"/g) ?? [];
  assert.deepEqual(storage, ['localStorage.setItem("models"', 'localStorage.setItem("grader"'], 'the page keeps something new in its storage');
  assert.ok(!/sessionStorage|indexedDB|document\.cookie/.test(app), 'the page keeps something in another store');

  // The box for the path: labelled, and not checked, filled in, or changed
  // by the system as it is typed.
  const form = app.slice(app.indexOf('<form v-else class="key-path"'), app.indexOf('</form>'));
  assert.ok(form, 'Settings has lost the way to add Grok');
  for (const part of ['@submit.prevent="addOutside"', 'for="xai-key-path"', 'id="xai-key-path"', 'v-model="outsidePath"', 'spellcheck="false"', 'autocomplete="off"', 'autocapitalize="off"', 'autocorrect="off"']) {
    assert.ok(form.includes(part), `the box for the path has lost ${part}`);
  }
  assert.ok(!/type="password"/.test(form), 'the box asks for the key, not the path');
  const settings = app.slice(app.indexOf('<h3 class="gap-above">Outside engines</h3>'), app.indexOf('<header v-show="!settingsOpen"'));
  assert.ok(settings, 'Settings has lost its outside engines');
  for (const part of ['@click="refreshOutside"', '@click="removeOutside"', '{{ outside.keyPath }}', '{{ outside.warning }}', '{{ outsideModelsLine(outside) }}', '{{ outsideNote }}', '@click="setGate(true)"', 'The key never shows here.']) {
    assert.ok(settings.includes(part), `Settings' outside engines have lost ${part}`);
  }
  // Vue counts an empty string as true for :disabled, and the busy word is ""
  // while nothing is under way, so it would lock the box and Remove for good.
  assert.ok(!/:disabled="outsideBusy\b/.test(app), 'the box for the path or a button stays locked while nothing is under way');
  // A chip says when a model is asked online.
  assert.ok(app.includes('<span class="quiet small chip-detail">{{ chipDetail(entry) }}</span>'), 'a chip no longer says when its model is asked online');
  // A saved chat still names a Grok model once the gate is off or Grok is removed.
  assert.match(pageFunction(app, 'modelName'), /\.\.\.\(outside\.value\?\.models \?\? \[\]\)\]/, 'a saved chat loses the name of the Grok model that answered it');
});

test("every rule for Grok has its Rust test", () => {
  const source = read(OUTSIDE);
  for (const name of RUST_TESTS) {
    assert.match(source, new RegExp(`#\\[test\\]\\s+fn ${name}\\(`), `${OUTSIDE} has lost the test ${name}`);
  }
});
