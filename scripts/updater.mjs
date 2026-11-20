// Signs the files Install and restart puts in place, and writes latest.json,
// the list the app reads to find them and their signatures. The Release
// workflow runs it twice, on the folder of downloads the build jobs made:
//
//   node scripts/updater.mjs sign dist    in the updater job, with the key
//   node scripts/updater.mjs check dist   in the publish job, before release
//
// Brad, 2026-11-19: "when can we do an in app install rather than a new dmg
// download? is that after the signing? if we can do it now, then roll that
// next"
//
// sign reads TAURI_SIGNING_PRIVATE_KEY, the text of the key file `tauri
// signer generate` wrote, and TAURI_SIGNING_PRIVATE_KEY_PASSWORD. It opens
// the key with its password and stops, with nothing signed, unless it is the
// key whose public half tauri.conf.json holds. Then it signs each file the
// way `tauri signer sign` does, as minisign does, with the app's version in
// the signed comment, and writes latest.json. It uses Node's own crypto and
// nothing else: no package from npm, and no other program, runs while the
// key is in hand, and the key is never written down.
//
// check reads latest.json, the four files, and the public key in
// tauri.conf.json, and stops unless latest.json names this version and the
// four files on this version's release, and nothing else, and each
// signature matches its file and that key. sign runs it on what it wrote.
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');

// Where the app looks: update.rs's REPOSITORY, SITE_HOST, and MANIFEST.
const RELEASES = 'https://github.com/Bmorrical/rusty-business/releases/download';
const MANIFEST = 'latest.json';

// Each file Install and restart puts in place, and its name in latest.json,
// as update.rs's package() gives them. The Linux .tar.gz has none.
const PACKAGES = [
  ['darwin-aarch64', 'Rusty-Business-mac-arm64.app.tar.gz'],
  ['windows-x86_64', 'Rusty-Business-windows-x64-setup.exe'],
  ['linux-x86_64-deb', 'Rusty-Business-linux-x64.deb'],
  ['linux-x86_64-rpm', 'Rusty-Business-linux-x64.rpm'],
];

// The app reads at most this much of latest.json.
const MOST_MANIFEST_BYTES = 1024 * 1024;

const USAGE = 'use: node scripts/updater.mjs sign <folder>, or node scripts/updater.mjs check <folder>';
const UNTRUSTED = 'untrusted comment: ';
const TRUSTED = 'trusted comment: ';

class Stop extends Error {}

function fail(message) {
  throw new Stop(message);
}

// --- What the app is --------------------------------------------------------

function read(file) {
  return fs.readFileSync(path.join(ROOT, file), 'utf8');
}

// The app's version, from the three files that say it. They must agree.
function versionOfApp() {
  const packageJson = JSON.parse(read('apps/desktop/package.json')).version;
  const tauri = JSON.parse(read('apps/desktop/src-tauri/tauri.conf.json')).version;
  const cargo = /^\[package\]\n(?:(?!\[).*\n)*?version = "([^"]*)"$/m.exec(read('apps/desktop/src-tauri/Cargo.toml'))?.[1];
  if (!/^[0-9]+\.[0-9]+\.[0-9]+$/.test(packageJson ?? '')) {
    fail(`apps/desktop/package.json gives no plain version, such as 1.2.3: ${packageJson}`);
  }
  if (tauri !== packageJson || cargo !== packageJson) {
    fail(`the app's versions differ: package.json ${packageJson}, tauri.conf.json ${tauri}, Cargo.toml ${cargo}`);
  }
  return packageJson;
}

// Base64 with nothing around it and nothing left over.
function base64(text, what) {
  if (typeof text !== 'string' || !/^[A-Za-z0-9+/]+={0,2}$/.test(text)) fail(`${what} is not base64`);
  const bytes = Buffer.from(text, 'base64');
  if (bytes.toString('base64') !== text) fail(`${what} is not base64`);
  return bytes;
}

// The public key built into the app: minisign's, Ed25519, with its key id.
function publicKey() {
  const text = base64(JSON.parse(read('apps/desktop/src-tauri/tauri.conf.json')).plugins?.updater?.pubkey, 'the public key in tauri.conf.json').toString('utf8');
  const lines = text.split('\n');
  if (!/^untrusted comment: minisign public key: [0-9A-F]{16}$/.test(lines[0]) || lines.length > 3 || (lines[2] ?? '') !== '') {
    fail('the public key in tauri.conf.json is not a minisign public key');
  }
  const box = base64(lines[1], 'the public key in tauri.conf.json');
  if (box.length !== 42 || box.subarray(0, 2).toString('latin1') !== 'Ed') {
    fail('the public key in tauri.conf.json is not an Ed25519 key of minisign');
  }
  const raw = box.subarray(10);
  return {
    id: box.subarray(2, 10),
    raw,
    key: crypto.createPublicKey({ key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), raw]), format: 'der', type: 'spki' }),
  };
}

// --- The files --------------------------------------------------------------

// A plain file in the folder, not a link, and not empty.
function plainFile(folder, name) {
  const file = path.join(folder, name);
  let stat;
  try {
    stat = fs.lstatSync(file);
  } catch {
    fail(`${folder} holds no ${name}`);
  }
  if (!stat.isFile() || stat.size === 0) fail(`${name} in ${folder} is not a plain file with something in it`);
  return { file, size: stat.size };
}

// BLAKE2b-512 of a file, read a piece at a time: minisign signs this hash.
function hashOf(file) {
  const hash = crypto.createHash('blake2b512');
  const piece = Buffer.alloc(1024 * 1024);
  const fd = fs.openSync(file, 'r');
  try {
    for (let got; (got = fs.readSync(fd, piece, 0, piece.length, null)) > 0; ) hash.update(piece.subarray(0, got));
  } finally {
    fs.closeSync(fd);
  }
  return hash.digest();
}

function addressOf(version, name) {
  return `${RELEASES}/v${version}/${name}`;
}

// --- The signing key --------------------------------------------------------

// libsodium's choice of scrypt's settings from minisign's two limits.
function scryptSettings(opslimit, memlimit) {
  const ops = opslimit < 32768n ? 32768n : opslimit;
  const r = 8n;
  let logN = 1n;
  if (ops < memlimit / 32n) {
    const most = ops / (r * 4n);
    while (logN < 63n && 1n << logN <= most / 2n) logN += 1n;
    return { N: 1n << logN, r, p: 1n };
  }
  const most = memlimit / (r * 128n);
  while (logN < 63n && 1n << logN <= most / 2n) logN += 1n;
  let rp = ops / 4n / (1n << logN);
  if (rp > 0x3fffffffn) rp = 0x3fffffffn;
  return { N: 1n << logN, r, p: rp / r };
}

// Opens the key the release environment holds, and stops unless its public
// half is the one built into the app.
function signingKey(app) {
  const text = process.env.TAURI_SIGNING_PRIVATE_KEY?.trim();
  const password = process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD;
  if (!text) fail('TAURI_SIGNING_PRIVATE_KEY is not set. Nothing was signed.');
  if (!password) fail('TAURI_SIGNING_PRIVATE_KEY_PASSWORD is not set. Nothing was signed.');
  const NOT_A_KEY = 'TAURI_SIGNING_PRIVATE_KEY is not the text of a key file from tauri signer generate. Nothing was signed.';
  let lines;
  try {
    lines = base64(text, 'the key').toString('utf8').split('\n');
  } catch {
    fail(NOT_A_KEY);
  }
  if (!lines[0].startsWith(UNTRUSTED) || lines.length > 3 || (lines[2] ?? '') !== '') fail(NOT_A_KEY);
  let box;
  try {
    box = base64(lines[1], 'the key');
  } catch {
    fail(NOT_A_KEY);
  }
  if (box.length !== 158 || box.subarray(0, 6).toString('latin1') !== 'EdScB2') {
    fail('TAURI_SIGNING_PRIVATE_KEY is not an Ed25519 key of minisign, locked with a password. Nothing was signed.');
  }
  const opslimit = box.readBigUInt64LE(38);
  const memlimit = box.readBigUInt64LE(46);
  const { N, r, p } = scryptSettings(opslimit, memlimit);
  if (N > 1n << 18n || p < 1n || r * p > 16n) fail('The key asks for other scrypt settings than a key of tauri signer generate. Nothing was signed.');
  const stream = crypto.scryptSync(Buffer.from(password, 'utf8'), box.subarray(6, 38), 104, {
    N: Number(N),
    r: Number(r),
    p: Number(p),
    maxmem: Number(256n * N * r + 1024n * 1024n),
  });
  const opened = Buffer.from(box.subarray(54, 158));
  for (let i = 0; i < opened.length; i += 1) opened[i] ^= stream[i];
  stream.fill(0);
  const id = Buffer.from(opened.subarray(0, 8));
  const der = Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), opened.subarray(8, 40)]);
  const key = crypto.createPrivateKey({ key: der, format: 'der', type: 'pkcs8' });
  der.fill(0);
  const derived = crypto.createPublicKey(key).export({ format: 'der', type: 'spki' }).subarray(12);
  const opens = derived.equals(opened.subarray(40, 72));
  opened.fill(0);
  if (!opens) fail('The password does not open TAURI_SIGNING_PRIVATE_KEY. Nothing was signed.');
  if (!derived.equals(app.raw) || !id.equals(app.id)) {
    fail('TAURI_SIGNING_PRIVATE_KEY is not the key whose public half tauri.conf.json holds. Nothing was signed.');
  }
  return { id: app.id, key };
}

// --- Signatures -------------------------------------------------------------

function trustedComment(name, version, time) {
  return `timestamp:${time}\tfile:${name}\tversion:${version}`;
}

// A signature as `tauri signer sign --app-version` writes it, in base64.
function signatureOf(file, name, version, signer) {
  const signature = crypto.sign(null, hashOf(file), signer.key);
  const comment = trustedComment(name, version, Math.floor(Date.now() / 1000));
  const whole = crypto.sign(null, Buffer.concat([signature, Buffer.from(comment, 'utf8')]), signer.key);
  const text = [
    `${UNTRUSTED}signature from tauri secret key`,
    Buffer.concat([Buffer.from('ED', 'latin1'), signer.id, signature]).toString('base64'),
    `${TRUSTED}${comment}`,
    whole.toString('base64'),
    '',
  ].join('\n');
  return Buffer.from(text, 'utf8').toString('base64');
}

// Stops unless the signature is the app key's, over this file, for this
// file's name and this version.
function checkSignature(signature, file, name, version, app) {
  const bad = (why) => fail(`the signature for ${name} in ${MANIFEST} ${why}`);
  let lines;
  try {
    lines = base64(signature, 'it').toString('utf8').split('\n');
  } catch {
    bad('is not base64');
  }
  if (lines.length !== 5 || lines[4] !== '' || !lines[0].startsWith(UNTRUSTED) || !lines[2].startsWith(TRUSTED)) {
    bad('is not a minisign signature');
  }
  let line;
  let whole;
  try {
    line = base64(lines[1], 'it');
    whole = base64(lines[3], 'it');
  } catch {
    bad('is not a minisign signature');
  }
  if (line.length !== 74 || whole.length !== 64 || line.subarray(0, 2).toString('latin1') !== 'ED') {
    bad('is not a minisign signature of a file\'s hash');
  }
  if (!line.subarray(2, 10).equals(app.id)) bad('is from another key than the app\'s');
  const comment = lines[2].slice(TRUSTED.length);
  const shape = /^timestamp:([0-9]+)\tfile:([^\t]*)\tversion:([^\t]*)$/.exec(comment);
  if (!shape) bad('does not say its time, its file, and its version');
  if (shape[2] !== name) bad(`names another file: ${shape[2]}`);
  if (shape[3] !== version) bad(`names another version: ${shape[3]}`);
  const signed = line.subarray(10);
  if (!crypto.verify(null, hashOf(file), app.key, signed)) bad('does not match the file');
  if (!crypto.verify(null, Buffer.concat([signed, Buffer.from(comment, 'utf8')]), app.key, whole)) {
    bad('does not match its own comment');
  }
}

// --- latest.json ------------------------------------------------------------

function sameKeys(value, keys) {
  return value !== null && typeof value === 'object' && !Array.isArray(value) && JSON.stringify(Object.keys(value)) === JSON.stringify(keys);
}

function check(folder) {
  const version = versionOfApp();
  const app = publicKey();
  const { file, size } = plainFile(folder, MANIFEST);
  if (size > MOST_MANIFEST_BYTES) fail(`${MANIFEST} is larger than the app reads`);
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    fail(`${MANIFEST} is not JSON`);
  }
  if (!sameKeys(manifest, ['version', 'pub_date', 'platforms'])) fail(`${MANIFEST} holds other than version, pub_date, and platforms`);
  if (manifest.version !== version) fail(`${MANIFEST} names version ${manifest.version}, and the app is ${version}`);
  if (typeof manifest.pub_date !== 'string' || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(manifest.pub_date) || Number.isNaN(Date.parse(manifest.pub_date))) {
    fail(`${MANIFEST} gives no plain time, such as 2026-11-20T12:00:00Z: ${manifest.pub_date}`);
  }
  if (!sameKeys(manifest.platforms, PACKAGES.map(([target]) => target))) {
    fail(`${MANIFEST} names other systems than ${PACKAGES.map(([target]) => target).join(', ')}`);
  }
  for (const [target, name] of PACKAGES) {
    const entry = manifest.platforms[target];
    if (!sameKeys(entry, ['signature', 'url'])) fail(`${MANIFEST} holds other than a signature and an address for ${target}`);
    if (entry.url !== addressOf(version, name)) fail(`${MANIFEST} sends ${target} to ${entry.url}`);
    checkSignature(entry.signature, plainFile(folder, name).file, name, version, app);
  }
  console.log(`${MANIFEST} names version ${version} and its ${PACKAGES.length} files, and each signature matches.`);
}

function sign(folder) {
  const version = versionOfApp();
  const app = publicKey();
  const files = PACKAGES.map(([target, name]) => [target, name, plainFile(folder, name).file]);
  if (fs.existsSync(path.join(folder, MANIFEST))) fail(`${folder} already holds a ${MANIFEST}. Nothing was signed.`);
  const signer = signingKey(app);
  const platforms = {};
  for (const [target, name, file] of files) {
    platforms[target] = { signature: signatureOf(file, name, version, signer), url: addressOf(version, name) };
  }
  const manifest = { version, pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'), platforms };
  fs.writeFileSync(path.join(folder, MANIFEST), `${JSON.stringify(manifest, null, 2)}\n`, { flag: 'wx' });
  console.log(`Signed ${files.length} files for version ${version}.`);
  check(folder);
}

function main([goal, folder, ...rest]) {
  if (!['sign', 'check'].includes(goal) || !folder || rest.length > 0) fail(USAGE);
  const where = path.resolve(folder);
  if (!fs.existsSync(where) || !fs.lstatSync(where).isDirectory()) fail(`${folder} is not a folder`);
  if (goal === 'sign') sign(where);
  else check(where);
}

try {
  main(process.argv.slice(2));
} catch (error) {
  if (!(error instanceof Stop)) throw error;
  console.error(`updater: ${error.message}`);
  process.exit(1);
}
