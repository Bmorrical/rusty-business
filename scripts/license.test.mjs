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

/** What sits inside the group with this id in a drawing, groups within it included, or null. */
export function groupOf(svg, id) {
  const open = new RegExp(`<g\\b[^>]*\\sid="${id}"[^>]*>`).exec(svg);
  if (!open) return null;
  const start = open.index + open[0].length;
  const tags = /<g\b[^>]*>|<\/g>/g;
  tags.lastIndex = start;
  let depth = 1;
  for (let tag = tags.exec(svg); tag; tag = tags.exec(svg)) {
    if (tag[0].endsWith('/>')) continue;
    depth += tag[0] === '</g>' ? -1 : 1;
    if (depth === 0) return svg.slice(start, tag.index);
  }
  return null;
}

/**
 * A nail in the banner, measured from its paths: its head (the short stroke
 * that stands up), its tip (the corner of its closed point farthest from the
 * head), and how far it reaches left, right, and up. Null when the group, the
 * head, or the point is missing.
 */
export function nailOf(svg, id) {
  const group = groupOf(svg, id);
  if (group === null) return null;
  const paths = [...group.matchAll(/<path\b[^>]*\sd="([^"]+)"/g)].map((match) => {
    const numbers = (match[1].match(/-?\d+(?:\.\d+)?/g) ?? []).map(Number);
    const corners = [];
    for (let at = 0; at + 1 < numbers.length; at += 2) corners.push({ x: numbers[at], y: numbers[at + 1] });
    return { closed: /z\s*$/i.test(match[1]), corners };
  });
  const stands = ({ closed, corners: [from, to, more] }) =>
    !closed && to && !more && Math.abs(from.x - to.x) <= 3 && Math.abs(from.y - to.y) >= 10;
  const stroke = paths.find(stands);
  const points = paths.filter((shape) => shape.closed);
  if (!stroke || points.length !== 1) return null;

  const [from, to] = stroke.corners;
  const head = { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2, top: Math.min(from.y, to.y) };
  const far = (corner) => Math.hypot(corner.x - head.x, corner.y - head.y);
  const tip = points[0].corners.reduce((best, corner) => (far(corner) > far(best) ? corner : best));
  const all = paths.flatMap((shape) => shape.corners);
  return {
    head,
    tip,
    left: Math.min(...all.map((corner) => corner.x)),
    right: Math.max(...all.map((corner) => corner.x)),
    top: Math.min(...all.map((corner) => corner.y)),
  };
}

/** How far the banner's ladder reaches left and right, from the ends of its lines, or null. */
function ladderOf(svg) {
  const group = groupOf(svg, 'rb-ladder');
  if (group === null) return null;
  const ends = [...group.matchAll(/\sx[12]="(-?\d+(?:\.\d+)?)"/g)].map((match) => Number(match[1]));
  if (ends.length === 0) return null;
  return { left: Math.min(...ends), right: Math.max(...ends) };
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

// Brad, 2026-11-16, on the banner: "flip the right nail horizontally so the
// upwards pin of the nail is left side instead of right.  it doesn't look
// like he will hit that nail tip as it's drawn today"
//
// So each nail lies with its point toward the ladder, and the right nail's
// point stands up at its end nearest the ladder. The ladder is drawn upright
// and then turned a few degrees on one foot, which moves its feet a point or
// two. ROOM is more than that.
const ROOM = 20;

test('the banner has two nails, one on each side of the ladder, each with its point toward it', () => {
  const svg = read(BANNER);
  const nails = [...svg.matchAll(/\sid="(rb-nail-[^"]*)"/g)].map((match) => match[1]);
  assert.deepEqual(nails, ['rb-nail-left', 'rb-nail-right']);

  const ladder = ladderOf(svg);
  const left = nailOf(svg, 'rb-nail-left');
  const right = nailOf(svg, 'rb-nail-right');
  assert.ok(ladder, 'the banner has no ladder to measure');
  assert.ok(left, 'the left nail has no head, or not exactly one point');
  assert.ok(right, 'the right nail has no head, or not exactly one point');

  assert.ok(left.right + ROOM <= ladder.left, 'the left nail is not to the left of the ladder');
  assert.ok(right.left - ROOM >= ladder.right, 'the right nail is not to the right of the ladder');

  assert.equal(left.tip.x, left.right, "the left nail's point is not its end nearest the ladder");
  assert.ok(left.head.x < left.tip.x, "the left nail's head is not at its far end");
  assert.ok(right.tip.x < right.head.x, "the right nail's point is on its right side, away from the ladder");
});

test("the right nail's point stands up, at its left end", () => {
  const right = nailOf(read(BANNER), 'rb-nail-right');
  assert.ok(right, 'the right nail has no head, or not exactly one point');

  const length = right.right - right.left;
  assert.ok(right.tip.x - right.left <= length / 10, "the right nail's point is not at its left end");
  assert.ok(right.right - right.head.x <= length / 10, "the right nail's head is not at its right end");

  assert.equal(right.tip.y, right.top, "the right nail's point is not its highest part");
  assert.ok(right.head.top - right.tip.y >= 15, "the right nail's point does not stand above its head");
});

test('a group and a nail are read from a drawing', () => {
  const drawing = [
    '<svg>',
    '<g id="a" fill="none"><g><path d="M 0 0 L 1 1"/></g><g/><path d="M 2 2 L 3 3"/></g>',
    '<g id="nail"><path d="M 10 50 L 60 50"/><path d="M 10 40 L 11 60"/><path d="M 60 44 L 80 50 L 60 56 Z"/></g>',
    '<g id="headless"><path d="M 10 50 L 60 50"/><path d="M 60 44 L 80 50 L 60 56 Z"/></g>',
    '<g id="pointless"><path d="M 10 50 L 60 50"/><path d="M 10 40 L 11 60"/></g>',
    '<g id="open"><path d="M 10 40 L 11 60"/>',
    '</svg>',
  ].join('\n');

  assert.equal(groupOf(drawing, 'a'), '<g><path d="M 0 0 L 1 1"/></g><g/><path d="M 2 2 L 3 3"/>');
  assert.equal(groupOf(drawing, 'missing'), null);
  assert.equal(groupOf(drawing, 'open'), null);

  assert.deepEqual(nailOf(drawing, 'nail'), {
    head: { x: 10.5, y: 50, top: 40 },
    tip: { x: 80, y: 50 },
    left: 10,
    right: 80,
    top: 40,
  });
  assert.equal(nailOf(drawing, 'headless'), null);
  assert.equal(nailOf(drawing, 'pointless'), null);
  assert.equal(nailOf(drawing, 'missing'), null);
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
