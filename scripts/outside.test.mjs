// The engines outside this computer, Grok by xAI, GPT by OpenAI, Claude by
// Anthropic, and Gemini by Google, checked on every test run.
//
// They answer the asks that started them: "user can add their outside
// inference, start with Grok and Grok ai. user adds it manually, they will
// need to provide or specify a path to a key, now, the Rusty Business™ can
// query Grok for the same question", "you can use grok by it's self too.",
// and "and claude and codex/gpt and gemmini".
//
// Rust keeps each path and reads each key, and its tests in outside.rs check
// how it behaves; chat.test.js checks the page's helpers. These fail when a
// key could reach the page, a log, or anywhere but its maker's API, when a
// maker's address can be changed, when a key is read before the gate is
// checked, when an outside engine waits for the turn the models on this
// computer take, when the page keeps a path, leaves a pasted key on screen,
// offers an engine with the gate off, or loses the way to add, refresh, and
// remove one, or when a rule loses its Rust test.
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

/** The Rust tests that keep the outside engines' rules. */
const RUST_TESTS = [
  'the_key_never_leaves_rust',
  'a_key_pasted_where_the_path_goes_is_refused_and_not_kept',
  'a_file_that_is_not_just_a_key_is_refused',
  'a_key_file_others_can_read_is_warned_of',
  'with_the_gate_off_grok_is_not_asked_and_the_key_is_not_read',
  'only_grok_models_found_are_asked',
  'a_model_is_asked_only_of_the_engine_that_listed_it',
  'an_engine_the_app_does_not_know_is_refused',
  'grok_is_asked_at_xais_api_alone_over_https',
  'gpt_is_asked_at_openais_api_alone_over_https',
  'claude_is_asked_at_anthropics_api_alone_over_https',
  'claude_is_signed_with_its_own_header',
  'gemini_is_asked_at_googles_api_alone_over_https',
  'gemini_is_signed_with_its_own_header',
  'xais_list_gives_grok_models_for_text_with_their_names',
  'openais_list_gives_gpt_models_for_text_newest_first_with_their_names',
  'anthropics_list_gives_claude_models_newest_first_with_their_names',
  'googles_list_gives_gemini_models_newest_first_with_their_names',
  'claude_is_asked_with_the_system_on_top_and_turns_that_take_turns',
  'gemini_is_asked_with_the_system_on_top_and_turns_that_take_turns',
  'claudes_stream_is_read_as_it_comes',
  'geminis_stream_is_read_as_it_comes',
  'xais_refusals_are_said_plainly',
  'turning_the_gate_off_stops_grok_part_way_through',
  'removing_grok_forgets_the_path_and_keeps_the_models',
  'each_engine_keeps_its_own_key_path_and_models',
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
  // The key goes out only as the request's own sign-in, marked as sensitive
  // so nothing prints it: a bearer token for xAI and OpenAI, and a header of
  // their own for Anthropic and Google. It never goes in an address. Every
  // call is signed in one place, and an answer's words are never read back
  // into an error.
  const signed = rustFunction(source, 'signed');
  assert.ok(signed.includes('Self::Xai | Self::OpenAi => return builder.bearer_auth(&key.0),'), 'xAI or OpenAI is signed another way');
  assert.ok(signed.includes('Self::Anthropic => ("x-api-key", Some(ANTHROPIC_VERSION)),'), 'Claude is signed another way');
  assert.ok(signed.includes('Self::Google => ("x-goog-api-key", None),'), 'Gemini is signed another way');
  assert.ok(signed.includes('reqwest::header::HeaderValue::from_str(&key.0)'), 'Claude or Gemini is signed another way');
  assert.match(signed, /value\.set_sensitive\(true\);\s+let signed = builder\.header\(header, value\);/, "Claude's or Gemini's key is sent without being marked as sensitive");
  assert.equal(source.match(/key\.0/g)?.length, 2, 'the key is read somewhere new');
  assert.equal(source.match(/x-api-key/g)?.length, 1, "Claude's header is set somewhere new");
  assert.equal(source.match(/x-goog-api-key/g)?.length, 1, "Gemini's header is set somewhere new");
  assert.ok(!/[?&]key=/.test(source), 'a key could now go in an address');
  assert.equal(source.match(/engine\s*\.signed\(/g)?.length, 2, 'a call is signed somewhere new');
  assert.ok(!source.includes('.text()'), "an error could now repeat a maker's own words");
});

test("each engine is asked at its maker's API alone, over https, and only while the gate is on", () => {
  const source = code(OUTSIDE);
  assert.ok(source.includes('const XAI_BASE: &str = "https://api.x.ai/v1";'), "xAI's address has moved");
  assert.ok(source.includes('pub const XAI_HOST: &str = "api.x.ai";'), "xAI's host has moved");
  assert.ok(source.includes('const OPENAI_BASE: &str = "https://api.openai.com/v1";'), "OpenAI's address has moved");
  assert.ok(source.includes('pub const OPENAI_HOST: &str = "api.openai.com";'), "OpenAI's host has moved");
  assert.ok(source.includes('const ANTHROPIC_BASE: &str = "https://api.anthropic.com/v1";'), "Anthropic's address has moved");
  assert.ok(source.includes('pub const ANTHROPIC_HOST: &str = "api.anthropic.com";'), "Anthropic's host has moved");
  assert.ok(source.includes('const GOOGLE_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";'), "Google's address has moved");
  assert.ok(source.includes('pub const GOOGLE_HOST: &str = "generativelanguage.googleapis.com";'), "Google's host has moved");
  // Each engine has one address and one host, the ones above.
  assert.match(rustFunction(source, 'base'), /Self::Xai => XAI_BASE,\s+Self::OpenAi => OPENAI_BASE,\s+Self::Anthropic => ANTHROPIC_BASE,\s+Self::Google => GOOGLE_BASE,\s+\}/, 'an engine is asked at another address');
  assert.match(rustFunction(source, 'host'), /Self::Xai => XAI_HOST,\s+Self::OpenAi => OPENAI_HOST,\s+Self::Anthropic => ANTHROPIC_HOST,\s+Self::Google => GOOGLE_HOST,\s+\}/, 'an engine answers from another host');
  // Nothing outside the code can point an engine somewhere else.
  const reads = source.match(/env::var(?:_os)?\("[A-Z_]+"\)/g) ?? [];
  assert.deepEqual(reads, ['env::var_os("HOME")', 'env::var_os("USERPROFILE")'], `${OUTSIDE} reads a setting that could move an engine`);
  assert.equal(source.match(/format!\("\{\}\/[^"]*", engine\.base\(\)\)/g)?.length, 1, 'a list is asked at an address not built on its base');
  assert.equal(source.match(/engine\.base\(\)/g)?.length, 1, 'a list is asked at an address built some other way');
  assert.equal(source.match(/format!\(\s*"\{\}\/[^"]*",\s+self\.base\(\),?\s*\)/g)?.length, 3, 'a chat is asked at an address not built on its base');
  assert.equal(source.match(/self\.base\(\)/g)?.length, 3, 'a chat is asked at an address built some other way');
  // Where a chat is asked is fixed in the code, one path for each engine.
  // Google names the model in the path, so only a model it listed goes there.
  assert.match(
    rustFunction(source, 'chat_url'),
    /Self::Xai \| Self::OpenAi => format!\("\{\}\/chat\/completions", self\.base\(\)\),\s+Self::Anthropic => format!\("\{\}\/messages", self\.base\(\)\),\s+Self::Google => format!\(\s*"\{\}\/models\/\{model\}:streamGenerateContent\?alt=sse",\s+self\.base\(\),?\s*\),\s+\}/,
    'a chat is asked at another path',
  );
  // So is where each list is asked. Anthropic gives 20 models a page and
  // Google 50, unless asked for more, so each list is asked for at once.
  const finding = rustFunction(source, 'find_models');
  for (const path of ['"language-models"', '"models"', '"models?limit=100"', '"models?pageSize=1000"']) {
    assert.ok(finding.includes(`key, ${path}).await`), `no list is asked at ${path}`);
  }
  assert.equal(finding.match(/list\(engine, client, key, "/g)?.length, 5, 'a list is asked at a path not fixed in the code');

  const client = rustFunction(source, 'client');
  assert.match(client, /\{\s+gate\.client\(&engine\.gate_off\(\),/, "an engine's client does not go through the gate");
  for (const rule of ['.no_proxy()', '.https_only(true)', 'redirect::Policy::none()', '.referer(false)']) {
    assert.ok(client.includes(rule), `the outside engines' client has lost ${rule}`);
  }
  // An answer from anywhere but the engine's own host is refused, both for
  // the list and for a chat.
  assert.equal(source.match(/if !engine\.owns\(response\.url\(\)\)/g)?.length, 2, "an answer is no longer checked to come from its maker's API");
  assert.ok(rustFunction(source, 'owns').includes('url.domain() == Some(self.host())'), "an answer is no longer checked against the engine's own host");

  // The gate is checked before the key is read, and turning it off stops
  // the engine, whether it is finding models or answering.
  for (const name of ['run_chat', 'refresh']) {
    const body = rustFunction(source, name);
    const gate = body.indexOf('client(engine, gate)?');
    assert.ok(gate >= 0, `${name} does not ask the gate first`);
    assert.ok(body.indexOf('read_key(') > gate, `${name} reads the key before it checks the gate`);
    assert.ok(body.includes('gate.closed()'), `${name} goes on when the gate is turned off`);
  }
  // A model goes only to the engine that listed it.
  const chat = rustFunction(source, 'run_chat');
  const known = chat.indexOf('outside.knows(engine, model)');
  assert.ok(known >= 0, "a model is no longer checked against its engine's list");
  assert.ok(known < chat.indexOf('client(engine, gate)?'), 'a model is asked before it is checked against its engine\'s list');
  assert.ok(known < chat.indexOf('engine.chat_url(model)'), "a model goes in Google's address before it is checked against its list");
});

test('an outside engine waits for no turn, and the app knows its commands', () => {
  const engine = code(ENGINE);
  const stream = rustFunction(engine, 'chat_stream');
  const outside = stream.indexOf('let result = if outside::is_outside(&request.model) {');
  assert.ok(outside >= 0, "the chat no longer sends an outside engine's models to it");
  assert.ok(stream.indexOf('wait_for_turn(') > outside, 'an outside engine waits for the turn the models on this computer take');
  assert.ok(stream.includes('outside::run_chat(&gate, &outside,'), 'an outside engine is asked without the gate');

  const lib = read(LIB);
  assert.ok(lib.includes('app.manage(outside::Outside::new(data.join("outside.json")));'), "the app no longer keeps the paths in its own folder");
  for (const command of ['outside_status', 'add_outside', 'refresh_outside', 'remove_outside']) {
    assert.ok(lib.includes(`outside::${command},`), `the page can no longer ask for ${command}`);
  }
  // Each command names the engine it changes, and one the app does not know
  // is refused.
  for (const command of ['add_outside', 'refresh_outside', 'remove_outside']) {
    const body = rustFunction(code(OUTSIDE), command);
    assert.ok(body.includes('engine: String,') && body.includes('Engine::named(&engine)?'), `${command} no longer names its engine`);
  }
  // Remove forgets the path. The key file is the person's, and the app never
  // removes or changes it.
  const source = code(OUTSIDE);
  for (const way of ['remove_file', 'remove_dir', 'set_permissions', 'OpenOptions']) {
    assert.ok(!source.includes(way), `${OUTSIDE} can now change a file with ${way}`);
  }
});

test('the page sends only the path, keeps none of it, and offers an engine only with the gate on', () => {
  const app = read(APP);
  assert.ok(
    app.includes('const outsideListed = computed(() => outside.value.filter((entry) => entry.added).flatMap((entry) => entry.models ?? []));') &&
      app.includes('const outsideModels = computed(() => (gate.value ? outsideListed.value : []));'),
    "an outside engine's models are offered with the gate off, or before it is added",
  );
  assert.match(app, /const onHand = computed\(\(\) => \[\.\.\.\(engine\.value\?\.models \?\? \[\]\), \.\.\.outsideModels\.value\]\);/, 'an outside engine can no longer answer alone or as one of several');
  assert.ok(app.includes('const models = computed(() => switchedOn(onHand.value, off.value));'), 'an outside engine switched off is still asked');
  // Rust gives every engine, as a list.
  assert.ok(pageFunction(app, 'readOutside').includes('outside.value = Array.isArray(listed) ? listed : [];'), 'the page no longer reads the engines as a list');

  const add = pageFunction(app, 'addOutside');
  assert.ok(add.includes('invoke') === false && add.includes('changeOutside("add_outside", id, { keyPath: given })'), 'adding an engine sends more than the path');
  assert.ok(add.includes('if (added || !looksLikePath(given)) outsidePaths.value = { ...outsidePaths.value, [id]: "" };'), 'a key pasted where the path goes stays on screen');
  for (const name of ['refreshOutside', 'removeOutside']) {
    assert.match(pageFunction(app, name), new RegExp(`changeOutside\\("${name.replace('Outside', '_outside')}", id\\)`), `${name} no longer asks Rust`);
  }
  const change = pageFunction(app, 'changeOutside');
  assert.ok(change.includes('invoke(command, { engine: id, ...args })'), 'a change no longer names its engine');
  assert.ok(change.includes('entry.id === changed.id ? changed : entry'), 'a change lands on another engine');
  // Each engine has its own note: what one says leaves the other's alone.
  const notes = [...change.matchAll(/outsideNotes\.value = ([^;]+);/g)].map((found) => found[1]);
  assert.ok(notes.length >= 2 && notes.every((each) => each.startsWith('{ ...outsideNotes.value, [id]: ')), "one engine's note wipes the other's");
  assert.ok(change.includes('pickModels(picked.value);'), "the models picked no longer follow the engines'");
  assert.ok(pageFunction(app, 'setGate').includes('pickModels(picked.value);'), "an outside engine's models stay picked once the gate is off");

  // Nothing about a key or its path is kept by the page: only the models
  // picked to answer, and the one picked to grade.
  const storage = app.match(/localStorage\.setItem\("[^"]*"/g) ?? [];
  assert.deepEqual(storage, ['localStorage.setItem("models"', 'localStorage.setItem("grader"'], 'the page keeps something new in its storage');
  assert.ok(!/sessionStorage|indexedDB|document\.cookie/.test(app), 'the page keeps something in another store');

  // One card for each engine Rust gives.
  const engines = app.indexOf('<h3 class="gap-above">Outside engines</h3>');
  assert.ok(engines >= 0, 'the Installers page has lost its outside engines');
  const settings = app.slice(engines, app.indexOf('</section>', engines));
  assert.ok(settings.includes('v-for="entry in outside"'), 'the Installers page no longer shows every outside engine');
  // The box for the path: labelled, and not checked, filled in, or changed
  // by the system as it is typed.
  const form = settings.slice(settings.indexOf('<form v-else class="key-path"'), settings.indexOf('</form>'));
  assert.ok(form, 'the Installers page has lost the way to add an engine');
  for (const part of ['@submit.prevent="addOutside(entry.id)"', ':for="`${entry.id}-key-path`"', ':id="`${entry.id}-key-path`"', 'v-model="outsidePaths[entry.id]"', 'spellcheck="false"', 'autocomplete="off"', 'autocapitalize="off"', 'autocorrect="off"']) {
    assert.ok(form.includes(part), `the box for the path has lost ${part}`);
  }
  assert.ok(!/type="password"/.test(form), 'the box asks for the key, not the path');
  for (const part of ['@click="refreshOutside(entry.id)"', '@click="removeOutside(entry.id)"', '{{ entry.keyPath }}', '{{ entry.warning }}', '{{ outsideModelsLine(entry) }}', '{{ outsideNotes[entry.id] }}', '{{ outsideNote }}', '@click="setGate(true)"', 'The key never shows here.']) {
    assert.ok(settings.includes(part), `the Installers page's outside engines have lost ${part}`);
  }
  // Vue counts an empty string as true for :disabled, and the busy word is ""
  // while nothing is under way, so it would lock the box and Remove for good.
  assert.ok(!/:disabled="outsideBusy\b/.test(app), 'the box for the path or a button stays locked while nothing is under way');
  // A chip says when a model is asked online.
  assert.ok(app.includes('<span class="quiet small chip-detail">{{ chipDetail(entry) }}</span>'), 'a chip no longer says when its model is asked online');
  // A saved chat still names an outside model once the gate is off or its
  // engine is removed.
  assert.ok(pageFunction(app, 'modelName').includes('...outside.value.flatMap((entry) => entry.models ?? [])]'), 'a saved chat loses the name of the outside model that answered it');
});

test("every rule for the outside engines has its Rust test", () => {
  const source = read(OUTSIDE);
  for (const name of RUST_TESTS) {
    assert.match(source, new RegExp(`#\\[test\\]\\s+fn ${name}\\(`), `${OUTSIDE} has lost the test ${name}`);
  }
});
