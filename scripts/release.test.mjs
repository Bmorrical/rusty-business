// What scripts/release.sh asks GitHub to do, checked without GitHub: each
// test puts a stand-in `gh` first on the PATH, and the stand-in writes down
// every command it is given. Nothing is released.
//
// Brad, 2026-11-16: "I should be able to just pull the .dmg or .exe from read
// me and install and go, one click ready." The README links the newest
// release's .dmg and .exe, and this script is what puts them there.
//
// Brad, 2026-11-17, to a request for Linux downloads (.deb, .rpm, and
// .tar.gz): "I got you in coming build". The same release carries those
// three.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const SCRIPT = path.join(REPO, 'scripts', 'release.sh');
const COMMIT = '0123456789abcdef0123456789abcdef01234567';
const REPOSITORY = 'Bmorrical/rusty-business';
const TOKEN = 'stand-in-token';
const WORD = '\u001f';
const RECORD = '\u001e';

const read = (file) => readFileSync(path.join(REPO, file), 'utf8');
const DMG = /^DMG=(\S+)$/m.exec(read('scripts/mac-app.sh'))?.[1];
const EXE = /^EXE=(\S+)$/m.exec(read('scripts/windows-app.sh'))?.[1];
const [DEB, RPM, TGZ] = ['DEB', 'RPM', 'TGZ'].map((name) => new RegExp(`^${name}=(\\S+)$`, 'm').exec(read('scripts/linux-app.sh'))?.[1]);
const { version: VERSION } = JSON.parse(read('apps/desktop/package.json'));
const TAG = `v${VERSION}`;
// A version after this one, for the tests that raise it.
const NEXT = VERSION.replace(/^(\d+)\.(\d+)\.\d+$/, (_, major, minor) => `${major}.${Number(minor) + 1}.0`);

// The stand-in. It writes the token it was given, then each word of the
// command, separated by WORD, and ends the record with RECORD, since the
// release notes run over several lines. Asked for tags, it answers with
// STUB_TAGS, as gh would after its --jq. STUB_FAIL makes the command that
// matches it fail.
const GH = `#!/bin/sh
{
  printf '%s' "$GH_TOKEN"
  for word in gh "$@"; do printf '\\037%s' "$word"; done
  printf '\\036'
} >> "$STUB_LOG"
case "$*" in $STUB_FAIL) exit 1 ;; esac
case "$1" in api) printf '%s' "$STUB_TAGS" ;; esac
`;

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

/** What stands in for each Linux download, unless a test gives its own. */
const LINUX = { deb: 'not really a .deb\n', rpm: 'not really an .rpm\n', tgz: 'not really a .tar.gz\n' };

/**
 * A folder like the one the Release workflow downloads: the .dmg, the .exe,
 * the .deb, the .rpm, and the .tar.gz, and their checksums, as `make dmg`,
 * scripts/windows-app.sh, and scripts/linux-app.sh write them.
 */
function distIn(
  folder,
  { dmg = 'not really a disk image\n', checksum, exe = 'not really an installer\n', exeChecksum, linux = {}, linuxChecksums = {} } = {},
) {
  const dist = path.join(folder, 'dist');
  mkdirSync(dist);
  writeFileSync(path.join(dist, DMG), dmg);
  writeFileSync(path.join(dist, `${DMG}.sha256`), checksum ?? `${sha256(dmg)}  ${DMG}\n`);
  writeFileSync(path.join(dist, EXE), exe);
  writeFileSync(path.join(dist, `${EXE}.sha256`), exeChecksum ?? `${sha256(exe)}  ${EXE}\n`);
  for (const [kind, name] of [['deb', DEB], ['rpm', RPM], ['tgz', TGZ]]) {
    const bytes = linux[kind] ?? LINUX[kind];
    writeFileSync(path.join(dist, name), bytes);
    writeFileSync(path.join(dist, `${name}.sha256`), linuxChecksums[kind] ?? `${sha256(bytes)}  ${name}\n`);
  }
  return dist;
}

/**
 * Runs the script with the stand-in, and returns what it printed and what it
 * asked GitHub. `prepare` gets the scratch folder and returns the arguments.
 */
function run(prepare, { env = {}, script = SCRIPT } = {}) {
  const folder = mkdtempSync(path.join(tmpdir(), 'release-'));
  try {
    const bin = path.join(folder, 'bin');
    mkdirSync(bin);
    writeFileSync(path.join(bin, 'gh'), GH);
    chmodSync(path.join(bin, 'gh'), 0o755);
    const log = path.join(folder, 'gh.log');
    const summary = path.join(folder, 'summary.md');
    writeFileSync(log, '');
    const args = prepare(folder);
    const result = spawnSync('bash', [script, ...args], {
      cwd: tmpdir(),
      encoding: 'utf8',
      env: {
        PATH: `${bin}:${process.env.PATH}`,
        GITHUB_REPOSITORY: REPOSITORY,
        GITHUB_SHA: COMMIT,
        GITHUB_STEP_SUMMARY: summary,
        GH_TOKEN: TOKEN,
        MAC_SIGNED: 'false',
        STUB_LOG: log,
        STUB_FAIL: '__never__',
        STUB_TAGS: '',
        ...env,
      },
    });
    const records = readFileSync(log, 'utf8').split(RECORD).filter(Boolean);
    const tokens = records.map((record) => record.split(WORD)[0]);
    const calls = records.map((record) => record.split(WORD).slice(2));
    let written = '';
    try {
      written = readFileSync(summary, 'utf8');
    } catch {
      // No summary was written.
    }
    return { status: result.status, stdout: result.stdout, stderr: result.stderr, calls, tokens, summary: written, folder };
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
}

const FILTER = (tag) => `.[] | select(.ref == "refs/tags/${tag}") | .ref`;
const ASK = (tag) => ['api', `repos/${REPOSITORY}/git/matching-refs/tags/${tag}`, '--jq', FILTER(tag)];

function createOf(tag, version, notes, dist) {
  return [
    'release',
    'create',
    tag,
    '--repo',
    REPOSITORY,
    '--target',
    COMMIT,
    '--title',
    `Rusty Business™ ${version}`,
    '--notes',
    notes,
    '--latest',
    ...[DMG, EXE, DEB, RPM, TGZ].flatMap((name) => [path.join(dist, name), path.join(dist, `${name}.sha256`)]),
  ];
}

test('a release asks once whether the tag is there, then makes it at this commit, with the .dmg, the .exe, the .deb, the .rpm, the .tar.gz, and their checksums, as the newest', () => {
  assert.ok(DMG, 'scripts/mac-app.sh names no .dmg');
  assert.ok(EXE, 'scripts/windows-app.sh names no .exe');
  assert.ok(DEB && RPM && TGZ, 'scripts/linux-app.sh does not name its three downloads');
  let dist;
  const dmg = 'not really a disk image\n';
  const exe = 'not really an installer\n';
  const result = run((folder) => [
    'publish',
    (dist = distIn(folder, { dmg, exe })),
  ]);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.calls.length, 2, 'expected two calls: the check and the release');
  assert.deepEqual(result.calls[0], ASK(TAG));
  const notes = result.calls[1][10];
  assert.deepEqual(result.calls[1], createOf(TAG, VERSION, notes, dist));

  // What a person reads on the release page. The roadmap, for Windows:
  // "Until Windows has an engine, the window opens and says so. Unsigned at
  // first; see pin 1."
  for (const words of [
    `Rusty Business™ ${VERSION}, for a Mac, for Windows, and for Linux.`,
    'On a Mac with Apple silicon and macOS 14 or later, with its model engine inside:',
    `Download ${DMG}, open it, and drag Rusty Business™ to Applications.`,
    'It is not signed yet, so macOS stops it',
    'Privacy & Security, and choose Open Anyway.',
    'On 64-bit Windows 10 or 11. The app has no model engine for Windows yet, so the window opens and says so.',
    `Download ${EXE} and open it.`,
    'It is not signed yet, so Windows may stop it the first time: choose More info, then Run anyway.',
    `    shasum -a 256 -c ${DMG}.sha256`,
    `    (Get-FileHash ${EXE}).Hash -eq (Get-Content ${EXE}.sha256).Split()[0]`,
    // The roadmap, for Linux: "A `.deb`, an `.rpm`, and a `.tar.gz`, linked
    // from the README beside the Mac's `.dmg` and the Windows `.exe`".
    'On 64-bit Linux on an Intel or AMD processor. The app has no model engine for Linux yet, so the window opens and says so.',
    `On Ubuntu 22.04 or later, or Debian 12 or later, download ${DEB} and run: sudo apt install ./${DEB}`,
    `On Fedora, download ${RPM} and run: sudo dnf install ./${RPM}`,
    `Anywhere else, download ${TGZ}, unpack it, and run ${TGZ.replace(/\.tar\.gz$/, '')}/bin/rusty-business. It needs WebKitGTK 4.1, OpenSSL 3, and glibc 2.35 or later from the system.`,
    `    sha256sum -c ${DEB}.sha256`,
    `https://github.com/${REPOSITORY}/commit/${COMMIT}`,
    'after a green gate',
  ]) {
    assert.ok(notes.includes(words), `the release notes do not say: ${words}`);
  }

  assert.deepEqual(result.tokens, [TOKEN, TOKEN], 'gh did not get the token in its environment');
  for (const call of result.calls) {
    for (const word of call) {
      assert.ok(!word.includes(TOKEN), 'the token is on a command line');
    }
  }
  assert.match(result.stdout, new RegExp(`^Released ${TAG}: https://github\\.com/${REPOSITORY}/releases/tag/${TAG}$`, 'm'));
  assert.ok(result.summary.includes(`\`${DMG}\`: \`${sha256(dmg)}\``), "the run summary does not give the .dmg's checksum");
  assert.equal(
    result.summary,
    [
      `### Released ${TAG}`,
      '',
      `- \`${DMG}\`: \`${sha256(dmg)}\``,
      `- \`${EXE}\`: \`${sha256(exe)}\``,
      `- \`${DEB}\`: \`${sha256(LINUX.deb)}\``,
      `- \`${RPM}\`: \`${sha256(LINUX.rpm)}\``,
      `- \`${TGZ}\`: \`${sha256(LINUX.tgz)}\``,
      '',
    ].join('\n'),
    'the run summary does not give the five checksums',
  );
});

// Brad, 2026-11-19: "keep shipping self-signed as an option until all the
// peices are ready, when ready we can light it up". The sign job says
// whether it signed, and the notes give the steps for the app that went out.
test('the notes give the steps to open the Mac app as it went out, signed or not', () => {
  const notesOf = (signed) => {
    const result = run((folder) => ['publish', distIn(folder)], { env: { MAC_SIGNED: signed } });
    assert.equal(result.status, 0, result.stderr);
    return result.calls[1][10];
  };
  const unsigned = notesOf('false');
  const signed = notesOf('true');
  const open = 'Privacy & Security, and choose Open Anyway.';
  const asked = '2. Open the app. It is signed, and notarized by Apple, so macOS asks once whether to open an app from the internet: choose Open.';
  assert.ok(unsigned.includes(`2. Open the app. It is not signed yet, so macOS stops it the first time: open System Settings, then ${open}`));
  assert.ok(!unsigned.includes(asked), 'the unsigned notes say the app is signed');
  assert.ok(signed.includes(asked), 'the signed notes do not say the app is signed');
  assert.ok(!signed.includes(open), 'the signed notes still send a person to Open Anyway');
  assert.ok(!signed.includes('so macOS stops it'), 'the signed notes still say macOS stops it');
  // Windows is not signed either way, yet.
  for (const notes of [unsigned, signed]) {
    assert.ok(notes.includes('It is not signed yet, so Windows may stop it the first time: choose More info, then Run anyway.'));
  }
  assert.equal(signed.replace(asked, ''), unsigned.replace(/^2\. Open the app\. It is not signed yet, so macOS.*$/m, ''), 'the notes differ in more than the Mac step');
});

test('a version already tagged stops the run before anything is made', () => {
  const result = run((folder) => ['publish', distIn(folder)], { env: { STUB_TAGS: `refs/tags/${TAG}\n` } });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /already on GitHub, and a release that is out is never changed/);
  assert.deepEqual(result.calls, [ASK(TAG)]);
});

test('only the exact tag counts as already there', (t) => {
  // gh runs the --jq filter itself; jq runs it here the same way.
  const jq = spawnSync('jq', ['--version'], { encoding: 'utf8' });
  if (jq.status !== 0) {
    t.skip('jq is not on this computer');
    return;
  }
  const answer = JSON.stringify([{ ref: `refs/tags/${TAG}.1` }, { ref: `refs/tags/${TAG}-beta` }, { ref: `refs/tags/${TAG}` }]);
  const near = JSON.stringify([{ ref: `refs/tags/${TAG}.1` }, { ref: `refs/tags/${TAG}-beta` }]);
  const filter = (input) => spawnSync('jq', ['-r', FILTER(TAG)], { input, encoding: 'utf8' }).stdout;
  assert.equal(filter(answer), `refs/tags/${TAG}\n`);
  assert.equal(filter(near), '');
  assert.equal(filter('[]'), '');
});

test('a check that fails stops the run, and so does a release that fails', () => {
  const asked = run((folder) => ['publish', distIn(folder)], { env: { STUB_FAIL: 'api*' } });
  assert.notEqual(asked.status, 0);
  assert.match(asked.stderr, /could not ask GitHub whether/);
  assert.deepEqual(asked.calls, [ASK(TAG)]);

  const made = run((folder) => ['publish', distIn(folder)], { env: { STUB_FAIL: 'release create*' } });
  assert.notEqual(made.status, 0);
  assert.equal(made.calls.length, 2);
  assert.doesNotMatch(made.stdout, /Released/);
  assert.equal(made.summary, '', 'a failed release wrote a summary');
});

test('a folder that does not hold exactly the five downloads and their matching checksums reaches no gh call', () => {
  const dmg = 'not really a disk image\n';
  const sum = sha256(dmg);
  const exeSum = sha256('not really an installer\n');
  /** A folder with one of its files taken out, or made a link to a copy outside it. */
  const without = (name) => (folder) => {
    const dist = distIn(folder);
    rmSync(path.join(dist, name));
    return dist;
  };
  const linked = (name) => (folder) => {
    const dist = distIn(folder);
    const real = path.join(folder, `real-${name}`);
    copyFileSync(path.join(dist, name), real);
    rmSync(path.join(dist, name));
    symlinkSync(real, path.join(dist, name));
    return dist;
  };
  const cases = {
    'a checksum of something else': (folder) => distIn(folder, { checksum: `${sha256('other')}  ${DMG}\n` }),
    'a checksum that names another file': (folder) => distIn(folder, { checksum: `${sum}  other.dmg\n` }),
    'a checksum in capitals': (folder) => distIn(folder, { checksum: `${sum.toUpperCase()}  ${DMG}\n` }),
    'a checksum with one space': (folder) => distIn(folder, { checksum: `${sum} ${DMG}\n` }),
    'a checksum with more after it': (folder) => distIn(folder, { checksum: `${sum}  ${DMG}\n${sum}  ${DMG}\n` }),
    'an empty .dmg': (folder) => distIn(folder, { dmg: '' }),
    'no checksum': without(`${DMG}.sha256`),
    'no .dmg': without(DMG),
    "a checksum of something else for the .exe": (folder) => distIn(folder, { exeChecksum: `${sha256('other')}  ${EXE}\n` }),
    "the .dmg's checksum beside the .exe": (folder) => distIn(folder, { exeChecksum: `${sum}  ${EXE}\n` }),
    "an .exe checksum that names the .dmg": (folder) => distIn(folder, { exeChecksum: `${exeSum}  ${DMG}\n` }),
    "an .exe checksum with Git Bash's star": (folder) => distIn(folder, { exeChecksum: `${exeSum} *${EXE}\n` }),
    "an .exe checksum with Windows line endings": (folder) => distIn(folder, { exeChecksum: `${exeSum}  ${EXE}\r\n` }),
    'an empty .exe': (folder) => distIn(folder, { exe: '' }),
    'no .exe checksum': without(`${EXE}.sha256`),
    'no .exe': without(EXE),
    'an .exe that is a link': linked(EXE),
    'no .deb': without(DEB),
    'no .rpm checksum': without(`${RPM}.sha256`),
    'no .tar.gz': without(TGZ),
    'an empty .rpm': (folder) => distIn(folder, { linux: { rpm: '' } }),
    "the .deb's checksum beside the .rpm": (folder) =>
      distIn(folder, { linuxChecksums: { rpm: `${sha256(LINUX.deb)}  ${RPM}\n` } }),
    'a .tar.gz checksum that names the .deb': (folder) =>
      distIn(folder, { linuxChecksums: { tgz: `${sha256(LINUX.tgz)}  ${DEB}\n` } }),
    'a .deb checksum with a star': (folder) => distIn(folder, { linuxChecksums: { deb: `${sha256(LINUX.deb)} *${DEB}\n` } }),
    'a .tar.gz that is a link': linked(TGZ),
    'a .tgz beside the .tar.gz': (folder) => {
      const dist = distIn(folder);
      writeFileSync(path.join(dist, TGZ.replace(/\.tar\.gz$/, '.tgz')), LINUX.tgz);
      return dist;
    },
    'a file more': (folder) => {
      const dist = distIn(folder);
      writeFileSync(path.join(dist, 'notes.txt'), 'x');
      return dist;
    },
    'a hidden file more': (folder) => {
      const dist = distIn(folder);
      writeFileSync(path.join(dist, '.DS_Store'), 'x');
      return dist;
    },
    'a .dmg that is a link': linked(DMG),
    'no folder': (folder) => path.join(folder, 'missing'),
    'a file, not a folder': (folder) => {
      writeFileSync(path.join(folder, 'dist'), 'x');
      return path.join(folder, 'dist');
    },
  };
  for (const [name, make] of Object.entries(cases)) {
    const result = run((folder) => ['publish', make(folder)]);
    assert.notEqual(result.status, 0, `${name}: the run went on`);
    assert.match(result.stderr, /^release: /m, `${name}: the run did not say why it stopped`);
    assert.deepEqual(result.calls, [], `${name}: gh was asked something`);
  }
});

test('bad arguments, or a bad commit, repository, token, or word from the sign job, reach no gh call', () => {
  const argCases = [[], ['publish'], ['publish', 'dist', 'more'], ['delete', 'dist'], ['upload', 'dist'], ['edit', 'dist']];
  for (const args of argCases) {
    const result = run((folder) => args.map((arg) => (arg === 'dist' ? distIn(folder) : arg)));
    assert.notEqual(result.status, 0, `${JSON.stringify(args)}: the run went on`);
    assert.match(result.stderr, /use: scripts\/release\.sh publish <folder>/);
    assert.deepEqual(result.calls, [], `${JSON.stringify(args)}: gh was asked something`);
  }

  const envCases = {
    'a short commit': { GITHUB_SHA: COMMIT.slice(0, 12) },
    'a commit in capitals': { GITHUB_SHA: COMMIT.toUpperCase() },
    'no commit': { GITHUB_SHA: '' },
    'a repository with no owner': { GITHUB_REPOSITORY: 'rusty-business' },
    'a repository with a command in it': { GITHUB_REPOSITORY: 'Bmorrical/rusty;true' },
    'a repository with a path in it': { GITHUB_REPOSITORY: 'Bmorrical/rusty/../x' },
    'no token': { GH_TOKEN: '' },
    'no word from the sign job': { MAC_SIGNED: '' },
    'a word from the sign job but true or false': { MAC_SIGNED: 'yes' },
    'true in capitals': { MAC_SIGNED: 'True' },
    'true, then more': { MAC_SIGNED: 'true\nfalse' },
  };
  for (const [name, env] of Object.entries(envCases)) {
    const result = run((folder) => ['publish', distIn(folder)], { env });
    assert.notEqual(result.status, 0, `${name}: the run went on`);
    assert.match(result.stderr, /^release: /m, `${name}: the run did not say why it stopped`);
    assert.deepEqual(result.calls, [], `${name}: gh was asked something`);
  }
});

test("the app's three version numbers must agree, and the release is named for them", () => {
  const FILES = {
    package: 'apps/desktop/package.json',
    tauri: 'apps/desktop/src-tauri/tauri.conf.json',
    cargo: 'apps/desktop/src-tauri/Cargo.toml',
  };
  const at = {
    package: /^ {2}"version": "[^"]*",$/m,
    tauri: /^ {2}"version": "[^"]*",$/m,
    cargo: /^version = "[^"]*"$/m,
  };
  const line = {
    package: (version) => `  "version": "${version}",`,
    tauri: (version) => `  "version": "${version}",`,
    cargo: (version) => `version = "${version}"`,
  };
  for (const name of Object.keys(FILES)) {
    assert.match(read(FILES[name]), at[name], `${FILES[name]} gives its version some other way`);
  }

  /** Runs a copy of the script in a copy of the tree, with the versions given. */
  function withVersions(versions, { cargoAfter = '' } = {}) {
    const tree = mkdtempSync(path.join(tmpdir(), 'release-tree-'));
    try {
      mkdirSync(path.join(tree, 'scripts'));
      mkdirSync(path.join(tree, 'apps/desktop/src-tauri'), { recursive: true });
      copyFileSync(SCRIPT, path.join(tree, 'scripts/release.sh'));
      for (const [name, file] of Object.entries(FILES)) {
        const text = read(file).replace(at[name], line[name](versions[name] ?? VERSION));
        writeFileSync(path.join(tree, file), name === 'cargo' ? text + cargoAfter : text);
      }
      return run((folder) => ['publish', distIn(folder)], { script: path.join(tree, 'scripts/release.sh') });
    } finally {
      rmSync(tree, { recursive: true, force: true });
    }
  }

  for (const versions of [
    { package: NEXT },
    { tauri: NEXT },
    { cargo: NEXT },
    { package: NEXT, tauri: NEXT },
  ]) {
    const result = withVersions(versions);
    assert.notEqual(result.status, 0, `${JSON.stringify(versions)}: the run went on`);
    assert.match(result.stderr, /the app's versions differ: package\.json \S+, tauri\.conf\.json \S+, Cargo\.toml \S+/);
    assert.deepEqual(result.calls, []);
  }

  for (const version of ['0.2', '0.2.0-beta', 'v0.2.0', '']) {
    const result = withVersions({ package: version, tauri: version, cargo: version });
    assert.notEqual(result.status, 0, `"${version}": the run went on`);
    assert.match(result.stderr, /gives no plain version/);
    assert.deepEqual(result.calls, []);
  }

  assert.notEqual(NEXT, VERSION, 'the next version is worked out wrong');
  const raised = withVersions({ package: NEXT, tauri: NEXT, cargo: NEXT });
  assert.equal(raised.status, 0, raised.stderr);
  assert.deepEqual(raised.calls[0], ASK(`v${NEXT}`));
  assert.deepEqual(raised.calls[1].slice(0, 9), createOf(`v${NEXT}`, NEXT, '', '').slice(0, 9));

  // Only the version under [package] is the app's.
  const other = withVersions({}, { cargoAfter: '\n[workspace.package]\nversion = "9.9.9"\n' });
  assert.equal(other.status, 0, other.stderr);
  assert.deepEqual(other.calls[0], ASK(TAG));
});

test("the lockfiles, the README, and SECURITY.md give the app's version too", () => {
  const lock = JSON.parse(read('apps/desktop/package-lock.json'));
  assert.equal(lock.version, VERSION, 'apps/desktop/package-lock.json gives another version');
  assert.equal(lock.packages?.['']?.version, VERSION, 'apps/desktop/package-lock.json gives the app another version');
  const { name } = /^name = "(?<name>[^"]+)"$/m.exec(read('apps/desktop/src-tauri/Cargo.toml')).groups;
  const entry = new RegExp(`^name = "${name}"\\nversion = "([^"]*)"$`, 'm').exec(read('apps/desktop/src-tauri/Cargo.lock'));
  assert.equal(entry?.[1], VERSION, 'apps/desktop/src-tauri/Cargo.lock gives the app another version');
  assert.match(read('README.md'), new RegExp(`^\\*\\*Status:\\*\\* version ${VERSION.replaceAll('.', '\\.')},`, 'm'));
  assert.match(read('SECURITY.md'), new RegExp(`^The app's version is ${VERSION.replaceAll('.', '\\.')}\\.`, 'm'));
});

test('the script asks gh for nothing but the check and the new release, and never removes, edits, or replaces one', () => {
  const code = readFileSync(SCRIPT, 'utf8')
    .split('\n')
    .filter((each) => !/^\s*#/.test(each))
    .join('\n');
  const asks = [...code.matchAll(/(?:^|[\s(|&;"$])gh\s+(\S+)(?:\s+(\S+))?/g)].map((match) =>
    match[1] === 'release' ? `release ${match[2]}` : match[1],
  );
  assert.deepEqual(asks, ['api', 'release create']);
  for (const word of [/\bdelete\b/, /\bedit\b/, /\bupload\b/, /--clobber/, /--force/, /--draft/, /--prerelease/, /(?:^|[\s;&|(])git\s/, /\bcurl\b/]) {
    assert.doesNotMatch(code, word, `release.sh holds ${word}`);
  }
  assert.match(code, /^ {8}--latest \\$/m, 'the release is not marked as the newest, so the README would not find it');
});

test("the README's download links name the files that make dmg, scripts/windows-app.sh, and scripts/linux-app.sh make, and so do the script, the workflow, and the notice of a new version", () => {
  assert.ok(DMG, 'scripts/mac-app.sh names no .dmg');
  assert.ok(EXE, 'scripts/windows-app.sh names no .exe');
  assert.ok(DEB && RPM && TGZ, 'scripts/linux-app.sh does not name its three downloads');
  const workflow = read('.github/workflows/release.yml');
  for (const [file, maker, folder, name] of [
    [DMG, 'DMG', 'mac', '.dmg'],
    [EXE, 'EXE', 'windows', '.exe'],
    [DEB, 'DEB', 'linux', '.deb'],
    [RPM, 'RPM', 'linux', '.rpm'],
    [TGZ, 'TGZ', 'linux', '.tar.gz'],
  ]) {
    const link = `https://github.com/${REPOSITORY}/releases/latest/download/${file}`;
    assert.ok(read('README.md').includes(`](${link})`), `the README does not link ${link}`);
    assert.equal(new RegExp(`^${maker}=(\\S+)$`, 'm').exec(read('scripts/release.sh'))?.[1], file, `release.sh names another ${name}`);
    assert.ok(
      workflow.includes(`target/${folder}/${file}\n`) && workflow.includes(`target/${folder}/${file}.sha256\n`),
      `release.yml keeps another ${name}`,
    );
    assert.ok(read('apps/desktop/src-tauri/src/update.rs').includes(`=> "${file}",`), `the notice of a new version opens another ${name}`);
  }
  // Each build writes its checksum the way the script checks it: the
  // checksum, two spaces, and the name. The Mac's is shasum's own line, and
  // Linux's is sha256sum's; Windows' is written out, since Git Bash's
  // sha256sum puts a * before the name.
  assert.match(read('scripts/mac-app.sh'), /^\(cd "\$WORK" && shasum -a 256 "\$DMG" >"\$DMG\.sha256"\)$/m);
  assert.match(read('scripts/linux-app.sh'), /^for file in "\$DEB" "\$RPM" "\$TGZ"; do\n {2}\(cd "\$WORK" && sha256sum "\$file" >"\$file\.sha256"\)\ndone$/m);
  assert.match(
    read('scripts/windows-app.sh'),
    /^printf '%s  %s\\n' "\$\(sha256sum "\$WORK\/\$EXE" \| cut -c1-64\)" "\$EXE" >"\$WORK\/\$EXE\.sha256"$/m,
  );
});
