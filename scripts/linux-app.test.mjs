// What scripts/linux-app.sh does to build the Linux downloads, the .deb, the
// .rpm, and the .tar.gz, the one command for them, run on 64-bit Linux the way
// the Release workflow runs it.
//
// Brad, 2026-11-16: "I should be able to just pull the .dmg or .exe from read
// me and install and go, one click ready."
//
// Brad, 2026-11-17, to a request for Linux downloads (.deb, .rpm, and
// .tar.gz): "I got you in coming build"
//
// So these fail when the script reaches past its own folder: when it reads a
// setting from the account it runs in, installs or writes anything on the
// system, removes anything outside target/linux, downloads anything not pinned
// by its checksum, or builds with another Rust or Node than the Mac's. They
// also fail when it stops naming the packages the system needs, or names
// other ones than the Release workflow installs.
//
// Most checks read the files as text. The script itself is run only with an
// argument it refuses, and, away from 64-bit Linux, with none: on 64-bit Linux,
// where the gate runs, a run with none would start the build. Its two checks of
// the computer are run on their own instead, with stand-ins for what they ask.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const read = (file) => readFileSync(path.join(REPO, file), 'utf8');

const SCRIPT = read('scripts/linux-app.sh');
const MAC = read('scripts/mac-app.sh');
const RELEASE = read('.github/workflows/release.yml');

const HEX = /^[0-9a-f]{64}$/;
const SYSTEM = ['/usr/bin', '/bin', '/usr/sbin', '/sbin'];
const REFUSED = 'The Linux downloads are built on 64-bit Linux, on an Intel or AMD processor.\n';

/** The script's commands, one per entry: comments and blank lines taken out, and a line that ends in a backslash joined to the next. */
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

/** The value a script gives a NAME=value line of its own, as written. */
function valueOf(name, commands = COMMANDS) {
  const line = commands.find((command) => command.startsWith(`${name}=`));
  assert.ok(line, `the script sets no ${name}`);
  return line.slice(name.length + 1);
}

const commandsWith = (pattern) => COMMANDS.filter((command) => pattern.test(command));
const firstAt = (pattern) => COMMANDS.findIndex((command) => pattern.test(command));

/** A part of the script, as written, from the line that starts it to the line that ends it. */
function partOf(start, end) {
  const from = SCRIPT.indexOf(start);
  assert.ok(from >= 0, `the script has no ${start}`);
  const to = SCRIPT.indexOf(end, from);
  assert.ok(to > from, `the script has no ${end} after ${start}`);
  return SCRIPT.slice(from, to + end.length);
}

const FAIL = partOf('fail() {\n', '\n}');
const PLATFORM = partOf('case "$(uname -s)/$(uname -m)" in\n', '\nesac');
const NEEDS = valueOf('NEEDS').replace(/^"|"$/g, '');
const SYSTEM_CHECK = partOf('{ command -v cc ', '"The build needs WebKitGTK, OpenSSL, and a C compiler. On Ubuntu or Debian, run: $NEEDS"');

/** Runs some of the script in bash, with `fail` and the stand-ins before it, and says "through" when it gets past. */
const runPart = (standIns, part) =>
  spawnSync('/bin/bash', ['-c', `set -euo pipefail\n${FAIL}\n${standIns}\n${part}\necho through`], { encoding: 'utf8' });

const run = (...args) => spawnSync('/bin/bash', [path.join(REPO, 'scripts/linux-app.sh'), ...args], { encoding: 'utf8' });
const onLinuxX64 = process.platform === 'linux' && process.arch === 'x64';

test('it builds only on 64-bit Linux, on an Intel or AMD processor', () => {
  const on = (system, machine) => runPart(`uname() { case "$1" in -s) echo '${system}' ;; -m) echo '${machine}' ;; esac; }`, PLATFORM);
  const through = on('Linux', 'x86_64');
  assert.equal(through.status, 0, through.stderr);
  assert.equal(through.stdout, 'through\n');
  for (const [system, machine] of [['Linux', 'aarch64'], ['Linux', 'i686'], ['Darwin', 'arm64'], ['Darwin', 'x86_64'], ['MINGW64_NT-10.0-26100', 'x86_64']]) {
    const stopped = on(system, machine);
    assert.equal(stopped.status, 1, `${system}/${machine} was let through`);
    assert.equal(stopped.stderr, REFUSED, `${system}/${machine} was stopped with something else`);
    assert.equal(stopped.stdout, '');
  }
});

test('it stops on any other computer before it fetches or builds anything', { skip: onLinuxX64 && 'on 64-bit Linux, the script would start the build' }, () => {
  const here = run();
  assert.equal(here.status, 1, 'scripts/linux-app.sh did not stop on this computer');
  assert.equal(here.stderr, REFUSED);
  assert.equal(here.stdout, '');
});

test('it takes nothing after its name', () => {
  for (const args of [['deb'], ['rpm'], ['--help']]) {
    const asked = run(...args);
    assert.equal(asked.status, 1, `scripts/linux-app.sh ${args.join(' ')} did not stop`);
    assert.equal(asked.stderr, 'scripts/linux-app.sh takes nothing after its name.\n');
    assert.equal(asked.stdout, '');
  }
});

test('when the system lacks what the build needs, it says which packages to install, and installs nothing itself', () => {
  assert.equal(NEEDS, 'sudo apt-get install build-essential curl pkg-config libwebkit2gtk-4.1-dev libssl-dev');
  const all = 'cc() { :; }\ncurl() { :; }';
  const has = (found) => `pkg-config() { [ "$*" = "--exists webkit2gtk-4.1 openssl" ] && ${found}; }`;
  const ready = runPart(`NEEDS="${NEEDS}"\n${all}\n${has('true')}`, SYSTEM_CHECK);
  assert.equal(ready.status, 0, ready.stderr);
  assert.equal(ready.stdout, 'through\n');
  const message = `The build needs WebKitGTK, OpenSSL, and a C compiler. On Ubuntu or Debian, run: ${NEEDS}\n`;
  for (const [missing, standIns] of [
    ['WebKitGTK or OpenSSL', `${all}\n${has('false')}`],
    ['pkg-config', `${all}\nPATH=/nowhere`],
    ['a C compiler', `curl() { :; }\n${has('true')}\nPATH=/nowhere`],
  ]) {
    const stopped = runPart(`NEEDS="${NEEDS}"\n${standIns}`, SYSTEM_CHECK);
    assert.equal(stopped.status, 1, `the build went on without ${missing}`);
    assert.equal(stopped.stderr, message, `without ${missing}, the script said something else`);
  }

  // The Release workflow installs the same packages. It works as the bare
  // Ubuntu image's own root, so it needs no sudo, and it adds the
  // certificates the script's downloads check against, which every Ubuntu
  // a person runs already has. sudo is only ever named, in that line.
  const install = /^ +run: apt-get update && apt-get install --yes --no-install-recommends ca-certificates ([a-z0-9 .+-]+)$/m.exec(RELEASE);
  assert.ok(install, 'the Release workflow does not install the packages the build needs');
  assert.equal(install[1], NEEDS.replace('sudo apt-get install ', ''), 'the workflow and the script name different packages');
  assert.deepEqual(commandsWith(/\bsudo\b/), [`NEEDS="${NEEDS}"`]);
});

test('nothing that fetches, builds, or writes comes before those checks', () => {
  const checked = firstAt(/fail "The build needs WebKitGTK/);
  assert.ok(checked > firstAt(/^\*\) fail "The Linux downloads are built on 64-bit Linux/), 'the system is checked before the computer');
  assert.ok(firstAt(/^\*\) fail "The Linux downloads/) > firstAt(/^\[ "\$#" -eq 0 \] \|\| fail /), 'the checks are out of order');
  for (const work of [/\bmkdir\b/, /(^|[;&|{(]\s*)curl\s/, /\bfetch "/, /\brm\b/, /\bnpm\b/, /\bcargo\b/, /rustup/, /\bcp\b/, /\bmv\b/, /\btar\b/]) {
    const at = firstAt(work);
    assert.ok(at === -1 || at > checked, `${work} comes before the computer is checked`);
  }
});

test('it runs again with nothing from the account but who it is', () => {
  assert.match(SCRIPT, /^set -euo pipefail\ncd "\$\(dirname "\$0"\)\/\.\."$/m, 'the script does not stop at the first error, or does not start at the root');
  const again = commandsWith(/\/usr\/bin\/env -i LINUX_APP_CLEAN=1/);
  assert.equal(again.length, 1, 'the script does not run itself again under env -i');
  const at = COMMANDS.indexOf(again[0]);
  assert.ok(at < firstAt(/^fail\(\)/), 'the script does work before it runs itself again');
  assert.deepEqual(COMMANDS.slice(at - 1, at + 2), ['if [ "${LINUX_APP_CLEAN:-}" != 1 ]; then', again[0], 'fi'], 'the script may skip running itself again');
  const kept = [...again[0].matchAll(/\b([A-Z_]+)=/g)].map((match) => match[1]);
  assert.deepEqual(kept, ['LINUX_APP_CLEAN', 'HOME', 'USER', 'LOGNAME', 'TMPDIR', 'LANG', 'PATH']);
  assert.match(again[0], / PATH=\/usr\/bin:\/bin:\/usr\/sbin:\/sbin \/bin\/bash scripts\/linux-app\.sh "\$@"$/);
});

test("it builds with the Mac's Rust and Node, each download over HTTPS and kept only when its checksum is the pinned one", () => {
  const mac = commandsOf(MAC);
  for (const name of ['RUST', 'RUSTUP', 'NODE']) {
    assert.match(valueOf(name), /^\d+\.\d+\.\d+$/, `${name} is not an exact version`);
    assert.equal(valueOf(name), valueOf(name, mac), `${name} differs from scripts/mac-app.sh`);
  }
  const calls = [...SCRIPT.matchAll(/^\s*fetch "([^"]+)" "\$([A-Z0-9_]+)" ([a-z.-]+)$/gm)];
  assert.deepEqual(
    calls.map(([, address, sum, name]) => [address.replace(/\$([A-Z_]+)/g, (_, key) => valueOf(key)), sum, name]),
    [
      [`https://nodejs.org/dist/v${valueOf('NODE')}/node-v${valueOf('NODE')}-linux-x64.tar.gz`, 'NODE_SHA256', 'node.tar.gz'],
      [`https://static.rust-lang.org/rustup/archive/${valueOf('RUSTUP')}/x86_64-unknown-linux-gnu/rustup-init`, 'RUSTUP_SHA256', 'rustup-init'],
      [`https://github.com/ggml-org/llama.cpp/releases/download/${valueOf('LLAMA_TAG')}/llama-${valueOf('LLAMA_TAG')}-bin-ubuntu-vulkan-x64.tar.gz`, 'LLAMA_SHA256', 'llama-server.tar.gz'],
    ],
    'expected three downloads: Node, rustup, and llama-server, for 64-bit Linux',
  );
  for (const [, , sum] of calls) assert.match(valueOf(sum), HEX, `${sum} is not a SHA-256`);

  // One curl, in fetch, over HTTPS only, and the file is checked before it is kept.
  assert.equal(commandsWith(/(^|[;&|{(]\s*)curl\s/).length, 1);
  assert.deepEqual(commandsWith(/\b(wget|git clone)\b/), []);
  const fetch = /^fetch\(\) \{\n([\s\S]*?)\n\}$/m.exec(SCRIPT)[1];
  assert.match(fetch, /curl --fail .*--proto '=https' --proto-redir '=https' --tlsv1\.2/);
  const kept = fetch.indexOf('mv "$WORK/downloads/$3.part" "$WORK/downloads/$3"');
  const checked = fetch.indexOf('check "$2" "$WORK/downloads/$3.part" || fail');
  assert.ok(checked > 0 && kept > checked, 'a download is kept before its checksum is checked');
  assert.match(SCRIPT, /^check\(\) \{\n\s+\[ "\$\(sha256sum "\$2" \| cut -c1-64\)" = "\$1" \]\n\}$/m);
});

test('it fetches the pinned llama-server and packs it with the app', () => {
  assert.match(valueOf('LLAMA_TAG'), /^b\d+$/, 'LLAMA_TAG is not a pinned llama.cpp build');
  assert.match(valueOf('LLAMA_SHA256'), HEX, 'LLAMA_SHA256 is not a SHA-256');

  // The engine is fetched before the build, unpacked into src-tauri/engine
  // (which the bundle's resources pack), and its presence is checked after.
  const fetched = COMMANDS.find((command) => command.startsWith('fetch "https://github.com/ggml-org/llama.cpp/releases/download/'));
  const built = COMMANDS.find((command) => command.startsWith('(cd "$DESKTOP"'));
  assert.ok(fetched, 'the engine is not fetched');
  assert.ok(built, 'the build was not found');
  assert.ok(COMMANDS.indexOf(fetched) < COMMANDS.indexOf(built), 'the engine is not fetched before the build');
  assert.ok(
    COMMANDS.includes('tar -xzf "$WORK/downloads/llama-server.tar.gz" -C "$TAURI/engine" --strip-components=1'),
    'the engine is not unpacked into src-tauri/engine',
  );
  assert.ok(COMMANDS.includes('rm -rf "$TAURI/engine"'), 'the engine folder is not cleaned first');
  assert.ok(
    COMMANDS.includes('[ -x "$TAURI/engine/llama-server" ] || fail "The llama.cpp download holds no llama-server."'),
    'the download is not checked for llama-server',
  );
  assert.ok(
    COMMANDS.some((command) => command.includes('find "$usr"') && command.includes('llama-server')),
    'the package is not checked for the engine',
  );
  const bundle = JSON.parse(read('apps/desktop/src-tauri/tauri.conf.json')).bundle;
  assert.ok(!bundle.resources, 'tauri.conf.json packs the engine folder for every system');
  const linuxBundle = JSON.parse(read('apps/desktop/src-tauri/linux-bundle.json'));
  assert.deepEqual(linuxBundle.bundle.resources, ['engine/**/*'], 'the Linux override does not pack the engine folder');
});

test('every tool and cache it makes stays inside target/linux, which git ignores', () => {
  assert.equal(valueOf('TAURI'), '$DESKTOP/src-tauri');
  assert.equal(valueOf('WORK'), '$(pwd -P)/$TAURI/target/linux');
  assert.match(read('apps/desktop/src-tauri/.gitignore'), /^\/target\/$/m);

  for (const [name, value] of Object.entries({
    RUSTUP_HOME: '$WORK/tools/rustup',
    CARGO_HOME: '$WORK/tools/cargo',
    RUSTUP_TOOLCHAIN: '$RUST',
    npm_config_cache: '$WORK/tools/npm-cache',
    npm_config_userconfig: '$WORK/tools/npmrc',
  })) {
    assert.ok(COMMANDS.includes(`export ${name}=${value}`), `${name} is not ${value}`);
  }
  assert.match(SCRIPT, /rustup-init" -y --quiet --no-modify-path /, 'rustup may change the shell profile');
  assert.equal(valueOf('node_home'), '$WORK/tools/node-$NODE');

  // The account's home folder is passed on once, to the script run again, and never written.
  assert.deepEqual(commandsWith(/\$HOME\b|\$\{HOME\b|~\//), commandsWith(/\/usr\/bin\/env -i LINUX_APP_CLEAN=1/));
  for (const reach of [
    /\b(apt|apt-get|dpkg|dnf|yum|rpm|zypper|pacman|snap|flatpak) (install|-i|add)\b/,
    /\/usr\/local\b|\/opt\/|\/etc\//,
    /--user\b/,
    /\bnpm (install|i) -g\b/,
    /\bcargo install\b/,
  ]) {
    assert.deepEqual(commandsWith(reach).filter((command) => !command.startsWith('NEEDS=')), [], `the script reaches past its folder with ${reach}`);
  }

  // Each PATH is the system's own folders and the tools fetched into target/linux.
  for (const command of commandsWith(/\bPATH=/)) {
    for (const [, value] of command.matchAll(/\bPATH=(\S+)/g)) {
      for (const folder of value.split(':')) {
        assert.ok(SYSTEM.includes(folder) || ['$CARGO_HOME/bin', '$node_home/bin'].includes(folder), `${folder} is on a PATH`);
      }
    }
  }
});

test('it removes only what it made', () => {
  const removals = commandsWith(/(^|[;&|(]\s*)rm(dir)?\s/);
  assert.equal(removals.length, 6, 'the removals were not found');
  for (const command of removals) {
    const targets = command.replace(/^.*?\brm(dir)?\s+(-[a-z]+\s+)?/, '').split(/\s+/);
    for (const target of targets) {
      assert.ok(
        target.startsWith('"$WORK/') || target === '"$TAURI/engine"',
        `${command} removes ${target}`,
      );
    }
  }
});

test('it builds the two packages, packs the same files as a .tar.gz, and keeps each under one name with its checksum beside it', () => {
  assert.equal(valueOf('DEB'), 'Rusty-Business-linux-x64.deb');
  assert.equal(valueOf('RPM'), 'Rusty-Business-linux-x64.rpm');
  assert.equal(valueOf('TGZ'), 'Rusty-Business-linux-x64.tar.gz');
  assert.ok(COMMANDS.includes('(cd "$DESKTOP" && { [ -d node_modules ] || npm ci; } && npm run tauri -- build --bundles deb,rpm --config src-tauri/linux-bundle.json)'), 'the build is not the .deb and the .rpm alone, from the lockfile');
  assert.equal(valueOf('APP'), `$(node -p "require('./$TAURI/tauri.conf.json').productName")`);
  assert.equal(valueOf('VERSION'), `$(node -p "require('./$TAURI/tauri.conf.json').version")`);
  assert.equal(valueOf('deb'), '"$TAURI/target/release/bundle/deb/${APP}_${VERSION}_amd64.deb"');
  assert.equal(valueOf('rpm'), '"$TAURI/target/release/bundle/rpm/${APP}-${VERSION}-1.x86_64.rpm"');
  assert.equal(valueOf('usr'), '"$TAURI/target/release/bundle/deb/${APP}_${VERSION}_amd64/data/usr"');
  assert.match(read('apps/desktop/src-tauri/Cargo.toml'), /^\[package\]\nname = "rusty-business"$/m, 'the program the packages carry has another name');

  const order = [
    /^\[ -f "\$deb" \] \|\| fail /,
    /^\[ -f "\$rpm" \] \|\| fail /,
    /^\[ -x "\$usr\/bin\/rusty-business" \] \|\| fail /,
    /^rm -f "\$WORK\/\$DEB" "\$WORK\/\$DEB\.sha256" "\$WORK\/\$RPM" "\$WORK\/\$RPM\.sha256" "\$WORK\/\$TGZ" "\$WORK\/\$TGZ\.sha256"$/,
    /^cp "\$deb" "\$WORK\/\$DEB"$/,
    /^cp "\$rpm" "\$WORK\/\$RPM"$/,
    /^cp -R "\$usr\/\." "\$WORK\/tarball\/\$folder\/"$/,
    /^tar --create --gzip --file "\$WORK\/\$TGZ" --directory "\$WORK\/tarball" --owner=0 --group=0 --numeric-owner --sort=name "\$folder"$/,
    /^for file in "\$DEB" "\$RPM" "\$TGZ"; do$/,
    /^\(cd "\$WORK" && sha256sum "\$file" >"\$file\.sha256"\)$/,
  ].map(firstAt);
  assert.ok(order.every((at) => at >= 0), `a step is missing: ${order}`);
  assert.deepEqual(order, [...order].sort((a, b) => a - b), 'the steps are out of order');
  assert.equal(valueOf('folder'), '${TGZ%.tar.gz}');

  // The file names Tauri makes from the app's name stay plain, and the
  // packages carry the Linux icons.
  const conf = JSON.parse(read('apps/desktop/src-tauri/tauri.conf.json'));
  assert.doesNotMatch(conf.productName, /™/, 'the package names would carry the ™');
  for (const icon of ['icons/32x32.png', 'icons/128x128.png', 'icons/128x128@2x.png']) assert.ok(conf.bundle.icon.includes(icon), `the packages have no ${icon}`);
});

test('the .deb and the .rpm bring in what the program and the engine link, and the tarball says the rest', () => {
  // The program links OpenSSL through its web client, and the bundled
  // llama-server links GCC's OpenMP runtime and the Vulkan loader. Tauri
  // names WebKitGTK and GTK on its own. libssl3 is OpenSSL 3 on Ubuntu 22.04
  // and later and Debian 12 and later; libvulkan1 is the Vulkan loader and
  // libgomp1 the OpenMP runtime; the .rpm names the same libraries, as
  // Fedora's own packages do.
  assert.match(read('apps/desktop/src-tauri/Cargo.lock'), /^name = "openssl-sys"$/m, 'the program no longer links OpenSSL, so these may go');
  const { linux } = JSON.parse(read('apps/desktop/src-tauri/tauri.conf.json')).bundle;
  assert.deepEqual(linux?.deb?.depends, ['libssl3', 'libvulkan1', 'libgomp1'], 'the .deb does not bring in OpenSSL 3, the Vulkan loader, and the OpenMP runtime');
  assert.deepEqual(linux?.rpm?.depends, ['libssl.so.3()(64bit)', 'libcrypto.so.3()(64bit)', 'libvulkan.so.1()(64bit)', 'libgomp.so.1()(64bit)'], 'the .rpm does not bring in OpenSSL 3, the Vulkan loader, and the OpenMP runtime');
  assert.ok(NEEDS.includes(' libssl-dev'), 'the build no longer installs what it links against');

  // The .tar.gz brings in nothing, so the README and the release notes say
  // what it needs from the system, the Vulkan loader and OpenMP now included.
  const needs = 'It needs WebKitGTK 4.1, OpenSSL 3, the Vulkan loader, the OpenMP runtime, and glibc 2.35 or later from the system.';
  for (const file of ['README.md', 'scripts/release.sh']) {
    assert.ok(read(file).replace(/\s+/g, ' ').includes(needs), `${file} does not say what the .tar.gz needs`);
  }
});
