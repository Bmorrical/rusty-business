// What scripts/windows-app.sh does to build the Windows installer, the one
// command for it, run in Git Bash the way the Release workflow runs it.
//
// Brad, 2026-11-16: "I should be able to just pull the .dmg or .exe from read
// me and install and go, one click ready."
//
// Brad, 2026-11-17: "this should be must easier for the user to boot up, run
// it in isolation, one command to rule it all, start a makefile"
//
// So these fail when the script reaches past its own folder: when it keeps a
// setting from the shell that changes the build, writes to the home folder or
// the system, removes anything outside target/windows, downloads anything not
// pinned by its checksum, or builds with another Rust or Node than the Mac's.
// They also fail when the scripts could check out with Windows line endings,
// which Git Bash cannot run.
//
// Most checks read the files as text. Two run the script on this computer,
// which stops it before it fetches or builds anything.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const read = (file) => readFileSync(path.join(REPO, file), 'utf8');

const SCRIPT = read('scripts/windows-app.sh');
const MAC = read('scripts/mac-app.sh');

const HEX = /^[0-9a-f]{64}$/;

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

const run = (...args) => spawnSync('/bin/bash', [path.join(REPO, 'scripts/windows-app.sh'), ...args], { encoding: 'utf8' });

test('it builds only on 64-bit Windows, in Git Bash, and stops anywhere else before it fetches or builds anything', () => {
  assert.notEqual(process.platform, 'win32', 'these tests run on a Mac or Linux');
  const here = run();
  assert.equal(here.status, 1, 'scripts/windows-app.sh did not stop on this computer');
  assert.equal(here.stderr, 'The Windows installer is built on 64-bit Windows, in Git Bash.\n');
  assert.equal(here.stdout, '');
  // Git Bash names itself MINGW64 or MSYS, and only the 64-bit kind is let through.
  assert.ok(COMMANDS.includes('case "$(uname -s)/$(uname -m)" in'));
  assert.ok(COMMANDS.includes('MINGW64_NT-*/x86_64 | MSYS_NT-*/x86_64) ;;'));
});

test('it takes nothing after its name', () => {
  for (const args of [['app'], ['exe'], ['--help']]) {
    const asked = run(...args);
    assert.equal(asked.status, 1, `scripts/windows-app.sh ${args.join(' ')} did not stop`);
    assert.equal(asked.stderr, 'scripts/windows-app.sh takes nothing after its name.\n');
  }
});

test('nothing that fetches, builds, or writes comes before those checks', () => {
  assert.match(SCRIPT, /^set -euo pipefail\ncd "\$\(dirname "\$0"\)\/\.\."$/m, 'the script does not stop at the first error, or does not start at the root');
  const checked = firstAt(/^\*\) fail "The Windows installer is built on 64-bit Windows, in Git Bash\."/);
  assert.ok(checked > firstAt(/^\[ "\$#" -eq 0 \] \|\| fail /), 'the checks are out of order');
  for (const work of [/\bmkdir\b/, /\bcurl\b/, /\bfetch "/, /\brm\b/, /\bnpm\b/, /\bcargo\b/, /rustup/, /\bcp\b/, /\bmv\b/]) {
    const at = firstAt(work);
    assert.ok(at === -1 || at > checked, `${work} comes before the computer is checked`);
  }
});

test("it builds with the Mac's Rust and Node, each download over HTTPS and kept only when its checksum is the pinned one", () => {
  const mac = commandsOf(MAC);
  for (const name of ['RUST', 'RUSTUP', 'NODE']) {
    assert.match(valueOf(name), /^\d+\.\d+\.\d+$/, `${name} is not an exact version`);
    assert.equal(valueOf(name), valueOf(name, mac), `${name} differs from scripts/mac-app.sh`);
  }
  const calls = [...SCRIPT.matchAll(/^\s*fetch "([^"]+)"(?: \\\n\s*| )"\$([A-Z0-9_]+)" ([a-z.-]+)$/gm)];
  assert.deepEqual(
    calls.map(([, address, sum, name]) => [address.replace(/\$([A-Z_]+)/g, (_, key) => valueOf(key)), sum, name]),
    [
      [`https://nodejs.org/dist/v${valueOf('NODE')}/node-v${valueOf('NODE')}-win-x64.zip`, 'NODE_SHA256', 'node.zip'],
      [`https://static.rust-lang.org/rustup/archive/${valueOf('RUSTUP')}/x86_64-pc-windows-msvc/rustup-init.exe`, 'RUSTUP_SHA256', 'rustup-init.exe'],
    ],
    'expected two downloads: Node and rustup, for 64-bit Windows',
  );
  for (const [, , sum] of calls) assert.match(valueOf(sum), HEX, `${sum} is not a SHA-256`);

  // One curl, in fetch, over HTTPS only, and the file is checked before it is kept.
  assert.equal(commandsWith(/\bcurl\b/).length, 1);
  assert.deepEqual(commandsWith(/\b(wget|git clone|Invoke-WebRequest|iwr|bitsadmin|certutil)\b/i), []);
  const fetch = /^fetch\(\) \{\n([\s\S]*?)\n\}$/m.exec(SCRIPT)[1];
  assert.match(fetch, /curl --fail .*--proto '=https' --proto-redir '=https' --tlsv1\.2/);
  const kept = fetch.indexOf('mv "$WORK/downloads/$3.part" "$WORK/downloads/$3"');
  const checked = fetch.indexOf('check "$2" "$WORK/downloads/$3.part" || fail');
  assert.ok(checked > 0 && kept > checked, 'a download is kept before its checksum is checked');
  assert.match(SCRIPT, /^check\(\) \{\n\s+\[ "\$\(sha256sum "\$2" \| cut -c1-64\)" = "\$1" \]\n\}$/m);
});

test('every tool and cache it makes stays inside target/windows, which git ignores', () => {
  assert.equal(valueOf('TAURI'), '$DESKTOP/src-tauri');
  assert.equal(valueOf('WORK'), '$(pwd -P)/$TAURI/target/windows');
  assert.match(read('apps/desktop/src-tauri/.gitignore'), /^\/target\/$/m);

  for (const [name, value] of Object.entries({
    RUSTUP_HOME: '$(cygpath -w "$WORK/tools/rustup")',
    CARGO_HOME: '$(cygpath -w "$WORK/tools/cargo")',
    npm_config_cache: '$(cygpath -w "$WORK/tools/npm-cache")',
    npm_config_userconfig: '$(cygpath -w "$WORK/tools/npmrc")',
  })) {
    assert.equal(valueOf(name), value, `${name} is not ${value}`);
  }
  assert.ok(COMMANDS.includes('export RUSTUP_TOOLCHAIN=$RUST'), 'Rust is not held to its pinned version');
  assert.equal(valueOf('node_home'), '$WORK/tools/node-$NODE');
  assert.equal(valueOf('cargo_bin'), '$WORK/tools/cargo/bin');
  assert.match(SCRIPT, /rustup-init\.exe" -y --quiet --no-modify-path /, "rustup may change the account's PATH");

  // Nothing is written to the account or the system.
  for (const reach of [
    /\$HOME\b|\$\{HOME\b|~\//,
    /USERPROFILE|APPDATA/,
    /\bsudo\b|\brunas\b/,
    /\b(choco|winget|scoop|pacman|setx|reg|msiexec)\b/,
    /\bnpm (install|i) -g\b/,
    /\bcargo install\b/,
    /\/c\/(Program Files|Windows|Users)/i,
  ]) {
    assert.deepEqual(commandsWith(reach), [], `the script reaches past its folder with ${reach}`);
  }

  // Each PATH is Git Bash's own folders, Windows' own, and the tools fetched into target/windows.
  const allowed = ['/mingw64/bin', '/usr/bin', '/bin', '$SYSTEM', '$SYSTEM/Wbem', '$SYSTEM/WindowsPowerShell/v1.0', '$cargo_bin', '$node_home', '$PATH'];
  const paths = commandsWith(/\bPATH=/);
  assert.equal(paths.length, 2);
  assert.ok(paths[0].startsWith('export PATH=/mingw64/bin:'), "the first PATH does not start with Git Bash's own folders");
  for (const command of paths) {
    for (const [, value] of command.matchAll(/\bPATH=(\S+)/g)) {
      for (const folder of value.split(':')) assert.ok(allowed.includes(folder), `${folder} is on a PATH`);
    }
  }
  assert.equal(valueOf('SYSTEM'), '$(cygpath -u "${SYSTEMROOT:-${SystemRoot:-C:\\\\Windows}}")/System32');
});

test("it clears the shell's settings that change what is built, before it fetches anything", () => {
  const clear = /^while IFS= read -r name; do\n {2}case "\$name" in\n([\s\S]*?)\n {6}unset "\$name"\n {6};;\n {2}esac\ndone < <\(compgen -e\)$/m.exec(SCRIPT);
  assert.ok(clear, 'the script does not clear the settings the shell holds');
  const names = clear[1].replace(/\\\n/g, ' ').replace(/\)\s*$/, '').split('|').map((name) => name.trim());
  for (const name of ['CARGO*', 'RUST*', 'NODE*', 'NPM*', 'npm_*', 'TAURI*', 'VITE*', 'CC', 'CXX', 'CFLAGS*', 'LDFLAGS*', 'CL', '_CL_', 'LINK', '_LINK_', 'INCLUDE', 'LIB', 'LIBPATH']) {
    assert.ok(names.includes(name), `${name} is not cleared`);
  }
  assert.ok(firstAt(/^done < <\(compgen -e\)$/) < firstAt(/^mkdir -p /), 'the settings are cleared after work starts');
});

test('it removes only what it made, inside target/windows', () => {
  const removals = commandsWith(/(^|[;&|(]\s*)rm(dir)?\s/);
  assert.equal(removals.length, 3, 'the removals were not found');
  for (const command of removals) {
    const targets = command.replace(/^.*?\brm(dir)?\s+(-[a-z]+\s+)?/, '').split(/\s+/);
    for (const target of targets) assert.ok(target.startsWith('"$WORK/'), `${command} removes ${target}`);
  }
});

test('it builds the installer alone, and keeps it under one name with its checksum beside it', () => {
  assert.equal(valueOf('EXE'), 'Rusty-Business-windows-x64-setup.exe');
  assert.ok(COMMANDS.includes('(cd "$DESKTOP" && { [ -d node_modules ] || npm ci; } && npm run tauri -- build --bundles nsis)'), 'the build is not the NSIS installer alone, from the lockfile');
  assert.equal(valueOf('APP'), `$(node -p "require('./$TAURI/tauri.conf.json').productName")`);
  assert.equal(valueOf('VERSION'), `$(node -p "require('./$TAURI/tauri.conf.json').version")`);
  assert.equal(valueOf('built'), '"$TAURI/target/release/bundle/nsis/${APP}_${VERSION}_x64-setup.exe"');
  const order = [
    /^\[ -f "\$built" \] \|\| fail /,
    /^rm -f "\$WORK\/\$EXE" "\$WORK\/\$EXE\.sha256"$/,
    /^cp "\$built" "\$WORK\/\$EXE"$/,
    /^printf '%s {2}%s\\n' /,
  ].map(firstAt);
  assert.ok(order.every((at) => at >= 0), `a step is missing: ${order}`);
  assert.deepEqual(order, [...order].sort((a, b) => a - b), 'the steps are out of order');
  // The Windows installer's own setting: one account, no prompt for an administrator.
  const conf = JSON.parse(read('apps/desktop/src-tauri/tauri.conf.json'));
  assert.equal(conf.bundle.windows?.nsis?.installMode ?? 'currentUser', 'currentUser');
  assert.ok(conf.bundle.icon.includes('icons/icon.ico'), 'the installer has no Windows icon');
});

test('the scripts check out with plain line endings, even on Windows', () => {
  const rules = read('.gitattributes').split('\n').filter((line) => line.trim() && !line.startsWith('#'));
  assert.ok(rules.includes('*.sh text eol=lf'), '.gitattributes does not keep the .sh files at LF');
  for (const name of readdirSync(path.join(REPO, 'scripts')).filter((file) => file.endsWith('.sh'))) {
    assert.doesNotMatch(read(`scripts/${name}`), /\r/, `scripts/${name} has a Windows line ending`);
  }
});
