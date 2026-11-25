// The Installers page, as the page shows it, checked on every test run.
//
// It answers the ask that started it: "one click from an installers page would
// be good". Each model on the app's list is one Install button away, whoever
// its maker is, and the outside engines are added there too. Install
// downloads the model and then adds its maker, so the model shows at the top
// of the window.
//
// Rust checks every file as it arrives, and its tests in engine.rs and
// makers.rs check that. These fail when the side bar loses its Installers
// button, when the page stops listing every maker or stops asking Rust
// afresh, when Install skips the gate or adds a maker for a download that
// did not finish, when Settings starts downloading again, or when more than
// one page can show at a time.
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

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** One function's text in App.vue, up to the next one. */
function functionOf(app, name) {
  const start = app.search(new RegExp(`\\n(async )?function ${name}\\(`));
  assert.ok(start >= 0, `${APP} has no ${name}`);
  const end = app.slice(start + 1).search(/\n(async )?function /);
  return app.slice(start, end < 0 ? undefined : start + 1 + end);
}

/**
 * One part of the Settings page, from its marker to the next. Installers,
 * Utilities, and Settings' own tabs share the page since the gear came.
 */
const MARKERS = { installers: '<!-- Installers -->', utilities: '<!-- Utilities -->', settings: "<!-- Settings' own tabs -->" };
function pageOf(app, name) {
  const start = app.indexOf(MARKERS[name]);
  assert.ok(start >= 0, `${APP} has lost its ${name} page`);
  return app.slice(start, app.indexOf('<!-- ', start + 1));
}

test('Installers is one click from the Settings page, which the gear opens', () => {
  // Brad, 2026-11-17: "one click from an installers page would be good".
  // Todd, 2026-11-24: Installers and Utilities "feel like they should be part
  // of the Settings page".
  const app = read(APP);
  const foot = app.slice(app.indexOf('<div class="side-foot">'), app.indexOf('</aside>'));
  const settings = foot.indexOf(`@click="togglePage('settings')"`);
  assert.ok(settings >= 0, 'the gear has lost Settings');
  assert.ok(settings < foot.indexOf('Check for updates'), 'Settings no longer sits above Check for updates');
  assert.ok(foot.includes(`:aria-current="settingsPage ? 'page' : undefined"`), 'the gear no longer says Settings is open');
  for (const name of ['installers', 'utilities']) {
    assert.ok(!foot.includes(`togglePage('${name}')`), `${name} has its own button again, apart from Settings`);
  }

  const nav = app.slice(app.indexOf('<nav class="settings-tabs" aria-label="Settings">'), app.indexOf('</nav>', app.indexOf('aria-label="Settings">')));
  const order = ["openTab('settings', 'appearance')", "openTab('installers', 'chat')", "openTab('installers', 'outside')", "openTab('utilities')"].map((part) => nav.indexOf(part));
  assert.ok(order.every((at) => at > 0), "the Settings tabs have lost an installer or Utilities");
  assert.deepEqual([...order].sort((a, b) => a - b), order, "Settings' own tabs, the installers, and Utilities are out of order");
  assert.match(app, /<section v-if="settingsPage" class="settings" aria-labelledby="settings-title"/, 'Settings, Installers, and Utilities no longer share one page');

  const open = functionOf(app, 'openPage');
  assert.match(open, /await readMakers\(\);/, 'the page opens without asking Rust which models are here');
  assert.match(open, /await readOutside\(\);/, 'the page opens without asking Rust about the outside engines');
  assert.match(open, /if \(arriving\) nextTick\(\(\) => pageTitle\.value\?\.focus\(\)\);/, 'the page title no longer takes the focus, or takes it from the tabs');
  assert.match(open, /if \(name === "settings" && arriving\) \{\s+memoryDraft\.value = memory\.value;/, 'moving between tabs throws away a memory being written');
  assert.match(functionOf(app, 'togglePage'), /return open \? closePage\(\) : openPage\(name\);/, 'a second click no longer goes back to the chat');
  assert.match(functionOf(app, 'togglePage'), /name === "settings" \? settingsPage\.value/, 'Settings from the gear does not close an installer or Utilities');
  assert.match(functionOf(app, 'openTab'), /if \(page\.value !== where\) await openPage\(where\);/, 'a tab elsewhere on the page does not ask Rust afresh');
  assert.match(functionOf(app, 'closePage'), /page\.value = "chat";\s+focusComposer\(\);/, 'going back no longer returns to the chat');
});

test('the Installers page lists every maker, and installs any model in one click', () => {
  // Brad, 2026-11-17: "and claude and codex/gpt and gemmini and meta/google
  // models locally can be installed, one click from an installers page would
  // be good".
  const app = read(APP);
  const page = pageOf(app, 'installers');
  // Escape and Back to the chat are the shared page's, above every tab.
  const head = app.slice(app.indexOf('<section v-if="settingsPage"'), app.indexOf('<!-- Installers -->'));
  assert.ok(head.includes('@keydown.esc="closePage"'), 'Escape no longer goes back to the chat');
  assert.ok(head.includes('<button type="button" @click="closePage">Back to the chat</button>'), 'the page has lost Back to the chat');
  for (const part of [
    '<h3>{{ lanEngine ? "Models on your LAN server" : "Models to run here" }}</h3>',
    'Nothing downloads until you press Install.',
    '<span>Turn on the {{ installGateName }} to install a model.</span>',
    '<li v-for="maker in displayMakers" :key="maker.name" class="maker">',
    '<span v-if="!maker.picked" class="quiet small">Not added</span>',
    '<li v-for="entry in maker.models" :key="entry.name" class="offer">',
    '{{ installLine(maker, entry) }}',
    '<button type="button" :disabled="stopping" @click="cancelDownload">{{ lanEngine ? "Pause" : "Cancel" }}</button>',
    '<p v-if="makersNote" class="small" role="alert">{{ makersNote }}</p>',
    '<p v-if="stoppedNote" class="quiet small">{{ stoppedNote }}</p>',
  ]) {
    assert.ok(page.includes(part), `the Installers page has lost ${part}`);
  }
  // Every maker's models are offered, not only the makers already added.
  assert.ok(!/v-for="maker in makers\.filter|v-if="maker\.picked"/.test(page), 'the page lists only the makers already added');
  // Brad, 2026-11-17: "off by default, everything stops". Install is off
  // while the gate is, and while a download or a change of makers runs.
  assert.match(
    page,
    /v-else-if="entry\.offer && !entry\.onDisk"\s+type="button"\s+:aria-label="`Install \$\{entry\.name\}`"\s+:disabled="makersLocked \|\| !canDownload"\s+@click="install\(maker\.name, entry\.offer\)"\s*>\s*Install\s*</,
    'Install is no longer one click, or no longer waits for the gate',
  );
  // A model already here whose maker is not added is one click from showing.
  assert.match(
    page,
    /v-else-if="entry\.onDisk && !maker\.picked"\s+type="button"\s+:aria-label="`Show \$\{entry\.name\}`"\s+:disabled="makersLocked"\s+@click="setMaker\(maker\.name, true\)"\s*>\s*Show\s*</,
    'a model on this computer no longer shows in one click',
  );
  assert.ok(page.indexOf('v-else-if="entry.onDisk && !maker.picked"') < page.indexOf('v-else-if="entry.offer && !entry.onDisk"'), 'Show and Install are out of order');
});

test('Install adds the maker only once the whole model is here', () => {
  const app = read(APP);
  const install = functionOf(app, 'install');
  assert.match(install, /^\nasync function install\(maker, offer\) \{\n {2}if \(!\(await download\(offer\)\)\) return;\n {2}if \(!pickedMakers\(makers\.value\)\.includes\(maker\)\) await setMaker\(maker, true\);\n\}/, 'Install adds the maker for a download that stopped or failed, or adds one already added');
  const download = functionOf(app, 'download');
  assert.match(download, /if \(downloading\.value \|\| !canDownload\.value\) return false;/, 'a second or blocked download is taken as whole');
  assert.match(download, /let whole = false;/, 'a failed download is taken as whole');
  assert.match(download, /whole = report\.whole === true;/, 'a stopped download is taken as whole');
  assert.match(download, /\n {2}return whole;\n\}/, 'download() no longer says whether the model is whole');
});

test('the outside engines are added on the Installers page, and nowhere else', () => {
  // Brad, 2026-11-17: "and claude and codex/gpt and gemmini". GPT is added
  // the way Grok is, on a card of its own beside Grok's.
  const app = read(APP);
  const page = pageOf(app, 'installers');
  // Since 0.8.4 they have a tab of their own, so the heading opens it.
  assert.ok(app.includes(`@click="openTab('installers', 'outside')">Outside engines</button>`), 'the Installers page has lost its Outside engines tab');
  for (const part of [
    '<h3>Outside engines</h3>',
    'v-for="entry in outside"',
    '<form v-else class="key-path" @submit.prevent="addOutside(entry.id)">',
    'Path to your {{ entry.maker }} key file',
    '{{ entry.name }} <span class="quiet small">by {{ entry.maker }}</span>',
    '{{ outsideNotes[entry.id] }}',
    '{{ outsideNote }}',
    '<span>The WAN gate is off, so no outside engine can be asked.</span>',
  ]) {
    assert.ok(page.includes(part), `the Installers page has lost ${part}`);
  }
  assert.equal(app.split('class="key-path"').length, 2, 'the way to add an engine shows in more than one place');
  assert.equal(app.split('class="engine-card"').length, 2, 'the outside engines have cards in more than one place');
  // Each card is named for its engine, so a screen reader says whose it is.
  assert.ok(page.includes(':aria-labelledby="`${entry.id}-name`"') && page.includes(':id="`${entry.id}-name`"'), "a card no longer says whose it is");
  assert.ok(page.includes(':aria-label="`Remove ${entry.name}`"'), 'Remove no longer says which engine it removes');
});

test('Settings changes the makers and points to the Installers page, but downloads nothing', () => {
  const app = read(APP);
  const settings = pageOf(app, 'settings');
  for (const part of ['download(', 'install(', '<progress', 'cancelDownload', 'stoppedNote', 'key-path']) {
    // Uninstall, in Settings under Models, moves a model to the Trash; it is not an install.
    assert.ok(!settings.replaceAll('uninstall(', '').includes(part), `Settings has ${part} again; it belongs on the Installers page`);
  }
  assert.match(settings, /To install a model or add an outside engine, go to the\s+<button class="link in-text" type="button" @click="openPage\('installers'\)">Installers<\/button> page\./, 'Settings no longer points to the Installers page');
  assert.ok(app.includes('id="settings-title" ref="pageTitle" tabindex="-1">Settings</h2>'), 'the Settings title no longer takes the focus');

  // The first run points there too, for the models it does not offer and for
  // the outside engines.
  const first = app.slice(app.indexOf('<div v-if="offers.length" class="first-run">'), app.indexOf('<template v-else>', app.indexOf('class="first-run"')));
  assert.match(first, /More models, and the outside engines, are on the\s+<button class="link in-text" type="button" @click="openPage\('installers'\)">Installers<\/button> page\./, 'the first run no longer points to the Installers page');
});

test('one page shows at a time, and a chat takes the window back', () => {
  const app = read(APP);
  assert.ok(app.includes('const page = ref("chat");'), 'the window no longer opens on the chat');
  assert.ok(!/settingsOpen|settingsTitle|openSettings|closeSettings/.test(app), 'the old Settings switch is back beside the pages');
  assert.equal(app.split(`v-show="page === 'chat'"`).length, 4, 'the chat header, thread, and box no longer hide together');
  // Workflow, then the Settings page, whose installers come first.
  assert.equal(app.split(`v-if="page === '`).length, 3, 'a page was added or lost without this check changing');
  assert.equal(app.split('v-if="settingsPage"').length, 2, 'the Settings page shows in more than one place');
  for (const name of ['newChat', 'openSession']) {
    assert.match(functionOf(app, name), /page\.value = "chat";/, `${name} leaves a page open over the chat`);
  }
});
