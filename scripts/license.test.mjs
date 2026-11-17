// The repository's rule: it says it is open source in every place a person
// or a tool looks.
//
// The code is MIT. The name stays Brad Morrical's. The banner at the top of
// the README is the picture the name asks for: something risky, with rusty
// nails on the ground below.
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const REPOSITORY = 'https://github.com/Bmorrical/rusty-business';
const MARK = 'Rusty Business™ is a trademark of Brad Morrical.';
const BANNER = 'docs/rusty-business.svg';

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** One table of a Cargo.toml, as its keys and values, read from the text. */
export function tomlTable(text, table) {
  const lines = text.split('\n');
  const start = lines.indexOf(`[${table}]`);
  if (start === -1) return {};
  const pairs = {};
  for (const line of lines.slice(start + 1)) {
    if (line.startsWith('[')) break;
    const pair = /^([\w.-]+)\s*=\s*("[^"]*"|[^#]*?)\s*(?:#.*)?$/.exec(line);
    if (pair) pairs[pair[1]] = pair[2].replace(/^"(.*)"$/, '$1');
  }
  return pairs;
}

/** The pictures a Markdown page shows, in order: the words for each and its address. */
export function picturesOf(markdown) {
  return [...markdown.matchAll(/!\[([^\]]*)\]\(([^)\s]+)\)/g)].map((match) => ({
    alt: match[1],
    src: match[2],
  }));
}

test("the license is MIT, in Brad Morrical's name", () => {
  const text = read('LICENSE');
  assert.match(text, /^MIT License\n/);
  assert.match(text, /^Copyright \(c\) \d{4} Brad Morrical$/m);
  assert.match(text, /Permission is hereby granted, free of charge, to any person/);
  assert.match(text, /THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND/);
});

test('the trademark page names the mark and its owner, and points to the license', () => {
  const text = read('TRADEMARKS.md');
  assert.ok(text.includes(MARK), `TRADEMARKS.md does not say: ${MARK}`);
  assert.match(text, /\]\(LICENSE\)/);
});

test('the README names the license and the mark, and links both pages', () => {
  const text = read('README.md');
  assert.match(text, /^## License$/m);
  assert.match(text, /\[MIT License\]\(LICENSE\)/);
  assert.match(text, /\]\(TRADEMARKS\.md\)/);
  assert.ok(text.includes(MARK), `README.md does not say: ${MARK}`);
});

test('every package says MIT and points to this repository', () => {
  const workspace = read('Cargo.toml');
  const shared = tomlTable(workspace, 'workspace.package');
  assert.equal(shared.license, 'MIT');
  assert.equal(shared.repository, REPOSITORY);

  const members = [...(tomlTable(workspace, 'workspace').members ?? '').matchAll(/"([^"]+)"/g)].map(
    (match) => match[1],
  );
  assert.ok(members.length >= 3, 'the list of workspace members looks too short to be right');
  for (const member of members) {
    const own = tomlTable(read(`${member}/Cargo.toml`), 'package');
    for (const key of ['license', 'repository']) {
      assert.equal(own[`${key}.workspace`], 'true', `${member} does not take its ${key} from the workspace`);
    }
  }

  const desktop = tomlTable(read('apps/desktop/src-tauri/Cargo.toml'), 'package');
  assert.equal(desktop.license, 'MIT');
  assert.equal(desktop.repository, REPOSITORY);

  const page = JSON.parse(read('apps/desktop/package.json'));
  assert.equal(page.license, 'MIT');
  assert.equal(page.repository?.url, `git+${REPOSITORY}.git`);

  const lock = JSON.parse(read('apps/desktop/package-lock.json'));
  assert.equal(lock.packages[''].license, 'MIT', 'package-lock.json is behind package.json');
});

test('the banner is the first picture in the README, with words for it', () => {
  const [first] = picturesOf(read('README.md'));
  assert.ok(first, 'README.md shows no picture');
  assert.equal(first.src, BANNER);
  assert.match(first.alt, /wobbling/);
  assert.match(first.alt, /ladder/);
  assert.match(first.alt, /two rusty nails/);
});

test('the banner describes itself and pulls nothing in from outside', () => {
  const svg = read(BANNER);
  assert.match(svg, /<title[^>]*>Rusty Business<\/title>/);

  const described = /<desc[^>]*>([^<]+)<\/desc>/.exec(svg);
  assert.ok(described, 'the banner has no description');
  assert.match(described[1], /wobbling/);
  assert.match(described[1], /ladder/);
  assert.match(described[1], /two rusty nails/);

  const outside = [/<script/i, /<style/i, /<image/i, /<use/i, /<foreignObject/i, /\bhref\s*=/i, /url\(/i, /@import/i, /\son\w+\s*=/i];
  for (const pattern of outside) {
    assert.doesNotMatch(svg, pattern, `the banner holds ${pattern}`);
  }
});

test('a table and a picture are read from the text, and comments are ignored', () => {
  const text = [
    '# license = "nope"',
    '[package]',
    'name = "x" # the name',
    'license.workspace = true',
    '# license = "GPL"',
    '',
    '[dependencies]',
    'license = "other"',
    '',
  ].join('\n');
  assert.deepEqual(tomlTable(text, 'package'), { name: 'x', 'license.workspace': 'true' });
  assert.deepEqual(tomlTable(text, 'missing'), {});

  assert.deepEqual(picturesOf('a ![one thing](a/b.svg) b [a link](c) ![](d.png)'), [
    { alt: 'one thing', src: 'a/b.svg' },
    { alt: '', src: 'd.png' },
  ]);
});
