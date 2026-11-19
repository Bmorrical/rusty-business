// The rules for the GitHub workflows, checked on every test run. The one
// they all serve: nothing deploys and nothing is released until the gate is
// green.
//
// The checks read the files as text. That is enough for these rules, and
// it means nothing has to be installed to run them.

import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const FOLDER = fileURLToPath(new URL('../.github/workflows/', import.meta.url));

const workflows = readdirSync(FOLDER)
  .filter((name) => /\.ya?ml$/.test(name))
  .sort()
  .map((name) => ({ name, text: readFileSync(path.join(FOLDER, name), 'utf8') }));

/** A workflow's text with comments taken out, so a comment can't pass a check. */
export function withoutComments(text) {
  return text
    .split('\n')
    .map((line) => line.replace(/(^|\s)#.*$/, ''))
    .join('\n');
}

/** Each job in a workflow: its name and the text of its block. */
export function jobsOf(text) {
  const lines = withoutComments(text).split('\n');
  const start = lines.indexOf('jobs:');
  if (start === -1) return [];
  const jobs = [];
  for (const line of lines.slice(start + 1)) {
    if (/^\S/.test(line)) break;
    const header = /^ {2}([A-Za-z0-9_-]+):\s*$/.exec(line);
    if (header) jobs.push({ id: header[1], body: '' });
    else if (jobs.length) jobs.at(-1).body += `${line}\n`;
  }
  return jobs;
}

/** What starts a workflow: the names under its top-level `on:`. */
export function triggersOf(text) {
  const lines = withoutComments(text).split('\n');
  const start = lines.indexOf('on:');
  if (start === -1) return [];
  const triggers = [];
  for (const line of lines.slice(start + 1)) {
    if (/^\S/.test(line)) break;
    const name = /^ {2}([a-z_]+):/.exec(line);
    if (name) triggers.push(name[1]);
  }
  return triggers.sort();
}

/** The permissions a job sets for itself, one "name: level" each, or null when it sets none. */
export function jobPermissions(body) {
  const lines = body.split('\n');
  const start = lines.findIndex((line) => /^ {4}permissions:/.test(line));
  if (start === -1) return null;
  const inline = /^ {4}permissions:\s*(\S.*?)\s*$/.exec(lines[start]);
  if (inline) return [inline[1]];
  const found = [];
  for (const line of lines.slice(start + 1)) {
    const entry = /^ {6}([a-z-]+):\s*(\S+)\s*$/.exec(line);
    if (!entry) break;
    found.push(`${entry[1]}: ${entry[2]}`);
  }
  return found;
}

/**
 * The text of every `run:` in a workflow, a block of several lines included.
 * Read with comments left in: GitHub fills in an expression even inside a
 * shell comment.
 */
export function runsOf(text) {
  const lines = text.split('\n');
  const runs = [];
  for (let at = 0; at < lines.length; at += 1) {
    const step = /^(\s*)(?:- )?run:\s*(.*)$/.exec(lines[at]);
    if (!step) continue;
    let body = step[2];
    if (/^[|>][-+]?\s*$/.test(body)) {
      body = '';
      while (at + 1 < lines.length && (lines[at + 1].trim() === '' || lines[at + 1].search(/\S/) > step[1].length)) {
        at += 1;
        body += `${lines[at]}\n`;
      }
    }
    runs.push(body);
  }
  return runs;
}

// The jobs that may write, and what each may write. Deploy's two image jobs
// may also write packages: they push the images. Release's last job may
// write the repository's contents, and nothing else: it makes the release
// and its tag. No other job, in any workflow, sets permissions of its own.
const WRITERS = new Map([
  ['deploy.yml', { jobs: ['build', 'publish'], permissions: ['contents: read', 'packages: write'] }],
  ['release.yml', { jobs: ['publish'], permissions: ['contents: write'] }],
]);

// GitHub's own token, and each workflow has one line that may hand it on: to
// Docker's sign-in in Deploy, and to gh in Release. Release also hands the
// five Mac signing secrets to its sign job, each by its own name, and to no
// other job.
const MAC_SECRETS = ['MAC_CERTIFICATE', 'MAC_CERTIFICATE_PASSWORD', 'MAC_NOTARY_KEY', 'MAC_NOTARY_KEY_ID', 'MAC_NOTARY_ISSUER'];
const TOKEN_LINES = new Map([
  ['deploy.yml', /^ {10}REGISTRY_TOKEN: \$\{\{ secrets\.GITHUB_TOKEN \}\}$/],
  [
    'release.yml',
    new RegExp(
      `^ {10}(?:GH_TOKEN: \\$\\{\\{ secrets\\.GITHUB_TOKEN \\}\\}|(${MAC_SECRETS.join('|')}): \\$\\{\\{ secrets\\.\\1 \\}\\})$`,
    ),
  ],
]);

// The sign job's one signing step, word for word: the switch from the
// release environment's variables, and the five secrets, in its own env.
const SIGN_STEP = [
  '      - name: Sign and notarize the app and the .dmg, when the switch is on',
  '        id: sign',
  '        env:',
  '          MAC_SIGNING: ${{ vars.MAC_SIGNING }}',
  ...MAC_SECRETS.map((name) => `          ${name}: \${{ secrets.${name} }}`),
  '        run: scripts/mac-sign.sh',
  '',
].join('\n');

// The one line that signs in to the registry, word for word. The token
// reaches Docker on its standard input, never on a command line.
const SIGN_IN =
  'run: printf \'%s\' "${REGISTRY_TOKEN}" | docker login ghcr.io --username "${GITHUB_ACTOR}" --password-stdin';

// The .dmg's name, as `make dmg` gives it, the .exe's, as
// scripts/windows-app.sh gives it, and the Linux three, as
// scripts/linux-app.sh gives them.
const DMG = /^DMG=(\S+)$/m.exec(readFileSync(new URL('mac-app.sh', import.meta.url), 'utf8'))?.[1];
const EXE = /^EXE=(\S+)$/m.exec(readFileSync(new URL('windows-app.sh', import.meta.url), 'utf8'))?.[1];
const LINUX = readFileSync(new URL('linux-app.sh', import.meta.url), 'utf8');
const [DEB, RPM, TGZ] = ['DEB', 'RPM', 'TGZ'].map((name) => new RegExp(`^${name}=(\\S+)$`, 'm').exec(LINUX)?.[1]);

function workflow(name) {
  const found = workflows.find((each) => each.name === name);
  assert.ok(found, `${name} is missing from .github/workflows`);
  return found.text;
}

test('the gate, the deploy workflow, and the release workflow are all here', () => {
  assert.deepEqual(
    workflows.map((each) => each.name),
    ['deploy.yml', 'gate.yml', 'release.yml'],
  );
});

test('every action is pinned to a full commit', () => {
  for (const { name, text } of workflows) {
    const uses = [...withoutComments(text).matchAll(/^\s*(?:- )?uses:\s*(\S+)\s*$/gm)].map(
      (match) => match[1],
    );
    assert.ok(uses.length > 0, `${name} uses nothing, which looks wrong`);
    for (const use of uses) {
      if (use.startsWith('./')) continue;
      assert.match(use, /^[\w.-]+\/[\w./-]+@[0-9a-f]{40}$/, `${name}: ${use} is not pinned to a commit`);
    }
  }
});

test('every workflow can only read the repository, and only the jobs listed write, and only what is listed', () => {
  for (const { name, text } of workflows) {
    const plain = withoutComments(text);
    assert.match(plain, /^permissions:\n {2}contents: read\n(?! )/m, `${name}: top-level permissions`);

    const { jobs: writers, permissions } = WRITERS.get(name) ?? { jobs: [], permissions: [] };
    const setting = jobsOf(text).filter((job) => jobPermissions(job.body) !== null);
    assert.deepEqual(
      setting.map((job) => job.id),
      writers,
      `${name}: the jobs that set their own permissions`,
    );
    for (const job of setting) {
      assert.deepEqual(jobPermissions(job.body), permissions, `${name}: the ${job.id} job's permissions`);
    }
    assert.equal(
      plain.match(/^\s*permissions:/gm).length,
      1 + writers.length,
      `${name}: permissions set somewhere other than the top and the jobs listed`,
    );
    assert.equal(
      (plain.match(/write/g) ?? []).length,
      writers.length,
      `${name}: write access given anywhere but the jobs listed`,
    );
  }
});

test('no expression is written into a command', () => {
  // A value reaches a command through env:, so it is never read as shell.
  for (const { name, text } of workflows) {
    const runs = runsOf(text);
    assert.ok(runs.length > 0, `${name} runs nothing, which looks wrong`);
    for (const run of runs) {
      assert.doesNotMatch(run, /\$\{\{/, `${name}: an expression inside run: ${run.trim()}`);
    }
  }
});

test('every checkout keeps no credentials behind', () => {
  for (const { name, text } of workflows) {
    const lines = withoutComments(text).split('\n');
    const checkouts = lines.flatMap((line, at) => (/uses: actions\/checkout@/.test(line) ? [at] : []));
    assert.ok(checkouts.length > 0, `${name} checks nothing out, which looks wrong`);
    for (const at of checkouts) {
      assert.match(lines[at + 1], /^\s+with:$/, `${name}: the checkout on line ${at + 1} has no with:`);
      assert.match(
        lines[at + 2],
        /^\s+persist-credentials: false$/,
        `${name}: the checkout on line ${at + 1} keeps its credentials`,
      );
    }
  }
});

test('the token reaches Docker only on its standard input, and gh only from its own step', () => {
  for (const { name, text } of workflows) {
    const plain = withoutComments(text);
    const allowed = TOKEN_LINES.get(name) ?? /^(?!)/;
    for (const [line] of plain.matchAll(/^.*secrets\..*$/gm)) {
      assert.match(line, allowed, `${name}: a secret used anywhere but its one place: ${line.trim()}`);
    }
    assert.doesNotMatch(plain, /github\.token/, `${name}: the token reached another way than secrets`);
    assert.doesNotMatch(plain, /--password(?!-stdin)|\s-p\s/, `${name}: a password on a command line`);
    assert.doesNotMatch(plain, /^\s+REGISTRY:/m, `${name}: the images sent to a registry other than GitHub's`);
  }

  const release = withoutComments(workflow('release.yml'));
  assert.equal(
    (release.match(/secrets\./g) ?? []).length,
    1 + MAC_SECRETS.length,
    'release.yml: a secret is handed on more than once',
  );
  const publish = jobsOf(workflow('release.yml')).find((each) => each.id === 'publish');
  assert.match(
    publish?.body ?? '',
    /^ {6}- name: .*\n {8}env:\n {10}GH_TOKEN: \$\{\{ secrets\.GITHUB_TOKEN \}\}\n {10}MAC_SIGNED: \$\{\{ needs\.sign\.outputs\.signed \}\}\n {8}run: scripts\/release\.sh publish dist\n/m,
    "release.yml: the token goes anywhere but the release step's own env",
  );
  const sign = jobsOf(workflow('release.yml')).find((each) => each.id === 'sign');
  assert.ok(sign?.body.includes(SIGN_STEP), "release.yml: the signing secrets go anywhere but the sign step's own env");
  for (const other of ['deploy.yml', 'gate.yml']) {
    assert.doesNotMatch(workflow(other), /MAC_|vars\.|environment:/, `${other}: a signing secret, the switch, or an environment`);
  }
  for (const other of ['deploy.yml', 'gate.yml']) {
    assert.doesNotMatch(workflow(other), /GH_TOKEN/, `${other}: the token reaches gh`);
  }

  for (const name of ['deploy.yml']) {
    const text = workflow(name);
    for (const id of WRITERS.get(name).jobs) {
      const job = jobsOf(text).find((each) => each.id === id);
      const signIn = job.body.indexOf(SIGN_IN);
      assert.ok(signIn > 0, `${name}: the ${id} job does not sign in the one way allowed`);
      assert.match(
        job.body.slice(job.body.lastIndexOf('- name:', signIn)),
        /^- name: .*\n {8}env:\n {10}REGISTRY_TOKEN: \$\{\{ secrets\.GITHUB_TOKEN \}\}\n {8}run: printf/,
        `${name}: the ${id} job's sign-in does not read the token from its own env`,
      );
      assert.ok(
        signIn < job.body.indexOf('scripts/publish-images.sh'),
        `${name}: the ${id} job pushes before it signs in`,
      );
    }
  }
});

test('Deploy builds each kind of image on its own kind of computer, then joins them', () => {
  const jobs = jobsOf(workflow('deploy.yml'));
  assert.deepEqual(
    jobs.map((job) => job.id),
    ['gate', 'build', 'publish'],
  );
  const [, build, publish] = jobs;

  assert.match(build.body, /^ {4}runs-on: \$\{\{ matrix\.runner \}\}$/m);
  assert.match(build.body, /^ {6}fail-fast: true$/m);
  assert.match(
    build.body,
    / {8}include:\n {10}- arch: amd64\n {12}runner: ubuntu-24\.04\n {10}- arch: arm64\n {12}runner: ubuntu-24\.04-arm\n {4}steps:/,
  );
  assert.match(build.body, / {10}ARCH: \$\{\{ matrix\.arch \}\}\n {8}run: scripts\/publish-images\.sh build "\$\{ARCH\}"\n/);

  assert.match(publish.body, /^ {4}needs: \[gate, build\]$/m);
  assert.match(publish.body, /^ {4}runs-on: ubuntu-24\.04$/m);
  assert.match(publish.body, /^ {8}run: scripts\/publish-images\.sh join$/m);
  assert.equal(
    (publish.body.match(/publish-images\.sh/g) ?? []).length,
    1,
    'the publish job runs the script once',
  );
});

test('no workflow runs on a pull request', () => {
  for (const { name, text } of workflows) {
    for (const trigger of triggersOf(text)) {
      assert.doesNotMatch(trigger, /^pull_request/, `${name} runs on ${trigger}`);
    }
  }
});

test('the gate runs on each push to main, on request, and when called', () => {
  const text = workflow('gate.yml');
  assert.deepEqual(triggersOf(text), ['push', 'workflow_call', 'workflow_dispatch']);
  assert.match(withoutComments(text), /^ {2}push:\n {4}branches: \[main\]\n/m);
});

test('the gate is green only when every other job is green', () => {
  const jobs = jobsOf(workflow('gate.yml'));
  const others = jobs.map((job) => job.id).filter((id) => id !== 'gate');
  assert.deepEqual(others, ['services', 'desktop', 'e2e']);

  const gate = jobs.find((job) => job.id === 'gate');
  assert.match(gate.body, /^ {4}if: always\(\)$/m, 'the gate job must run even when a job fails');
  assert.match(gate.body, new RegExp(`^ {4}needs: \\[${others.join(', ')}\\]$`, 'm'));
  assert.match(gate.body, /needs\.\*\.result/);
});

test('a deploy or a release starts only on request, from main, and every job waits for the gate', () => {
  for (const name of ['deploy.yml', 'release.yml']) {
    const text = workflow(name);
    assert.deepEqual(triggersOf(text), ['workflow_dispatch'], `${name}: what starts it`);

    const jobs = jobsOf(text);
    const gate = jobs.find((job) => job.id === 'gate');
    assert.ok(gate, `${name} has no gate job`);
    assert.match(gate.body, /^ {4}uses: \.\/\.github\/workflows\/gate\.yml$/m, `${name}: the gate job`);
    assert.match(gate.body, /^ {4}if: github\.ref == 'refs\/heads\/main'$/m, `${name}: the gate runs on other branches`);

    const others = jobs.filter((job) => job.id !== 'gate');
    assert.ok(others.length > 0, `${name} has no job but the gate`);
    for (const job of others) {
      assert.match(
        job.body,
        /^ {4}needs: (gate|\[(.*, )?gate(, .*)?\])$/m,
        `${name}: the ${job.id} job does not wait for the gate`,
      );
      assert.doesNotMatch(job.body, /^ {4}if: /m, `${name}: the ${job.id} job may run past a red gate`);
    }
  }
});

// Brad, 2026-11-16: "I should be able to just pull the .dmg or .exe from
// read me and install and go, one click ready." Release puts the .dmg and the
// .exe where the README's links find them, after a green gate.
//
// The roadmap: "An `.exe`, linked from the README beside the Mac's `.dmg`:
// download, open, and go."
//
// Brad, 2026-11-17, to a request for Linux downloads (.deb, .rpm, and
// .tar.gz): "I got you in coming build". Release builds those three too.
test('Release builds the Mac app on a Mac, the Windows installer on Windows, and the Linux packages on Linux, each with the command a person runs, and only its last job publishes', () => {
  const text = workflow('release.yml');
  assert.match(withoutComments(text), /^concurrency:\n {2}group: release\n {2}cancel-in-progress: false\n/m);

  const jobs = jobsOf(text);
  assert.deepEqual(
    jobs.map((job) => job.id),
    ['gate', 'mac', 'sign', 'windows', 'linux', 'publish'],
  );
  const [, mac, , windows, linux, publish] = jobs;
  assert.ok(DMG, 'scripts/mac-app.sh names no .dmg');
  assert.ok(EXE, 'scripts/windows-app.sh names no .exe');
  assert.ok(DEB && RPM && TGZ, 'scripts/linux-app.sh does not name its three downloads');

  /** The upload step's block, which must keep exactly these files and their checksums, and stop when any is missing. */
  const keeps = (name, folder, ...files) =>
    [
      '        with:',
      `          name: ${name}`,
      '          path: |',
      ...files.flatMap((file) => [`            ${folder}/${file}`, `            ${folder}/${file}.sha256`]),
      '          if-no-files-found: error',
    ].join('\n');

  // macos-15 is GitHub's Mac with Apple silicon, the only kind the Mac app is for.
  assert.match(mac.body, /^ {4}needs: gate$/m);
  assert.match(mac.body, /^ {4}runs-on: macos-15$/m);
  assert.deepEqual(runsOf(mac.body), ['make dmg'], 'the mac job runs anything but make dmg');
  assert.ok(
    mac.body.includes(keeps('mac-dmg-built', 'apps/desktop/src-tauri/target/mac', DMG)),
    'the mac job does not keep exactly the .dmg and its checksum, or goes on without them',
  );

  // windows-2025 is a 64-bit Windows with Microsoft's C++ build tools, which
  // Rust needs there. The script runs in Git Bash, as it does for a person.
  assert.match(windows.body, /^ {4}needs: gate$/m);
  assert.match(windows.body, /^ {4}runs-on: windows-2025$/m);
  assert.deepEqual(runsOf(windows.body), ['bash scripts/windows-app.sh'], 'the windows job runs anything but the build script');
  assert.match(windows.body, /^ {8}shell: bash\n {8}run: bash scripts\/windows-app\.sh$/m, 'the build script does not run in Git Bash');
  assert.ok(
    windows.body.includes(keeps('windows-exe', 'apps/desktop/src-tauri/target/windows', EXE)),
    'the windows job does not keep exactly the .exe and its checksum, or goes on without them',
  );

  // ubuntu-24.04 is a 64-bit Linux on an Intel or AMD processor, and the
  // packages are built on it in Ubuntu 22.04's image, pinned by digest: a
  // program built against a newer C library does not start on an older one,
  // so a build on 24.04 would not start on Ubuntu 22.04 or Debian 12.
  // WebKitGTK, OpenSSL, and a C compiler come from Ubuntu, as for a person,
  // and the script checks they are there.
  assert.match(linux.body, /^ {4}needs: gate$/m);
  assert.match(linux.body, /^ {4}runs-on: ubuntu-24\.04$/m);
  assert.match(linux.body, /^ {4}container: ubuntu:22\.04@sha256:[0-9a-f]{64}$/m, 'the Linux packages are not built in Ubuntu 22.04');
  // The README and the release notes name that Ubuntu as the oldest the
  // packages are for.
  const oldest = /^ {4}container: ubuntu:(\d+\.\d+)@/m.exec(linux.body)?.[1];
  for (const file of ['../README.md', 'release.sh']) {
    assert.ok(
      readFileSync(new URL(file, import.meta.url), 'utf8').includes(`Ubuntu ${oldest} or later`),
      `${file.replace('../', '')} does not say the Linux packages are for Ubuntu ${oldest} or later`,
    );
  }
  assert.deepEqual(
    runsOf(linux.body),
    [
      'apt-get update && apt-get install --yes --no-install-recommends ca-certificates build-essential curl pkg-config libwebkit2gtk-4.1-dev libssl-dev',
      'bash scripts/linux-app.sh',
    ],
    'the linux job runs anything but the packages it needs and the build script',
  );
  assert.ok(
    linux.body.includes(keeps('linux-packages', 'apps/desktop/src-tauri/target/linux', DEB, RPM, TGZ)),
    'the linux job does not keep exactly the three downloads and their checksums, or goes on without them',
  );

  assert.match(publish.body, /^ {4}needs: \[gate, mac, sign, windows, linux\]$/m);
  assert.match(publish.body, /^ {4}runs-on: ubuntu-24\.04$/m);
  for (const name of ['mac-dmg', 'windows-exe', 'linux-packages']) {
    assert.ok(
      publish.body.includes(['        with:', `          name: ${name}`, '          path: dist\n'].join('\n')),
      `the publish job does not take ${name} into dist`,
    );
  }
  assert.equal((publish.body.match(/download-artifact@/g) ?? []).length, 3, 'the publish job takes in more than the three builds');
  assert.deepEqual(runsOf(publish.body), ['scripts/release.sh publish dist'], 'the publish job runs anything but the release script');
});

// Brad, 2026-11-19: "keep shipping self-signed as an option until all the
// peices are ready, when ready we can light it up". Then: "self signed for
// now". The sign job is a switch, the release environment's MAC_SIGNING,
// and while it is off the .dmg goes on as the mac job built it.
test('Release signs the Mac app only in its sign job, on a Mac of its own, behind a switch in the release environment', () => {
  const text = workflow('release.yml');
  const plain = withoutComments(text);
  const jobs = jobsOf(text);
  const mac = jobs.find((job) => job.id === 'mac');
  const sign = jobs.find((job) => job.id === 'sign');
  const folder = 'apps/desktop/src-tauri/target/mac';

  // The job that builds, and runs what it fetches, is handed nothing.
  assert.doesNotMatch(mac.body, /secrets\.|vars\.|environment:/, 'the mac job is handed a secret, the switch, or the environment');

  assert.match(sign.body, /^ {4}needs: \[gate, mac\]$/m);
  assert.match(sign.body, /^ {4}runs-on: macos-15$/m);
  assert.match(sign.body, /^ {4}environment: release$/m, 'the sign job does not run in the release environment');
  assert.match(sign.body, /^ {4}outputs:\n {6}signed: \$\{\{ steps\.sign\.outputs\.signed \}\}\n/m);
  assert.equal((plain.match(/environment:/g) ?? []).length, 1, 'release.yml: an environment anywhere but the sign job');
  assert.equal((plain.match(/vars\./g) ?? []).length, 1, 'release.yml: a variable read anywhere but the switch');
  assert.deepEqual(runsOf(sign.body), ['scripts/mac-sign.sh'], 'the sign job runs anything but scripts/mac-sign.sh');

  // In order: the built .dmg in, signed, and kept for the publish job.
  const takes = ['        with:', '          name: mac-dmg-built', `          path: ${folder}\n`].join('\n');
  const keeps = [
    '        with:',
    '          name: mac-dmg',
    '          path: |',
    `            ${folder}/${DMG}`,
    `            ${folder}/${DMG}.sha256`,
    '          if-no-files-found: error',
  ].join('\n');
  assert.equal((sign.body.match(/download-artifact@/g) ?? []).length, 1, 'the sign job takes in more than the built .dmg');
  assert.ok(sign.body.indexOf(takes) > 0, 'the sign job does not take the built .dmg into its folder');
  assert.ok(sign.body.indexOf(takes) < sign.body.indexOf(SIGN_STEP), 'the sign job signs before the .dmg is in');
  assert.ok(sign.body.indexOf(SIGN_STEP) < sign.body.indexOf(keeps), 'the sign job keeps the .dmg before it is signed');
});

test('jobs and triggers are read from the text, and comments are ignored', () => {
  const text = [
    'on:',
    '  push: # pull_request:',
    '    branches: [main]',
    '  workflow_dispatch:',
    'jobs:',
    '  one:',
    '    runs-on: x',
    '  # two:',
    '  three:',
    '    needs: one',
    '',
  ].join('\n');
  assert.deepEqual(triggersOf(text), ['push', 'workflow_dispatch']);
  assert.deepEqual(
    jobsOf(text).map((job) => job.id),
    ['one', 'three'],
  );
  assert.match(jobsOf(text)[1].body, /needs: one/);
});

test('permissions and run lines are read from the text', () => {
  assert.equal(jobPermissions('    runs-on: x\n'), null);
  assert.deepEqual(jobPermissions('    permissions: write-all\n'), ['write-all']);
  assert.deepEqual(jobPermissions('    permissions:\n      contents: read\n      packages: write\n    steps:\n'), [
    'contents: read',
    'packages: write',
  ]);

  const text = [
    '      - run: echo one',
    '      - name: two',
    '        run: |',
    '          echo two',
    '          # ${{ hidden }}',
    '        env:',
    '  # run: not a step',
    '',
  ].join('\n');
  assert.deepEqual(runsOf(text), ['echo one', '          echo two\n          # ${{ hidden }}\n']);
});
