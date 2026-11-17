// The stack the desktop app starts, apps/desktop/src-tauri/stack-compose.yaml,
// checked against the stack `make up` starts, infra/compose.yaml. The app's
// file is the same stack with three changes, and these tests fail when the
// two drift apart in anything else.
//
// Brad, 2026-11-16: "it should search and up the local models and stack.
// b/c I want to demo this in my other mac session and I should be able to
// just pull the .dmg or .exe from read me and install and go, one click
// ready."
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const read = (file) => readFileSync(`${REPO}${file}`, 'utf8');

const INFRA = read('infra/compose.yaml');
const APP = read('apps/desktop/src-tauri/stack-compose.yaml');
const STACK_RS = read('apps/desktop/src-tauri/src/stack.rs');

const OWN = ['api', 'worker'];
const FRONT_DOOR = 'caddy';
const IMAGE = /^ghcr\.io\/bmorrical\/rusty-business\/(api|worker):sha-([0-9a-f]{40})@sha256:[0-9a-f]{64}$/;

/**
 * Each service in a Compose file, as its lines with comments and blank lines
 * taken out, so a comment can't pass a check or fail one.
 */
function servicesOf(text) {
  const services = new Map();
  let inServices = false;
  let current = null;
  for (const line of text.split('\n')) {
    if (!line.trim() || line.trim().startsWith('#')) continue;
    if (/^\S/.test(line)) {
      inServices = line.startsWith('services:');
      current = null;
      continue;
    }
    if (!inServices) continue;
    if (/^ {2}\S/.test(line)) {
      current = [];
      services.set(line.trim().replace(/:$/, ''), current);
    }
    current?.push(line);
  }
  return services;
}

/** The top-level blocks of a Compose file but `services`, the same way. */
function topOf(text) {
  const top = new Map();
  let current = null;
  for (const line of text.split('\n')) {
    if (!line.trim() || line.trim().startsWith('#')) continue;
    if (/^\S/.test(line)) {
      const key = line.split(':')[0];
      current = key === 'services' ? null : [];
      if (current) top.set(key, current);
    }
    current?.push(line);
  }
  return top;
}

/** A service's lines without the setting at four spaces named `key`, and what sits under it. */
function without(lines, key) {
  const kept = [];
  let skipping = false;
  for (const line of lines) {
    if (line.startsWith(`    ${key}:`)) {
      skipping = true;
      continue;
    }
    if (skipping && /^ {5,}/.test(line)) continue;
    skipping = false;
    kept.push(line);
  }
  return kept;
}

/** The one setting at four spaces named `key`, with what sits under it. */
function setting(lines, key) {
  const at = lines.findIndex((line) => line.startsWith(`    ${key}:`));
  if (at < 0) return [];
  const found = [lines[at]];
  for (const line of lines.slice(at + 1)) {
    if (!/^ {5,}/.test(line)) break;
    found.push(line);
  }
  return found;
}

/**
 * The lines the two files must share once the three changes are taken out:
 * the API and worker images and their builds, the ports on every service but
 * the front door, and the words each file's settings file is named by.
 */
function shared(lines, service) {
  let kept = without(lines, 'build');
  if (service !== FRONT_DOOR) kept = without(kept, 'ports');
  if (OWN.includes(service)) kept = without(kept, 'image');
  return kept.map((line) =>
    line
      .replaceAll(':?missing in infra/.env, run make bootstrap}', ':?missing}')
      .replaceAll(":?missing in the app's stack.env}", ':?missing}')
      .replace('./caddy/Caddyfile:', './Caddyfile:'),
  );
}

const infraServices = servicesOf(INFRA);
const appServices = servicesOf(APP);
const imageOf = (service) => setting(appServices.get(service) ?? [], 'image')[0]?.replace(/^\s+image:\s*/, '') ?? '';

test('the app starts the same services as `make up`, each the same but for the three changes', () => {
  assert.deepEqual([...appServices.keys()], [...infraServices.keys()], 'the same services, in the same order');
  for (const [service, lines] of infraServices) {
    assert.deepEqual(shared(appServices.get(service), service), shared(lines, service), `${service} drifted from infra/compose.yaml`);
  }
});

test('the app keeps the same shared settings and volumes, under a name of its own', () => {
  const infra = topOf(INFRA);
  const app = topOf(APP);
  assert.deepEqual([...app.keys()], [...infra.keys()], 'the same top-level blocks');
  for (const [key, lines] of infra) {
    if (key === 'name') continue;
    assert.deepEqual(app.get(key), lines, `${key} drifted from infra/compose.yaml`);
  }
  assert.deepEqual(infra.get('name'), ['name: rusty-business']);
  assert.deepEqual(app.get('name'), ['name: rusty-business-app'], "its own name, so its volumes are not the make-up stack's");
  assert.match(STACK_RS, /\nconst PROJECT: &str = "rusty-business-app";\n/, 'the name the app gives Compose');
});

test('only the front door has a port on this computer, and it is the port `make up` uses', () => {
  for (const [service, lines] of appServices) {
    const ports = setting(lines, 'ports');
    if (service === FRONT_DOOR) {
      assert.deepEqual(ports, ['    ports:', '      - "127.0.0.1:${PORT_FRONT_DOOR:-4700}:8080"'], 'the front door, on 127.0.0.1 only');
    } else {
      assert.deepEqual(ports, [], `${service} has no port on this computer`);
    }
  }
  assert.ok(setting(infraServices.get(FRONT_DOOR), 'ports').includes('      - "127.0.0.1:${PORT_FRONT_DOOR:-4700}:8080"'), "the make-up stack's front door is on 4700 too");
  assert.match(STACK_RS, /\npub const FRONT_DOOR_PORT: u16 = 4700;\n/, 'the port the app asks for and checks');
});

test('the app pulls the API and worker from GitHub, pinned by digest, and builds nothing', () => {
  assert.doesNotMatch(APP, /^\s+build:/m, 'nothing is built where the app runs');
  for (const service of OWN) {
    assert.ok(setting(infraServices.get(service), 'build').length, `make up builds ${service} here`);
    const match = IMAGE.exec(imageOf(service));
    assert.ok(match, `${service} is pinned to a commit and a digest: ${imageOf(service)}`);
    assert.equal(match[1], service, `${service} pulls its own image`);
  }
  const commits = OWN.map((service) => IMAGE.exec(imageOf(service))?.[2]);
  assert.equal(new Set(commits).size, 1, 'the API and worker come from one build');
});

test('every setting the app stack needs is in the stack.env the app writes', () => {
  const needed = new Set([...APP.matchAll(/\$\{([A-Z][A-Z0-9_]*)/g)].map((match) => match[1]));
  needed.delete('PORT_FRONT_DOOR');
  const body = /fn new_settings\(\) -> String \{([\s\S]*?)\n\}/.exec(STACK_RS)?.[1] ?? '';
  const written = new Set([...body.matchAll(/^\s+([A-Z][A-Z0-9_]*)=/gm)].map((match) => match[1]));
  assert.ok(written.size, 'new_settings is where stack.rs writes stack.env');
  assert.deepEqual([...needed].filter((name) => !written.has(name)), [], 'read by the stack, never written');
  assert.match(STACK_RS, /command\.env\("PORT_FRONT_DOOR", port\.to_string\(\)\)/, 'the port is passed, not kept in stack.env');
});

test("the app writes the stack file and the make-up stack's Caddyfile, as they are in the repository", () => {
  assert.match(STACK_RS, /\nconst COMPOSE: &str = include_str!\("\.\.\/stack-compose\.yaml"\);\n/);
  assert.match(STACK_RS, /\nconst CADDYFILE: &str = include_str!\("\.\.\/\.\.\/\.\.\/\.\.\/infra\/caddy\/Caddyfile"\);\n/);
  assert.ok(setting(appServices.get(FRONT_DOOR), 'volumes').includes('      - ./Caddyfile:/etc/caddy/Caddyfile:ro'));
});
