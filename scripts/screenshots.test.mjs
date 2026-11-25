// The screenshots' rules, checked on every test run.
//
// The README once featured one wide capture of the window. Brad, on that
// picture: "give me a better screenshot collection, this is fine but it
// shouldn't be featured, I can't blow it up, too small to read".
//
// So there is a set of pictures made large enough to read, each one a link
// that opens the picture by itself at full size, and the first picture is
// kept but shown only in the gallery.
//
// The checks read the files as text, and the first bytes of each picture.
// They start no browser: `make screenshots` is what remakes the pictures.

import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

import {
  dayLabel,
  firstRunOffers,
  formatSize,
  groups,
  latestAnswers,
  onDiskLine,
  quickest,
  shouldStart,
  showsStartHint,
  stackLine,
  startLine,
  titleFrom,
} from '../apps/desktop/src/chat.js';
import {
  FIRST_MAKER,
  HEIGHT,
  NOT_STARTED,
  NO_MODEL,
  OUTSIDE_NOT_ADDED,
  PAGE_BUTTONS,
  PICTURE_OFFERS,
  SCALE,
  SCENES,
  STACK_RUNNING,
  VISION_NOT_INSTALLED,
  WIDTH,
  engineFor,
  engineOff,
  firstRun,
  forMakers,
  makersOf,
  makersStatus,
  pickedAtFirst,
  scenesNamed,
  sessionsFrom,
  standIn,
} from './screenshots.mjs';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const BANNER = 'docs/rusty-business.svg';
const FIRST_PICTURE = 'docs/chat-window.webp';
const FOLDER = 'docs/screenshots';
const GALLERY = `${FOLDER}/README.md`;
const ENGINE = 'apps/desktop/src-tauri/src/engine.rs';
const STARTER = 'apps/desktop/src-tauri/src/starter.rs';
const STACK = 'apps/desktop/src-tauri/src/stack.rs';
const PINNED = 'apps/desktop/src-tauri/pinned-models.json';
const MAKERS = 'apps/desktop/src-tauri/src/makers.rs';
const OUTSIDE = 'apps/desktop/src-tauri/src/outside.rs';
const VISION = 'apps/desktop/src-tauri/src/vision.rs';

// The pages that show the app's pictures.
const PAGES = ['README.md', 'apps/desktop/README.md', GALLERY];

// Folders that hold other people's files or build output, never this
// repository's own pages.
const NOT_OURS = new Set(['.git', '.claude', '.vite', 'dist', 'node_modules', 'target']);

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

function recorded() {
  return JSON.parse(read(`${FOLDER}/chats.json`));
}

/** The same value with nothing left but what JSON carries. */
function plain(value) {
  return JSON.parse(JSON.stringify(value));
}

/**
 * Every picture a Markdown page shows: its alt text, its file as written,
 * and the address the picture links to (null when it is not a link).
 */
function shownIn(markdown) {
  const pictures = [];
  const pattern = /(\[)?!\[([^\]]*)\]\(([^)\s]+)\)(?:\]\(([^)\s]+)\))?/g;
  for (const [, opened, alt, src, link] of markdown.matchAll(pattern)) {
    pictures.push({ alt, src, link: opened && link ? link : null });
  }
  return pictures;
}

/** A file named on a page, as a path from the repository's root. */
function fromRoot(page, file) {
  return path.posix.normalize(path.posix.join(path.posix.dirname(page), file));
}

/** The text under a `## ` heading, up to the next one; null when it is not there. */
function sectionOf(markdown, heading) {
  const lines = markdown.split('\n');
  const start = lines.findIndex((line) => line.trim() === `## ${heading}`);
  if (start < 0) return null;
  const rest = lines.slice(start + 1);
  const end = rest.findIndex((line) => line.startsWith('## '));
  return rest.slice(0, end < 0 ? rest.length : end).join('\n');
}

/** A PNG's width and height in pixels, from its first bytes; null when it is not a PNG. */
function pngSize(bytes) {
  const isPng =
    bytes.length >= 24 &&
    bytes.subarray(0, 8).toString('hex') === '89504e470d0a1a0a' &&
    bytes.subarray(12, 16).toString('latin1') === 'IHDR';
  return isPng ? { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) } : null;
}

/** Every Markdown page in the repository, as paths from its root. */
function markdownPages(folder = '') {
  const pages = [];
  for (const entry of readdirSync(path.join(REPO, folder), { withFileTypes: true })) {
    const here = folder ? `${folder}/${entry.name}` : entry.name;
    if (entry.isDirectory()) {
      if (!NOT_OURS.has(entry.name)) pages.push(...markdownPages(here));
    } else if (entry.name.endsWith('.md')) {
      pages.push(here);
    }
  }
  return pages;
}

/** The fields of one of the app's reports, as the page reads them, from the Rust source. */
function fieldsOf(file, report) {
  const [, body] = read(file).match(new RegExp(`pub struct ${report} \\{\\n([^}]+)\\}`)) ?? [];
  // A field renamed for the page goes by its new name.
  return [...(body ?? '').matchAll(/^\s*(?:#\[serde\(rename = "(\w+)"\)\]\n\s*)?pub (\w+):/gm)].map(([, renamed, field]) =>
    renamed ?? field.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase()),
  );
}

/** The fields of the app's report about the engine. */
function reportFields() {
  return fieldsOf(ENGINE, 'EngineStatus');
}

/** The fields of the app's report about starting the engine. */
function startFields() {
  return fieldsOf(STARTER, 'StartReport');
}

/** What the page draws when no engine answers; undefined when it has no such panel. */
function offPanel() {
  const page = read('apps/desktop/src/App.vue');
  return page.match(/<template v-else>\n(\s+<p id="engine-error"><span class="dot off"[\s\S]*?)<\/template>/)?.[1];
}

/** The app's allow-list, read from the Rust source. */
function catalog() {
  const entry =
    /CatalogEntry \{\s+needle: "([^"]+)",\s+name: "([^"]+)",\s+maker: "([^"]+)",\s+license: "([^"]+)",\s+context: ([\d_]+),\s+pictures: (true|false),?\s+\}/g;
  return [...read(ENGINE).matchAll(entry)].map(([, needle, name, maker, license, context, pictures]) => ({
    needle,
    name,
    maker,
    license,
    context: Number(context.replaceAll('_', '')),
    pictures: pictures === 'true',
  }));
}

/** The chat models on the list: a model that reads pictures runs elsewhere. */
function chatCatalog() {
  return catalog().filter((entry) => !entry.pictures);
}

test('the first, wide picture is kept, and shown only in the gallery', () => {
  // "this is fine but it shouldn't be featured"
  assert.ok(existsSync(path.join(REPO, FIRST_PICTURE)), `${FIRST_PICTURE} is kept`);

  const showing = markdownPages().filter((page) =>
    shownIn(read(page)).some((picture) => fromRoot(page, picture.src) === FIRST_PICTURE),
  );
  assert.deepEqual(showing, [GALLERY]);
});

test('the README shows the whole set, in order, before anything else it explains', () => {
  // "give me a better screenshot collection"
  const readme = read('README.md');
  const headings = readme.split('\n').filter((line) => line.startsWith('## '));
  assert.equal(headings[0], '## Screenshots', 'Screenshots is the first section');

  const section = sectionOf(readme, 'Screenshots');
  assert.deepEqual(
    shownIn(section).map((picture) => picture.src),
    SCENES.map((scene) => `${FOLDER}/${scene.file}`),
  );
  assert.match(section, /\]\(docs\/screenshots\)/, 'the section links to the gallery');
  assert.match(section, /full size/, 'the section says a picture opens full size');

  const above = readme.slice(0, readme.indexOf('## Screenshots'));
  assert.deepEqual(
    shownIn(above).map((picture) => picture.src),
    [BANNER],
    'the banner is the only picture above the set',
  );
});

test('every picture opens by itself at full size, and has words for it', () => {
  // "I can't blow it up". On GitHub a link that ends in ?raw=true opens the
  // picture alone, where the browser can enlarge it. A plain link opens the
  // file's page, with the picture fitted to it.
  let checked = 0;
  for (const page of PAGES) {
    for (const picture of shownIn(read(page))) {
      if (fromRoot(page, picture.src) === BANNER) continue;
      const where = `${page}: ${picture.src}`;
      assert.equal(picture.link, `${picture.src}?raw=true`, `${where} links to itself with ?raw=true`);
      assert.ok(existsSync(path.join(REPO, fromRoot(page, picture.src))), `${where} is a file`);
      assert.ok(picture.alt.length >= 40, `${where} has alt text that says what the picture shows`);
      assert.doesNotMatch(picture.alt, /\|/, `${where} has alt text that fits in a table`);
      checked += 1;
    }
  }
  // The set in the README, one for the desktop app, and in the gallery the
  // set and the first picture.
  assert.equal(checked, SCENES.length * 2 + 2);
});

test('the pictures are large enough to read', () => {
  // "too small to read". A README's column is under 1,000 points wide, so a
  // picture of a wider window is shrunk to fit and its words shrink with it.
  // Twice the pixels keeps the words sharp once the picture is opened.
  assert.ok(WIDTH <= 1000, `a window ${WIDTH} points wide is shrunk in a README`);
  assert.ok(SCALE >= 2, 'at least two pixels for each point');

  for (const scene of SCENES) {
    const size = pngSize(readFileSync(path.join(REPO, FOLDER, scene.file)));
    assert.deepEqual(
      size,
      { width: WIDTH * SCALE, height: HEIGHT * SCALE },
      `${scene.file} is the size the script makes. Run \`make screenshots\`.`,
    );
  }
});

test('the folder holds the set, the chats behind it, and its README, and nothing else', () => {
  const files = readdirSync(path.join(REPO, FOLDER))
    .filter((name) => !name.startsWith('.'))
    .sort();
  const expected = [...SCENES.map((scene) => scene.file), 'README.md', 'chats.json'].sort();
  assert.deepEqual(files, expected);

  const gallery = read(GALLERY);
  const shown = shownIn(gallery).map((picture) => picture.src);
  assert.deepEqual(shown.slice(0, SCENES.length), SCENES.map((scene) => scene.file), 'the gallery shows the set, in order');
  for (const scene of SCENES) {
    assert.ok(gallery.includes(`\`${scene.file}\``), `the gallery's table lists ${scene.file}`);
  }
});

test('the gallery says how the pictures are made, and that they are not captures of the window', () => {
  const made = sectionOf(read(GALLERY), 'How these were made');
  assert.ok(made, 'the gallery has a section on how the pictures were made');
  assert.match(made, /make screenshots/);
  assert.match(made, /not captures of the app's window/);
  assert.match(made, /\]\(chats\.json\)/);
  assert.match(made, /Nothing is sent to a model/);
});

test('each scene is a picture the script can make', () => {
  const { chats } = recorded();
  const ids = chats.map((chat) => chat.id);

  assert.equal(new Set(SCENES.map((scene) => scene.file)).size, SCENES.length, 'no file is made twice');
  for (const scene of SCENES) {
    assert.match(scene.file, /^[a-z-]+\.png$/, `${scene.file} is a PNG with a plain name`);
    assert.ok(['light', 'dark'].includes(scene.scheme), `${scene.file} is light or dark`);
    assert.ok(['on', 'off', 'first-run'].includes(scene.engine), `${scene.file} has the engine on, off, or on a first run`);
    if (scene.open) assert.ok(ids.includes(scene.open), `${scene.file} opens a recorded chat`);
    if (scene.page) {
      assert.ok(scene.page in PAGE_BUTTONS, `${scene.file} opens a page the side bar has a button for`);
      const script = read('scripts/screenshots.mjs');
      assert.ok(script.includes("document.querySelector('.gear').click()"), 'the script looks for a page button without opening the gear');
      assert.ok(script.includes("document.querySelectorAll('.gear-menu button')"), "the script looks for the page's button outside the gear's menu");
      assert.ok(!scene.open, `${scene.file} opens a page, not a chat`);
    }
    if (scene.tab) {
      const holder = { settings: 'settingsTab', installers: 'installersTab' }[scene.page];
      assert.ok(holder, `${scene.file} opens a tab, and only Settings and Installers have tabs`);
      assert.match(
        read('apps/desktop/src/App.vue'),
        new RegExp(`@click="openTab\\('${scene.page}', '[a-z]+'\\)">${scene.tab}</button>`),
        `${scene.file} opens a tab its page has`,
      );
    }
    if (scene.thinking) {
      const chat = chats.find((entry) => entry.id === scene.open);
      assert.ok(
        chat?.messages.some((message) => message.thinking),
        `${scene.file} opens a chat that has thinking to show`,
      );
    }
  }
});

test('named pictures are made on their own, and a name that is not a picture is refused', () => {
  assert.deepEqual(scenesNamed([]), SCENES);
  assert.deepEqual(
    scenesNamed(['settings.png']).map((scene) => scene.file),
    ['settings.png'],
  );
  assert.throws(() => scenesNamed(['settings.png', 'nope.png']), /No picture is called nope\.png\./);
});

test('the recorded engine offers only models on the app\'s list', () => {
  // The pictures show the models at the top of the window, so what was
  // recorded has to match the allow-list in the Rust code. Brad, 2026-11-17: "user can pick google,
  // gemini, meta from the app if they want those instead." So the list holds
  // models from Google and Meta too, and the recording offers every model on
  // it, so Settings can show each maker.
  const { engine, chats } = recorded();
  const allowed = catalog();
  assert.ok(allowed.length > 0, 'the allow-list was read from engine.rs');

  // The list holds for the models the engine offers and for the models the
  // app found on the disk.
  assert.ok(engine.onDisk.length > 0, 'the recorded report names models on the disk');
  for (const model of [...engine.models, ...engine.onDisk]) {
    const entry = allowed.find((candidate) => model.id.toLowerCase().includes(candidate.needle));
    assert.ok(entry, `${model.id} is on the app's list`);
    assert.deepEqual(
      { name: model.name, maker: model.maker, context: model.context },
      { name: entry.name, maker: entry.maker, context: entry.context },
      `${model.id} is described as the app describes it`,
    );
  }
  // And for the models it offers to download, each with its license.
  for (const offer of engine.offers) {
    const entry = allowed.find((candidate) => offer.id.toLowerCase().includes(candidate.needle));
    assert.ok(entry, `${offer.id} is on the app's list`);
    assert.deepEqual(
      { name: offer.name, maker: offer.maker, license: offer.license },
      { name: entry.name, maker: entry.maker, license: entry.license },
    );
  }
  assert.deepEqual(
    engine.offers.map((offer) => offer.name),
    chatCatalog().map((entry) => entry.name),
    'every chat model on the list is offered, in the list\'s order',
  );
  // A model that reads pictures is offered on its own, through PICTURE_OFFERS.
  assert.deepEqual(
    PICTURE_OFFERS.map((offer) => offer.name),
    allowed.filter((entry) => entry.pictures).map((entry) => entry.name),
  );

  // A chat names the model it was started with. Its answers may come from
  // several: Brad, 2026-11-17: "user can run the same question in 1-nth
  // models and checks the results of each one". Each answer names a model the
  // engine offers, and no model answers one question twice.
  const offered = engine.models.map((model) => model.id);
  let many = 0;
  for (const chat of chats) {
    assert.ok(offered.includes(chat.model), `the chat "${chat.id}" uses a model the engine offers`);
    for (const group of groups(chat.messages)) {
      const models = group.answers.filter((answer) => answer.role === 'assistant').map((answer) => answer.model);
      for (const model of models) {
        assert.ok(offered.includes(model), `an answer in "${chat.id}" names ${model}, which the engine offers`);
      }
      assert.equal(new Set(models).size, models.length, `no model answers one question in "${chat.id}" twice`);
      if (models.length > 1) many += 1;
    }
  }
  assert.ok(many > 0, 'one recorded question is answered by several models, for compare.png');
});

test('the recorded report has the fields the app reports, in its order', () => {
  // The pictures are only as true as the report behind them. A field the app
  // gains or renames has to be recorded too, or a picture shows a page the
  // app never draws.
  const fields = reportFields();
  assert.ok(fields.includes('onDisk') && fields.includes('startHint'), 'the fields were read from engine.rs');
  assert.deepEqual(Object.keys(recorded().engine), fields);
});

test('the recorded downloads are the pinned ones, in order, with their sizes', () => {
  // The picture of the first run shows each size. It is the size of the
  // files the app would download, added up, so a picture never promises a
  // smaller download than the app makes.
  const pins = JSON.parse(read(PINNED));
  assert.deepEqual(
    recorded().engine.offers.map((offer) => ({ id: offer.id, bytes: offer.bytes })),
    pins
      .filter((pin) => !PICTURE_OFFERS.some((offer) => offer.id === pin.id))
      .map((pin) => ({ id: pin.id, bytes: pin.files.reduce((sum, file) => sum + file.bytes, 0) })),
  );
});

test('on a first run, the app offers its models with their sizes, and the quickest', () => {
  // Brad: "I should be able to just pull the .dmg or .exe from read me and
  // install and go, one click ready." He picked "Picker with sizes". So on a
  // computer with none of the app's models, the page lists the ones it can
  // download, each with its size, and points to the quickest to start.
  const { engine } = recorded();
  const before = plain(engine);
  const first = firstRun(engine);

  assert.deepEqual(first.onDisk, []);
  assert.deepEqual(first.models, []);
  assert.equal(first.reachable, false);
  assert.deepEqual(firstRunOffers(first), engine.offers);
  assert.deepEqual(firstRunOffers(engine), [], 'with models on hand, nothing is offered');
  assert.deepEqual(firstRunOffers(engineOff(engine)), [], 'nor with models on the disk and the engine off');
  assert.deepEqual(
    engine.offers.map((offer) => formatSize(offer.bytes)),
    ['8.3 GB', '12.1 GB', '17.8 GB', '15.4 GB', '4.5 GB'],
  );
  assert.equal(quickest(engine.offers), 'mlx-community/Llama-3.1-8B-Instruct-4bit');

  // Brad, 2026-11-17: "I think the user can pick one or more at install with
  // microsoft default". A first run picks Microsoft alone, so it offers
  // Microsoft's model, and that one is the quickest to start.
  const offered = firstRunOffers(forMakers(first, pickedAtFirst(first)));
  assert.deepEqual(pickedAtFirst(first), ['Microsoft']);
  assert.deepEqual(offered.map((offer) => offer.name), ['Phi-4']);
  assert.equal(quickest(offered), 'mlx-community/phi-4-4bit');
  assert.deepEqual(plain(engine), before, 'the recorded engine is left as it was');

  // With nothing to serve, the app does not start the engine, and says so in
  // the app's own words. A first run has no saved chats yet.
  const sessions = sessionsFrom(recorded().chats);
  assert.deepEqual(engineFor({ engine: 'first-run' }, engine, sessions), { report: first, start: NO_MODEL, saved: [] });
  assert.deepEqual(engineFor({ engine: 'off' }, engine, sessions).saved, sessions);
  assert.deepEqual(engineFor({ engine: 'on' }, engine, sessions).report, engine);
  assert.deepEqual(Object.keys(NO_MODEL), startFields());
  assert.equal(NO_MODEL.started, false, 'no engine is started while the pictures are made');
  assert.ok(read(STARTER).includes(`"${NO_MODEL.reason}"`), `${STARTER} no longer holds the sentence in the picture`);
  assert.equal(startLine(NO_MODEL, first), NO_MODEL.reason);
});

test('the page downloads a model only when its Download button is pressed', () => {
  // "Inside the app (Recommended)": the app downloads models itself, and
  // only when asked.
  const page = read('apps/desktop/src/App.vue');
  const asks = [...page.matchAll(/invoke\("download_model"/g)];
  assert.equal(asks.length, 1, 'the page asks Rust for a download in one place');
  // It is called from the first-run picker, and from install(), which only
  // the Install buttons call.
  const calls = [...page.matchAll(/\bdownload\(/g)].map((match) => page.slice(match.index - 60, match.index));
  assert.deepEqual(
    calls.map((before) => /async function $|@click="$|function install\(maker, offer\) \{\n {2}if \(!\(await $/.test(before)),
    [true, true, true],
    'download() is defined once and called only from a button or from install()',
  );
  const installs = [...page.matchAll(/\binstall\(/g)].map((match) => page.slice(match.index - 30, match.index));
  assert.deepEqual(
    installs.map((before) => /async function $|@click="$/.test(before)),
    // One Install on the Chat tab, one on the Pictures tab.
    [true, true, true],
    'install() is defined once and called only from a button',
  );
  assert.match(page, /\{\{ formatSize\(offer\.bytes\) \}\}/, 'each offer shows its size');
  assert.match(page, />\s*Download\s*</, 'each offer has a Download button');
  assert.match(page, /invoke\("stop_download"\)/, 'a download can be stopped');
});

test('while a model downloads, the page shows it, stops it on Cancel, and starts the engine once it is whole', () => {
  // Brad: "Picker with sizes (Recommended)", and of the engine: "Inside the
  // app (Recommended)". The page points to the quickest, runs one download at
  // a time, says when one stopped or failed, and starts the engine once a
  // model is whole.
  const page = read('apps/desktop/src/App.vue');
  const download = page.match(/\nasync function download\(offer\) \{\n([\s\S]*?)\n\}\n/)?.[1] ?? '';
  assert.match(download, /^\s*if \(downloading\.value \|\| !canDownload\.value\) return false;/, 'download() does nothing while one is running or its gate is off');
  assert.match(download, /invoke\("download_model", \{ id: offer\.id, onProgress: channel \}\)/, 'Rust is told which model, and reports its progress');
  assert.match(download, /if \(report\.stopped\) \{[\s\S]*?stoppedNote\.value = lanEngine\.value[\s\S]*?paused[\s\S]*?stopped, and none of it was kept/, 'the download reports a pause or cancellation');
  assert.match(
    download,
    /if \(report\.whole\) await Promise\.all\(\[checkEngine\(\), readMakers\(\)\]\);/,
    'a whole model starts the engine, and the Installers page shows it on this computer',
  );
  assert.match(download, /catch \(error\) \{\n\s*problem\.value = String\(error\);/, 'a download that fails says why');
  const cancel = page.match(/\nasync function cancelDownload\(\) \{\n([\s\S]*?)\n\}\n/)?.[1] ?? '';
  assert.match(cancel, /await invoke\("stop_download"\)/, 'Cancel asks Rust to stop the download');

  const row = page.match(/<li v-for="offer in offers"[\s\S]*?<\/li>/)?.[0] ?? '';
  assert.match(row, /\{\{\s*offer\.id === quick \? " · quickest to start" : ""\s*\}\}/, 'the quickest to start is pointed to');
  assert.match(row, /by \{\{ offer\.maker \}\} · \{\{ offer\.license \}\} · .*\{\{ formatSize\(offer\.bytes\) \}\}/, 'each offer gives its maker, its license, and its size');
  assert.match(row, /<progress\s+:value="progress\?\.total \? downloaded\(progress\) : undefined"/, 'a download shows how far it has come');
  assert.match(row, /\{\{ stopping \? "Stopping\.\.\." : progress\?\.total \? downloadLine\(progress\) : "Waiting for the server…" \}\}/, 'and in words');
  assert.match(row, /<button type="button" :disabled="stopping" @click="cancelDownload">\{\{ lanEngine \? "Pause" : "Cancel" \}\}<\/button>/, 'Cancel stops it');
  // Brad, 2026-11-17: "off by default, everything stops". Download is off too
  // while the outbound gate is, and the picker offers to turn the gate on.
  assert.match(row, /:disabled="Boolean\(downloading\) \|\| !canDownload"\s*\n\s*@click="download\(offer\)"/, 'Download is off while a model downloads, or while the gate is off');
  const listed = page.match(/<li v-for="entry in maker\.models"[\s\S]*?\n {16}<\/li>/)?.[0] ?? '';
  assert.match(
    listed,
    /v-else-if="entry\.offer && !entry\.onDisk"[\s\S]*?:disabled="makersLocked \|\| !canDownload"\s*\n\s*@click="install\(maker\.name, entry\.offer\)"/,
    'Install is off there too, and shows only for a model that is not here yet',
  );
  assert.match(listed, /<button type="button" :disabled="stopping" @click="cancelDownload">\{\{ lanEngine \? "Pause" : "Cancel" \}\}<\/button>/, 'Cancel stops it there too');
  assert.match(
    page,
    /<p v-if="!installGateOpen" class="gate-ask">\s*<span>Turn on the \{\{ installGateName \}\} to download a model\.<\/span>\s*<button class="primary" type="button" :disabled="installGateBusy" @click="turnOnInstallGate">Turn on the \{\{ installGateName \}\}<\/button>\s*<\/p>/,
    'the first-run picker offers to turn the gate on',
  );
  assert.match(page, /<p v-if="stoppedNote" class="quiet small">\{\{ stoppedNote \}\}<\/p>/, 'the stopped note is shown');
  assert.match(page, /<p v-if="problem" class="problem" role="alert">\s*\{\{ problem \}\}/, 'and so is the reason a download failed, on the Installers page too');
});

test('the saved chats are listed newest first, and read "Today" and "Yesterday" at any hour', () => {
  const { chats } = recorded();
  for (const now of [new Date(2026, 0, 15, 0, 5), new Date(2026, 0, 15, 12, 0), new Date(2026, 0, 15, 23, 55)]) {
    const sessions = sessionsFrom(chats, now);
    assert.deepEqual(
      sessions.map((entry) => entry.id),
      chats.map((chat) => chat.id),
    );
    sessions.forEach((entry, index) => {
      const chat = chats[index];
      assert.equal(entry.title, titleFrom(chat.messages[0].content));
      assert.equal(entry.model, chat.model);
      assert.deepEqual(entry.messages, chat.messages);
      if (index > 0) {
        assert.ok(entry.updatedAt < sessions[index - 1].updatedAt, `"${entry.id}" is older than the chat above it`);
      }
      const label = ['Today', 'Yesterday'][chat.daysAgo];
      if (label) assert.equal(dayLabel(entry.updatedAt, now.getTime()), label, `"${entry.id}" at ${now.toTimeString()}`);
    });
  }
});

test('with the engine off, the app is told nothing answers', () => {
  const { engine } = recorded();
  const before = plain(engine);
  const off = engineOff(engine);

  assert.equal(off.reachable, false);
  assert.deepEqual(off.models, []);
  assert.equal(off.hidden, 0);
  assert.equal(off.error, 'The engine is not answering.');
  assert.equal(off.name, engine.name);
  assert.equal(off.url, engine.url);
  assert.equal(off.startHint, engine.startHint);
  assert.deepEqual(plain(engine), before, 'the recorded engine is left as it was');
});

test('with the engine off, the app still names its models on this computer', () => {
  // Brad, on what the app does by itself: "it should search and up the local
  // models and stack." The search does not need the engine, so its answer is
  // on the page when nothing answers at the engine's address.
  const { engine } = recorded();
  const off = engineOff(engine);
  assert.deepEqual(off.onDisk, engine.onDisk, 'a model on the disk is still on the disk');
  assert.equal(onDiskLine(off.onDisk), 'On this computer: Phi-4, gpt-oss 20B, and Nemotron 3.5 Lightning.');

  // The page draws that sentence where it says the engine is not answering,
  // above the command that starts it.
  const panel = offPanel();
  assert.ok(panel, 'the page has a panel for an engine that is not answering');
  const sentence = panel.indexOf('{{ onDiskLine(engine.onDisk) }}');
  assert.ok(sentence > 0, 'the panel names the models on this computer');
  assert.ok(sentence < panel.indexOf('{{ engine.startHint }}'), 'before the command that starts the engine');
});

test('with the engine off, the app asks for one to be started, and says why when it cannot', () => {
  // Brad: "it should search and up the local models and stack." and "install
  // and go, one click ready". So when no engine answers, the page asks Rust
  // to start one, and when Rust cannot, the page says why in one sentence.
  const { engine } = recorded();
  const off = engineOff(engine);
  assert.equal(shouldStart(off), true, 'no engine answers, so one is asked for');
  assert.equal(shouldStart(engine), false, 'an engine that answers is left alone');
  assert.match(read('apps/desktop/src/App.vue'), /invoke\("start_engine"\)/, 'the page asks Rust to start the engine');

  // What the pictures are told is what the app says: the same fields in the
  // same order, and a sentence the Rust tests hold word for word.
  const fields = startFields();
  assert.ok(fields.includes('reason'), 'the fields were read from starter.rs');
  assert.deepEqual(Object.keys(NOT_STARTED), fields);
  assert.equal(NOT_STARTED.answering, false);
  assert.equal(NOT_STARTED.started, false, 'no engine is started while the pictures are made');
  assert.ok(read(STARTER).includes(`"${NOT_STARTED.reason}"`), `${STARTER} no longer holds the sentence in the picture`);
  assert.equal(startLine(NOT_STARTED, off), NOT_STARTED.reason);

  // The page draws the reason under the models it found, and above the
  // command a person can run instead. While it waits, it says it is starting.
  const panel = offPanel();
  assert.ok(panel, 'the page has a panel for an engine that is not answering');
  const reason = panel.indexOf('{{ startNote }}');
  assert.ok(reason > panel.indexOf('{{ onDiskLine(engine.onDisk) }}'), 'the reason comes after the models on this computer');
  assert.ok(reason < panel.indexOf('{{ engine.startHint }}'), 'and before the command that starts the engine');
  assert.ok(panel.includes('Starting the engine...'), 'the page says so while the engine is on its way up');
});

test('on Windows and Linux, the app starts llama-server itself and shows no command to start one', () => {
  // Brad, 2026-11-16: "I should be able to just pull the .dmg or .exe from
  // read me and install and go, one click ready." And 2026-11-17, of the
  // Linux downloads: "I got you in coming build". Then, 2026-11-19: "add an
  // engine to linux and windows distro's."
  const starter = read(STARTER);
  assert.match(starter, /"windows" \| "linux" => models\.map\(\|models\| Engine::Llama \{ models \}\)/, 'Windows and Linux get llama-server');
  assert.match(starter, /Engine::Llama \{ \.\. \} if os == "windows" => "llama-server\.exe"/);
  assert.match(starter, /Engine::Llama \{ \.\. \} => "llama-server"/);
  // Any other system still opens the window and says why in one sentence.
  const said = 'The app cannot start an engine on this system yet.';
  assert.ok(starter.includes(`"${said}"`), `${STARTER} no longer holds the sentence for other systems`);
  for (const system of ['Windows', 'Linux']) {
    const off = { ...engineOff(recorded().engine), startHint: '' };
    assert.equal(startLine({ ...NOT_STARTED, reason: said }, off), said, `the footer shows why on ${system}`);
    assert.equal(showsStartHint(off, false), false, `and no command to run on ${system}`);
  }

  // Both lines about starting the engine by hand wait on the same answer.
  const panel = offPanel();
  assert.ok(panel, 'the page has a panel for an engine that is not answering');
  assert.match(panel, /<p v-if="handStart" class="quiet small">To start it yourself:<\/p>/);
  assert.match(panel, /<code v-if="handStart" class="hint">\{\{ engine\.startHint \}\}<\/code>/);
  assert.match(read('apps/desktop/src/App.vue'), /const handStart = computed\(\(\) => showsStartHint\(engine\.value, starting\.value\)\);/);
});

test('with the window open, the app asks for the stack to be started, and says why when it cannot', () => {
  // Brad: "it should search and up the local models and stack." and "install
  // and go, one click ready". So when the window opens, the page asks Rust to
  // start the stack, and the footer says where it answers or why it does not.
  const page = read('apps/desktop/src/App.vue');
  assert.match(page, /invoke\("start_stack"\)/, 'the page asks Rust to start the stack');
  const mounted = page.match(/onMounted\(async \(\) => \{\n([\s\S]*?)\n\}\);/)?.[1] ?? '';
  assert.match(mounted, /^\s+checkStack\(\);$/m, 'it asks when the window opens, without waiting on it');
  assert.ok(!/addEventListener\("focus", checkStack\)/.test(page), 'and not each time the window is focused');

  // The footer draws the stack's line under the engine's, with Try again
  // when the stack is not running.
  const footer = page.match(/<footer class="engine"[\s\S]*?<\/footer>/)?.[0] ?? '';
  const line = footer.indexOf('{{ stackLine(stack, false) }}');
  assert.ok(line > footer.indexOf('{{ engine.startHint }}'), 'the stack comes after the engine');
  assert.ok(footer.includes('@click="checkStack">Try again</button>'), 'Try again asks once more');

  // What the pictures are told is what the app says: the same fields, in the
  // same order, and the address of the app's own front door.
  assert.deepEqual(Object.keys(STACK_RUNNING), fieldsOf(STACK, 'StackReport'));
  const port = read(STACK).match(/pub const FRONT_DOOR_PORT: u16 = (\d+);/)?.[1];
  assert.equal(STACK_RUNNING.address, `127.0.0.1:${port}`);
  assert.equal(STACK_RUNNING.started, false, 'no stack is started while the pictures are made');
  assert.equal(stackLine(STACK_RUNNING, false), `Stack on 127.0.0.1:${port}`);
});

test('the stand-in picks the makers as the app does before the person picks', () => {
  // Brad, 2026-11-17: "I think the user can pick one or more at install with
  // microsoft default". With nothing picked yet, the app picks Microsoft and
  // the maker of each model already on the disk, so the pictures do too.
  const { engine } = recorded();
  assert.ok(read(MAKERS).includes(`pub const FIRST: &str = "${FIRST_MAKER}";`), `${MAKERS} picks ${FIRST_MAKER} first`);
  assert.deepEqual(makersOf(engine), [...new Set(catalog().map((entry) => entry.maker))], 'the makers, in the list\'s order');
  assert.deepEqual(pickedAtFirst(engine), ['Microsoft', 'OpenAI', 'NVIDIA']);
  assert.deepEqual(pickedAtFirst(firstRun(engine)), ['Microsoft']);

  // Only the picked makers' models are reported, and hidden models still count.
  const google = forMakers(engine, ['Google']);
  assert.deepEqual([google.models, google.onDisk], [[], []]);
  assert.deepEqual(google.setAside, engine.models, 'a saved chat still names the model that wrote it');
  assert.deepEqual(Object.keys(google), Object.keys(engine), 'the report keeps its fields, in order');
  assert.deepEqual(google.offers.map((offer) => offer.name), ['Gemma 4 26B']);
  assert.equal(google.hidden, engine.hidden);

  // Settings lists every maker with its models, as makers.rs reports them.
  const status = makersStatus(engine, ['Microsoft', 'Meta']);
  assert.deepEqual(
    status.map((maker) => [maker.name, maker.picked]),
    [['Microsoft', true], ['OpenAI', false], ['NVIDIA', false], ['Google', false], ['Meta', true]],
  );
  assert.deepEqual(Object.keys(status[0]), fieldsOf(MAKERS, 'Maker'));
  assert.deepEqual(Object.keys(status[0].models[0]), fieldsOf(MAKERS, 'Listed'));
  assert.deepEqual(
    status.map((maker) => maker.models.map((model) => [model.name, model.license, model.onDisk])),
    [
      [['Phi-4', 'MIT', true]],
      [['gpt-oss 20B', 'Apache 2.0', true]],
      [['Nemotron 3.5 Lightning', 'OpenMDW 1.1', true]],
      [['Gemma 4 26B', 'Apache 2.0', false]],
      [['Llama 3.1 8B', 'Llama 3.1 Community License', false]],
    ],
  );
});

test('the stand-in answers the twelve calls the pictures need, and refuses the rest', async () => {
  const { engine, chats } = recorded();
  const sessions = sessionsFrom(chats, new Date(2026, 0, 15, 9, 0));

  let cleared = 0;
  const window = { localStorage: { clear: () => (cleared += 1) } };
  vm.runInNewContext(standIn(engine, sessions), { window, structuredClone });
  const { invoke } = window.__TAURI_INTERNALS__;

  assert.equal(cleared, 1, 'what the page kept from an earlier picture is cleared');
  const picked = pickedAtFirst(engine);
  assert.deepEqual(plain(await invoke('engine_status')), plain(forMakers(engine, picked)));
  assert.deepEqual(plain(await invoke('makers_status')), plain(makersStatus(engine, picked, PICTURE_OFFERS)));

  // Phi-3.5 Vision is offered under Microsoft after Phi-4, as on a Mac, and
  // marked as reading pictures. The engine's offers, the first run's, leave it
  // out, and no picture engine answers, so none is started.
  const microsoft = (await invoke('makers_status')).find((maker) => maker.name === 'Microsoft');
  assert.deepEqual(microsoft.models.map((model) => [model.name, model.pictures]), [['Phi-4', false], ['Phi-3.5 Vision', true]]);
  assert.ok(!(await invoke('engine_status')).offers.some((offer) => offer.name === 'Phi-3.5 Vision'));
  const pins = JSON.parse(read(PINNED));
  for (const offer of PICTURE_OFFERS) {
    const pin = pins.find((entry) => entry.id === offer.id);
    assert.ok(pin, `${offer.name} is pinned`);
    assert.equal(offer.bytes, pin.files.reduce((sum, file) => sum + file.bytes, 0), `${offer.name} is offered at its pinned size`);
    assert.ok(read(ENGINE).includes(`name: "${offer.name}"`), `engine.rs names ${offer.name}`);
  }
  assert.deepEqual(plain(await invoke('picture_engine_status')), VISION_NOT_INSTALLED);
  const [, visionBody] = read(VISION).match(/pub struct PictureEngine \{\n([^}]+)\}/) ?? [];
  const visionFields = [...(visionBody ?? '').matchAll(/^\s*pub (\w+):/gm)].map(([, field]) => field.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase()));
  assert.deepEqual(Object.keys(VISION_NOT_INSTALLED), visionFields, 'the stand-in gives the picture engine in the fields vision.rs gives');
  assert.ok(read(VISION).includes(`"${VISION_NOT_INSTALLED.startHint}"`), 'vision.rs gives the same command to install the picture engine');
  assert.deepEqual(
    plain(await invoke('list_sessions')),
    sessions.map((entry) => ({
      id: entry.id,
      title: entry.title,
      model: entry.model,
      models: latestAnswers(entry.messages),
      updatedAt: entry.updatedAt,
      turns: entry.messages.length,
    })),
  );
  assert.deepEqual(plain(await invoke('load_session', { id: sessions[1].id })), plain(sessions[1]));

  // Asked to start the engine, the stand-in starts nothing and says why.
  assert.deepEqual(plain(await invoke('start_engine')), NOT_STARTED);
  const first = { window: { localStorage: { clear: () => {} } }, structuredClone };
  vm.runInNewContext(standIn(firstRun(engine), sessions, NO_MODEL), first);
  assert.deepEqual(plain(await first.window.__TAURI_INTERNALS__.invoke('start_engine')), NO_MODEL);
  // On a first run Microsoft alone is picked, so Phi-4 alone is offered.
  const firstOffers = (await first.window.__TAURI_INTERNALS__.invoke('engine_status')).offers;
  assert.deepEqual(firstOffers.map((offer) => offer.name), ['Phi-4']);

  // Asked to start the stack, it starts nothing and says the stack runs.
  assert.deepEqual(plain(await invoke('start_stack')), STACK_RUNNING);

  // Both gates are off, as on a first run.
  assert.equal(await invoke('gate_status'), false);
  assert.equal(await invoke('lan_gate_status'), false);

  // No model is switched off, and Uninstall would move one to the Trash, as on a Mac.
  assert.deepEqual(plain(await invoke('deactivated_models')), []);
  assert.deepEqual(plain(await invoke('uninstall_place')), { trash: 'the Trash', sharedCache: true });

  // No outside engine is added, and each comes in the fields outside.rs gives
  // the page, in the order it gives them.
  assert.deepEqual(plain(await invoke('outside_status')), OUTSIDE_NOT_ADDED);
  const [, statusBody] = read(OUTSIDE).match(/pub struct OutsideStatus \{\n([^}]+)\}/) ?? [];
  const fields = [...(statusBody ?? '').matchAll(/^\s*(?:pub )?(\w+):/gm)].map(([, field]) => field.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase()));
  for (const entry of OUTSIDE_NOT_ADDED) {
    assert.deepEqual(Object.keys(entry), fields, `the stand-in gives ${entry.name} in fields outside.rs does not have`);
  }
  const source = read(OUTSIDE);
  const idOf = source.slice(source.indexOf('    fn id(self) -> &\'static str {'), source.indexOf('\n    }\n', source.indexOf('    fn id(self) -> &\'static str {')));
  const ids = [...idOf.matchAll(/Self::\w+ => "(\w+)",/g)].map(([, id]) => id);
  assert.ok(source.includes('const ALL: [Self; 4] = [Self::Xai, Self::OpenAi, Self::Anthropic, Self::Google];'), 'outside.rs gives the engines in another order');
  assert.deepEqual(OUTSIDE_NOT_ADDED.map((entry) => entry.id), ids, 'the stand-in gives engines outside.rs does not, or in another order');
  assert.ok(read(OUTSIDE).includes(`pub const XAI_HOST: &str = "${OUTSIDE_NOT_ADDED[0].host}";`), 'the stand-in gives Grok a host outside.rs does not ask');
  assert.ok(read(OUTSIDE).includes(`pub const OPENAI_HOST: &str = "${OUTSIDE_NOT_ADDED[1].host}";`), 'the stand-in gives GPT a host outside.rs does not ask');
  assert.ok(read(OUTSIDE).includes(`pub const ANTHROPIC_HOST: &str = "${OUTSIDE_NOT_ADDED[2].host}";`), 'the stand-in gives Claude a host outside.rs does not ask');
  assert.ok(read(OUTSIDE).includes(`pub const GOOGLE_HOST: &str = "${OUTSIDE_NOT_ADDED[3].host}";`), 'the stand-in gives Gemini a host outside.rs does not ask');

  // A call that would send, save, trash, download, switch a model off, or
  // uninstall one has no place in a picture.
  for (const command of ['send_message', 'save_session', 'trash_session', 'download_model', 'stop_download', 'set_gate', 'set_makers', 'add_outside', 'remove_outside', 'refresh_outside', 'set_model_active', 'uninstall_model']) {
    await assert.rejects(invoke(command, {}), /is not part of the pictures/);
  }

  // A page with no storage (a blank one) still gets the stand-in.
  const blank = {
    get localStorage() {
      throw new Error('no storage here');
    },
  };
  vm.runInNewContext(standIn(engine, sessions), { window: blank, structuredClone });
  assert.equal(typeof blank.__TAURI_INTERNALS__.invoke, 'function');
});

test('`make screenshots` builds the page, then makes the pictures', () => {
  const makefile = read('Makefile');
  const target = /^screenshots:.*## .+\n((?:\t.+\n)+)/m.exec(makefile);
  assert.ok(target, 'the Makefile has a screenshots command that `make` lists');

  const steps = target[1];
  assert.ok(
    steps.indexOf('npm run build') >= 0 && steps.indexOf('npm run build') < steps.indexOf('node scripts/screenshots.mjs'),
    'the page is built before the script runs',
  );

  const phony = /^\.PHONY:((?:.*\\\n)*.*)$/m.exec(makefile);
  assert.match(phony[1], /\bscreenshots\b/, 'screenshots is in .PHONY');
});

test('the helpers read pictures from a page, and sizes from a PNG', () => {
  const page = [
    '![Plain](a.png)',
    '[![Linked](b.png)](b.png?raw=true)',
    '| [![In a table](c/d.png)](elsewhere) | [a link](e.png) |',
  ].join('\n');
  assert.deepEqual(shownIn(page), [
    { alt: 'Plain', src: 'a.png', link: null },
    { alt: 'Linked', src: 'b.png', link: 'b.png?raw=true' },
    { alt: 'In a table', src: 'c/d.png', link: 'elsewhere' },
  ]);

  assert.equal(fromRoot('docs/screenshots/README.md', '../chat-window.webp'), 'docs/chat-window.webp');
  assert.equal(fromRoot('README.md', 'docs/screenshots/chat.png'), 'docs/screenshots/chat.png');
  assert.equal(sectionOf('# A\n\n## One\nfirst\n\n## Two\nsecond\n', 'One'), 'first\n');
  assert.equal(sectionOf('# A\n\n## One\nfirst\n', 'Three'), null);

  const png = Buffer.alloc(24);
  Buffer.from('89504e470d0a1a0a', 'hex').copy(png, 0);
  png.writeUInt32BE(13, 8);
  png.write('IHDR', 12, 'latin1');
  png.writeUInt32BE(1840, 16);
  png.writeUInt32BE(1280, 20);
  assert.deepEqual(pngSize(png), { width: 1840, height: 1280 });
  assert.equal(pngSize(Buffer.from('RIFF....WEBPVP8 ........')), null);
  assert.equal(pngSize(Buffer.alloc(4)), null);
});
