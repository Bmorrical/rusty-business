// What scripts/mac-sign.sh does with the Mac app in the Release run: the
// switch that signs it, off to start.
//
// Brad, 2026-11-19: "keep shipping self-signed as an option until all the
// peices are ready, when ready we can light it up, this is the bootstrapping
// but I don't want to lock yet to a signed release, I see there is paperwork
// involved". Then: "self signed for now".
//
// So these fail when the switch, off or unset, does anything but leave the
// .dmg as it is and say so; when a value that is neither on nor off, or a
// missing or malformed secret, lets the run go on; when the script would sign
// anywhere but GitHub's own Mac; when it touches any keychain but the one it
// makes for the run, or leaves that one behind; when a secret could reach the
// log; when it signs without the hardened runtime or a timestamp, or with an
// entitlement; or when it packs the .dmg another way than `make dmg` does.
//
// Every run here is of a copy of the script in a folder of its own, which
// holds no .dmg, so no run gets as far as a keychain or a signature.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const read = (file) => readFileSync(path.join(REPO, file), 'utf8');

const SCRIPT = read('scripts/mac-sign.sh');
const MAC_APP = read('scripts/mac-app.sh');

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
const commandsWith = (pattern) => COMMANDS.filter((command) => pattern.test(command));
const firstAt = (pattern) => COMMANDS.findIndex((command) => pattern.test(command));
const lastAt = (pattern) => COMMANDS.findLastIndex((command) => pattern.test(command));

/** The command that sets NAME, in a script's text. */
const setting = (text, name) => commandsOf(text).find((command) => command.startsWith(`${name}=`));

// Made up, in the shapes Apple gives. None of them signs anything.
const FAKE = {
  MAC_CERTIFICATE: Buffer.from('not a certificate').toString('base64'),
  MAC_CERTIFICATE_PASSWORD: 'not-a-password',
  MAC_NOTARY_KEY: '-----BEGIN PRIVATE KEY-----\nnot a key\n-----END PRIVATE KEY-----',
  MAC_NOTARY_KEY_ID: 'A1B2C3D4E5',
  MAC_NOTARY_ISSUER: '00000000-0000-4000-8000-000000000000',
};
const SECRETS = Object.keys(FAKE);

// The tools that would reach a keychain, a signature, Apple, or the .dmg.
const TOOLS = /(^|[\s(|;&])(security|codesign|xcrun|spctl|hdiutil|ditto|openssl|base64|sign|notarize)\b/;
// The last check before any of them: the .dmg against its checksum.
const LAST_CHECK = /^\(cd "\$WORK" && shasum -a 256 -c -s "\$DMG\.sha256"\) \|\| fail /;

/**
 * Runs a copy of the script, from a folder of its own, with only the settings
 * given and a PATH. Returns the run, what it told the next job, and what it
 * left in the runner's temporary folder.
 */
function run(env, ...args) {
  const folder = mkdtempSync(path.join(tmpdir(), 'mac-sign-'));
  try {
    mkdirSync(path.join(folder, 'scripts'));
    mkdirSync(path.join(folder, 'runner'));
    const copy = path.join(folder, 'scripts', 'mac-sign.sh');
    copyFileSync(path.join(REPO, 'scripts', 'mac-sign.sh'), copy);
    const output = path.join(folder, 'output');
    writeFileSync(output, '');
    const resolved = Object.fromEntries(
      Object.entries(env).map(([name, value]) => [name, value === RUNNER ? path.join(folder, 'runner') : value]),
    );
    const result = spawnSync('bash', [copy, ...args], {
      encoding: 'utf8',
      env: { PATH: process.env.PATH, GITHUB_OUTPUT: output, ...resolved },
    });
    return {
      status: result.status,
      stdout: result.stdout,
      stderr: result.stderr,
      told: readFileSync(output, 'utf8'),
      left: readdirSync(path.join(folder, 'runner')),
    };
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
}
// Stands for the run's own temporary folder, made fresh for each run.
const RUNNER = Symbol('runner');
const IN_GITHUB = { GITHUB_ACTIONS: 'true', RUNNER_TEMP: RUNNER };

/** Asserts the run stopped with `message`, told the next job nothing, and made nothing. */
function stopped(result, message) {
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.ok(result.stderr.includes(message), `expected "${message}", got: ${result.stderr}`);
  assert.equal(result.told, '', 'a run that stopped still told the next job something');
  assert.deepEqual(result.left, [], 'a run that stopped left something in the runner\'s temporary folder');
}

test('the script is a program the workflow can run as it is', () => {
  assert.ok(SCRIPT.startsWith('#!/usr/bin/env bash\n'));
  assert.ok(statSync(path.join(REPO, 'scripts', 'mac-sign.sh')).mode & 0o100, 'scripts/mac-sign.sh is not executable');
  assert.equal(COMMANDS[0], 'set -euo pipefail');
  assert.ok(COMMANDS.includes('export PATH=/usr/bin:/bin:/usr/sbin:/sbin'), 'the script keeps the PATH it was handed');
});

test('with the switch off or unset, the .dmg stays as make dmg made it, and the next job is told so', () => {
  for (const env of [{}, { MAC_SIGNING: '' }, { MAC_SIGNING: 'off' }, { MAC_SIGNING: 'off', ...FAKE }]) {
    const result = run(env);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout, 'Mac signing is off, so the .dmg stays as make dmg made it, signed for itself alone.\n');
    assert.equal(result.stderr, '');
    assert.equal(result.told, 'signed=false\n');
  }
  // Off, it asks for nothing: no secret, no Mac, and no GitHub.
  const off = COMMANDS.slice(0, firstAt(/^esac$/));
  assert.ok(!off.some((command) => /MAC_(CERTIFICATE|NOTARY)|uname|GITHUB_ACTIONS/.test(command)));
});

test('a switch that is neither on nor off, or an argument, stops the run', () => {
  for (const value of ['On', 'ON', 'true', 'yes', '1', ' on', 'on ', 'on\n', 'offf']) {
    stopped(run({ MAC_SIGNING: value, ...IN_GITHUB, ...FAKE }), 'MAC_SIGNING is on or off, and it is "');
  }
  for (const value of ['on', 'off']) {
    stopped(run({ MAC_SIGNING: value }, value), 'scripts/mac-sign.sh takes no argument. The switch is MAC_SIGNING, on or off.');
  }
});

test('with the switch on, it signs only in a GitHub run, with its temporary folder', () => {
  const message = "Mac signing runs only in GitHub's Release run, never on a computer of its own. Nothing was signed.";
  for (const env of [{}, { GITHUB_ACTIONS: 'false', RUNNER_TEMP: RUNNER }, { GITHUB_ACTIONS: 'true' }, { RUNNER_TEMP: RUNNER }, { GITHUB_ACTIONS: 'true', RUNNER_TEMP: '/no/such/folder' }]) {
    stopped(run({ MAC_SIGNING: 'on', ...FAKE, ...env }), message);
  }
});

test('with the switch on, a missing secret stops the run, and each one missing is named', () => {
  stopped(run({ MAC_SIGNING: 'on', ...IN_GITHUB }), `Mac signing is on, and these secrets are missing: ${SECRETS.join(' ')}. Nothing was signed.`);
  for (const name of SECRETS) {
    stopped(run({ MAC_SIGNING: 'on', ...IN_GITHUB, ...FAKE, [name]: '' }), `Mac signing is on, and these secrets are missing: ${name}. Nothing was signed.`);
  }
});

test('with the switch on, a secret that is not in the shape Apple gives stops the run', () => {
  const cases = {
    MAC_NOTARY_KEY_ID: [['a1b2c3d4e5', 'A1B2C3D4E', 'A1B2C3D4E5F', 'A1B2C3D4E5\n'], 'MAC_NOTARY_KEY_ID is not an App Store Connect key ID, ten letters and digits. Nothing was signed.'],
    MAC_NOTARY_ISSUER: [['00000000-0000-4000-8000-00000000000A', '000000000000400080000000000000000', 'not-an-issuer'], 'MAC_NOTARY_ISSUER is not an App Store Connect issuer ID. Nothing was signed.'],
    MAC_NOTARY_KEY: [['not a key', 'BEGIN PRIVATE KEY'], 'MAC_NOTARY_KEY is not the text of a .p8 key. Nothing was signed.'],
  };
  for (const [name, [values, message]] of Object.entries(cases)) {
    for (const value of values) {
      stopped(run({ MAC_SIGNING: 'on', ...IN_GITHUB, ...FAKE, [name]: value }), message);
    }
  }
});

test('with the switch on and every secret in shape, it still checks the computer and the .dmg before it makes anything', () => {
  // This run goes furthest, so it starts only once nothing that makes a
  // keychain, signs, or opens the .dmg can come before the last check.
  const last = firstAt(LAST_CHECK);
  assert.ok(last > 0, 'the script does not check the .dmg against its checksum');
  assert.ok(firstAt(TOOLS) > last, `${COMMANDS[firstAt(TOOLS)]} comes before the .dmg is checked`);
  assert.ok(firstAt(/^umask 077$/) > last && firstAt(/mktemp/) > last);

  const result = run({ MAC_SIGNING: 'on', ...IN_GITHUB, ...FAKE });
  const onMac = process.platform === 'darwin' && process.arch === 'arm64';
  // On a Mac, the copy's folder has no .dmg. Anywhere else, the computer is wrong.
  stopped(result, onMac
    ? 'There is no .dmg from make dmg, with its checksum, in '
    : 'Mac signing runs on a Mac with Apple silicon. Nothing was signed.');
  // The secrets' values never reach the log.
  for (const value of Object.values(FAKE)) {
    assert.ok(!result.stdout.includes(value) && !result.stderr.includes(value), 'a secret reached the log');
  }
});

test('the checks come in order: the switch, GitHub, the secrets, the computer, then the .dmg', () => {
  const order = [
    /^case "\$\{MAC_SIGNING:-\}" in$/,
    /GITHUB_ACTIONS/,
    /^\[ -z "\$missing" \]/,
    /MAC_NOTARY_KEY_ID" =~/,
    /MAC_NOTARY_ISSUER" =~/,
    /MAC_NOTARY_KEY" ==/,
    /uname -s/,
    /\$WORK\/\$DMG\.sha256" \] \|\|$/,
    LAST_CHECK,
  ].map(firstAt);
  assert.ok(order.every((at) => at >= 0), 'a check is missing');
  assert.deepEqual([...order].sort((a, b) => a - b), order);
});

test('a secret appears only where it is checked or used, never in a message', () => {
  const secret = new RegExp(`\\$\\{?(${SECRETS.join('|')})\\b`);
  assert.deepEqual(commandsWith(secret), [
    '[[ "$MAC_NOTARY_KEY_ID" =~ ^[A-Z0-9]{10}$ ]] ||',
    '[[ "$MAC_NOTARY_ISSUER" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]] ||',
    '[[ "$MAC_NOTARY_KEY" == *"-----BEGIN PRIVATE KEY-----"* ]] ||',
    'printf \'%s\' "$MAC_CERTIFICATE" | base64 --decode >"$scratch/certificate.p12" ||',
    'security import "$scratch/certificate.p12" -k "$keychain" -f pkcs12 -P "$MAC_CERTIFICATE_PASSWORD" -T /usr/bin/codesign >/dev/null ||',
    'printf \'%s\\n\' "$MAC_NOTARY_KEY" >"$scratch/notary.p8"',
    'notary=(--key "$scratch/notary.p8" --key-id "$MAC_NOTARY_KEY_ID" --issuer "$MAC_NOTARY_ISSUER")',
  ]);
  assert.ok(!/set -x|xtrace/.test(SCRIPT), 'the script prints its commands, secrets and all');
});

test("it signs with a keychain of its own, in the runner's temporary folder, and removes it at the end", () => {
  assert.ok(COMMANDS.includes('scratch=$(mktemp -d "$RUNNER_TEMP/mac-sign.XXXXXX")'));
  assert.ok(COMMANDS.includes('keychain=$scratch/signing.keychain-db'));
  assert.ok(COMMANDS.includes('trap finish EXIT'));
  const finish = COMMANDS.slice(firstAt(/^finish\(\) \{$/), firstAt(/^trap finish EXIT$/));
  assert.ok(finish.includes('security delete-keychain "$keychain" >/dev/null 2>&1 || true'));
  assert.ok(finish.includes('rm -rf "$scratch"'));
  // Every security command names this run's keychain or its own file, and
  // the one that changes the search list keeps the others on it.
  for (const command of commandsWith(/(^|[\s(|])security /)) {
    assert.ok(/"\$keychain"|security list-keychains -d user\)?$/.test(command), `security reaches past its keychain: ${command}`);
    assert.ok(!/default-keychain|login\.keychain|-d (system|common|dynamic)|delete-identity|delete-certificate|add-trusted|remove-trusted|authorizationdb/.test(command), command);
  }
  assert.ok(COMMANDS.includes('security list-keychains -d user -s "$keychain" ${others[@]+"${others[@]}"}'));
  // It removes only what it made.
  assert.deepEqual(commandsWith(/(^|[\s(|;&])rm /), [
    'rm -rf "$scratch"',
    'rm -f "$scratch/certificate.p12"',
    'rm -f "$WORK/$APP_TGZ" "$WORK/$APP_TGZ.sha256"',
    'rm -f "$WORK/$DMG" "$WORK/$DMG.sha256"',
  ]);
  assert.ok(!/\$HOME|~\//.test(COMMANDS.join('\n')), 'the script reaches into the home folder');
});

test('it opens the .dmg read-only, after its checksum, and takes the app out to sign it', () => {
  assert.deepEqual(commandsWith(/hdiutil attach/), [
    'hdiutil attach -quiet -readonly -nobrowse -noautoopen -mountpoint "$scratch/built" "$WORK/$DMG"',
  ]);
  assert.ok(firstAt(/hdiutil attach/) > firstAt(LAST_CHECK));
  assert.ok(COMMANDS.includes('ditto "$scratch/built/$APP.app" "$scratch/app/$APP.app"'));
});

test('every program file is signed with the hardened runtime and a timestamp, with no entitlement, then the app seals them', () => {
  assert.deepEqual(commandsWith(/(^|\s)codesign /), [
    'codesign --force --timestamp --keychain "$keychain" --sign "$identity" "$@"',
    'codesign --verify --deep --strict "$app"',
    'codesign --verify --strict "$WORK/$DMG"',
  ]);
  assert.deepEqual(commandsWith(/^sign /), [
    'sign --options runtime "$path"',
    'sign --options runtime "$app"',
    'sign "$WORK/$DMG"',
  ]);
  assert.ok(!/entitlements/.test(COMMANDS.join('\n')), 'the script signs with an entitlement');
  assert.ok(firstAt(/^sign --options runtime "\$path"$/) < firstAt(/^sign --options runtime "\$app"$/), 'the app is signed before what is inside it');
  assert.ok(COMMANDS.includes('[ "$parts" -gt 0 ] || fail "The app holds no program files but its own, so the engine is missing. Nothing was signed."'));
});

test('the app and the .dmg are each notarized, stapled, and checked, and only then is the next job told they are signed', () => {
  const steps = [
    'notarize "$scratch/app.zip"',
    'xcrun stapler staple "$app"',
    'xcrun stapler validate "$app"',
    'spctl --assess --type execute "$app"',
    'notarize "$WORK/$DMG"',
    'xcrun stapler staple "$WORK/$DMG"',
    'xcrun stapler validate "$WORK/$DMG"',
    'spctl --assess --type open --context context:primary-signature "$WORK/$DMG"',
    '(cd "$WORK" && shasum -a 256 "$DMG" >"$DMG.sha256")',
    'tell signed true',
  ].map((step) => COMMANDS.indexOf(step));
  assert.ok(steps.every((at) => at >= 0), 'a step is missing');
  assert.deepEqual([...steps].sort((a, b) => a - b), steps);
  assert.equal(lastAt(/^tell signed true$/), firstAt(/^tell signed true$/));
  assert.ok(firstAt(/^tell signed true$/) > lastAt(/^(notarize |xcrun stapler |spctl )/));
  // Anything Apple does not accept stops the run.
  assert.ok(COMMANDS.some((command) => command.startsWith('fail "Apple did not accept $(basename "$1"): ${status:-no answer}. Nothing was published."')));
});

// Brad, 2026-11-19: "when can we do an in app install rather than a new dmg
// download?" Install and restart puts the app from the .app.tar.gz in place,
// so once the app is signed, that file holds the signed, stapled app too.
test('the .app.tar.gz is packed again around the stapled app, the way make dmg packs it, under the same name, before the next job is told', () => {
  assert.ok(setting(SCRIPT, 'APP_TGZ'), 'the script names no .app.tar.gz');
  assert.equal(setting(SCRIPT, 'APP_TGZ'), setting(MAC_APP, 'APP_TGZ'));
  const packed = commandsOf(MAC_APP).filter((command) => /^tar\s.*-czf/.test(command));
  assert.equal(packed.length, 1);
  assert.deepEqual(commandsWith(/^tar\s/), [packed[0].replace('"$WORK/app"', '"$scratch/app"')]);
  const order = [
    /^xcrun stapler validate "\$app"$/,
    /^spctl --assess --type execute "\$app"$/,
    /^rm -f "\$WORK\/\$APP_TGZ" "\$WORK\/\$APP_TGZ\.sha256"$/,
    /^tar\s/,
    /^\(cd "\$WORK" && shasum -a 256 "\$APP_TGZ" >"\$APP_TGZ\.sha256"\)$/,
    /^tell signed true$/,
  ].map(firstAt);
  assert.ok(order.every((at) => at >= 0), `a step is missing: ${order}`);
  assert.deepEqual(order, [...order].sort((a, b) => a - b), 'the steps are out of order');
});

test('the .dmg is packed again the way make dmg packs it, under the same name', () => {
  assert.equal(setting(SCRIPT, 'DMG'), setting(MAC_APP, 'DMG'));
  const made = commandsOf(MAC_APP).filter((command) => command.startsWith('hdiutil create '));
  assert.equal(made.length, 1);
  assert.deepEqual(commandsWith(/^hdiutil create /), [made[0].replace('"$WORK/app"', '"$scratch/app"')]);
  assert.ok(COMMANDS.includes('ln -s /Applications "$scratch/app/Applications"'));
});
