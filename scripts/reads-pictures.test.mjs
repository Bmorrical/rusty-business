// Models that read pictures, checked on every test run.
//
// It answers Brad's ask, 2026-11-22: "allow all 3 models, user can on click
// to install, start a grouping, maybe tabbed screens, for each "type"
// collections of models, they can exists in more than one tab where
// applicable." He took "Phi-3.5 now, rest later", "Picture in chat in 0.8.4",
// and "Mac only first".
//
// The page tests in chat.test.js check what is sent: pictures go only to a
// model that reads them, and a model that cannot is only told. The Rust tests
// in vision.rs and downloads.rs check the engine, the pin, and the refusals.
// These fail when Installers loses its tabs, when Phi-3.5 Vision leaves the
// Chat or Pictures tab or loses its Install or Uninstall, when the two models
// still to come gain a download, when a picture can reach an outside engine,
// when the pin loses its revision or gains code, when the picture model joins
// the first-run offers, or when it is offered off a Mac.
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
const ENGINE = 'apps/desktop/src-tauri/src/engine.rs';
const VISION = 'apps/desktop/src-tauri/src/vision.rs';
const DOWNLOADS = 'apps/desktop/src-tauri/src/downloads.rs';
const PINS = 'apps/desktop/src-tauri/pinned-models.json';

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** One Installers tab, from its opening tag to the next tab. */
function installersPane(app, tab) {
  const start = app.indexOf(`installersTab === '${tab}'" class="settings-pane"`);
  assert.ok(start >= 0, `Installers has no ${tab} tab`);
  const next = app.indexOf('class="settings-pane"', start + 40);
  return app.slice(start, next < 0 ? undefined : next);
}

/** One Rust function's text, up to the next function. */
function rustFunction(text, name) {
  const start = text.search(new RegExp(`\\n(pub(\\(crate\\))? )?(async )?fn ${name}\\b`));
  assert.ok(start >= 0, `no fn ${name}`);
  const end = text.slice(start + 1).search(/\n(pub(\(crate\))? )?(async )?fn /);
  return text.slice(start, end < 0 ? undefined : start + 1 + end);
}

test('Installers groups models by type in tabs', () => {
  const app = read(APP);
  for (const [tab, label] of [
    ['chat', 'Chat'],
    ['pictures', 'Pictures'],
    ['check', 'Picture check'],
    ['outside', 'Outside engines'],
  ]) {
    assert.ok(app.includes(`@click="installersTab = '${tab}'">${label}</button>`), `no ${label} tab`);
  }
  assert.match(app, /const installersTab = ref\("chat"\);/, 'Installers must open on Chat');
});

test('a model that reads pictures shows in more than one tab, with one-click Install and Uninstall', () => {
  const app = read(APP);
  const chat = installersPane(app, 'chat');
  assert.match(chat, /v-for="entry in maker\.models"/, 'the Chat tab must list every model, pictures too');
  assert.match(chat, /Reads pictures/, 'the Chat tab must mark a model that reads pictures');
  assert.match(app, /maker\.models\.filter\(\(entry\) => entry\.pictures\)/, 'the Pictures tab must take models that read pictures');
  const pictures = installersPane(app, 'pictures');
  assert.match(pictures, /v-for="\{ maker, entry \} in pictureEntries"/);
  assert.match(pictures, /@click="install\(maker\.name, entry\.offer\)"/, 'Install must be one click');
  assert.match(pictures, /@click="uninstall\(entry\)"/, 'Uninstall must be there too');
  assert.match(pictures, /Models that read pictures need a Mac for now\./, 'another system must be told');
});

test('the two models still to come are listed, with nothing to download', () => {
  const app = read(APP);
  const list = app.slice(app.indexOf('const PICTURE_MODELS_COMING'), app.indexOf('];', app.indexOf('const PICTURE_MODELS_COMING')));
  for (const name of ['Phi-4-multimodal', 'Molmo-7B-O']) assert.ok(list.includes(name), `${name} is missing`);
  const coming = installersPane(app, 'pictures');
  const row = coming.slice(coming.indexOf('in PICTURE_MODELS_COMING'), coming.indexOf('</li>', coming.indexOf('in PICTURE_MODELS_COMING')));
  assert.doesNotMatch(row, /<button/, 'a model still to come must have no button');
  assert.match(row, /coming in a later build/);
});

test('a picture never leaves this computer, and a model that cannot read it is never sent one', () => {
  const engine = read(ENGINE);
  assert.match(
    engine,
    /if outside::is_outside\(&request\.model\) && vision::has_pictures\(&request\.messages\) \{[^}]*Err\(vision::STAYS_HERE\.to_string\(\)\)/s,
    'an outside engine must refuse a picture',
  );
  const checked = rustFunction(read(VISION), 'checked_pictures');
  assert.match(checked, /CANNOT_SEE/, 'Rust must refuse a picture for a model that cannot read it');
  const chat = read(CHAT);
  assert.match(chat, /const images = seesPictures \? pictures : \[\];/, 'the page must send pictures only to a model that reads them');
  const app = read(APP);
  assert.equal((app.match(/requestMessages\([^\n]*, "", entry\.pictures === true\)/g) ?? []).length, 2, 'both sends must say whether the model reads pictures');
});

test('Phi-3.5 Vision is pinned to one revision, with no code to run', () => {
  const pins = JSON.parse(read(PINS));
  const pin = pins.find((entry) => entry.id === 'mlx-community/Phi-3.5-vision-instruct-4bit');
  assert.ok(pin, 'Phi-3.5 Vision is not pinned');
  assert.match(pin.revision, /^[0-9a-f]{40}$/);
  for (const file of pin.files) {
    assert.doesNotMatch(file.name, /\.py$/, `${file.name} would be code`);
    assert.match(file.sha256, /^[0-9a-f]{64}$/);
  }
  assert.doesNotMatch(read(VISION).split('#[cfg(test)]')[0], /trust-remote-code/, 'the engine must not run a model\'s code');
});

test('the picture model is offered on a Mac only, and never as a first model', () => {
  const downloads = read(DOWNLOADS);
  const offers = rustFunction(downloads, 'picture_offers');
  assert.match(offers, /if cfg!\(target_os = "macos"\) \{\s*offers_of\(pins_here\(\), true\)\s*\} else \{\s*Vec::new\(\)/);
  assert.match(rustFunction(downloads, 'offers_from'), /offers_of\(pins, false\)/, 'the first-run offers must leave it out');
});
