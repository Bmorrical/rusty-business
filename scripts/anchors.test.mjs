// The README's headings, each with an address that a link can point to.
//
// Brad, 2026-11-17: "ensure the readme has clickable anchors so I can link to
// that section in the teams message that spawned it". GitHub gives every
// heading an address, made from its words, so a link such as
// https://github.com/Bmorrical/rusty-business#run-the-chat-window-linux opens
// the page at that section. These checks keep the addresses the README links
// to, and the one a message outside the repository links to, from breaking
// when a heading is reworded.
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const read = (file) => readFileSync(new URL(file, `file://${REPO}`), 'utf8');

// The sections a link from outside the repository points to, by address.
// Rewording one of these headings breaks those links, so it fails here first.
const LINKED_FROM_OUTSIDE = {
  'run-the-chat-window-macos': 'Run the chat window (macOS)',
  'run-the-chat-window-windows': 'Run the chat window (Windows)',
  'run-the-chat-window-linux': 'Run the chat window (Linux)',
  limits: 'Limits',
};

/** The address GitHub gives a heading: lower case, spaces as hyphens, no marks such as ™, (, or ?. */
export function addressOf(heading) {
  return heading
    .replace(/`/g, '')
    .toLowerCase()
    .replace(/[^\p{L}\p{M}\p{N}\p{Pc} -]/gu, '')
    .replace(/ /g, '-');
}

/** Every heading in a Markdown text, outside code blocks, each with its address; a repeated one gets -1, -2, and so on, as on GitHub. */
export function headingsOf(markdown) {
  const seen = new Map();
  const headings = [];
  let fenced = false;
  for (const line of markdown.split('\n')) {
    if (/^\s*```/.test(line)) fenced = !fenced;
    const match = fenced ? null : /^(#{1,6}) +(.+?) *#* *$/.exec(line);
    if (!match) continue;
    const base = addressOf(match[2]);
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    headings.push({ level: match[1].length, text: match[2], address: count ? `${base}-${count}` : base });
  }
  return headings;
}

const readme = read('README.md');
const headings = headingsOf(readme);
const addresses = new Set(headings.map((each) => each.address));

test("a heading's address is the one GitHub gives it", () => {
  assert.equal(addressOf('Run the chat window (Linux)'), 'run-the-chat-window-linux');
  assert.equal(addressOf('Rusty Business™ or Ollama?'), 'rusty-business-or-ollama');
  assert.equal(addressOf('What `release.yml` publishes'), 'what-releaseyml-publishes');
  assert.equal(addressOf('Why choose Rusty Business™'), 'why-choose-rusty-business');
  assert.deepEqual(
    headingsOf('## Once\n### Again\n```bash\n# not a heading\n```\n### Again\n### Again\n').map((each) => each.address),
    ['once', 'again', 'again-1', 'again-2'],
  );
});

test('the sections linked from outside the repository keep their addresses', () => {
  for (const [address, text] of Object.entries(LINKED_FROM_OUTSIDE)) {
    const found = headings.find((each) => each.address === address);
    assert.ok(found, `README.md has no heading at #${address}; a link to it from outside would open the top of the page`);
    assert.equal(found.text, text, `#${address} is the heading "${found.text}"`);
    assert.equal(found.level, 2, `"${text}" is a section of its own, a "## " heading`);
  }
});

test('the status line links the Linux section, beside the Mac and Windows ones', () => {
  const opening = readme.split('\n## ')[0].replace(/\n(?!\n)/g, ' ');
  const status = opening.split('\n').find((line) => line.trim().startsWith('**Status:**')) ?? '';
  for (const address of ['run-the-chat-window-macos', 'run-the-chat-window-windows', 'run-the-chat-window-linux']) {
    assert.ok(status.includes(`](#${address})`), `the status line does not link #${address}`);
  }
});

test('every link in the README to a place on the page lands on a heading', () => {
  const links = [...readme.matchAll(/\]\(#([^)]+)\)/g)].map((match) => match[1]);
  assert.ok(links.length > 0, 'the README links no place on its own page');
  for (const address of links) {
    assert.ok(addresses.has(address), `README.md links #${address}, and no heading has that address`);
  }
});
