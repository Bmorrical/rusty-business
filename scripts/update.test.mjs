// The notice of a new version, checked on every test run.
//
// It answers the ask that started it: "when a new build is ready, the user is
// notified in app, and it gives them the download".
//
// So these fail when the window stops asking for a new version when it opens
// and once a day, when the notice loses its Download button or the steps
// that follow it, or when the check reaches further than it says: past GitHub's API, past this
// repository's newest release, through a proxy or a redirect, with an
// account, a token, or a cookie. They also fail when the app starts writing,
// downloading, or installing anything itself, when the page gains an address
// of its own to open, or when the README, the security policy, and the
// roadmap stop saying what the check does.
//
// The checks read the files as text, so nothing has to be installed to run
// them. The Rust tests in update.rs check how the code behaves.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const UPDATE = 'apps/desktop/src-tauri/src/update.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';
const APP = 'apps/desktop/src/App.vue';
const CARGO = 'apps/desktop/src-tauri/Cargo.toml';
const TAURI = 'apps/desktop/src-tauri/tauri.conf.json';
const CAPABILITY = 'apps/desktop/src-tauri/capabilities/default.json';
const REPOSITORY = 'Bmorrical/rusty-business';

/** The Rust tests that keep the notice's rules. */
const KEPT_BY = [
  'a_newer_release_with_this_systems_file_is_a_new_version',
  'the_same_or_an_older_release_is_not',
  'versions_compare_as_numbers',
  'a_release_without_this_systems_file_is_not_yet',
  'a_draft_a_prerelease_or_an_odd_tag_is_not',
  'each_system_gets_its_own_file_and_steps',
  'the_check_asks_only_githubs_api_for_this_repositorys_newest_release',
  'a_redirect_is_never_followed',
  'an_answer_is_read_up_to_its_size_and_no_further',
  'only_this_repositorys_releases_are_opened',
  'a_check_that_fails_keeps_what_the_last_one_found',
];

/** What would let the check carry an account, or write or install anything. */
const NEVER_IN_UPDATE = [
  'std::fs',
  'File::create',
  'OpenOptions',
  'set_permissions',
  'cookie_store',
  'cookie_provider',
  'bearer_auth',
  'basic_auth',
  'AUTHORIZATION',
  'Authorization',
  'env::var',
  'GITHUB_TOKEN',
  'http://',
];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** A Rust file's code above its tests, without its comment lines. */
export function codeOf(rust) {
  const at = rust.indexOf('#[cfg(test)]');
  return (at < 0 ? rust : rust.slice(0, at))
    .split('\n')
    .filter((line) => !/^\s*\/\//.test(line))
    .join('\n');
}

/** Every https address a Rust file builds or names, as written. */
export function httpsIn(rust) {
  return [...rust.matchAll(/https:\/\/[^\s"`]*/g)].map((match) => match[0]);
}

test('the window asks for a new version when it opens and once a day, and stops asking when it closes', () => {
  const app = read(APP);
  assert.match(app, /await invoke\("check_update"\)/, `${APP} no longer asks for a new version`);
  assert.match(app, /onMounted\([\s\S]*?checkUpdate\(\);[\s\S]*?\}\);/, `${APP} no longer asks when the window opens`);
  assert.match(app, /updateTimer = window\.setInterval\(checkUpdate, UPDATE_EVERY\);/, `${APP} no longer asks once a day`);
  assert.match(app, /onBeforeUnmount\([\s\S]*?window\.clearInterval\(updateTimer\);[\s\S]*?\}\);/, `${APP} keeps asking after it closes`);
});

test('the notice gives the download, how to install it, the notes, and a way to put it off', () => {
  const app = read(APP);
  assert.match(app, /<section v-if="showsUpdate\(update, updateLater\)" class="update" role="status"/, `${APP} has no notice of a new version`);
  assert.ok(app.includes(`@click="openUpdate('download')">Download</button>`), `${APP} has no Download button`);
  assert.ok(app.includes(`@click="openUpdate('notes')">What changed</button>`), `${APP} has no What changed button`);
  assert.ok(app.includes('@click="updateLater = update.version">Later</button>'), `${APP} has no Later button`);
  // Once Download has opened the file, the notice says how to install it.
  assert.ok(app.includes('if (part === "download") updateOpened.value = update.value?.version ?? "";'), `${APP} no longer marks the download as opened`);
  assert.ok(app.includes('<template v-if="updateOpened === update.version">'), `${APP} no longer shows the steps after Download`);
  assert.ok(app.includes('<p class="small">{{ update.steps }}</p>'), `${APP} no longer says how to install the new version`);
  assert.ok(app.includes('<code v-if="update.command" class="hint small">{{ update.command }}</code>'), `${APP} no longer shows the command that installs it`);
  assert.ok(app.includes('If no download started, it is on {{ update.page }}.'), `${APP} no longer says where the download is`);
  // A browser that does not open is said so.
  assert.ok(app.includes('    updateProblem.value = String(error);\n'), `${APP} no longer keeps why the browser did not open`);
  assert.ok(app.includes('<p v-if="updateProblem" class="update-problem small" role="alert">{{ updateProblem }}</p>'), `${APP} no longer says the browser did not open`);
  // The page names which part, never an address.
  assert.deepEqual([...app.matchAll(/invoke\("open_update", ([^)]*)\)/g)].map((match) => match[1]), ['{ part }']);
  assert.doesNotMatch(app, /github\.com/i, `${APP} names an address on GitHub, where only Rust should`);
});

test('the two commands are on the list the page may call, with what the last check found', () => {
  const lib = read(LIB);
  assert.match(lib, /^mod update;$/m);
  assert.ok(lib.includes('.manage(update::Updates::default())'), `${LIB} keeps nothing of what the check found`);
  assert.ok(lib.includes('update::check_update,'), `${LIB} does not list check_update`);
  assert.ok(lib.includes('update::open_update,'), `${LIB} does not list open_update`);
});

test("the check asks only GitHub's API, for this repository's newest release, with no proxy, redirect, or account", () => {
  const code = codeOf(read(UPDATE));
  assert.ok(code.includes(`const REPOSITORY: &str = "${REPOSITORY}";`), `${UPDATE} follows another repository`);
  assert.ok(code.includes('const API_HOST: &str = "api.github.com";'), `${UPDATE} asks another host`);
  assert.ok(code.includes('const SITE_HOST: &str = "github.com";'), `${UPDATE} opens another host`);
  assert.deepEqual(
    httpsIn(code),
    [
      'https://{API_HOST}/repos/{REPOSITORY}/releases/latest',
      'https://{SITE_HOST}/{REPOSITORY}/releases/download/v{}/{}',
      'https://{SITE_HOST}/{REPOSITORY}/releases/tag/v{}',
    ],
    `${UPDATE} reaches another address`,
  );
  for (const rule of ['.no_proxy()', '.https_only(true)', '.redirect(reqwest::redirect::Policy::none())', '.referer(false)']) {
    assert.ok(code.includes(rule), `${UPDATE} has lost ${rule}`);
  }
  assert.match(code, /read_capped\(chunks, MOST_BYTES\)/, `${UPDATE} reads an answer of any size`);
  for (const word of NEVER_IN_UPDATE) {
    assert.ok(!code.includes(word), `${UPDATE} holds "${word}"`);
  }
});

test('the app opens the browser and nothing else, and installs nothing itself', () => {
  const code = codeOf(read(UPDATE));
  assert.equal([...code.matchAll(/Command::new\(/g)].length, 1, `${UPDATE} starts another program`);
  for (const opener of ['"/usr/bin/open"', '"rundll32.exe", &["url.dll,FileProtocolHandler"]', '"xdg-open"']) {
    assert.ok(code.includes(opener), `${UPDATE} no longer opens the browser with ${opener}`);
  }
  assert.match(code, /let url = address\(&update, part\)\?;\n\s+let mut child = browser\(&url\)/, `${UPDATE} opens an address it did not check`);
  assert.doesNotMatch(read(CARGO), /tauri-plugin-(?:updater|opener|shell|process)/, `${CARGO} brings in a plugin that could install or run things`);
  assert.equal(JSON.parse(read(TAURI)).plugins?.updater, undefined, `${TAURI} turns on Tauri's updater`);
  assert.deepEqual(JSON.parse(read(CAPABILITY)).permissions, ['core:default'], `${CAPABILITY} lets the page do more`);
});

test('each rule of the notice is kept by a test in the Rust', () => {
  const rust = read(UPDATE);
  for (const name of KEPT_BY) {
    assert.match(rust, new RegExp(`#\\[test\\]\\s+fn ${name}\\(`), `${UPDATE} has lost the test ${name}`);
  }
  assert.ok(rust.includes('"when a new build is ready, the user is\n// notified in app, and it gives them the download"'), `${UPDATE} no longer quotes the ask`);
});

test('the README, the security policy, and the roadmap say what the check does', () => {
  const readme = read('README.md');
  assert.match(readme, /once a day while it stays open, the app asks GitHub\s+whether a new version is out\./, 'the README no longer says when the app asks GitHub');
  assert.match(readme, /^- \*\*A new version is a download\.\*\*/m, "the README's limits no longer say a new version is a download");
  assert.match(readme, /There\s+is\s+no\s+switch\s+to\s+turn\s+the\s+check\s+off\./, 'the README no longer says the check has no switch');
  const security = read('SECURITY.md').replace(/\s+/g, ' ');
  assert.ok(
    security.includes("The check for a new version asks GitHub's API for this repository's newest release, over https, and nothing else."),
    'SECURITY.md no longer promises what the check asks',
  );
  assert.ok(security.includes('follows no redirect, and downloads and installs nothing'), 'SECURITY.md no longer promises the check installs nothing');
  assert.match(read('ROADMAP.md'), /^- \*\*A notice of each new version\.\*\*/m, 'the roadmap no longer lists the notice as done');
  assert.ok(read('apps/desktop/README.md').includes('[src-tauri/src/update.rs](src-tauri/src/update.rs)'), "the desktop app's README no longer names update.rs");
  assert.match(read('apps/desktop/src-tauri/src/README.md'), /^\| `update\.rs` +\|/m, "the Rust folder's README no longer names update.rs");
});

test('code and addresses are read from the text', () => {
  assert.equal(codeOf('fn a() {}\n#[cfg(test)]\nmod tests {}'), 'fn a() {}\n');
  assert.equal(codeOf('// a cookie\n/// a token\nfn a() {}'), 'fn a() {}');
  assert.deepEqual(httpsIn('let a = "https://x/y"; // see https://z\nformat!("https://{H}/{R}")'), ['https://x/y', 'https://z', 'https://{H}/{R}']);
});
