// The repository's rule: the name carries its mark wherever it is used.
//
// Brad, 2026-11-17, on the banner: "add the TM to the artwork". Then:
// "anywhere Rusty Business is used, it gets a TM".
//
// So every "Rusty Business" in the repository's text is followed by ™: the
// README, the roadmap, the pages for each folder, the window's title, the
// page, the tests, and the banner's words and its drawn name.
//
// Two places are left plain, on purpose: the app's productName in
// tauri.conf.json, and the app's file name in the README's command for
// opening the Mac app. productName is the name of the app's file and of its
// installers, where a ™ would turn into %E2%84%A2 in every download link.
//
// This file quotes Brad's words as he wrote them, so it is the one file the
// check skips.

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const SELF = 'scripts/trademark.test.mjs';
const BANNER = 'docs/rusty-business.svg';

// Each line that may name the app without the mark, by file, written out in
// full. A line that differs in any way is checked like any other.
const PLAIN = new Map([
  ['apps/desktop/src-tauri/tauri.conf.json', ['  "productName": "Rusty Business",']],
  // The app's file name, in the command that opens the Mac app (see the last
  // test below). A ™ there would name a file that is not there.
  ['README.md', ['   xattr -dr com.apple.quarantine "/Applications/Rusty Business.app"']],
  // The same file name, in the test that installs only over the Mac app put
  // in place.
  ['apps/desktop/src-tauri/src/update.rs', ['    const BUNDLE: &str = "Rusty Business.app";']],
]);

/** Each use of the name with no ™ after it, as its line number, even when the name runs across two lines. */
export function plainNames(text) {
  const found = [];
  for (const match of text.matchAll(/Rusty\s+Business(?!™)/gi)) {
    found.push(text.slice(0, match.index).split('\n').length);
  }
  return found;
}

/** Each place, as file:line, where the files given use the name without ™. */
export function missesIn(files, textOf) {
  const misses = [];
  for (const file of files) {
    if (file === SELF) continue;
    const text = textOf(file);
    if (text === null) continue;
    const lines = text.split('\n');
    const allowed = PLAIN.get(file) ?? [];
    for (const line of plainNames(text)) {
      if (!allowed.includes(lines[line - 1])) misses.push(`${file}:${line}`);
    }
  }
  return misses;
}

/** Every file git tracks or would track, that is really here. Build output and node_modules are left out because git never lists them. */
function projectFiles() {
  const listed = execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', '-z'], {
    cwd: REPO,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  return listed
    .split('\0')
    .filter(Boolean)
    .filter((file) => existsSync(path.join(REPO, file)));
}

/** A file's text, or null for a picture or anything else that is not text. */
function textOf(file) {
  const bytes = readFileSync(path.join(REPO, file));
  if (bytes.subarray(0, 8000).includes(0)) return null;
  return bytes.toString('utf8');
}

test('every use of the name in the repository carries ™', () => {
  const files = projectFiles();
  assert.ok(files.length > 50, 'the list of files looks too short to be right');

  const uses = files
    .filter((file) => file !== SELF)
    .map(textOf)
    .reduce((count, text) => count + (text?.match(/Rusty\s+Business/gi) ?? []).length, 0);
  assert.ok(uses > 20, `the name was found ${uses} times, which looks too few to be right`);
  const misses = missesIn(files, textOf);
  assert.deepEqual(misses, [], `these use the name without ™:\n${misses.join('\n')}`);
});

test("the banner's drawn name carries a smaller ™, raised, right after it", () => {
  const svg = readFileSync(path.join(REPO, BANNER), 'utf8');
  const wordmark = /<g\b[^>]*\sfont-size="(\d+(?:\.\d+)?)"[^>]*>([\s\S]*?)<\/g>/.exec(svg);
  assert.ok(wordmark && wordmark[2].includes('>Rusty<'), 'the banner has no group for its drawn name');

  const mark = /<text\b[^>]*>Business<tspan\b([^>]*)>™<\/tspan><\/text>/.exec(wordmark[2]);
  assert.ok(mark, 'the drawn name does not end in ™, right after "Business"');
  const size = /\sfont-size="(\d+(?:\.\d+)?)"/.exec(mark[1]);
  const raised = /\sdy="(-?\d+(?:\.\d+)?)"/.exec(mark[1]);
  assert.ok(size && Number(size[1]) < Number(wordmark[1]), 'the ™ is not smaller than the name');
  assert.ok(raised && Number(raised[1]) < 0, 'the ™ is not raised above the line');
});

test('a use of the name without ™ is found, on whichever line it starts', () => {
  const text = [
    'Rusty Business™ is marked.',
    'Rusty Business is not.',
    'This one runs on, Rusty',
    '  Business, across two lines.',
    'rusty business, in small letters.',
    'Rusty Business™’s mark, then a plain RUSTY BUSINESS.',
    'rusty-business and rusty_business are names in code.',
  ].join('\n');
  assert.deepEqual(plainNames(text), [2, 3, 5, 6]);
  assert.deepEqual(plainNames('Rusty Business™\nRusty\nBusiness™'), []);
});

test("only the app's file name may stay plain, and only this file is skipped", () => {
  const files = {
    'apps/desktop/src-tauri/tauri.conf.json':
      '{\n  "productName": "Rusty Business",\n  "title": "Rusty Business",\n    "productName": "Rusty Business",\n}',
    'README.md': '  "productName": "Rusty Business",',
    'scripts/other.test.mjs': '// Rusty Business',
    [SELF]: '// Rusty Business',
    'docs/picture.png': null,
  };
  assert.deepEqual(missesIn(Object.keys(files), (file) => files[file]), [
    'apps/desktop/src-tauri/tauri.conf.json:3',
    'apps/desktop/src-tauri/tauri.conf.json:4',
    'README.md:1',
    'scripts/other.test.mjs:1',
  ]);
});

// Brad, 2026-11-19: "give me the bypass command for mac and ensure it's in
// the readme for now", "to run the self-signed install". The app is signed
// only on the computer that built it, so macOS stops the download until the
// mark it puts on a download is cleared. This goes once pin 1 signs the app.
test("the README gives the command that opens the Mac app, with the app's own file name", () => {
  const read = (file) => readFileSync(path.join(REPO, file), 'utf8');
  const { productName } = JSON.parse(read('apps/desktop/src-tauri/tauri.conf.json'));
  const readme = read('README.md');
  const start = readme.indexOf('\n## Run the chat window (macOS)\n');
  assert.ok(start >= 0, 'the Mac section is missing');
  const mac = readme.slice(start, readme.indexOf('\n## ', start + 1));
  const command = `xattr -dr com.apple.quarantine "/Applications/${productName}.app"`;
  assert.ok(mac.includes(`   \`\`\`bash\n   ${command}\n   \`\`\`\n`), `the Mac section does not give: ${command}`);
  assert.deepEqual(PLAIN.get('README.md'), [`   ${command}`], 'the line let off is not the command');
  assert.ok(mac.includes('choose Open Anyway'), 'the Mac section no longer gives the System Settings way');
  const limits = readme.replace(/\s+/g, ' ');
  assert.ok(
    limits.includes('until you choose Open Anyway in System Settings or run the command in [Run the chat window (macOS)](#run-the-chat-window-macos).'),
    "the README's limits no longer point to the command",
  );
});
