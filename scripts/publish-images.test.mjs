// What scripts/publish-images.sh asks Docker to do, checked without Docker:
// each test puts a stand-in `docker` and `uname` first on the PATH, and the
// stand-in writes down every command it is given. Nothing is built, signed
// in to, or pushed.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const SCRIPT = path.join(REPO, 'scripts', 'publish-images.sh');
const COMMIT = '0123456789abcdef0123456789abcdef01234567';
const DIGEST = `sha256:${'ab'.repeat(32)}`;
const SEP = '\u001f';

// The stand-ins. `docker` writes each command as one line, its words
// separated by SEP, then answers the two questions the script asks the
// registry. STUB_FAIL makes the command that matches it fail.
const DOCKER = `#!/bin/sh
line="docker"
for word in "$@"; do line="$line$(printf '\\037')$word"; done
printf '%s\\n' "$line" >> "$STUB_LOG"
case "$*" in $STUB_FAIL) exit 1 ;; esac
case "$*" in
  *"imagetools inspect"*Manifest.Digest*) printf '%s\\n' "$STUB_DIGEST" ;;
  *"imagetools inspect"*Platform*) printf '%b' "$STUB_PLATFORMS" ;;
esac
`;
const UNAME = `#!/bin/sh
printf '%s\\n' "$STUB_MACHINE"
`;

/** Runs the script with the stand-ins, and returns what it printed and what it asked Docker. */
function run(args, { machine = 'x86_64', env = {} } = {}) {
  const folder = mkdtempSync(path.join(tmpdir(), 'publish-images-'));
  try {
    for (const [name, body] of [
      ['docker', DOCKER],
      ['uname', UNAME],
    ]) {
      writeFileSync(path.join(folder, name), body);
      chmodSync(path.join(folder, name), 0o755);
    }
    const log = path.join(folder, 'docker.log');
    const summary = path.join(folder, 'summary.md');
    writeFileSync(log, '');
    const result = spawnSync('bash', [SCRIPT, ...args], {
      cwd: tmpdir(),
      encoding: 'utf8',
      env: {
        PATH: `${folder}:${process.env.PATH}`,
        GITHUB_REPOSITORY: 'Bmorrical/rusty-business',
        GITHUB_SHA: COMMIT,
        GITHUB_STEP_SUMMARY: summary,
        STUB_LOG: log,
        STUB_FAIL: '__never__',
        STUB_MACHINE: machine,
        STUB_DIGEST: DIGEST,
        STUB_PLATFORMS: 'linux/arm64\\nlinux/amd64\\n',
        ...env,
      },
    });
    const calls = readFileSync(log, 'utf8')
      .split('\n')
      .filter(Boolean)
      .map((line) => line.split(SEP).slice(1));
    let written = '';
    try {
      written = readFileSync(summary, 'utf8');
    } catch {
      // No summary was written.
    }
    return { status: result.status, stdout: result.stdout, stderr: result.stderr, calls, summary: written };
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
}

const NAME = 'ghcr.io/bmorrical/rusty-business';
const TAG = `sha-${COMMIT}`;

function buildOf(service, arch, prefix = NAME) {
  return [
    'build',
    '--file',
    'infra/docker/service.Dockerfile',
    '--build-arg',
    `SERVICE=${service}`,
    '--platform',
    `linux/${arch}`,
    '--provenance=false',
    '--sbom=false',
    '--label',
    'org.opencontainers.image.source=https://github.com/Bmorrical/rusty-business',
    '--label',
    `org.opencontainers.image.revision=${COMMIT}`,
    '--label',
    'org.opencontainers.image.licenses=MIT',
    '--label',
    `org.opencontainers.image.title=Rusty Business™ ${service}`,
    '--tag',
    `${prefix}/${service}:${TAG}-${arch}`,
    '.',
  ];
}

test('build pushes both services for the kind of computer it runs on', () => {
  for (const [arch, machine] of [
    ['amd64', 'x86_64'],
    ['arm64', 'aarch64'],
    ['arm64', 'arm64'],
  ]) {
    const result = run(['build', arch], { machine });
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(result.calls, [
      buildOf('api', arch),
      ['push', `${NAME}/api:${TAG}-${arch}`],
      buildOf('worker', arch),
      ['push', `${NAME}/worker:${TAG}-${arch}`],
    ]);
  }
});

test('build emulates nothing: asked for the other kind of computer, it stops', () => {
  for (const [arch, machine] of [
    ['arm64', 'x86_64'],
    ['amd64', 'aarch64'],
    ['amd64', 'riscv64'],
  ]) {
    const result = run(['build', arch], { machine });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /nothing is emulated/);
    assert.deepEqual(result.calls, []);
  }
});

test('the image names are lower case, and the source keeps the owner as GitHub writes it', () => {
  const result = run(['build', 'amd64'], { env: { GITHUB_REPOSITORY: 'BMorrical/Rusty-Business' } });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.calls[0].at(-2), `ghcr.io/bmorrical/rusty-business/api:${TAG}-amd64`);
  assert.ok(result.calls[0].includes('org.opencontainers.image.source=https://github.com/BMorrical/Rusty-Business'));
});

test('a registry on this computer can stand in for GitHub', () => {
  const result = run(['build', 'amd64'], { env: { REGISTRY: 'localhost:5001' } });
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(result.calls[0], buildOf('api', 'amd64', 'localhost:5001/bmorrical/rusty-business'));
});

test('nothing reaches Docker when the request or the settings are not plain', () => {
  const refused = [
    [['build', 'x86'], {}],
    [['build'], {}],
    [['build', 'amd64', 'arm64'], {}],
    [['join', 'extra'], {}],
    [['push'], {}],
    [[], {}],
    [['build', 'amd64'], { GITHUB_SHA: COMMIT.slice(0, 12) }],
    [['build', 'amd64'], { GITHUB_SHA: COMMIT.toUpperCase() }],
    [['build', 'amd64'], { GITHUB_SHA: `${COMMIT}\n` }],
    [['build', 'amd64'], { GITHUB_SHA: '' }],
    [['build', 'amd64'], { GITHUB_REPOSITORY: '' }],
    [['build', 'amd64'], { GITHUB_REPOSITORY: 'rusty-business' }],
    [['build', 'amd64'], { GITHUB_REPOSITORY: 'a/b/c' }],
    [['build', 'amd64'], { GITHUB_REPOSITORY: 'owner/../x' }],
    [['build', 'amd64'], { GITHUB_REPOSITORY: 'owner/.hidden' }],
    [['build', 'amd64'], { GITHUB_REPOSITORY: 'own er/name' }],
    [['build', 'amd64'], { REGISTRY: 'ghcr.io/somewhere' }],
    [['build', 'amd64'], { REGISTRY: 'https://ghcr.io' }],
    [['build', 'amd64'], { REGISTRY: 'GHCR.io' }],
    [['join'], { GITHUB_SHA: 'main' }],
  ];
  for (const [args, env] of refused) {
    const result = run(args, { env });
    assert.notEqual(result.status, 0, `${JSON.stringify(args)} ${JSON.stringify(env)} went through`);
    assert.match(result.stderr, /^publish-images: /m);
    assert.deepEqual(result.calls, [], `${JSON.stringify(args)} ${JSON.stringify(env)} reached Docker`);
  }
});

test('build stops at the first command that fails', () => {
  const failedBuild = run(['build', 'amd64'], { env: { STUB_FAIL: 'build*SERVICE=api*' } });
  assert.notEqual(failedBuild.status, 0);
  assert.deepEqual(failedBuild.calls, [buildOf('api', 'amd64')]);

  const failedPush = run(['build', 'amd64'], { env: { STUB_FAIL: 'push*/api:*' } });
  assert.notEqual(failedPush.status, 0);
  assert.deepEqual(failedPush.calls, [buildOf('api', 'amd64'), ['push', `${NAME}/api:${TAG}-amd64`]]);
});

function joinOf(service) {
  const name = `${NAME}/${service}`;
  return [
    [
      'buildx',
      'imagetools',
      'create',
      '--tag',
      `${name}:${TAG}`,
      '--annotation',
      'index:org.opencontainers.image.source=https://github.com/Bmorrical/rusty-business',
      '--annotation',
      `index:org.opencontainers.image.revision=${COMMIT}`,
      '--annotation',
      'index:org.opencontainers.image.licenses=MIT',
      '--annotation',
      `index:org.opencontainers.image.description=The Rusty Business™ ${service}, for the desktop app's Docker stack`,
      `${name}:${TAG}-amd64`,
      `${name}:${TAG}-arm64`,
    ],
    ['buildx', 'imagetools', 'inspect', `${name}:${TAG}`, '--format', '{{.Manifest.Digest}}'],
    [
      'buildx',
      'imagetools',
      'inspect',
      `${name}@${DIGEST}`,
      '--format',
      '{{range .Manifest.Manifests}}{{.Platform.OS}}/{{.Platform.Architecture}}{{"\\n"}}{{end}}',
    ],
  ];
}

test('join puts each service under one tag, and prints the digest to pin', () => {
  const result = run(['join']);
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(result.calls, [...joinOf('api'), ...joinOf('worker')]);
  assert.equal(result.stdout, `api ${NAME}/api@${DIGEST}\nworker ${NAME}/worker@${DIGEST}\n`);
  assert.equal(
    result.summary,
    `### The images to pin\n\n- api: \`${NAME}/api@${DIGEST}\`\n- worker: \`${NAME}/worker@${DIGEST}\`\n`,
  );
});

test('join runs anywhere: it builds nothing', () => {
  const result = run(['join'], { machine: 'riscv64' });
  assert.equal(result.status, 0, result.stderr);
});

test('join refuses a digest that is not plain, or an image that holds the wrong kinds', () => {
  for (const env of [
    { STUB_DIGEST: 'sha256:abc' },
    { STUB_DIGEST: `${DIGEST} extra` },
    { STUB_DIGEST: '' },
    { STUB_PLATFORMS: 'linux/amd64\\n' },
    { STUB_PLATFORMS: 'linux/amd64\\nlinux/arm64\\nunknown/unknown\\n' },
    { STUB_PLATFORMS: 'linux/amd64\\nlinux/arm\\n' },
    { STUB_FAIL: '*imagetools create*' },
    { STUB_FAIL: '*imagetools inspect*Platform*' },
  ]) {
    const result = run(['join'], { env });
    assert.notEqual(result.status, 0, `${JSON.stringify(env)} went through`);
    assert.equal(result.stdout, '', `${JSON.stringify(env)} printed a digest to pin`);
    assert.doesNotMatch(result.summary, /- api:/, `${JSON.stringify(env)} wrote a digest to pin`);
  }
});

test('the script signs in to nothing and removes nothing', () => {
  const text = readFileSync(SCRIPT, 'utf8')
    .split('\n')
    .filter((line) => !/^\s*#/.test(line))
    .join('\n');
  assert.doesNotMatch(text, /\blogin\b|\blogout\b|\brmi?\b|\bdelete\b|\bprune\b|--force|--push/);
});
