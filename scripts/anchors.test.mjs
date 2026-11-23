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
  'rusty-business-ollama-or-colibri': 'Rusty Business™, Ollama, or Colibri?',
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

// Brad, 2026-11-17: "make a better clickable index at the top of the page,
// under the graphic", and then: "i had to scroll and hunt for the downloads".
// So the picture is followed at once by a table with every download, one
// click each, and then by an index of every section.
const lines = readme.split('\n');
const banner = lines.findIndex((line) => /^!\[Rusty Business™: .+\]\(docs\/rusty-business\.svg\)$/.test(line));
const firstSection = lines.findIndex((line) => line.startsWith('## '));

test('right under the picture, a table gives every download, one click each, with its first-time steps', () => {
  assert.equal(banner, 2, 'the picture is no longer right under the name');
  assert.equal(lines[banner + 1], '', 'nothing separates the picture from the downloads');
  assert.match(lines[banner + 2], /^\| Download \|/, 'the line under the picture is not the table of downloads');
  let end = banner + 2;
  while (lines[end + 1]?.startsWith('|')) end += 1;
  const table = lines.slice(banner + 2, end + 1).join('\n');

  const downloads = new Set([...readme.matchAll(/\]\((https:\/\/github\.com\/Bmorrical\/rusty-business\/releases\/latest\/download\/[^)]+)\)/g)].map((match) => match[1]));
  assert.ok(downloads.size >= 5, `the README links only ${downloads.size} downloads`);
  for (const link of downloads) assert.ok(table.includes(`](${link})`), `the table at the top does not link ${link.split('/').pop()}`);

  const rows = table.split('\n').slice(2);
  assert.deepEqual(
    rows.map((row) => /\]\(#(run-the-chat-window-[a-z]+)\) \|$/.exec(row)?.[1]),
    ['run-the-chat-window-macos', 'run-the-chat-window-windows', 'run-the-chat-window-linux'],
    'each row does not end with a link to its own steps',
  );
  // Brad, 2026-11-19: "add an engine to linux and windows distro's." The
  // Windows app starts llama.cpp once it is installed, so its row says what to
  // install; the Linux packages carry the engine.
  const [windows, linux] = rows.slice(1);
  assert.ok(windows.includes('with llama.cpp installed'), `the Windows row does not say it needs llama.cpp: ${windows}`);
  assert.ok(!linux.includes('with llama.cpp installed'), `the Linux row still says it needs llama.cpp: ${linux}`);
});

test('under the downloads, the index links every section, in the order of the page', () => {
  const at = lines.findIndex((line) => line.startsWith('**On this page:** '));
  assert.ok(at > banner && at < firstSection, 'the index is not between the picture and the first section');
  assert.ok(lines.slice(banner + 2, at - 1).every((line) => line.startsWith('|')), 'something other than the downloads comes between the picture and the index');
  let end = at;
  while (lines[end + 1]) end += 1;
  const index = lines.slice(at, end + 1).join(' ');
  const linked = [...index.matchAll(/\[([^\]]+)\]\(#([^)]+)\)/g)].map((match) => match[2]);
  const sections = headings.filter((each) => each.level === 2).map((each) => each.address);
  assert.deepEqual(linked, sections, 'the index and the sections differ');
});

// Brad, 2026-11-19: "user can install 3 ways, it spells it out clearly at the
// top:
//
// compiled binary
// recompile from source -> needs
// run from localhost"
//
// Then: "those options are a hyperlink to the section that describes it".
// So, right under the index, a numbered list gives the three ways, each a link
// to its own section, and the second says what building from source needs.
const WAYS = [
  { text: 'Download the built app', address: 'download-the-built-app' },
  { text: 'Build it from source', address: 'build-it-from-source' },
  { text: 'Run it from localhost', address: 'run-it-from-localhost' },
];
const BUILD_NEEDS = ["Apple's command line tools", "Microsoft's C++ build tools", 'WebKitGTK', 'OpenSSL', 'a C compiler'];

/** The text of a "## " section, from its heading to the next "## " heading. */
function sectionOf(address) {
  const level2 = headings.filter((each) => each.level === 2);
  const at = level2.findIndex((each) => each.address === address);
  const start = lines.indexOf(`## ${level2[at].text}`);
  const next = level2[at + 1] ? lines.indexOf(`## ${level2[at + 1].text}`, start + 1) : lines.length;
  return lines.slice(start, next).join('\n');
}

test('at the top, three ways to install it, each a link to the section that describes it', () => {
  const index = lines.findIndex((line) => line.startsWith('**On this page:** '));
  let afterIndex = index;
  while (lines[afterIndex + 1]) afterIndex += 1;
  assert.equal(lines[afterIndex + 1], '', 'nothing separates the index from what follows it');
  const at = afterIndex + 2;
  assert.equal(lines[at], '**Three ways to install it:**', 'the three ways do not come right after the index');
  assert.ok(at < firstSection, 'the three ways are not above the first section');

  let end = at + 2;
  while (lines[end + 1]) end += 1;
  const list = lines.slice(at + 2, end + 1).join('\n');
  const items = list.split(/\n(?=\d+\. )/).map((item) => item.replace(/\n +/g, ' '));
  assert.equal(items.length, 3, `the list has ${items.length} ways, not three`);

  const level2 = headings.filter((each) => each.level === 2);
  let last = -1;
  WAYS.forEach((way, n) => {
    assert.ok(items[n].startsWith(`${n + 1}. [**${way.text}**](#${way.address}). `), `way ${n + 1} does not start with a link to #${way.address}: ${items[n]}`);
    const found = level2.findIndex((each) => each.address === way.address);
    assert.ok(found >= 0, `no "## ${way.text}" section for way ${n + 1} to land on`);
    assert.equal(level2[found].text, way.text, `#${way.address} is the heading "${level2[found].text}"`);
    assert.ok(found > last, `"${way.text}" comes before the way above it on the page`);
    last = found;
  });

  for (const need of BUILD_NEEDS) {
    assert.ok(items[1].includes(need), `building from source does not say it needs ${need}`);
    assert.ok(sectionOf('build-it-from-source').includes(need), `the Build it from source section does not say it needs ${need}`);
  }
  for (const need of ['Rust', 'Node 20 or later']) {
    assert.ok(items[2].includes(need), `running it from localhost does not say it needs ${need}`);
    assert.ok(sectionOf('run-it-from-localhost').includes(need), `the Run it from localhost section does not say it needs ${need}`);
  }

  const build = sectionOf('build-it-from-source');
  for (const line of ['make app', 'scripts/windows-app.sh', 'scripts/linux-app.sh']) {
    assert.ok(build.includes(line), `the Build it from source section does not give ${line}`);
  }
  for (const system of ['On a Mac', 'On Windows', 'On Linux']) {
    assert.ok(build.includes(`\n### ${system}\n`), `the Build it from source section has no "### ${system}"`);
  }
  assert.ok(sectionOf('run-it-from-localhost').includes('npm run tauri dev'), 'the Run it from localhost section does not give npm run tauri dev');
  assert.ok(sectionOf('run-it-from-localhost').includes('127.0.0.1:1420'), 'the Run it from localhost section does not say where the page is served');
});

// Brad, 2026-11-19: "add an engine to linux and windows distro's." Since
// 0.6.0 the Windows and Linux apps start llama.cpp's llama-server once it is
// installed, though the downloads do not carry it yet. So the README says
// where the app looks, links llama.cpp's own install page, and no longer says
// those apps have no engine.
const LLAMA_INSTALL = 'https://github.com/ggml-org/llama.cpp/blob/master/docs/install.md';
const STALE = [', no model engine yet', 'has no model engine', 'have none yet', 'with no engine yet', 'Chats need a Mac'];

test('the Windows section says the app starts an installed llama.cpp, and how to get one', () => {
  const body = sectionOf('run-the-chat-window-windows').replace(/\s+/g, ' ');
  assert.ok(body.includes("The app starts llama.cpp's"), 'the Windows section does not say the app starts llama.cpp');
  assert.ok(body.includes(`](${LLAMA_INSTALL})`), "the Windows section does not link llama.cpp's install page");
  assert.ok(body.includes('`RUSTY_ENGINE_PROGRAM` names one kept somewhere else, by its full path'), 'the Windows section does not say how to name one kept elsewhere');
  assert.ok(body.includes('Without it, the window opens and says so') || body.includes('otherwise the window opens and says so'), 'the Windows section does not say what happens without it');
  assert.ok(body.includes('(On the roadmap: An engine for Windows.)'), 'the Windows section does not point at the roadmap');
  assert.ok(sectionOf('run-the-chat-window-windows').includes('`llama-server.exe` when it finds it in a folder on `PATH`'), 'the Windows section does not say it looks on PATH');
});

test('the Linux section says the app carries the engine, and still names one found on the computer', () => {
  const body = sectionOf('run-the-chat-window-linux').replace(/\s+/g, ' ');
  assert.ok(body.includes('The three files carry the engine inside them'), 'the Linux section does not say it carries the engine');
  assert.ok(body.includes('`RUSTY_ENGINE_PROGRAM` names one kept somewhere else, by its full path'), 'the Linux section does not say how to name one kept elsewhere');
  assert.ok(body.includes('`~/.local/bin`, Homebrew\'s folder, `/usr/local/bin`, and `/usr/bin`'), 'the Linux section does not give the folders the app looks in');
  const flat = readme.replace(/\s+/g, ' ');
  for (const words of STALE) assert.ok(!flat.includes(words), `the README still says "${words}"`);
});
