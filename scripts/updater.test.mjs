// What scripts/updater.mjs signs and checks, run on a copy of the files it
// reads, in a folder made for each test, with a key made for that test and
// thrown away after it. No key of the app's is read, and nothing is released.
//
// Brad, 2026-11-19: "when can we do an in app install rather than a new dmg
// download? is that after the signing? if we can do it now, then roll that
// next". Asked how, he picked "Click to install" and ".deb and .rpm". Install
// and restart installs a file only when its signature in latest.json matches
// the key built into the app, so the release run signs the Mac's
// .app.tar.gz, the Windows installer, the .deb, and the .rpm, and lists them
// in latest.json. This is the script that does it.

import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import {
  appendFileSync,
  cpSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
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
const read = (file) => readFileSync(path.join(REPO, file), 'utf8');
const SCRIPT = read('scripts/updater.mjs');
const MANIFEST = 'latest.json';
const { version: VERSION } = JSON.parse(read('apps/desktop/package.json'));
const NEXT = VERSION.replace(/^(\d+)\.(\d+)\.\d+$/, (_, major, minor) => `${major}.${Number(minor) + 1}.0`);

// The files the script signs, as it names them, and where it says they are.
const PACKAGES = [...SCRIPT.matchAll(/^ {2}\['([a-z0-9_-]+)', '([^']+)'\],$/gm)].map(([, target, file]) => [target, file]);
const RELEASES = /^const RELEASES = '([^']+)';$/m.exec(SCRIPT)?.[1];
const [MAC, WINDOWS, DEB, RPM] = PACKAGES.map(([, file]) => file);
const addressOf = (version, file) => `${RELEASES}/v${version}/${file}`;

// The files the script reads, copied into each test's folder.
const READS = ['apps/desktop/package.json', 'apps/desktop/src-tauri/tauri.conf.json', 'apps/desktop/src-tauri/Cargo.toml', 'scripts/updater.mjs'];

// tauri signer generate's scrypt limits: N = 2^15, r = 8, p = 1.
const TAURI_LIMITS = { opslimit: 1048576n, memlimit: 33554432n };
const PASSWORD = `a password made for this test alone ${crypto.randomBytes(8).toString('hex')}`;

const b64 = (bytes) => Buffer.from(bytes).toString('base64');
const le64 = (value) => {
  const bytes = Buffer.alloc(8);
  bytes.writeBigUInt64LE(value);
  return bytes;
};

// A key as tauri signer generate writes it: minisign's, Ed25519, locked with
// the password by scrypt. scrypt's settings are worked out here by hand for
// the two limits used, so the test does not borrow the script's own sums.
function makeKey({ password = PASSWORD, limits = TAURI_LIMITS, settings = { N: 2 ** 15, r: 8, p: 1 }, id = crypto.randomBytes(8), kind = 'EdScB2' } = {}) {
  const { privateKey, publicKey } = crypto.generateKeyPairSync('ed25519');
  const seed = privateKey.export({ format: 'der', type: 'pkcs8' }).subarray(16);
  const pk = publicKey.export({ format: 'der', type: 'spki' }).subarray(12);
  const salt = crypto.randomBytes(32);
  const plain = Buffer.concat([id, seed, pk, Buffer.alloc(32)]);
  const stream = crypto.scryptSync(Buffer.from(password, 'utf8'), salt, 104, { ...settings, maxmem: 256 * 1024 * 1024 });
  const locked = Buffer.from(plain.map((byte, i) => byte ^ stream[i]));
  const box = Buffer.concat([Buffer.from(kind, 'latin1'), salt, le64(limits.opslimit), le64(limits.memlimit), locked]);
  const text = b64(`untrusted comment: rsign encrypted secret key\n${b64(box)}\n`);
  const hex = Buffer.from(id).reverse().toString('hex').toUpperCase();
  const pubkey = b64(`untrusted comment: minisign public key: ${hex}\n${b64(Buffer.concat([Buffer.from('Ed', 'latin1'), id, pk]))}\n`);
  return { text, pubkey, id, seed, privateKey, publicKey };
}

// A folder with copies of the files the script reads, the app's public key
// swapped for this test's, and a dist folder holding the four files.
function folderFor(key) {
  const root = mkdtempSync(path.join(tmpdir(), 'updater-test-'));
  for (const file of READS) {
    mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    writeFileSync(path.join(root, file), read(file));
  }
  const conf = path.join(root, 'apps/desktop/src-tauri/tauri.conf.json');
  const settings = JSON.parse(readFileSync(conf, 'utf8'));
  settings.plugins.updater.pubkey = key.pubkey;
  writeFileSync(conf, `${JSON.stringify(settings, null, 2)}\n`);
  const dist = path.join(root, 'dist');
  mkdirSync(dist);
  for (const [target, file] of PACKAGES) writeFileSync(path.join(dist, file), `not really the ${target} file\n${crypto.randomBytes(64).toString('hex')}\n`);
  return { root, dist };
}

// Runs the copy, with no environment but the two secrets it is given.
function run(root, args, env = {}) {
  const result = spawnSync(process.execPath, [path.join(root, 'scripts/updater.mjs'), ...args], { env, encoding: 'utf8' });
  return { status: result.status, stdout: result.stdout, stderr: result.stderr };
}

const secrets = (key, password = PASSWORD) => ({ TAURI_SIGNING_PRIVATE_KEY: key.text, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: password });
const manifestOf = (dist) => JSON.parse(readFileSync(path.join(dist, MANIFEST), 'utf8'));
const writeManifest = (dist, manifest) => writeFileSync(path.join(dist, MANIFEST), `${JSON.stringify(manifest, null, 2)}\n`);

// A signature in minisign's format, made here: for the cases the script must
// refuse, and to check the ones it makes without its own code.
function signatureLike({ key, file, name, version, time = 1763640000, id = key.id }) {
  const hash = crypto.createHash('blake2b512').update(readFileSync(file)).digest();
  const signature = crypto.sign(null, hash, key.privateKey);
  const comment = `timestamp:${time}\tfile:${name}\tversion:${version}`;
  const whole = crypto.sign(null, Buffer.concat([signature, Buffer.from(comment, 'utf8')]), key.privateKey);
  return b64(`untrusted comment: signature from tauri secret key\n${b64(Buffer.concat([Buffer.from('ED', 'latin1'), id, signature]))}\ntrusted comment: ${comment}\n${b64(whole)}\n`);
}

// Reads a signature the script made, and checks it here.
function opened(signature, key, file) {
  const lines = Buffer.from(signature, 'base64').toString('utf8').split('\n');
  assert.equal(lines.length, 5);
  assert.equal(lines[0], 'untrusted comment: signature from tauri secret key');
  assert.equal(lines[4], '');
  const line = Buffer.from(lines[1], 'base64');
  assert.equal(line.subarray(0, 2).toString('latin1'), 'ED', 'a signature of the file\'s BLAKE2b-512 hash, as minisign makes by default');
  assert.deepEqual(line.subarray(2, 10), key.id);
  const hash = crypto.createHash('blake2b512').update(readFileSync(file)).digest();
  assert.ok(crypto.verify(null, hash, key.publicKey, line.subarray(10)), 'the signature matches the file');
  assert.ok(lines[2].startsWith('trusted comment: '));
  const comment = lines[2].slice('trusted comment: '.length);
  assert.ok(crypto.verify(null, Buffer.concat([line.subarray(10), Buffer.from(comment, 'utf8')]), key.publicKey, Buffer.from(lines[3], 'base64')), 'the comment is signed too');
  return comment;
}

function everyFile(folder) {
  return readdirSync(folder, { recursive: true })
    .map((name) => path.join(folder, name))
    .filter((file) => lstatSync(file).isFile());
}

test('the files and the addresses are the ones the app asks for', () => {
  const rust = read('apps/desktop/src-tauri/src/update.rs');
  const constant = (name) => new RegExp(`^const ${name}: &str = "([^"]+)";$`, 'm').exec(rust)?.[1];
  const packages = [...rust.matchAll(/Download::\w+ => \("([^"]+)", "([^"]+)"\),/g)].map(([, file, target]) => [target, file]);
  assert.equal(packages.length, 4, 'update.rs names four files that install');
  assert.deepEqual(PACKAGES, packages, 'the same four files, under the same names, in the same order');
  assert.equal(RELEASES, `https://${constant('SITE_HOST')}/${constant('REPOSITORY')}/releases/download`);
  assert.equal(constant('MANIFEST'), MANIFEST);
  assert.match(SCRIPT, new RegExp(`^const MANIFEST = '${MANIFEST}';$`, 'm'));

  const value = (file, name) => new RegExp(`^${name}=(\\S+)$`, 'm').exec(read(file))?.[1];
  assert.equal(MAC, value('scripts/mac-app.sh', 'APP_TGZ'), 'the .app.tar.gz make dmg packs');
  assert.equal(WINDOWS, value('scripts/windows-app.sh', 'EXE'));
  assert.equal(DEB, value('scripts/linux-app.sh', 'DEB'));
  assert.equal(RPM, value('scripts/linux-app.sh', 'RPM'));
  assert.ok(!PACKAGES.some(([, file]) => file === value('scripts/linux-app.sh', 'TGZ')), 'the .tar.gz updates by download, and is not signed');
});

test('sign writes latest.json, naming this version and the four files on its release, each signed by the app\'s key, as Install and restart reads them', () => {
  const key = makeKey();
  const { root, dist } = folderFor(key);
  try {
    const result = run(root, ['sign', dist], secrets(key));
    assert.equal(result.stderr, '');
    assert.equal(result.status, 0);
    assert.equal(result.stdout, `Signed 4 files for version ${VERSION}.\n${MANIFEST} names version ${VERSION} and its 4 files, and each signature matches.\n`);

    const manifest = manifestOf(dist);
    assert.deepEqual(Object.keys(manifest), ['version', 'pub_date', 'platforms']);
    assert.equal(manifest.version, VERSION);
    assert.match(manifest.pub_date, /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
    assert.ok(Math.abs(Date.parse(manifest.pub_date) - Date.now()) < 5 * 60 * 1000, 'the time it was signed');
    assert.deepEqual(Object.keys(manifest.platforms), PACKAGES.map(([target]) => target));
    for (const [target, file] of PACKAGES) {
      const entry = manifest.platforms[target];
      assert.deepEqual(Object.keys(entry), ['signature', 'url']);
      assert.equal(entry.url, addressOf(VERSION, file));
      const comment = opened(entry.signature, key, path.join(dist, file));
      assert.match(comment, new RegExp(`^timestamp:[0-9]+\\tfile:${file.replaceAll('.', '\\.')}\\tversion:${VERSION.replaceAll('.', '\\.')}$`), 'the plugin reads the version from the signed comment');
    }
    assert.deepEqual(readdirSync(dist).sort(), [...PACKAGES.map(([, file]) => file), MANIFEST].sort(), 'it adds latest.json alone');

    const again = run(root, ['check', dist]);
    assert.equal(again.status, 0, again.stderr);
    assert.equal(again.stdout, `${MANIFEST} names version ${VERSION} and its 4 files, and each signature matches.\n`);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('the key and its password reach neither the log nor a file', () => {
  const key = makeKey();
  const { root, dist } = folderFor(key);
  try {
    const outputs = [run(root, ['sign', dist], secrets(key)), run(root, ['check', dist]), run(root, ['sign', dist], secrets(key)), run(root, ['sign', dist], secrets(key, `${PASSWORD} wrong`))];
    assert.equal(outputs[0].status, 0, outputs[0].stderr);
    const needles = [key.text, PASSWORD, key.seed.toString('hex'), key.seed.toString('base64'), Buffer.from(key.text, 'base64').toString('utf8').split('\n')[1]];
    for (const { stdout, stderr } of outputs) {
      for (const needle of needles) assert.ok(!`${stdout}${stderr}`.includes(needle), 'nothing of the key in the log');
    }
    for (const file of everyFile(root)) {
      const text = readFileSync(file);
      for (const needle of needles) assert.ok(!text.includes(needle), `nothing of the key in ${path.relative(root, file)}`);
      assert.ok(!text.includes(key.seed), `nothing of the key in ${path.relative(root, file)}`);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('sign stops, with nothing signed, unless it holds the app\'s own key and its password', () => {
  const key = makeKey();
  const { root, dist } = folderFor(key);
  const stops = (name, env, message) => {
    const result = run(root, ['sign', dist], env);
    assert.equal(result.stderr, `updater: ${message}\n`, name);
    assert.equal(result.status, 1, name);
    assert.equal(result.stdout, '', name);
    assert.ok(!existsSync(path.join(dist, MANIFEST)), `${name}: no latest.json`);
  };
  const NOT_A_KEY = 'TAURI_SIGNING_PRIVATE_KEY is not the text of a key file from tauri signer generate. Nothing was signed.';
  const NOT_LOCKED = 'TAURI_SIGNING_PRIVATE_KEY is not an Ed25519 key of minisign, locked with a password. Nothing was signed.';
  try {
    stops('no key', { TAURI_SIGNING_PRIVATE_KEY_PASSWORD: PASSWORD }, 'TAURI_SIGNING_PRIVATE_KEY is not set. Nothing was signed.');
    stops('an empty key', { ...secrets(key), TAURI_SIGNING_PRIVATE_KEY: ' \n' }, 'TAURI_SIGNING_PRIVATE_KEY is not set. Nothing was signed.');
    stops('no password', { TAURI_SIGNING_PRIVATE_KEY: key.text }, 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD is not set. Nothing was signed.');
    stops('an empty password', secrets(key, ''), 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD is not set. Nothing was signed.');
    stops('the wrong password', secrets(key, `${PASSWORD} wrong`), 'The password does not open TAURI_SIGNING_PRIVATE_KEY. Nothing was signed.');
    stops('another key, with the same password', secrets(makeKey()), 'TAURI_SIGNING_PRIVATE_KEY is not the key whose public half tauri.conf.json holds. Nothing was signed.');
    stops('another key, with the same key id', secrets(makeKey({ id: key.id })), 'TAURI_SIGNING_PRIVATE_KEY is not the key whose public half tauri.conf.json holds. Nothing was signed.');
    stops('the public key', secrets({ text: key.pubkey }), NOT_LOCKED);
    stops('not base64', secrets({ text: 'not a key' }), NOT_A_KEY);
    stops('base64 of something else', secrets({ text: b64('hello\n') }), NOT_A_KEY);
    stops('a key file holding more', secrets({ text: b64(`${Buffer.from(key.text, 'base64').toString('utf8')}more\n`) }), NOT_A_KEY);
    stops('a key with no password', secrets(makeKey({ kind: 'Ed\0\0B2' })), NOT_LOCKED);
    stops(
      'a key that asks scrypt for a gigabyte',
      secrets(makeKey({ limits: { opslimit: 33554432n, memlimit: 1073741824n }, settings: { N: 2 ** 10, r: 8, p: 1 } })),
      'The key asks for other scrypt settings than a key of tauri signer generate. Nothing was signed.',
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('sign stops, with nothing signed, when a file is missing, empty, or a link, or latest.json is already there', () => {
  const key = makeKey();
  const { root, dist } = folderFor(key);
  const stops = (name, change, message, args = ['sign', dist]) => {
    const folder = path.join(root, name.replaceAll(' ', '-'));
    cpSync(dist, folder, { recursive: true });
    change(folder);
    const before = existsSync(path.join(folder, MANIFEST)) ? readFileSync(path.join(folder, MANIFEST), 'utf8') : null;
    const result = run(root, args.map((arg) => (arg === dist ? folder : arg)), secrets(key));
    assert.equal(result.stderr, `updater: ${message(folder)}\n`, name);
    assert.equal(result.status, 1, name);
    const after = existsSync(path.join(folder, MANIFEST)) ? readFileSync(path.join(folder, MANIFEST), 'utf8') : null;
    assert.equal(after, before, `${name}: latest.json as it was`);
  };
  try {
    stops('no Mac file', (folder) => rmSync(path.join(folder, MAC)), (folder) => `${folder} holds no ${MAC}`);
    stops('an empty rpm', (folder) => writeFileSync(path.join(folder, RPM), ''), (folder) => `${RPM} in ${folder} is not a plain file with something in it`);
    stops(
      'a deb that is a link',
      (folder) => {
        rmSync(path.join(folder, DEB));
        symlinkSync(path.join(folder, RPM), path.join(folder, DEB));
      },
      (folder) => `${DEB} in ${folder} is not a plain file with something in it`,
    );
    stops('a latest.json already there', (folder) => writeFileSync(path.join(folder, MANIFEST), 'one from before\n'), (folder) => `${folder} already holds a latest.json. Nothing was signed.`);
    stops('a folder that is not one', () => {}, () => `${path.join(root, 'nowhere')} is not a folder`, ['sign', path.join(root, 'nowhere')]);

    const usage = 'use: node scripts/updater.mjs sign <folder>, or node scripts/updater.mjs check <folder>';
    for (const args of [[], ['sign'], ['publish', dist], ['sign', dist, dist], ['--help']]) {
      const result = run(root, args, secrets(key));
      assert.equal(result.stderr, `updater: ${usage}\n`, args.join(' '));
      assert.equal(result.status, 1);
    }
    assert.ok(!existsSync(path.join(dist, MANIFEST)));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('check stops unless latest.json names this version and the four files on its release, and nothing else, and each signature matches', () => {
  const key = makeKey();
  const { root, dist } = folderFor(key);
  try {
    assert.equal(run(root, ['sign', dist], secrets(key)).status, 0);
    const signed = manifestOf(dist);
    const stops = (name, change, message) => {
      const folder = path.join(root, name.replaceAll(/[^a-z0-9]+/gi, '-'));
      cpSync(dist, folder, { recursive: true });
      const manifest = structuredClone(signed);
      const changed = change(manifest, folder);
      if (changed !== 'written') writeManifest(folder, manifest);
      const result = run(root, ['check', folder]);
      assert.equal(result.stderr, `updater: ${typeof message === 'function' ? message(folder) : message}\n`, name);
      assert.equal(result.status, 1, name);
      assert.equal(result.stdout, '', name);
    };
    const signature = (file, why) => `the signature for ${file} in ${MANIFEST} ${why}`;
    const comment = (entry, from, to) => {
      const lines = Buffer.from(entry.signature, 'base64').toString('utf8').split('\n');
      lines[2] = lines[2].replace(from, to);
      entry.signature = b64(lines.join('\n'));
    };

    stops('a file changed after it was signed', (_, folder) => appendFileSync(path.join(folder, DEB), 'one more line\n'), signature(DEB, 'does not match the file'));
    stops(
      'two signatures swapped',
      (manifest) => {
        const deb = manifest.platforms['linux-x86_64-deb'];
        const rpm = manifest.platforms['linux-x86_64-rpm'];
        [deb.signature, rpm.signature] = [rpm.signature, deb.signature];
      },
      signature(DEB, `names another file: ${RPM}`),
    );
    stops('a signature for another version', (manifest) => comment(manifest.platforms['windows-x86_64'], `version:${VERSION}`, `version:${NEXT}`), signature(WINDOWS, `names another version: ${NEXT}`));
    stops('a signed comment changed', (manifest) => comment(manifest.platforms['darwin-aarch64'], /timestamp:[0-9]+/, 'timestamp:1'), signature(MAC, 'does not match its own comment'));
    stops('a comment without its version', (manifest) => comment(manifest.platforms['darwin-aarch64'], `\tversion:${VERSION}`, ''), signature(MAC, 'does not say its time, its file, and its version'));
    stops(
      'a signature from another key',
      (manifest, folder) => {
        manifest.platforms['linux-x86_64-rpm'].signature = signatureLike({ key: makeKey(), file: path.join(folder, RPM), name: RPM, version: VERSION });
      },
      signature(RPM, 'is from another key than the app\'s'),
    );
    stops(
      'a signature from another key with the app\'s key id',
      (manifest, folder) => {
        manifest.platforms['linux-x86_64-rpm'].signature = signatureLike({ key: makeKey(), file: path.join(folder, RPM), name: RPM, version: VERSION, id: key.id });
      },
      signature(RPM, 'does not match the file'),
    );
    stops('a signature that is not base64', (manifest) => (manifest.platforms['windows-x86_64'].signature = 'not a signature'), signature(WINDOWS, 'is not base64'));
    stops('a signature that is something else', (manifest) => (manifest.platforms['windows-x86_64'].signature = b64('hello\n')), signature(WINDOWS, 'is not a minisign signature'));
    stops('another version', (manifest) => (manifest.version = NEXT), `${MANIFEST} names version ${NEXT}, and the app is ${VERSION}`);
    stops('a file sent elsewhere', (manifest) => (manifest.platforms['windows-x86_64'].url = 'https://example.com/setup.exe'), `${MANIFEST} sends windows-x86_64 to https://example.com/setup.exe`);
    stops('a file of another release', (manifest) => (manifest.platforms['darwin-aarch64'].url = addressOf(NEXT, MAC)), `${MANIFEST} sends darwin-aarch64 to ${addressOf(NEXT, MAC)}`);
    stops('a time with its milliseconds', (manifest) => (manifest.pub_date = '2026-11-20T12:00:00.000Z'), `${MANIFEST} gives no plain time, such as 2026-11-20T12:00:00Z: 2026-11-20T12:00:00.000Z`);
    stops('notes in latest.json', (manifest) => (manifest.notes = 'more'), `${MANIFEST} holds other than version, pub_date, and platforms`);
    stops('no time', (manifest) => delete manifest.pub_date, `${MANIFEST} holds other than version, pub_date, and platforms`);
    const systems = `${MANIFEST} names other systems than ${PACKAGES.map(([target]) => target).join(', ')}`;
    stops('a system missing', (manifest) => delete manifest.platforms['linux-x86_64-rpm'], systems);
    stops('the tar.gz as a system', (manifest) => (manifest.platforms['linux-x86_64'] = manifest.platforms['linux-x86_64-deb']), systems);
    stops(
      'the systems in another order',
      (manifest) => {
        const { 'darwin-aarch64': mac, ...rest } = manifest.platforms;
        manifest.platforms = { ...rest, 'darwin-aarch64': mac };
      },
      systems,
    );
    stops('more for one system', (manifest) => (manifest.platforms['windows-x86_64'].with_elevated_task = true), `${MANIFEST} holds other than a signature and an address for windows-x86_64`);
    stops('a file gone', (_, folder) => rmSync(path.join(folder, MAC)), (folder) => `${folder} holds no ${MAC}`);
    stops('not JSON', (_, folder) => (writeFileSync(path.join(folder, MANIFEST), '{"version":\n'), 'written'), `${MANIFEST} is not JSON`);
    stops('a list', (_, folder) => (writeFileSync(path.join(folder, MANIFEST), '[]\n'), 'written'), `${MANIFEST} holds other than version, pub_date, and platforms`);
    stops('larger than the app reads', (manifest, folder) => (writeFileSync(path.join(folder, MANIFEST), `${JSON.stringify(manifest)}${' '.repeat(1024 * 1024)}`), 'written'), `${MANIFEST} is larger than the app reads`);
    stops(
      'latest.json as a link',
      (manifest, folder) => {
        writeManifest(path.join(folder, '..'), manifest);
        rmSync(path.join(folder, MANIFEST));
        symlinkSync(path.join(folder, '..', MANIFEST), path.join(folder, MANIFEST));
        return 'written';
      },
      (folder) => `${MANIFEST} in ${folder} is not a plain file with something in it`,
    );
    stops('an empty latest.json', (_, folder) => (writeFileSync(path.join(folder, MANIFEST), ''), 'written'), (folder) => `${MANIFEST} in ${folder} is not a plain file with something in it`);

    assert.equal(run(root, ['check', dist]).status, 0, 'the folder it signed still checks');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('check and sign stop when the app\'s versions differ, or the public key is not minisign\'s', () => {
  const key = makeKey();
  const { root, dist } = folderFor(key);
  const conf = path.join(root, 'apps/desktop/src-tauri/tauri.conf.json');
  const original = readFileSync(conf, 'utf8');
  try {
    writeFileSync(conf, original.replace(`"version": "${VERSION}"`, `"version": "${NEXT}"`));
    let result = run(root, ['sign', dist], secrets(key));
    assert.match(result.stderr, new RegExp(`^updater: the app's versions differ: package\\.json ${VERSION.replaceAll('.', '\\.')}, tauri\\.conf\\.json ${NEXT.replaceAll('.', '\\.')}, Cargo\\.toml `));
    assert.equal(result.status, 1);

    writeFileSync(conf, original.replace(key.pubkey, b64('not a key\n')));
    result = run(root, ['check', dist]);
    assert.equal(result.stderr, 'updater: the public key in tauri.conf.json is not a minisign public key\n');
    assert.equal(result.status, 1);
    assert.ok(!existsSync(path.join(dist, MANIFEST)));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('the script uses Node\'s own modules alone, runs no other program, reads no setting but the two secrets, and writes latest.json alone', () => {
  const imports = [...SCRIPT.matchAll(/^import .* from '([^']+)';$/gm)].map(([, name]) => name);
  assert.deepEqual(imports, ['node:crypto', 'node:fs', 'node:path', 'node:url']);
  assert.ok(!/\bimport\s*\(|\brequire\s*\(/.test(SCRIPT), 'no other module, loaded any other way');
  const code = SCRIPT.replace(/^\/\/.*$/gm, '').replace(RELEASES, '');
  assert.ok(!/child_process|\bexecFile|\bexecSync|\bspawn|\bfork\(|\bfetch\(|\bhttps?\b|node:net|\beval\(|new Function|\bWorker\b|process\.binding|dlopen/.test(code), 'no other program and no request');
  assert.deepEqual([...new Set(SCRIPT.match(/process\.env\.\w+/g))], ['process.env.TAURI_SIGNING_PRIVATE_KEY', 'process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD']);
  const writes = SCRIPT.match(/\bfs\.\w*(?:write|append|rm|unlink|rename|copy|mkdir|symlink|link|chmod|truncate)\w*/gi) ?? [];
  assert.deepEqual(writes, ['fs.writeFileSync'], 'one write');
  assert.deepEqual(SCRIPT.match(/fs\.openSync\([^)]*\)/g), ["fs.openSync(file, 'r')"], 'each file opened to read');
  assert.match(SCRIPT, /fs\.writeFileSync\(path\.join\(folder, MANIFEST\), .*\{ flag: 'wx' \}\);/, 'latest.json, and only when it is not there');
});

test('no file in the repository holds a key that signs', () => {
  // Built here, so this file does not hold them.
  const needles = [
    b64('untrusted comment: rsign encrypted secret key'),
    b64('untrusted comment: minisign encrypted secret key'),
    ['rsign', 'encrypted', 'secret', 'key'].join(' '),
    ['minisign', 'encrypted', 'secret', 'key'].join(' '),
    b64(Buffer.from('EdScB2', 'latin1')).slice(0, 8),
  ];
  // A private key in PEM, with a body. mac-sign.sh names the first line to
  // check what it is given, and its test gives it a made-up body.
  const pem = new RegExp(`-----BEGIN [A-Z ]*${'PRIVATE'} KEY-----\\r?\\n[A-Za-z0-9+/=]{40,}`);
  const files = execFileSync('git', ['ls-files', '-z'], { cwd: REPO, encoding: 'utf8' }).split('\0').filter(Boolean);
  assert.ok(files.length > 50, 'the repository lists its files');
  for (const file of files) {
    const full = path.join(REPO, file);
    if (!existsSync(full) || !lstatSync(full).isFile()) continue;
    const text = readFileSync(full, 'latin1');
    for (const needle of needles) assert.ok(!text.includes(needle), `${file} holds what looks like a private key`);
    assert.ok(!pem.test(text), `${file} holds what looks like a private key`);
  }
});
