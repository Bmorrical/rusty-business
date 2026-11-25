// The notice of a new version, checked on every test run.
//
// It answers the ask that started it: "when a new build is ready, the user is
// notified in app, and it gives them the download". Then a button to ask now:
// "is the user able to click a refresh button? If not, I want that.", and
// "check for updates I imply". Then Install and restart: "when can we do an
// in app install rather than a new dmg download? is that after the signing?
// if we can do it now, then roll that next".
//
// So these fail when the window stops asking for a new version when it opens
// and once a day, when the notice loses its Download button or the steps
// that follow it, when the Check for updates button stops asking now or
// saying what it found, or when the check reaches further than it says: past
// GitHub's API, past this repository's newest release, through a proxy or a
// redirect, with an account, a token, or a cookie. They fail when Install and
// restart downloads before it is clicked, fetches anything but the release it
// found, follows a redirect off GitHub, or installs without Tauri's updater
// checking the signature against the key in tauri.conf.json. They also fail
// when the app writes anything itself, when the page gains an address of its
// own to open or a way to install by itself, or when the README, the security
// policy, and the roadmap stop saying what the check and the install do.
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
  'a_check_that_fails_says_why_and_keeps_what_the_last_one_found',
  'too_many_checks_are_said_plainly',
  'a_release_with_this_systems_package_and_latest_json_installs',
  'only_a_copy_put_in_place_installs_over_itself',
  'install_waits_for_the_gate_a_version_found_and_a_copy_that_installs',
  'one_install_at_a_time',
  'the_files_come_only_from_the_release_found',
  'a_file_follows_github_and_nothing_else',
  'a_file_sent_off_github_is_refused',
  'the_gate_stops_an_install_before_and_during_its_download',
  'a_failed_install_says_why_and_that_download_still_works',
];

/** What would let the check or the install carry an account, or write anything. */
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

test('"check for updates": a button asks now and says what it found', () => {
  const app = read(APP);
  // The button, which waits while a check is under way.
  assert.ok(
    app.includes('<button type="button" role="menuitem" :disabled="updateChecking" @click="checkNow">\n              {{ updateChecking ? "Checking..." : "Check for updates" }}\n            </button>'),
    `${APP} has no Check for updates button`,
  );
  assert.match(app, /async function checkNow\(\) \{\n  if \(!inApp \|\| updateChecking\.value\) return;\n  updateChecking\.value = true;/, `${APP} asks again while a check is under way`);
  assert.match(app, /\} finally \{\n    updateChecking\.value = false;\n  \}\n\}/, `${APP} leaves the button waiting after a check`);
  // What it found: a new version, which comes back even after Later, that
  // this is the newest, or why the check failed.
  const checkNow = app.slice(app.indexOf('async function checkNow()'), app.indexOf('async function openUpdate('));
  assert.ok(checkNow.includes('    update.value = await invoke("check_update");\n    updateLater.value = "";\n'), `${APP} keeps a notice hidden by Later after Check for updates finds it`);
  assert.ok(checkNow.includes('updateChecked.value = checkedLine(update.value, await getVersion());'), `${APP} no longer says what Check for updates found`);
  assert.ok(checkNow.includes('    updateChecked.value = String(error);\n'), `${APP} no longer says why Check for updates failed`);
  assert.ok(app.includes('<p v-if="updateChecked" class="small">{{ updateChecked }}</p>'), `${APP} no longer shows what Check for updates found`);
  assert.ok(app.includes('import { getVersion } from "@tauri-apps/api/app";'), `${APP} no longer reads this version from the app`);
  // A failed check answers why, and the button is the only one that shows it.
  assert.ok(codeOf(read(UPDATE)).includes('    updates.keep(checked)\n}'), `${UPDATE} no longer answers why a check failed`);
  assert.match(app, /async function checkUpdate\(\) \{\n  if \(!inApp\) return;\n  try \{\n    update\.value = await invoke\("check_update"\);\n  \} catch \{/, `${APP} shows a failed daily check`);
});

// Brad, 2026-11-17: "for update, it should tell them to enable the gate to
// check for updates."
test('with the gate off, Check for updates says to turn the gate on, and the line goes once it is on', () => {
  const update = codeOf(read(UPDATE));
  assert.ok(update.includes('const GATE_OFF: &str = "The WAN gate is off. Turn it on to check for updates.";'), `${UPDATE} no longer says to turn the gate on`);
  assert.match(update, /fn client\(gate: &Gate\)[^{]*\{\s+gate\.client\(GATE_OFF,/, `${UPDATE} checks without the gate`);
  const app = read(APP);
  const setGate = app.slice(app.indexOf('async function setGate('), app.indexOf('async function', app.indexOf('async function setGate(') + 1));
  assert.match(setGate, /if \(gate\.value\) \{\n    updateChecked\.value = "";\n    checkStack\(\);\n    checkUpdate\(\);/, `${APP} keeps saying the gate is off after it is turned on`);
});

test('the three commands are on the list the page may call, with what the last check found', () => {
  const lib = read(LIB);
  assert.match(lib, /^mod update;$/m);
  assert.ok(lib.includes('.manage(update::Updates::default())'), `${LIB} keeps nothing of what the check found`);
  assert.ok(lib.includes('update::check_update,'), `${LIB} does not list check_update`);
  assert.ok(lib.includes('update::open_update,'), `${LIB} does not list open_update`);
  assert.ok(lib.includes('update::install_update,'), `${LIB} does not list install_update`);
  // Tauri's updater is added with nothing of its own: no address and no key
  // but the one in tauri.conf.json.
  assert.ok(lib.includes('.plugin(tauri_plugin_updater::Builder::new().build())'), `${LIB} does not add Tauri's updater as it is`);
  assert.equal([...lib.matchAll(/\.plugin\(/g)].length, 1, `${LIB} adds another plugin`);
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
      'https://{SITE_HOST}/{REPOSITORY}/releases/download/v{version}/{file}',
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

test('the app opens the browser and nothing else, and installs only through Tauri\'s updater', () => {
  const code = codeOf(read(UPDATE));
  assert.equal([...code.matchAll(/Command::new\(/g)].length, 1, `${UPDATE} starts another program`);
  for (const opener of ['"/usr/bin/open"', '"rundll32.exe", &["url.dll,FileProtocolHandler"]', '"xdg-open"']) {
    assert.ok(code.includes(opener), `${UPDATE} no longer opens the browser with ${opener}`);
  }
  assert.match(code, /let url = to_open\(part, &updates, &gate\)\?;\n\s+let mut child = browser\(&url\)/, `${UPDATE} opens an address it did not check`);
  assert.match(code, /fn to_open\([^)]*\) -> Result<reqwest::Url, String> \{[^}]*\}[^}]*address\(&update, part\)\n\}/, `${UPDATE} opens an address it did not check`);
  // Tauri's updater is the one plugin, with its own TLS left to the system,
  // as the app's is.
  const cargo = read(CARGO);
  assert.deepEqual([...cargo.matchAll(/^(tauri-plugin-[a-z-]+)/gm)].map((match) => match[1]), ['tauri-plugin-updater'], `${CARGO} brings in a plugin that could run things`);
  assert.ok(cargo.includes('tauri-plugin-updater = { version = "2.14", default-features = false, features = ["native-tls"] }'), `${CARGO} changes how Tauri's updater is built`);
  // Its settings hold the public key and nothing else: no address of its own,
  // and nothing that lets it use http.
  const updater = JSON.parse(read(TAURI)).plugins?.updater;
  assert.deepEqual(Object.keys(updater ?? {}), ['pubkey'], `${TAURI} gives Tauri's updater more than its key`);
  assert.match(Buffer.from(updater.pubkey, 'base64').toString('utf8'), /^untrusted comment: minisign public key: [0-9A-F]{16}\n[A-Za-z0-9+/=]{56}\n?$/, `${TAURI} holds no minisign public key`);
  // The page can call none of the plugin's own commands.
  assert.deepEqual(JSON.parse(read(CAPABILITY)).permissions, ['core:default'], `${CAPABILITY} lets the page do more`);
  // The install fetches through the gate, lets Tauri's updater check the
  // signature, and installs only after that check.
  const install = code.slice(code.indexOf('pub async fn install_update('));
  assert.ok(install.includes('let (update, package) = to_install(&updates, &gate, this_download())?;'), `${UPDATE} installs without checking the gate, the version found, or this copy`);
  assert.ok(install.includes('.start_install()'), `${UPDATE} runs two installs at once`);
  assert.equal([...code.matchAll(/\.updater_builder\(\)/g)].length, 1, `${UPDATE} builds Tauri's updater twice`);
  assert.ok(install.includes('.endpoints(vec![manifest_url])'), `${UPDATE} lets Tauri's updater ask another address`);
  assert.ok(install.includes('.restore_update(manifest)'), `${UPDATE} no longer hands Tauri's updater the latest.json it fetched`);
  assert.ok(install.includes('signed_matches('), `${UPDATE} installs a file latest.json did not name`);
  const verify = install.indexOf('.verify(&file)');
  const put = install.indexOf('signed.install(file)');
  assert.ok(verify > 0 && put > verify, `${UPDATE} installs before the signature is checked`);
  assert.doesNotMatch(code, /\.check\(\)|download_and_install|\.download\(/, `${UPDATE} lets Tauri's updater reach out by itself`);
  // A file is fetched only through the gate, from GitHub, and up to a size.
  assert.match(code, /fn fetch_through\([^)]*\)[^{]*\{\n\s+let client = file_client\(gate\)\?;\n\s+until_turned_off\(gate, fetch\(client, url, most\)\)\.await\n\}/, `${UPDATE} fetches without the gate`);
  assert.match(code, /fn file_client\(gate: &Gate\)[^{]*\{\s+gate\.client\(GATE_OFF_INSTALL,/, `${UPDATE} fetches without the gate`);
  for (const rule of ['.no_proxy()', '.https_only(true)', '.redirect(file_redirects())', '.referer(false)']) {
    assert.ok(code.slice(code.indexOf('fn file_client(')).includes(rule), `${UPDATE}'s file client has lost ${rule}`);
  }
  assert.ok(code.includes('followed < MOST_HOPS && from_github(next)'), `${UPDATE} follows a redirect off GitHub`);
  assert.ok(code.includes('    if !from_github(response.url()) {\n        return Err(SENT_ELSEWHERE.to_string());'), `${UPDATE} keeps a file that came from off GitHub`);
  assert.ok(code.includes('read_capped(chunks, most)'), `${UPDATE} reads a file of any size`);
});

test('each rule of the notice is kept by a test in the Rust', () => {
  const rust = read(UPDATE);
  for (const name of KEPT_BY) {
    assert.match(rust, new RegExp(`#\\[test\\]\\s+fn ${name}\\(`), `${UPDATE} has lost the test ${name}`);
  }
  assert.ok(rust.includes('"when a new build is ready, the user is\n// notified in app, and it gives them the download"'), `${UPDATE} no longer quotes the ask`);
  assert.ok(rust.includes('("is the user able to click a refresh button? If not, I want that.",\n// then "check for updates I imply")'), `${UPDATE} no longer quotes the ask for the button`);
  assert.ok(
    rust.replace(/\n\/\/ /g, ' ').includes('"when can we do an in app install rather than a new dmg download? is that after the signing? if we can do it now, then roll that next"'),
    `${UPDATE} no longer quotes the ask for Install and restart`,
  );
});

// Brad, 2026-11-19: "when can we do an in app install rather than a new dmg
// download? is that after the signing? if we can do it now, then roll that
// next", then "Click to install": nothing downloads until the button is
// clicked, and Download stays beside it.
test('Install and restart downloads nothing until it is clicked, and Download stays beside it', () => {
  const app = read(APP);
  assert.ok(
    app.includes('<button v-if="update.install" class="primary" type="button" :disabled="updateInstalling" @click="installUpdate">\n            {{ installButton(updateInstalling) }}\n          </button>'),
    `${APP} has no Install and restart button`,
  );
  assert.ok(app.includes(`<button :class="{ primary: !update.install }" type="button" @click="openUpdate('download')">Download</button>`), `${APP} no longer keeps Download beside Install and restart`);
  // The page names nothing to install, and only the button asks.
  assert.deepEqual([...app.matchAll(/invoke\("install_update"([^)]*)\)/g)].map((match) => match[1]), [''], `${APP} names what to install, or asks more than once`);
  assert.deepEqual(
    [...app.matchAll(/^.*\binstallUpdate\b.*$/gm)].map((match) => match[0].trim()),
    [
      'async function installUpdate() {',
      '<button v-if="update.install" class="primary" type="button" :disabled="updateInstalling" @click="installUpdate">',
    ],
    `${APP} installs without the button being clicked`,
  );
  const installUpdate = app.slice(app.indexOf('async function installUpdate()'), app.indexOf('async function', app.indexOf('async function installUpdate()') + 1));
  assert.ok(installUpdate.includes('await invoke("install_update");'), `${APP} installs from somewhere other than its button`);
  assert.match(installUpdate, /if \(!inApp \|\| updateInstalling\.value\) return;\n  updateInstalling\.value = true;/, `${APP} starts a second install while one is under way`);
  assert.ok(installUpdate.includes('    updateProblem.value = String(error);\n'), `${APP} no longer says why an install failed`);
  assert.match(installUpdate, /\} finally \{\n    updateInstalling\.value = false;\n  \}/, `${APP} leaves the button waiting after a failed install`);
});

// Brad, 2026-11-17: "for update, it should tell them to enable the gate to
// check for updates."
test('the README, the security policy, and the roadmap say what the check does', () => {
  const readme = read('README.md');
  const words = readme.replace(/\s+/g, ' ');
  assert.ok(words.includes('once a day while it stays open, and when you click Check for updates, the app asks GitHub whether a new version is out.'), 'the README no longer says when the app asks GitHub');
  assert.ok(words.includes('Check for updates, under the saved chats, asks at any time.'), "the README's limits no longer name Check for updates");
  assert.match(readme, /^- \*\*Some copies update by download\.\*\*/m, "the README's limits no longer say which copies update by download");
  assert.ok(words.includes('Install and restart puts the new version in its place and opens it'), 'the README no longer says what Install and restart does');
  assert.ok(
    words.includes('While the outbound gate is off, the app does not ask, and the button says to turn the gate on.'),
    'the README no longer says the check waits for the outbound gate',
  );
  const security = read('SECURITY.md').replace(/\s+/g, ' ');
  assert.ok(
    security.includes("The check for a new version asks GitHub's API for this repository's newest release, over https, and nothing else."),
    'SECURITY.md no longer promises what the check asks',
  );
  assert.ok(security.includes('follows no redirect, and downloads nothing'), 'SECURITY.md no longer promises the check downloads nothing');
  assert.ok(
    security.includes('It installs that file only when its signature matches the public key built into the app'),
    'SECURITY.md no longer promises the install checks the signature',
  );
  const roadmap = read('ROADMAP.md');
  assert.match(roadmap, /^- \*\*A notice of each new version\.\*\*/m, 'the roadmap no longer lists the notice as done');
  // Brad, 2026-11-19: "if we can do it now, then roll that next".
  const done = roadmap.slice(roadmap.indexOf('\n## Done\n'));
  assert.match(done, /^- \*\*Install and restart\.\*\*/m, 'the roadmap no longer lists Install and restart as done');
  assert.ok(!roadmap.replace(/\s+/g, ' ').includes('It installs nothing itself.'), 'the roadmap still says the app installs nothing itself');
  assert.ok(roadmap.replace(/\s+/g, ' ').includes('installs the file only when its signature matches the key built into the app'), 'the roadmap no longer says the install checks the signature');
  assert.ok(read('apps/desktop/README.md').includes('[src-tauri/src/update.rs](src-tauri/src/update.rs)'), "the desktop app's README no longer names update.rs");
  assert.match(read('apps/desktop/src-tauri/src/README.md'), /^\| `update\.rs` +\|/m, "the Rust folder's README no longer names update.rs");
});

test('code and addresses are read from the text', () => {
  assert.equal(codeOf('fn a() {}\n#[cfg(test)]\nmod tests {}'), 'fn a() {}\n');
  assert.equal(codeOf('// a cookie\n/// a token\nfn a() {}'), 'fn a() {}');
  assert.deepEqual(httpsIn('let a = "https://x/y"; // see https://z\nformat!("https://{H}/{R}")'), ['https://x/y', 'https://z', 'https://{H}/{R}']);
});
