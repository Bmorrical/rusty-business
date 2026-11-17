// The README's comparison with Ollama, checked on every test run.
//
// The section answers the ask that started it: "the readme should call out a
// rusty.business vs ollama.  like why chose ours?". A page like that is worth
// reading only while it is fair and true. So it has to say where Ollama is
// ahead, each reason it gives for Rusty Business has to be kept by the code,
// and the README has to list what the app cannot do yet.
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const ENGINE = 'apps/desktop/src-tauri/src/engine.rs';
const SESSIONS = 'apps/desktop/src-tauri/src/sessions.rs';
const STARTER = 'apps/desktop/src-tauri/src/starter.rs';
const TAURI = 'apps/desktop/src-tauri/tauri.conf.json';
const PAGE = 'apps/desktop/src';
const OLLAMA = 'https://ollama.com';
const FEWEST_REASONS = 3;
const COUNTS = ['no', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine', 'ten'];

/** What a page could use to call out, none of which the chat window may hold. */
const WAYS_OUT = ['fetch(', 'XMLHttpRequest', 'WebSocket', 'EventSource', 'sendBeacon', '<iframe', 'http://', 'https://'];

/** The Rust tests that keep what the README says. A promise with no test is only a hope. */
const KEPT_BY = [
  { file: ENGINE, test: 'only_listed_models_are_offered' },
  { file: ENGINE, test: 'a_model_off_the_list_is_refused' },
  { file: ENGINE, test: 'the_engine_address_stays_on_this_computer' },
  { file: STARTER, test: 'an_engine_is_never_started_where_another_computer_could_reach_it' },
  { file: STARTER, test: 'the_engine_the_app_starts_stays_offline' },
  { file: SESSIONS, test: 'trash_moves_the_file_and_deletes_nothing' },
];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** A Markdown text's sections at one heading level, in order: each heading and the text under it. */
export function sectionsOf(markdown, marks = '##') {
  const sections = [];
  for (const line of markdown.split('\n')) {
    const heading = line.startsWith(`${marks} `) ? line.slice(marks.length + 1).trim() : null;
    if (heading) sections.push({ heading, body: '' });
    else if (sections.length) sections.at(-1).body += `${line}\n`;
  }
  return sections;
}

/** The list items in a text: each one's bold title, and its whole text on one line. */
export function itemsOf(body) {
  const items = [];
  for (const line of body.split('\n')) {
    const start = /^- +(.*)$/.exec(line);
    if (start) items.push(start[1]);
    else if (items.length && /^ {2,}\S/.test(line)) items[items.length - 1] += ` ${line.trim()}`;
  }
  return items.map((text) => ({ text, title: /^\*\*([^*]+)\*\*/.exec(text)?.[1] ?? null }));
}

/** Every address a text points to: its Markdown links, and any web address written out. */
export function addressesIn(text) {
  const linked = [...text.matchAll(/\]\(([^)\s]+)\)/g)].map((match) => match[1]);
  const written = text.match(/\bhttps?:\/\/[^\s)>\]]+/g) ?? [];
  return [...new Set([...linked, ...written])];
}

/** True for an address inside this repository: a file here, or a place on the same page. */
export function staysHere(address) {
  return !/^[a-z][a-z0-9+.-]*:/i.test(address) && !address.startsWith('//');
}

/** True for Ollama's own site, and for nothing that only looks like it. */
export function isOllama(address) {
  return address === OLLAMA || address.startsWith(`${OLLAMA}/`);
}

/** Anything in the text that reads as a date. */
export function datesIn(text) {
  const patterns = [
    /\b(?:19|20)\d{2}\b/g,
    /\bQ[1-4]\b/g,
    /\b(?:January|February|March|April|June|July|August|September|October|November|December)\b/g,
    /\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sept?|Oct|Nov|Dec)\.? \d/g,
    /\b\d{1,2}\/\d{1,2}\b/g,
    /\b(?:next|this) (?:week|month|quarter|year|spring|summer|fall|autumn|winter)\b/gi,
    /\bby (?:the end of|early|mid|late)\b/gi,
  ];
  return patterns.flatMap((pattern) => text.match(pattern) ?? []);
}

/** The makers on the app's list of models, read from the Rust that holds it. */
export function makersIn(rust) {
  const list = /const CATALOG: &\[CatalogEntry\] = &\[([\s\S]*?)\n\];/.exec(rust)?.[1] ?? '';
  return [...list.matchAll(/maker: "([^"]+)"/g)].map((match) => match[1]);
}

/** Where a content security policy lets a page connect: the sources after `connect-src`. */
export function connectSources(policy) {
  const rule = policy
    .split(';')
    .map((each) => each.trim().split(/\s+/))
    .find(([name]) => name === 'connect-src');
  return rule ? rule.slice(1) : null;
}

/** True for a source that cannot leave the computer: the app itself, or an address on this computer. */
export function onThisComputer(source) {
  if (source === "'self'" || source === 'ipc:') return true;
  const address = /^(?:http|ws):\/\/([^/:]+)(?::\d+)?$/.exec(source);
  return Boolean(address) && ['localhost', '127.0.0.1', 'ipc.localhost'].includes(address[1]);
}

/** The roadmap entries a text points to, written as "(On the roadmap: The title.)". */
export function roadmapPointers(text) {
  return [...text.matchAll(/\(On the roadmap: ([^)]+)\)/g)].map((match) => match[1].trim());
}

const readme = read('README.md');
const sections = sectionsOf(readme);
const comparisons = sections.filter(
  (each) => each.heading.includes('Rusty Business') && each.heading.includes('Ollama'),
);
const comparison = comparisons[0] ?? { heading: '', body: '' };
const parts = sectionsOf(comparison.body, '###');
const ours = parts.filter((each) => each.heading.includes('Rusty Business') && !each.heading.includes('Ollama'));
const theirs = parts.filter((each) => each.heading.includes('Ollama') && !each.heading.includes('Rusty Business'));
const same = parts.filter((each) => /\bsame\b/i.test(each.heading));
const limits = sections.find((each) => each.heading === 'Limits');

test('the README compares Rusty Business with Ollama, right after the screenshots', () => {
  assert.equal(comparisons.length, 1, 'README.md needs one "## " section that names both Rusty Business and Ollama');
  const at = sections.indexOf(comparison);
  assert.equal(
    sections[at - 1]?.heading,
    'Screenshots',
    'the comparison comes right after the screenshots, before the README turns to how things are run',
  );
});

test('the comparison gives reasons to choose each one, and says what is the same', () => {
  assert.equal(ours.length, 1, 'the comparison needs one "### " part for choosing Rusty Business');
  assert.equal(theirs.length, 1, 'the comparison needs one "### " part for choosing Ollama');
  assert.equal(same.length, 1, 'the comparison needs one "### " part for what is the same');

  for (const part of [ours[0], theirs[0]]) {
    const reasons = itemsOf(part.body);
    assert.ok(
      reasons.length >= FEWEST_REASONS,
      `"${part.heading}" gives ${reasons.length} reasons; a fair comparison gives at least ${FEWEST_REASONS} each way`,
    );
    for (const reason of reasons) {
      assert.match(reason.title ?? '', /\.$/, `a reason opens with a bold title that ends in a full stop: "${reason.text}"`);
    }
  }
  assert.ok(itemsOf(same[0].body).length >= 2, 'the part on what is the same lists at least two things');
});

test('each reason to choose Ollama says where Rusty Business stands', () => {
  for (const reason of itemsOf(theirs[0]?.body ?? '')) {
    assert.match(reason.text, /\bOllama\b/, `this reason does not name Ollama: "${reason.text}"`);
    assert.match(
      reason.text,
      /\bRusty Business\b/,
      `this reason does not say where Rusty Business stands: "${reason.text}"`,
    );
  }
});

test('the short list in the comparison is the list in the code', () => {
  const makers = makersIn(read(ENGINE));
  assert.ok(makers.length > 0 && makers.length < COUNTS.length, `${ENGINE} no longer holds a list this test can read`);

  const reasons = itemsOf(ours[0]?.body ?? '');
  const reason = reasons.find((each) => makers.some((maker) => each.text.includes(maker)));
  assert.ok(reason, 'no reason to choose Rusty Business names the makers of its models');
  assert.match(
    reason.text,
    new RegExp(`\\b${COUNTS[makers.length]} models\\b`),
    `the app's list holds ${COUNTS[makers.length]} models, and the comparison has to say so`,
  );
  for (const maker of makers) {
    assert.ok(reason.text.includes(maker), `the comparison does not name ${maker}, a maker on the app's list`);
  }
});

test('what the comparison says of Rusty Business is kept by a test in the Rust', () => {
  for (const { file, test: name } of KEPT_BY) {
    assert.match(read(file), new RegExp(`#\\[test\\]\\s+fn ${name}\\(`), `${file} has lost the test ${name}`);
  }
});

test('the chat window cannot reach past this computer', () => {
  const security = JSON.parse(read(TAURI)).app.security;
  for (const name of ['csp', 'devCsp']) {
    const sources = connectSources(security[name] ?? '');
    assert.ok(sources?.length, `${TAURI} has no connect-src rule in ${name}`);
    for (const source of sources) {
      assert.ok(onThisComputer(source), `${name} in ${TAURI} lets the page connect to ${source}`);
    }
  }
  assert.deepEqual(
    connectSources(security.csp).filter((source) => source !== "'self'" && !source.includes('ipc')),
    [],
    'the built app talks only to itself',
  );

  const engine = read(ENGINE);
  assert.ok(engine.includes('The engine address must be on this computer'), `${ENGINE} no longer refuses an outside address`);
  assert.ok(engine.includes('.no_proxy()'), `${ENGINE} may now send its calls through a proxy`);
  assert.ok(engine.includes('redirect::Policy::none()'), `${ENGINE} may now follow a redirect somewhere else`);

  const files = readdirSync(path.join(REPO, PAGE)).filter((file) => /\.(?:js|vue)$/.test(file) && !file.includes('.test.'));
  assert.ok(files.length > 0, `${PAGE} holds no page source`);
  for (const file of files) {
    const source = read(path.join(PAGE, file));
    for (const way of WAYS_OUT) {
      assert.ok(!source.includes(way), `${PAGE}/${file} holds "${way}", a way for the page to call out`);
    }
  }
});

test("the status line gives the app's version and points to the limits", () => {
  const opening = readme.split('\n## ')[0].replace(/\n(?!\n)/g, ' ');
  const status = opening.split('\n').find((line) => line.trim().startsWith('**Status:**'));
  assert.ok(status, 'README.md needs a "**Status:**" line above its first section');

  const { version } = JSON.parse(read('apps/desktop/package.json'));
  assert.ok(
    new RegExp(`\\bversion ${version.replaceAll('.', '\\.')}(?![\\d.]*\\d)`).test(status),
    `the status line has to say "version ${version}", from apps/desktop/package.json`,
  );
  assert.ok(status.includes('(#limits)'), 'the status line links the Limits section');
  assert.ok(limits, 'README.md has no "## Limits" section');
});

test("the comparison links only to Ollama's own site and to this repository", () => {
  const addresses = addressesIn(comparison.body);
  assert.ok(addresses.some(isOllama), `the comparison links ${OLLAMA}`);
  for (const address of addresses) {
    assert.ok(isOllama(address) || staysHere(address), `the comparison points to ${address}`);
  }
});

test('the comparison says whose name Ollama is, and that the two are not connected', () => {
  const text = comparison.body.replace(/\s+/g, ' ');
  assert.match(text, /Ollama is a trademark of its owner\./);
  assert.match(text, /Rusty Business is not affiliated with Ollama\b/);
});

test('the comparison and the limits promise no dates', () => {
  assert.deepEqual(datesIn(comparison.body), []);
  assert.deepEqual(datesIn(limits?.body ?? ''), []);
});

test('the README lists what the app cannot do yet, and each limit on the roadmap is still ahead', () => {
  assert.ok(limits, 'README.md has no "## Limits" section');
  assert.match(limits.body, /\]\(ROADMAP\.md\)/, 'the Limits section links the roadmap');

  const items = itemsOf(limits.body);
  assert.ok(items.length >= FEWEST_REASONS, `the Limits section lists ${items.length} limits; it needs at least ${FEWEST_REASONS}`);
  for (const item of items) {
    assert.match(item.title ?? '', /\.$/, `a limit opens with a bold title that ends in a full stop: "${item.text}"`);
  }

  const roadmap = sectionsOf(read('ROADMAP.md'));
  const titlesIn = (headings) =>
    roadmap
      .filter((each) => headings.includes(each.heading))
      .flatMap((each) => [...each.body.matchAll(/^(?:\d+\.|-) +\*\*([^*]+)\*\*/gm)].map((match) => match[1]));
  const ahead = titlesIn(['Pinned', 'Next', 'Later']);
  const done = titlesIn(['Done']);
  assert.ok(ahead.length > 0, 'ROADMAP.md has nothing ahead for a limit to point to');

  const pointers = roadmapPointers(limits.body.replace(/\s+/g, ' '));
  assert.ok(pointers.length > 0, 'no limit says where it is on the roadmap');
  for (const pointer of pointers) {
    assert.ok(!done.includes(pointer), `"${pointer}" is done on the roadmap, so it is no longer a limit`);
    assert.ok(ahead.includes(pointer), `a limit points to "${pointer}", which the roadmap does not have ahead`);
  }
});

test('sections, reasons, addresses, and policies are read from the text', () => {
  const page = [
    '# Page',
    '**Status:** early',
    '## One or Other?',
    'See [Other](https://ollama.com) and [here](docs/a.md), or https://example.com/x.',
    '### Why One',
    '- **First.** a',
    '  b',
    '- no title',
    '### Why Other',
    '- **Second.** c (On the roadmap: A thing.)',
    '## Two',
    'words',
    '',
  ].join('\n');
  const [one, two] = sectionsOf(page);
  assert.deepEqual([one.heading, two.heading], ['One or Other?', 'Two']);
  const [why, other] = sectionsOf(one.body, '###');
  assert.deepEqual([why.heading, other.heading], ['Why One', 'Why Other']);
  assert.deepEqual(itemsOf(why.body), [
    { text: '**First.** a b', title: 'First.' },
    { text: 'no title', title: null },
  ]);
  assert.deepEqual(roadmapPointers(other.body), ['A thing.']);

  assert.deepEqual(addressesIn(one.body), ['https://ollama.com', 'docs/a.md', 'https://example.com/x.']);
  assert.deepEqual(
    ['docs/a.md', '#limits', '../x', 'https://ollama.com', '//ollama.com', 'mailto:a@example.com'].map(staysHere),
    [true, true, true, false, false, false],
  );
  assert.deepEqual(
    ['https://ollama.com', 'https://ollama.com/download', 'https://ollama.com.example', 'http://ollama.com', 'https://example.com/ollama.com'].map(isOllama),
    [true, true, false, false, false],
  );

  assert.deepEqual(datesIn('It is early, and it is built from source. Version 0.1.0.'), []);
  assert.deepEqual(datesIn('Since March 2026, or by the end of Q3.'), ['2026', 'Q3', 'March', 'by the end of']);

  assert.deepEqual(
    makersIn('const CATALOG: &[CatalogEntry] = &[\n    CatalogEntry { maker: "A", context: 1 },\n    CatalogEntry { maker: "B", context: 2 },\n];\nconst OTHER: &str = "maker: \\"C\\"";'),
    ['A', 'B'],
  );
  assert.deepEqual(makersIn('nothing here'), []);

  const policy = "default-src 'self'; connect-src 'self' ipc: http://ipc.localhost ws://localhost:1420";
  assert.deepEqual(connectSources(policy), ["'self'", 'ipc:', 'http://ipc.localhost', 'ws://localhost:1420']);
  assert.equal(connectSources("default-src 'self'"), null);
  assert.deepEqual(
    ["'self'", 'ipc:', 'http://ipc.localhost', 'ws://127.0.0.1:1420', 'https://example.com', '*', 'https:', 'http://localhost.example.com', 'ws://example.com:1420'].map(onThisComputer),
    [true, true, true, true, false, false, false, false, false],
  );
});
