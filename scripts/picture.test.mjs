// The picture check, checked on every test run.
//
// It answers Brad's ask, 2026-11-22: "[Rusty Business™] can make a new pr to
// main with tests that allows a one click install on marqo should the user
// choose, and uninstall too".
//
// The Rust tests in picture.rs score drawn pictures with a tiny model and
// check the rules on the bytes. These fail when the check installs itself or
// downloads past the WAN gate, when Install or Uninstall leaves the
// Installers page, when Uninstall could delete instead of moving to the
// Trash, when the model's file could reach the repository or the installer,
// when scoring could write, copy, or hash the picture, when the score could
// reach a log, a saved chat, or the browser's storage, when the caution that
// the model can make mistakes stops showing with the score, or when a high
// score does anything but show the score.
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const APP = 'apps/desktop/src/App.vue';
const RUST = 'apps/desktop/src-tauri/src/picture.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';
const CARGO = 'apps/desktop/src-tauri/Cargo.toml';
const CONF = 'apps/desktop/src-tauri/tauri.conf.json';

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** picture.rs without its tests: what ships in the app. */
function shipped() {
  const rust = read(RUST);
  const at = rust.indexOf('#[cfg(test)]');
  assert.ok(at > 0, `${RUST} keeps its tests at the end`);
  return rust.slice(0, at);
}

/** One Rust function's text, up to the next one. */
function rustFn(source, name) {
  const start = source.search(new RegExp(`\\n *(pub(\\(crate\\))? )?(async )?fn ${name}\\b`));
  assert.ok(start >= 0, `${RUST} has no ${name}`);
  const end = source.slice(start + 1).search(/\n *(pub(\(crate\))? )?(async )?fn |\n *#\[/);
  return source.slice(start, end < 0 ? undefined : start + 1 + end);
}

/** One function of the page's script, up to the next one. */
function pageFn(source, name) {
  const start = source.indexOf(`function ${name}(`);
  assert.ok(start >= 0, `${APP} has no ${name}`);
  const end = source.slice(start + 1).search(/\n(async )?function |\n\/\*\*/);
  return source.slice(start, end < 0 ? undefined : start + 1 + end);
}

/** One page's template, from its section to the next. */
const MARKERS = { installers: '<!-- Installers -->', utilities: '<!-- Utilities -->', settings: "<!-- Settings' own tabs -->" };
/** One part of the Settings page, from its marker to the next part's. */
function page(app, name, next) {
  return app.slice(app.indexOf(MARKERS[name]), app.indexOf(MARKERS[next]));
}

/** The page's script, without its template. */
function script(app) {
  return app.slice(0, app.indexOf('<template>'));
}

// Brad, 2026-11-22: "one click install on marqo should the user choose"
test('the Installers page installs the picture check in one click, and nothing installs it otherwise', () => {
  const app = read(APP);
  const installers = page(app, 'installers', 'utilities');
  // Since 0.8.4 it has a tab of its own on Installers.
  assert.match(installers, /installersTab === 'check'" class="settings-pane">\s*<h3>Picture check<\/h3>/);
  assert.match(installers, /<button[^>]*aria-label="Install the picture check"[^>]*@click="installPictureCheck"/s);
  // Install is the only way in: one call in the page, from one function,
  // which only the button calls.
  assert.equal(app.split('invoke("install_picture_check"').length, 2, 'one call installs it');
  assert.match(pageFn(app, 'installPictureCheck'), /invoke\("install_picture_check"/);
  assert.equal(app.match(/\binstallPictureCheck\b/g).length, 2, 'the function and its button, nothing else');
  // It downloads through the WAN gate and the shared download slot.
  const install = rustFn(shipped(), 'install_picture_check');
  assert.match(install, /downloads::client\(&gate\)\?/);
  assert.match(install, /\.download_own\(&gate, &PIN,/);
  assert.match(installers, /:disabled="!gate \|\| Boolean\(downloading\) \|\| !pictureCheck"/);
  assert.match(installers, /@click="cancelDownload">Cancel<\/button>/);
});

// Brad, 2026-11-22: "and uninstall too"
test('Uninstall asks first, then moves the model to the Trash, and never deletes', () => {
  const app = read(APP);
  const installers = page(app, 'installers', 'utilities');
  assert.match(installers, /aria-label="Uninstall the picture check"[^>]*\n?[^<]*@click="askUninstall = PICTURE_CHECK"/s);
  assert.match(installers, /aria-label="Yes, move the picture check"[\s\S]*?@click="uninstallPictureCheck"/);
  const rust = shipped();
  assert.match(rustFn(rust, 'uninstall_picture_check'), /if downloads\.running\(\)[\s\S]*uninstall_from\(&check\.folder\(\), uninstall::to_trash\)/);
  assert.doesNotMatch(rust, /remove_file|remove_dir/, 'nothing is deleted');
});

test('the commands are registered, and the model lives in the app\'s own folder', () => {
  const lib = read(LIB);
  for (const name of ['picture_check_status', 'install_picture_check', 'uninstall_picture_check', 'score_picture', 'pick_picture']) {
    assert.match(lib, new RegExp(`picture::${name},`));
  }
  assert.match(lib, /app\.manage\(picture::PictureCheck::new\(data\.join\("picture-check"\)\)\);/);
});

test('the model is pinned and never goes in the repository or the installer', () => {
  const rust = shipped();
  assert.match(rust, /const ID: &str = "Marqo\/nsfw-image-detection-384";/);
  assert.match(rust, /const REVISION: &str = "[0-9a-f]{40}";/);
  assert.match(rust, /const WEIGHTS_SHA256: &str = "[0-9a-f]{64}";/);
  assert.doesNotMatch(read(CONF), /safetensors|Marqo|picture-check/);
  const found = [];
  const walk = (dir) => {
    for (const entry of readdirSync(path.join(REPO, dir), { withFileTypes: true })) {
      if (['.git', 'node_modules', 'target', 'dist'].includes(entry.name)) continue;
      const rel = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(rel);
      else if (/\.(safetensors|onnx|pt|pth|bin)$/i.test(entry.name)) found.push(rel);
    }
  };
  walk('.');
  assert.deepEqual(found, [], 'no model file in the repository');
  // The crates are pinned to one version each.
  const cargo = read(CARGO);
  assert.match(cargo, /^candle-core = "=\d+\.\d+\.\d+"$/m);
  assert.match(cargo, /^candle-nn = "=\d+\.\d+\.\d+"$/m);
  assert.match(cargo, /^image = \{ version = "=\d+\.\d+\.\d+", default-features = false/m);
});

// SL, relaying Brad's go: "no copy, no hash, no score".
test('scoring reads the picture into memory and writes nothing, copies nothing, and hashes only the model', () => {
  const rust = shipped();
  assert.doesNotMatch(rust, /fs::write|File::create|fs::copy|fs::rename|OpenOptions|tempfile|temp_dir/, 'nothing is written');
  assert.match(rustFn(rust, 'read_picture'), /fs::File::open\(path\)[\s\S]*\.take\(MOST_BYTES\)/);
  assert.match(rustFn(rust, 'prepare'), /ImageReader::new\(Cursor::new\(bytes\)\)/, 'decoded from memory');
  // SHA-256 checks the model's file, never the picture.
  const hashes = rust.match(/Sha256::digest\([^)]*\)/g) ?? [];
  assert.deepEqual(hashes, ['Sha256::digest(&bytes)']);
  assert.match(rustFn(rust, 'load'), /Sha256::digest\(&bytes\)/);
  assert.doesNotMatch(rustFn(rust, 'score_picture'), /Sha256|digest/);
});

test('the score reaches the page and nowhere else: no log, no saved chat, no storage', () => {
  const rust = shipped();
  assert.doesNotMatch(rust, /println!|eprintln!|dbg!|log::|tracing::|\.log\(/, 'picture.rs logs nothing');
  const app = read(APP);
  const code = script(app);
  assert.equal(code.split('invoke("score_picture"').length, 2, 'one call scores');
  const scoring = pageFn(code, 'scorePicture');
  assert.match(scoring, /pictureScore\.value = \{ name, nsfw: score\.nsfw, sfw: score\.sfw \};/);
  assert.doesNotMatch(scoring, /localStorage|sessionStorage|console\.|save|messages|history/);
  for (const line of code.split('\n').filter((one) => /pictureScore/.test(one))) {
    assert.doesNotMatch(line, /localStorage|sessionStorage|console\.|save_session|messages|JSON\.stringify/, line);
  }
  // A score from an earlier visit is gone when the page opens again.
  assert.match(pageFn(code, 'readUtilities'), /pictureScore\.value = null;/);
});

test('the score shows with the caution that the model can make mistakes', () => {
  const rust = shipped();
  assert.match(rust, /pub const CAUTION: &str =\s*"This is a score from a small model, not a verdict\. It can make mistakes\.";/);
  const utilities = page(read(APP), 'utilities', 'settings');
  const shown = utilities.slice(utilities.indexOf('<div v-if="pictureScore"'));
  assert.match(shown, /^<div v-if="pictureScore" class="picture-score" role="status">\s*<span class="offer-name">\{\{ pictureScore\.name \}\}<\/span>\s*<span>\{\{ scoreLine\(pictureScore\) \}\}<\/span>\s*<span class="quiet small">\{\{ pictureCheck\.caution \}\}<\/span>/);
});

test('a high score only shows the score', () => {
  const app = read(APP);
  // Nothing on the page reads the number itself to decide anything.
  assert.doesNotMatch(app, /pictureScore(\.value)?\.(nsfw|sfw)\b|score\.nsfw\s*[<>]|score\.sfw\s*[<>]/);
  assert.doesNotMatch(read('apps/desktop/src/utilities.js'), /nsfw\s*[<>]=?|sfw\s*[<>]=?/);
});

test('the Utilities card is off until the person installs it, and works with both gates off', () => {
  const app = read(APP);
  const utilities = page(app, 'utilities', 'settings');
  assert.match(utilities, /<article class="utility" aria-labelledby="picture-title">\s*<h3 id="picture-title">Picture check<\/h3>/);
  assert.match(utilities, /<p v-if="!pictureReady" class="gate-ask">[\s\S]*?@click="openPage\('installers'\)"/);
  const rust = shipped();
  assert.doesNotMatch(rustFn(rust, 'score_picture'), /gate|reqwest|client/, 'scoring needs no gate');
});
