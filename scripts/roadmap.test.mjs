// The roadmap's rules, checked on every test run.
//
// The roadmap says what is planned and in what order. It promises no dates,
// and a pin keeps its number. Pin 1 is signed installers, from the decision
// that started the page: "unsigned now, start a roadmap, this is pin #1".
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const SECTIONS = ['Pinned', 'Next', 'Later', 'Done'];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** A Markdown page's `## ` sections, in order: each heading and the text under it. */
export function sectionsOf(markdown) {
  const sections = [];
  for (const line of markdown.split('\n')) {
    const heading = /^## (.+)$/.exec(line);
    if (heading) sections.push({ heading: heading[1].trim(), body: '' });
    else if (sections.length) sections.at(-1).body += `${line}\n`;
  }
  return sections;
}

/** The list items in a section: each one's marker, its bold title, and its whole text. */
export function itemsOf(body) {
  const items = [];
  for (const line of body.split('\n')) {
    const start = /^(\d+\.|-) +(.*)$/.exec(line);
    if (start) items.push({ marker: start[1], text: start[2] });
    else if (items.length && /^ {2,}\S/.test(line)) items.at(-1).text += ` ${line.trim()}`;
  }
  return items.map((item) => ({
    ...item,
    title: /^\*\*([^*]+)\*\*/.exec(item.text)?.[1] ?? null,
  }));
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

const roadmap = read('ROADMAP.md');
const sections = sectionsOf(roadmap);

function section(heading) {
  const found = sections.find((each) => each.heading === heading);
  assert.ok(found, `ROADMAP.md has no "${heading}" section`);
  return found;
}

test('the roadmap has its four sections, in order', () => {
  assert.match(roadmap, /^# Roadmap\n/);
  assert.deepEqual(
    sections.map((each) => each.heading),
    SECTIONS,
  );
});

test('every item has a bold title, and no title is used twice', () => {
  const titles = [];
  for (const { heading, body } of sections) {
    for (const item of itemsOf(body)) {
      assert.ok(item.title, `${heading}: an item has no bold title: ${item.text}`);
      assert.match(item.title, /\.$/, `${heading}: the title "${item.title}" does not end in a full stop`);
      titles.push(item.title);
    }
  }
  assert.deepEqual(
    titles.filter((title, index) => titles.indexOf(title) !== index),
    [],
    'a title is in the roadmap twice',
  );
});

test('pins are numbered from 1 with no gaps, and only pins are numbered', () => {
  const pins = itemsOf(section('Pinned').body);
  assert.ok(pins.length > 0, 'there is no pin');
  assert.deepEqual(
    pins.map((pin) => pin.marker),
    pins.map((_, index) => `${index + 1}.`),
  );

  for (const { heading, body } of sections) {
    if (heading === 'Pinned') continue;
    for (const item of itemsOf(body)) {
      assert.equal(item.marker, '-', `${heading}: "${item.title}" is numbered, and only pins are`);
    }
  }
});

test('pin 1 is signed installers', () => {
  const [first] = itemsOf(section('Pinned').body);
  assert.equal(first.marker, '1.');
  assert.equal(first.title, 'Signed installers.');
  assert.match(first.text, /unsigned/);
  assert.match(first.text, /Apple Developer ID/);
  assert.match(first.text, /Windows/);
});

// Brad, 2026-11-19: "keep shipping self-signed as an option until all the
// peices are ready, when ready we can light it up".
test('pin 1 says signing waits behind a switch, and downloads ship unsigned until it is on', () => {
  const [first] = itemsOf(section('Pinned').body);
  assert.match(first.text, /behind a switch/, 'pin 1 does not say signing sits behind a switch');
  assert.match(first.text, /off to start/, 'pin 1 does not say the switch starts off');
  assert.match(first.text, /Until its switch is on, a download ships as it does today, unsigned\./);
});

// Brad, 2026-11-19: "add an engine to linux and windows distro's", with
// llama.cpp's Vulkan build, and models "Maker first, then ggml-org". Then,
// with the Mac's signing kept in 0.6.0: "everything else is roadmap". So the
// app starting llama.cpp is done, the engine inside the downloads is next,
// and Windows's signing switch follows the Mac's.
test('the engine for Windows is next, inside the installer, and is llama.cpp', () => {
  const engine = itemsOf(section('Next').body).find((item) => item.title === 'An engine for Windows.');
  assert.ok(engine, 'the roadmap does not have the engine for Windows next');
  assert.match(engine.text, /llama\.cpp's `llama-server` inside the download/);
  assert.match(engine.text, /the Vulkan build/);
  assert.match(engine.text, /checked by its SHA-256/);
  assert.match(engine.text, /the Vulkan loader/);
  const later = itemsOf(section('Later').body).map((item) => item.title);
  assert.ok(!later.includes('An engine for Windows.'), 'the engine is still in Later');
});

// Brad, 2026-11-25: "don't build it yet, in a x4 gpu rig, or even 1 slice of
// that. I can assign a model to a worker node, or logically split it's
// resource parts. this requires containerization, and heavy orchistration.
// mark it next to do". Then: "this is extension of LAN capabilities", "so if
// I had 4 hardware slices I could run 1 model on each one", and "asynch". So
// it is first in Next, on your own network behind the LAN gate, one model to
// each GPU or slice in its own container, and each answers without waiting.
test('many models at once, on your own network, is first in Next, one model to each slice', () => {
  const [first] = itemsOf(section('Next').body);
  assert.equal(first.title, 'Many models at once, on your own network.', 'many models at once is not first in Next');
  assert.match(first.text, /your private\s+network/);
  assert.match(first.text, /hardware slices/);
  assert.match(first.text, /one\s+model on each GPU or slice/);
  assert.match(first.text, /a container of its own/);
  assert.match(first.text, /only with\s+the LAN gate on/);
  assert.match(first.text, /at the\s+same time/);
  assert.match(first.text, /none waits for another/);
  assert.match(first.text, /the app only asks the workers on its list/);
  const done = itemsOf(section('Done').body).map((item) => item.title);
  assert.ok(!done.includes('Many models at once, on your own network.'), 'it is called done before it is built');
});

test('the engine for Linux is done, inside the packages, and is llama.cpp', () => {
  const linux = itemsOf(section('Done').body).find((item) => item.title === 'An engine for Linux.');
  assert.ok(linux, 'the roadmap does not have the Linux engine done');
  assert.match(linux.text, /llama\.cpp's `llama-server`/);
  assert.match(linux.text, /the Vulkan build/);
  assert.match(linux.text, /checked by its SHA-256/);
  assert.match(linux.text, /the Vulkan loader/);
  const next = itemsOf(section('Next').body).map((item) => item.title);
  assert.ok(!next.includes('An engine for Linux.'), 'the Linux engine is still in Next');
});

test('everything else is roadmap: the app starting llama.cpp is done, and Windows signing follows the Mac', () => {
  const started = itemsOf(section('Done').body).find((item) => item.title === 'The app starts llama.cpp on Windows and Linux.');
  assert.ok(started, 'the roadmap does not have the app starting llama.cpp done');
  assert.match(started.text, /when it is installed/);
  assert.match(started.text, /GGUF files, from the maker's own copy first, then ggml-org's/);
  const [first] = itemsOf(section('Pinned').body);
  assert.match(first.text, /the Mac's is being built first, and Windows's follows\./, 'pin 1 does not say the Mac signs first');
});

test('every mention of a pin points at a pin that is there', () => {
  const pins = itemsOf(section('Pinned').body).length;
  for (const file of ['ROADMAP.md', 'README.md']) {
    for (const [mention, number] of read(file).matchAll(/\bpin #?(\d+)\b/gi)) {
      assert.ok(Number(number) >= 1 && Number(number) <= pins, `${file}: "${mention}", and there is no such pin`);
    }
  }
});

test('the roadmap promises no dates', () => {
  assert.deepEqual(datesIn(roadmap), []);
});

test('the README has a Roadmap section that links the roadmap', () => {
  const found = sectionsOf(read('README.md')).find((each) => each.heading === 'Roadmap');
  assert.ok(found, 'README.md has no "## Roadmap" section');
  assert.match(found.body, /\]\(ROADMAP\.md\)/);
  assert.deepEqual(datesIn(found.body), []);
});

test('sections, items, and dates are read from the text', () => {
  const page = [
    '# Page',
    'before',
    '## One',
    '1. **First.** a',
    '   b',
    '2. no title',
    '',
    '## Two',
    'words',
    '- **Third.** c',
    '',
  ].join('\n');
  const [one, two] = sectionsOf(page);
  assert.deepEqual([one.heading, two.heading], ['One', 'Two']);
  assert.deepEqual(itemsOf(one.body), [
    { marker: '1.', text: '**First.** a b', title: 'First.' },
    { marker: '2.', text: 'no title', title: null },
  ]);
  assert.deepEqual(itemsOf(two.body), [{ marker: '-', text: '**Third.** c', title: 'Third.' }]);

  assert.deepEqual(datesIn('A thing ships when its tests are green. It may take a while.'), []);
  assert.deepEqual(
    datesIn('In Q3 2027, by the end of March, on May 5 or 12/1, maybe next month.'),
    ['2027', 'Q3', 'March', 'May 5', '12/1', 'next month', 'by the end of'],
  );
});
