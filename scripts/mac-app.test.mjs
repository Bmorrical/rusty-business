// What scripts/mac-app.sh does to build the Mac app with its engine inside,
// and the two commands that run it, `make app` and `make dmg`.
//
// Brad, 2026-11-16: "I should be able to just pull the .dmg or .exe from read
// me and install and go, one click ready."
//
// Brad, 2026-11-17: "this should be must easier for the user to boot up, run
// it in isolation, one command to rule it all, start a makefile"
//
// So these fail when the script reaches past its own folder: when it reads a
// setting from the account it runs in, writes to the home folder or the
// system, removes anything outside target/mac, downloads anything not pinned
// by its checksum, or installs a package not pinned by its hash. They also
// fail when the app stops finding the engine where the script puts it, or the
// Mac stops showing the name with its mark.
//
// Most checks read the files as text. One runs the script with a wrong
// argument, which stops it before it fetches or builds anything.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const read = (file) => readFileSync(path.join(REPO, file), 'utf8');

const SCRIPT = read('scripts/mac-app.sh');
const MAKEFILE = read('Makefile');
const CONF = JSON.parse(read('apps/desktop/src-tauri/tauri.conf.json'));
const INFO = read('apps/desktop/src-tauri/Info.plist');
const LIB_RS = read('apps/desktop/src-tauri/src/lib.rs');
const STARTER_RS = read('apps/desktop/src-tauri/src/starter.rs');
const PINS_IN = read('apps/desktop/engine/requirements.in');
const PINS = read('apps/desktop/engine/requirements.txt');
const GATE = read('.github/workflows/gate.yml');

const HEX = /^[0-9a-f]{64}$/;
const SYSTEM = ['/usr/bin', '/bin', '/usr/sbin', '/sbin'];

/**
 * The script's commands, one per entry: comments and blank lines taken out,
 * and a line that ends in a backslash joined to the next. The note inside the
 * engine's wrapper is a comment too, so it is left out with the rest.
 */
function commandsOf(text) {
  const commands = [];
  let current = '';
  for (const line of text.split('\n')) {
    if (!line.trim() || line.trim().startsWith('#')) continue;
    current += `${line.trim()} `;
    if (line.endsWith('\\')) {
      current = current.replace(/\\ $/, '');
      continue;
    }
    commands.push(current.trim());
    current = '';
  }
  return commands;
}

const COMMANDS = commandsOf(SCRIPT);

/** The value the script gives a NAME=value line of its own, as written. */
function valueOf(name) {
  const line = COMMANDS.find((command) => command.startsWith(`${name}=`));
  assert.ok(line, `the script sets no ${name}`);
  return line.slice(name.length + 1);
}

/** The commands that contain `pattern`. */
const commandsWith = (pattern) => COMMANDS.filter((command) => pattern.test(command));

/** Where in the script the first command that contains `pattern` is. */
const firstAt = (pattern) => COMMANDS.findIndex((command) => pattern.test(command));

test('make app and make dmg run the one script, and make lists them', () => {
  for (const goal of ['app', 'dmg']) {
    const rule = new RegExp(`^${goal}: ## .+\\n\\t@scripts/mac-app\\.sh ${goal}$`, 'm');
    assert.match(MAKEFILE, rule, `make ${goal} does not run scripts/mac-app.sh ${goal}`);
  }
  const phony = /^\.PHONY:((?:.*\\\n)*.*)$/m.exec(MAKEFILE)[1].split(/[\s\\]+/);
  for (const goal of ['app', 'dmg', 'engine-pins']) assert.ok(phony.includes(goal), `.PHONY leaves out ${goal}`);
  assert.match(MAKEFILE, /^engine-pins: ## /m, 'make does not list engine-pins');
});

test('a wrong argument stops the script before it fetches or builds anything', () => {
  for (const args of [[], ['install'], ['App']]) {
    const run = spawnSync('/bin/bash', [path.join(REPO, 'scripts/mac-app.sh'), ...args], { encoding: 'utf8' });
    assert.equal(run.status, 1, `scripts/mac-app.sh ${args.join(' ')} did not stop`);
    assert.match(run.stderr, /^Say what to build: scripts\/mac-app\.sh app, or scripts\/mac-app\.sh dmg\.$/m);
  }
  // Nothing that fetches, builds, or writes comes before that check.
  const check = firstAt(/^case "\$\{1:-\}" in/);
  assert.ok(check > 0, 'the script does not check what it was asked to build');
  for (const work of [/\bmkdir\b/, /\bcurl\b/, /\bfetch "/, /\brm\b/, /\bnpm\b/, /\bcargo\b/]) {
    const at = firstAt(work);
    assert.ok(at === -1 || at > check, `${work} comes before the argument is checked`);
  }
});

test('it runs again with nothing from the account but who it is', () => {
  assert.match(SCRIPT, /^set -euo pipefail\ncd "\$\(dirname "\$0"\)\/\.\."$/m, 'the script does not stop at the first error, or does not start at the root');
  const again = commandsWith(/\/usr\/bin\/env -i MAC_APP_CLEAN=1/);
  assert.equal(again.length, 1, 'the script does not run itself again under env -i');
  assert.ok(COMMANDS.indexOf(again[0]) < firstAt(/^fail\(\)/), 'the script does work before it runs itself again');
  // Always, until the run that is already clean.
  const at = COMMANDS.indexOf(again[0]);
  assert.deepEqual(COMMANDS.slice(at - 1, at + 2), ['if [ "${MAC_APP_CLEAN:-}" != 1 ]; then', again[0], 'fi'], 'the script may skip running itself again');

  const kept = [...again[0].matchAll(/\b([A-Z_]+)=/g)].map((match) => match[1]);
  assert.deepEqual(kept, ['MAC_APP_CLEAN', 'HOME', 'USER', 'LOGNAME', 'TMPDIR', 'LANG', 'PATH']);
  assert.match(again[0], / PATH=\/usr\/bin:\/bin:\/usr\/sbin:\/sbin \/bin\/bash scripts\/mac-app\.sh "\$@"$/);
});

test('every tool and cache it makes stays inside target/mac, which git ignores', () => {
  assert.equal(valueOf('TAURI'), '$DESKTOP/src-tauri');
  assert.equal(valueOf('WORK'), '$(pwd -P)/$TAURI/target/mac');
  assert.match(read('apps/desktop/src-tauri/.gitignore'), /^\/target\/$/m);

  const homes = {
    RUSTUP_HOME: '$WORK/tools/rustup',
    CARGO_HOME: '$WORK/tools/cargo',
    RUSTUP_TOOLCHAIN: '$RUST',
    npm_config_cache: '$WORK/tools/npm-cache',
    npm_config_userconfig: '$WORK/tools/npmrc',
  };
  for (const [name, value] of Object.entries(homes)) {
    assert.ok(COMMANDS.includes(`export ${name}=${value}`), `${name} is not ${value}`);
  }
  assert.match(SCRIPT, /--cache-dir "\$WORK\/tools\/pip-cache"/, "pip's cache is not in target/mac");
  assert.match(SCRIPT, /rustup-init" -y --quiet --no-modify-path /, 'rustup may change the shell profile');
  assert.equal(valueOf('node_home'), '$WORK/tools/node-$NODE');

  // The account's home folder is passed on once, to the script run again, and never written.
  assert.deepEqual(commandsWith(/\$HOME\b|\$\{HOME\b|~\//), commandsWith(/\/usr\/bin\/env -i MAC_APP_CLEAN=1/));
  for (const command of commandsWith(/\bHOME=/)) {
    for (const [, value] of command.matchAll(/\bHOME="([^"]*)"/g)) {
      assert.ok(value === '$HOME' || value.startsWith('$WORK/'), `a HOME of ${value}`);
    }
  }
  for (const reach of [/\bsudo\b/, /\bbrew\b/, /\/usr\/local\b/, /\/opt\/homebrew\b/, /--user\b/, /\bnpm (install|i) -g\b/, /\bcargo install\b/]) {
    assert.deepEqual(commandsWith(reach), [], `the script reaches past its folder with ${reach}`);
  }

  // Each PATH is the system's own folders and the tools fetched into target/mac.
  for (const command of commandsWith(/\bPATH=/)) {
    for (const [, value] of command.matchAll(/\bPATH=(\S+)/g)) {
      for (const folder of value.split(':')) {
        assert.ok(SYSTEM.includes(folder) || ['$CARGO_HOME/bin', '$node_home/bin'].includes(folder), `${folder} is on a PATH`);
      }
    }
  }
});

test('it removes only what it made, inside target/mac', () => {
  const removals = commandsWith(/(^|[;&|(]\s*)rm(dir)?\s/);
  assert.ok(removals.length >= 6, 'the removals were not found');
  for (const command of removals) {
    const targets = command.replace(/^.*?\brm(dir)?\s+(-[a-z]+\s+)?/, '').split(/\s+/);
    for (const target of targets) {
      assert.ok(target.startsWith('"$WORK/'), `${command} removes ${target}`);
    }
  }
});

test('every download is over HTTPS and kept only when its checksum is the pinned one', () => {
  for (const name of ['RUST', 'RUSTUP', 'NODE']) assert.match(valueOf(name), /^\d+\.\d+\.\d+$/, `${name} is not an exact version`);
  assert.match(GATE, new RegExp(`node-version: ${valueOf('NODE').split('.')[0]}\\b`), 'the gate tests on another Node');

  const calls = [...SCRIPT.matchAll(/^\s*fetch "([^"]+)" "\$([A-Z0-9_]+)" ([a-z.-]+)$/gm)];
  assert.equal(calls.length, 3, 'expected three downloads: Node, rustup, and Python');
  for (const [, address, sum] of calls) {
    const expanded = address.replace(/\$([A-Z_]+)/g, (_, name) => valueOf(name));
    assert.match(expanded, /^https:\/\/(nodejs\.org|static\.rust-lang\.org|github\.com\/astral-sh\/python-build-standalone)\//, expanded);
    assert.doesNotMatch(expanded, /latest|stable|\$/, `${expanded} is not one fixed file`);
    assert.match(valueOf(sum), HEX, `${sum} is not a SHA-256`);
  }
  assert.match(valueOf('PYTHON_URL'), /\/download\/\d{8}\/cpython-3\.12\.\d+%2B\d{8}-aarch64-apple-darwin-install_only_stripped\.tar\.gz$/);

  // One curl, in fetch, over HTTPS only, and the file is checked before it is kept.
  assert.equal(commandsWith(/\bcurl\b/).length, 1);
  assert.deepEqual(commandsWith(/\b(wget|git clone)\b/), []);
  const fetch = /^fetch\(\) \{\n([\s\S]*?)\n\}$/m.exec(SCRIPT)[1];
  assert.match(fetch, /curl --fail .*--proto '=https' --proto-redir '=https' --tlsv1\.2/);
  const kept = fetch.indexOf('mv "$WORK/downloads/$3.part" "$WORK/downloads/$3"');
  const checked = fetch.indexOf('check "$2" "$WORK/downloads/$3.part" || fail');
  assert.ok(checked > 0 && kept > checked, 'a download is kept before its checksum is checked');
  assert.match(SCRIPT, /^check\(\) \{\n\s+\[ "\$\(shasum -a 256 "\$2" \| cut -c1-64\)" = "\$1" \]\n\}$/m);
});

test("the engine's packages are wheels for the oldest macOS, each pinned by its hash", () => {
  assert.equal(PINS_IN.trim(), 'mlx-lm==0.32.2');
  const pins = [...PINS.matchAll(/^([a-z0-9][a-z0-9._-]*)==(\S+) \\\n((?:\s+--hash=sha256:\S+(?: \\)?\n)+)/gim)];
  assert.ok(pins.some(([, name, version]) => name === 'mlx-lm' && version === '0.32.2'), 'mlx-lm 0.32.2 is not pinned');
  assert.ok(pins.length >= 30, `only ${pins.length} pins were found`);
  for (const [, name, version, hashes] of pins) {
    assert.match(version, /^\d[\w.]*$/, `${name} is not pinned to one version`);
    for (const hash of hashes.match(/sha256:\S+/g)) assert.match(hash.slice(7), HEX, `${name} has a bad hash`);
  }
  const named = PINS.split('\n').filter((line) => /^[a-z0-9]/i.test(line));
  assert.equal(named.length, pins.length, 'a package is listed without a hash');
  assert.doesNotMatch(PINS, /^\s*(-e|--index-url|--extra-index-url|--trusted-host|--find-links)\b|git\+|https?:\/\//m);

  const install = commandsWith(/-m pip install/);
  assert.equal(install.length, 1);
  for (const flag of [
    '"$python" -I -m pip install --isolated ',
    ' --require-hashes ',
    ' --only-binary :all: ',
    ' --index-url https://pypi.org/simple ',
    ' --platform "macosx_${MACOS/./_}_arm64" ',
    ' --target "$site" ',
    ' --requirement "$DESKTOP/engine/requirements.txt"',
  ]) {
    assert.ok(install[0].includes(flag), `pip is not given ${flag.trim()}`);
  }
  assert.doesNotMatch(install[0], /--extra-index-url|--trusted-host|--no-deps|--pre\b/);
  assert.match(MAKEFILE, new RegExp(`MACOSX_DEPLOYMENT_TARGET=${valueOf('MACOS').replace('.', '\\.')} uv pip compile`));
});

test("the oldest macOS is the app's own setting, and every program in the engine allows it", () => {
  assert.equal(valueOf('MACOS'), '14.0');
  assert.equal(CONF.bundle.macOS.minimumSystemVersion, valueOf('MACOS'));
  assert.match(SCRIPT, /^\[ "\$minimum" = "\$MACOS" \] \|\| fail /m, 'the script does not compare its macOS with the app setting');
  assert.match(SCRIPT, /^\s+done < <\(find "\$engine" -type f \\\( -name '\*\.so' -o -name '\*\.dylib' -o -path '\*\/bin\/\*' \\\) -print0\)$/m);
  assert.match(SCRIPT, /^\s+file -b "\$file" \| grep -q '\^Mach-O' \|\| continue$/m);
  assert.match(SCRIPT, /^\s+needs=\$\(vtool -arch arm64 -show-build "\$file" \|.*\|\|\n\s+fail /m, 'a program vtool cannot read is let through');
  assert.match(SCRIPT, /^\s+\[ -n "\$needs" \] \|\| fail /m, 'a program that names no macOS is let through');

  // The comparison itself, run: the oldest macOS and older pass, anything newer stops the build.
  const compare = /^\s+awk -v needs="\$needs" -v allowed="\$MACOS" '([\s\S]*?)' \|\| fail "\$\{file#"\$engine\/"\} needs macOS \$needs, newer than \$MACOS\."$/m.exec(SCRIPT);
  assert.ok(compare, 'a program that needs a newer macOS is let through');
  for (const [needs, allowed] of [['11.0', true], ['13.5', true], ['14.0', true], ['14.1', false], ['15.0', false], ['26.0', false]]) {
    const run = spawnSync('awk', ['-v', `needs=${needs}`, '-v', `allowed=${valueOf('MACOS')}`, compare[1]]);
    assert.equal(run.status === 0, allowed, `a program that needs macOS ${needs} is ${allowed ? 'refused' : 'let through'}`);
  }
});

test("the engine the app carries keeps this computer's Python settings out, and writes nothing into the app", () => {
  const wrapper = /<<'WRAPPER'\n([\s\S]*?)\nWRAPPER\n/.exec(SCRIPT);
  assert.ok(wrapper, "the engine's wrapper is not written from a quoted heredoc");
  const lines = wrapper[1].split('\n').filter((line) => !line.startsWith('#') || line.startsWith('#!'));
  assert.deepEqual(lines, ['#!/bin/sh', 'exec "$(dirname "$0")/python3" -I -B -m mlx_lm.server "$@"']);
  assert.match(SCRIPT, /^\s+cat >"\$engine\/bin\/mlx_lm\.server" <<'WRAPPER'$/m);
  const compiled = COMMANDS.indexOf('"$python" -I -m compileall -q -f -j0 --invalidation-mode unchecked-hash "$engine/lib"');
  assert.ok(compiled > firstAt(/-m pip install/), 'the engine is not compiled once its packages are in');
  // Built again whenever its pins or this script change, and only then.
  assert.equal(valueOf('pins'), '$(cat "$DESKTOP/engine/requirements.txt" scripts/mac-app.sh | shasum -a 256 | cut -c1-64)');
  assert.ok(COMMANDS.includes('if [ ! -x "$python" ] || [ "$(cat "$WORK/engine.pins" 2>/dev/null)" != "$pins" ]; then'), 'the engine is not built again when its pins change');
  // Tried once before it is kept: no home folder of the account's, and no network.
  assert.match(SCRIPT, /^\s+\/usr\/bin\/env -i HOME="\$WORK\/home" HF_HOME="\$WORK\/home\/hf" HF_HUB_OFFLINE=1 PATH=\/usr\/bin:\/bin \\\n\s+"\$engine\/bin\/mlx_lm\.server" --help/m);
  assert.ok(firstAt(/^echo "\$pins" >"\$WORK\/engine\.pins"$/) > firstAt(/mlx_lm\.server" --help/), 'the engine is marked built before it is tried');
});

test('the app looks first for the engine it carries, where the script puts it', () => {
  assert.match(SCRIPT, /^\s*ditto "\$engine" "\$app\/Contents\/Resources\/engine"$/m);
  assert.match(STARTER_RS, /"macos" => Some\(Engine::Mlx\)/);
  assert.match(STARTER_RS, /Engine::Mlx => "mlx_lm\.server"/);
  const carried = /pub fn carried_folder\(exe: &Path\) -> Option<PathBuf> \{([\s\S]*?)\n\}/.exec(STARTER_RS);
  assert.ok(carried, 'starter.rs has no carried_folder');
  assert.match(carried[1], /== "MacOS" && contents\.file_name\(\)\? == "Contents"/);
  assert.match(carried[1], /contents\.join\("Resources"\)\.join\("engine"\)\.join\("bin"\)/);
  assert.match(STARTER_RS, /let carried = std::env::current_exe\(\)\s*\.ok\(\)\s*\.and_then\(\|exe\| carried_folder\(&exe\)\);/);
  assert.match(STARTER_RS, /find_program\(\s*engine\.program\(\),\s+\|key\| std::env::var\(key\)\.ok\(\),\s+home\.as_deref\(\),\s+carried\.as_deref\(\),?\s*\)/);
  // The named program comes before the carried one, and the carried one before the rest.
  const find = /pub fn find_program\([\s\S]*?\) -> Option<PathBuf> \{\n([\s\S]*?)\n\}/.exec(STARTER_RS)[1];
  const named = find.indexOf('set(PROGRAM_VARIABLE)');
  const first = find.indexOf('\n    carried\n');
  const rest = find.indexOf('.chain(places(');
  assert.ok(named >= 0 && first > named, 'the carried engine comes before the one named outright');
  assert.ok(rest > first, 'the usual folders come before the carried engine');
});

// Brad, 2026-11-19: "when can we do an in app install rather than a new dmg
// download?" Install and restart unpacks the .app.tar.gz and puts the one
// app inside in place.
test('the .app.tar.gz holds the checked app alone, with none of this Mac\'s metadata, and its checksum beside it', () => {
  assert.equal(valueOf('APP_TGZ'), 'Rusty-Business-mac-arm64.app.tar.gz');
  assert.deepEqual(commandsWith(/^tar\s.*-czf/), [
    'tar --no-mac-metadata --no-xattrs --no-acls --no-fflags -czf "$WORK/$APP_TGZ" -C "$WORK/app" "$APP.app"',
  ]);
  const order = [
    /^codesign --verify --deep --strict "\$app"$/,
    /^rm -f "\$WORK\/\$APP_TGZ" "\$WORK\/\$APP_TGZ\.sha256"$/,
    /^tar\s.*-czf/,
    /^\(cd "\$WORK" && shasum -a 256 "\$APP_TGZ" >"\$APP_TGZ\.sha256"\)$/,
    // The link to Applications is for the .dmg, and stays out of the archive.
    /^ln -s \/Applications "\$WORK\/app\/Applications"$/,
  ].map(firstAt);
  assert.ok(order.every((at) => at >= 0), `a step is missing: ${order}`);
  assert.deepEqual(order, [...order].sort((a, b) => a - b), 'the steps are out of order');
});

test('the app is checked once it is signed, and the .dmg once it is packed, with its checksum beside it', () => {
  const order = [
    /^ditto "\$engine" "\$app\/Contents\/Resources\/engine"$/,
    /^codesign --force --deep --sign - "\$app"$/,
    /^codesign --verify --deep --strict "\$app"$/,
    // Braces, so the macOS Bash does not read the ™ as part of the name.
    /^hdiutil create -quiet -volname "\$\{APP\}™" -srcfolder "\$WORK\/app" -format ULMO -o "\$WORK\/\$DMG"$/,
    /^hdiutil verify -quiet "\$WORK\/\$DMG"$/,
    /^\(cd "\$WORK" && shasum -a 256 "\$DMG" >"\$DMG\.sha256"\)$/,
  ].map(firstAt);
  assert.ok(order.every((at) => at >= 0), `a step is missing: ${order}`);
  assert.deepEqual(order, [...order].sort((a, b) => a - b), 'the steps are out of order');
  assert.equal(valueOf('DMG'), 'Rusty-Business-mac-arm64.dmg');
  assert.equal(valueOf('APP'), '$(/usr/bin/plutil -extract productName raw -o - "$TAURI/tauri.conf.json")');
});

test('the Mac shows the name with its mark, and the file name stays plain', () => {
  for (const key of ['CFBundleName', 'CFBundleDisplayName']) {
    assert.match(INFO, new RegExp(`<key>${key}</key>\\s*<string>Rusty Business™</string>`), `${key} has no ™`);
  }
  assert.match(LIB_RS, /context\.package_info_mut\(\)\.name = "Rusty Business™"\.to_string\(\);/);
  assert.match(LIB_RS, /\.build\(context\)/);
  assert.doesNotMatch(LIB_RS, /\.build\(tauri::generate_context!\(\)\)/);
  assert.equal(CONF.productName.includes('™'), false);
});
