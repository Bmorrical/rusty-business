// What scripts/release.sh asks GitHub to do, checked without GitHub: each
// test puts a stand-in `gh` first on the PATH, and the stand-in writes down
// every command it is given. Nothing is released.
//
// Brad, 2026-11-16: "I should be able to just pull the .dmg or .exe from read
// me and install and go, one click ready." The README links the newest
// release's .dmg, and this script is what puts it there.

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
const { version: VERSION } = JSON.parse(read('apps/desktop/package.json'));
const TAG = `v${VERSION}`;

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

/** A folder like the one the Release workflow downloads: the .dmg and its checksum, as `make dmg` writes them. */
function distIn(folder, { dmg = 'not really a disk image\n', checksum } = {}) {
  const dist = path.join(folder, 'dist');
  mkdirSync(dist);
  writeFileSync(path.join(dist, DMG), dmg);
  writeFileSync(path.join(dist, `${DMG}.sha256`), checksum ?? `${sha256(dmg)}  ${DMG}\n`);
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
    path.join(dist, DMG),
    path.join(dist, `${DMG}.sha256`),
  ];
}

test('a release asks once whether the tag is there, then makes it at this commit, with the .dmg and its checksum, as the newest', () => {
  assert.ok(DMG, 'scripts/mac-app.sh names no .dmg');
  let dist;
  const dmg = 'not really a disk image\n';
  const result = run((folder) => [
    'publish',
    (dist = distIn(folder, { dmg })),
  ]);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.calls.length, 2, 'expected two calls: the check and the release');
  assert.deepEqual(result.calls[0], ASK(TAG));
  const notes = result.calls[1][10];
  assert.deepEqual(result.calls[1], createOf(TAG, VERSION, notes, dist));

  // What a person reads on the release page.
  for (const words of [
    `Rusty Business™ ${VERSION} for a Mac with Apple silicon and macOS 14 or later`,
    `Download ${DMG}, open it, and drag Rusty Business™ to Applications.`,
    'It is not signed yet',
    'Privacy & Security, and choose Open Anyway.',
    `    shasum -a 256 -c ${DMG}.sha256`,
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
  assert.ok(result.summary.includes(`\`${DMG}\`: \`${sha256(dmg)}\``), 'the run summary does not give the checksum');
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

test('a folder that does not hold exactly the .dmg and its matching checksum reaches no gh call', () => {
  const dmg = 'not really a disk image\n';
  const sum = sha256(dmg);
  const cases = {
    'a checksum of something else': (folder) => distIn(folder, { checksum: `${sha256('other')}  ${DMG}\n` }),
    'a checksum that names another file': (folder) => distIn(folder, { checksum: `${sum}  other.dmg\n` }),
    'a checksum in capitals': (folder) => distIn(folder, { checksum: `${sum.toUpperCase()}  ${DMG}\n` }),
    'a checksum with one space': (folder) => distIn(folder, { checksum: `${sum} ${DMG}\n` }),
    'a checksum with more after it': (folder) => distIn(folder, { checksum: `${sum}  ${DMG}\n${sum}  ${DMG}\n` }),
    'an empty .dmg': (folder) => distIn(folder, { dmg: '' }),
    'no checksum': (folder) => {
      const dist = distIn(folder);
      rmSync(path.join(dist, `${DMG}.sha256`));
      return dist;
    },
    'no .dmg': (folder) => {
      const dist = distIn(folder);
      rmSync(path.join(dist, DMG));
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
    'a .dmg that is a link': (folder) => {
      const dist = distIn(folder);
      const real = path.join(folder, 'real.dmg');
      copyFileSync(path.join(dist, DMG), real);
      rmSync(path.join(dist, DMG));
      symlinkSync(real, path.join(dist, DMG));
      return dist;
    },
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

test('bad arguments, or a bad commit, repository, or token, reach no gh call', () => {
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
    { package: '0.2.0' },
    { tauri: '0.2.0' },
    { cargo: '0.2.0' },
    { package: '0.2.0', tauri: '0.2.0' },
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

  const raised = withVersions({ package: '0.2.0', tauri: '0.2.0', cargo: '0.2.0' });
  assert.equal(raised.status, 0, raised.stderr);
  assert.deepEqual(raised.calls[0], ASK('v0.2.0'));
  assert.deepEqual(raised.calls[1].slice(0, 9), createOf('v0.2.0', '0.2.0', '', '').slice(0, 9));

  // Only the version under [package] is the app's.
  const other = withVersions({}, { cargoAfter: '\n[workspace.package]\nversion = "9.9.9"\n' });
  assert.equal(other.status, 0, other.stderr);
  assert.deepEqual(other.calls[0], ASK(TAG));
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

test("the README's download link names the .dmg that make dmg makes, and so do the script and the workflow", () => {
  assert.ok(DMG, 'scripts/mac-app.sh names no .dmg');
  const link = `https://github.com/${REPOSITORY}/releases/latest/download/${DMG}`;
  assert.ok(read('README.md').includes(`](${link})`), `the README does not link ${link}`);
  assert.equal(/^DMG=(\S+)$/m.exec(read('scripts/release.sh'))?.[1], DMG, 'release.sh names another .dmg');
  const workflow = read('.github/workflows/release.yml');
  assert.ok(workflow.includes(`target/mac/${DMG}\n`) && workflow.includes(`target/mac/${DMG}.sha256\n`), 'release.yml keeps another .dmg');
  // make dmg writes the checksum the way the script checks it: shasum's own line.
  assert.match(read('scripts/mac-app.sh'), /^\(cd "\$WORK" && shasum -a 256 "\$DMG" >"\$DMG\.sha256"\)$/m);
});
