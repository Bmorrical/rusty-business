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
// I had 4 hardware slices I could run 1 model on each one", and "asynch".
// Then: "ammend so if I had 1 slice or 4. I can also sub-divide the
// resources. with 4 cards, I could run essentially run and split any model
// on any percentage of available, can easily tune the resource availability
// in app, like a slider or meter. I'm thinking like how we partition drives,
// with pie graphs, but in this case it's compute", "I should be able to span
// the resources across cards too, multiplex", "so if model 1 needs 2.5 cards
// of compute, no problem", and "and I can run them rich or lean to get the
// results I want". So it is first in Next, on your own network behind the
// LAN gate: a pie for each card, a slider for each model's share, a share
// that can span cards, run rich or lean, each model in its own container,
// and each answering without waiting.
// Brad, 2026-11-25: "the next thing that gets built in 8.5. It starts a
// rulesheet that I write and ammend", "It's the orchestrator. It's first
// listening to me, it never merges to main or has the capability to. in fact
// it doesn't merge anything", "the story may go to local, lan, or wan", and
// Copilot "does not merge into main only I do that". Then: "roadmap it all,
// descript is more general as this will grow". And: "it sets a drift threshold
// on runners, if they run past that that build is rejected and it tells the
// orchistrator to split into more shards".
test('the orchestrator, run by your rulesheet, is first in Next, rejects a build past its drift threshold, and merges nothing', () => {
  const [first] = itemsOf(section('Next').body);
  assert.equal(first.title, 'The orchestrator, run by your rulesheet.', 'the orchestrator is not first in Next');
  const text = first.text.replace(/\s+/g, ' ');
  assert.match(text, /a rulesheet you write and amend/);
  assert.match(text, /It listens to you first/);
  assert.match(text, /from the Workflow view/);
  assert.match(text, /on this computer, on your private network, or at an outside service/);
  assert.match(text, /Copilot takes over on GitHub/);
  assert.match(text, /until it is green/);
  assert.match(text, /only then does the pull request get its merge labels/);
  assert.match(text, /a label workflow that says who holds the ball/);
  assert.match(text, /hands the ball to the next one/);
  assert.match(text, /Watching those labels and moving them is the orchestrator's job/);
  assert.match(text, /It sets a drift threshold on its runners, the models it hands stories to/);
  assert.match(text, /A runner that runs past it has its build rejected/);
  assert.match(text, /the orchestrator is told to split the story into more shards/);
  assert.match(text, /The orchestrator merges nothing, and only you merge into main/);
  const done = itemsOf(section('Done').body).map((item) => item.title);
  assert.ok(!done.includes('The orchestrator, run by your rulesheet.'), 'it is called done before it is built');
});

test('many models at once, on your own network, is next after the orchestrator, with a pie and a slider for the compute', () => {
  const next = itemsOf(section('Next').body);
  const first = next[1];
  assert.equal(first?.title, 'Many models at once, on your own network.', 'many models at once is not next after the orchestrator');
  const text = first.text.replace(/\s+/g, ' ');
  assert.match(text, /your private network/);
  assert.match(text, /from one hardware slice of a card to four whole cards/);
  assert.match(text, /partition a drive/);
  assert.match(text, /each card as a pie/);
  assert.match(text, /a slider gives each model its share of what is free/);
  assert.match(text, /part of one card, or span several/);
  assert.match(text, /two and a half cards/);
  assert.match(text, /Run a model rich or lean/);
  assert.match(text, /a container of its own/);
  assert.match(text, /only with the LAN gate on/);
  assert.match(text, /at the same time/);
  assert.match(text, /none waits for another/);
  assert.match(text, /The app sets the shares, and the computer that runs the models carries them out/);
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

// Brad, 2026-11-25: "8.5 brings it, who makes it (country of origin)",
// "ensure users can 1 click install these too", "users choice what to
// install", "developers choice what to make available", and "put in the
// country of origin and it's filterable ... user can easily see what is us
// and what is not". Then: "they can also install qwen and deepseek, any
// other missing top models", "they don't ship any models by default from
// scratch as rules as already exists", and "user must click to get the ones
// they want".
test('newer models, and where each maker is from, is third in Next, with a filter by country', () => {
  const item = itemsOf(section('Next').body)[2];
  assert.equal(item?.title, 'Newer models, and where each maker is from.', 'newer models is not third in Next');
  const text = item.text.replace(/\s+/g, ' ');
  assert.match(text, /install in one click/);
  assert.match(text, /Inkling 2 Mini/);
  for (const name of ['Qwen', 'DeepSeek', 'Mistral', 'the other top open models']) assert.ok(text.includes(name), `it lost ${name}`);
  assert.match(text, /The project picks which models are on the list, and you pick which to install/);
  assert.match(text, /No model comes with the app/);
  assert.match(text, /downloaded only when you click to get it/);
  assert.match(text, /Each maker shows its country of origin/);
  assert.match(text, /filter the list by country/);
  assert.match(text, /made in the United States and which are not/);
  const done = itemsOf(section('Done').body).map((each) => each.title);
  assert.ok(!done.includes('Newer models, and where each maker is from.'), 'it is called done before it is built');
});

// Brad, 2026-11-25: "one [Rusty Business™] can talk to another [Rusty Business™]
// on the same LAN, it has it's own connection handshake", "I am driving one, and
// it's telling the other over the LAN wire", "anything bubbled back ... goes to
// rb to rb back to me", and "keep out the 'external connector' it doesn't fit
// the shape of the project". Then: "no fork. LAN only there is an API exposed,
// bring your own courier app, we shape it for my needs to start", and "that has
// a circuit breaker too".
test('one Rusty Business™ to another is fourth in Next, with its own handshake, a LAN-only API for your own courier app, and a circuit breaker, behind the LAN gate', () => {
  const item = itemsOf(section('Next').body)[3];
  assert.equal(item?.title, 'One Rusty Business™ to another, on your own network.', 'one Rusty Business™ to another is not fourth in Next');
  const text = item.text.replace(/\s+/g, ' ');
  assert.match(text, /Two installs of Rusty Business™ on your private network talk to each other/);
  assert.match(text, /a handshake of their own/);
  assert.match(text, /You drive one, and it passes what you ask over the wire to the other/);
  assert.match(text, /bubbles back the same way: to the first install, and then to you/);
  assert.match(text, /nothing but Rusty Business™ at each end/);
  assert.match(text, /opens an API on your private network/);
  assert.match(text, /bring your own courier app to carry work to it and back/);
  assert.match(text, /starts with what the first courier app needs/);
  assert.match(text, /The link and the API have a circuit breaker of their own/);
  assert.match(text, /off until you turn it on, that stops both at once/);
  assert.match(text, /a courier app reaches an install, only with that switch and the LAN gate on/);
  assert.match(text, /nothing passes past your private network/);
  assert.doesNotMatch(text, /connector/i, 'an external connector does not fit the shape of the project');
  assert.doesNotMatch(text, /\bfork/i, 'the courier API stays in this project, with no fork');
  const done = itemsOf(section('Done').body).map((each) => each.title);
  assert.ok(!done.includes('One Rusty Business™ to another, on your own network.'), 'it is called done before it is built');
});
