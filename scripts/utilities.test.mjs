// The Utilities page and its first utility, HEIC to JPEG, checked on every
// test run.
//
// It answers Brad's asks, 2026-11-22: "I want a new "utilities" page or
// button, and it's job is to list helpful utilities I used all the time.
// convert HEIC to JPEG is goal." Then: "Utilities Page, Mac only for now,
// strip first with a toggle to keep, take suggested".
//
// The Rust tests in utilities.rs convert drawn pictures and check the rules
// on the bytes. These fail when the page loses its button or its card, when
// the location is kept by default or the choice is saved, when a copy could
// replace a file or change the picture it was made from, when the converter
// reaches past this computer or needs a new crate, when Show can open any
// path, or when another system is offered what only a Mac does.
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
const RUST = 'apps/desktop/src-tauri/src/utilities.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';
const CARGO = 'apps/desktop/src-tauri/Cargo.toml';

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** One Rust function's text, up to the next one. */
function rustFn(source, name) {
  const start = source.search(new RegExp(`\\n(pub )?(async )?fn ${name}\\b`));
  assert.ok(start >= 0, `${RUST} has no ${name}`);
  const end = source.slice(start + 1).search(/\n(pub )?(async )?fn |\n#\[/);
  return source.slice(start, end < 0 ? undefined : start + 1 + end);
}

// Brad, 2026-11-22: "I want a new "utilities" page or button"
test('the side bar has a Utilities button after Installers, and the page lists each utility as a card', () => {
  const app = read(APP);
  const installers = app.indexOf(`@click="togglePage('installers')"`);
  const utilities = app.indexOf(`@click="togglePage('utilities')"`);
  const settings = app.indexOf(`@click="togglePage('settings')"`);
  assert.ok(installers > 0 && installers < utilities && utilities < settings, 'Utilities sits between Installers and Settings');
  const page = app.slice(app.indexOf(`<section v-if="page === 'utilities'"`), app.indexOf(`<section v-if="page === 'settings'"`));
  assert.match(page, /<h2 id="utilities-title"[^>]*>Utilities<\/h2>/);
  assert.match(page, /<article class="utility"[^>]*>\s*<h3 id="heic-title">HEIC to JPEG<\/h3>/);
  assert.match(page, /work with both gates off/);
});

// Brad, 2026-11-22: "Mac only for now"
test('HEIC to JPEG is offered on a Mac only, and other systems are told so', () => {
  const rust = read(RUST);
  assert.match(rustFn(rust, 'utilities_status'), /heic: cfg!\(target_os = "macos"\)/);
  assert.match(rust, /#\[cfg\(not\(target_os = "macos"\)\)\]\nfn to_jpeg\([^)]*\) -> Result<Vec<u8>, String> \{\n\s*Err\(MAC_ONLY\.to_string\(\)\)/);
  const page = read(APP);
  assert.match(page, /<p v-if="!heicReady" class="gate-ask">\s*<span>HEIC to JPEG needs a Mac for now\.<\/span>/);
  assert.match(page, /const heicReady = ref\(false\);/, 'the page offers nothing until Rust says yes');
});

// Brad, 2026-11-22: "strip first with a toggle to keep"
test('where a picture was taken is left out unless the person keeps it, each visit', () => {
  const app = read(APP);
  assert.match(app, /const heicKeepPlace = ref\(false\);/, 'the location is left out by default');
  assert.match(app, /<input v-model="heicKeepPlace" type="checkbox" \/>\s*<span>\s*Keep where each picture was taken/);
  assert.match(app, /keepLocation: heicKeepPlace\.value,/);
  assert.equal(app.split('heicKeepPlace').length, 4, 'the choice is not saved anywhere');
  const rust = read(RUST);
  assert.match(rust, /pub fn to_jpeg\(bytes: &\[u8\], quality: f64, keep_location: bool\)[\s\S]*?if !keep_location \{\n\s*pairs\.push\(\(kCGImagePropertyGPSDictionary, kCFNull\)\);/);
  assert.match(rust, /fn a_drawn_picture_becomes_a_jpeg_without_where_it_was_taken\(\)/);
  assert.match(rust, /fn a_drawn_heic_becomes_a_jpeg_without_where_it_was_taken\(\)/);
});

// Brad, 2026-11-22: "take suggested": saved beside the picture, the picture
// never changed, a file never replaced, quality 90, offline.
test('a copy never replaces a file and never changes the picture it was made from', () => {
  const rust = read(RUST);
  const write = rustFn(rust, 'write_beside');
  assert.match(write, /\.create_new\(true\)/, 'a name already taken is never opened');
  assert.doesNotMatch(rust, /File::create|\.truncate\(true\)|fs::write\(&out|fs::rename|remove_file\(source/);
  assert.match(write, /let _ = fs::remove_file\(&out\);/, 'only the file it just made is removed when a write fails');
  assert.equal(rust.match(/remove_file\(/g).length, 1);
  assert.match(rust, /fn a_file_already_there_is_never_replaced\(\)/);
  assert.match(read('apps/desktop/src/utilities.js'), /export const QUALITY = 90;/);
});

test('the converter stays on this computer and adds no crate', () => {
  const rust = read(RUST);
  assert.doesNotMatch(rust, /reqwest|gate::|http/, 'nothing reaches past this computer');
  assert.match(rust, /#\[link\(name = "ImageIO", kind = "framework"\)\]/, 'the Mac reads and writes the pictures');
  assert.doesNotMatch(read(CARGO), /^(libheif|libheif-rs|heic|core-foundation|core-graphics)\s*=/m);
  // The picture check brings the image crate, for JPEG, PNG, and WebP only.
  // HEIC stays with ImageIO: the converter never uses it, and it never reads HEIC.
  assert.doesNotMatch(rust, /\bimage::|use image\b/, 'the converter does not use the image crate');
  const image = read(CARGO).match(/^image = .*$/m)?.[0] ?? '';
  assert.doesNotMatch(image, /heif|heic|avif/i, 'the image crate reads no HEIC');
});

test('Show opens only a JPEG this window made, and the commands are registered', () => {
  const rust = read(RUST);
  const show = rustFn(rust, 'show_jpeg');
  assert.ok(show.indexOf('list.contains(&path)') < show.indexOf('Command::new'), 'Show checks the path before opening anything');
  assert.match(show, /\.arg\("-R"\)/, 'Show reveals the file, never opens it');
  assert.match(rustFn(rust, 'heic_to_jpeg'), /list\.insert\(out\.clone\(\)\)/);
  const lib = read(LIB);
  assert.match(lib, /\.manage\(utilities::Made::default\(\)\)/);
  for (const name of ['utilities_status', 'heic_pictures', 'heic_to_jpeg', 'pick_heic', 'show_jpeg']) {
    assert.ok(lib.includes(`utilities::${name},`), `${name} is not registered`);
  }
});
