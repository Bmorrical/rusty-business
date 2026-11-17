// The security policy's rules, checked on every test run.
//
// Brad, 2026-11-17, started a security policy from GitHub's own page, which
// fills one in from a template: a table of versions this project never had,
// and two sections that say "Use this section to tell people". This checks
// that the real one stays real: reports go to GitHub's private reporting,
// never to a public issue or an email address, and the only version it
// names is the app's own.
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const REPORT = 'https://github.com/Bmorrical/rusty-business/security/advisories/new';

// What GitHub's template says. None of it belongs in this policy.
const TEMPLATE = ['Use this section to tell people', ':white_check_mark:', ':x:', '5.1.x', '5.0.x', '4.0.x', '< 4.0'];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** Every version number in the text, such as 0.1.0 or 5.1.x. */
export function versionsIn(text) {
  return [...text.matchAll(/(?<![\d.])\d+\.\d+\.(?:\d+|x)(?!\w|\.\d)/g)].map((match) => match[0]);
}

/** Every email address in the text. */
export function addressesIn(text) {
  return [...text.matchAll(/[\w.+-]+@[\w-]+(?:\.[\w-]+)+/g)].map((match) => match[0]);
}

test('the policy is where GitHub looks for it, and the README links it', () => {
  assert.ok(existsSync(path.join(REPO, 'SECURITY.md')), 'no SECURITY.md at the root');
  const readme = read('README.md');
  const section = /^## Security\n([\s\S]*?)(?=^## )/m.exec(readme);
  assert.ok(section, 'the README has no Security section');
  assert.match(section[1], /\[SECURITY\.md\]\(SECURITY\.md\)/);
  assert.match(section[1], /not a public\s+issue/);
});

test("reports go to GitHub's private reporting, not to a public issue", () => {
  const policy = read('SECURITY.md');
  assert.ok(policy.includes(`](${REPORT})`), 'the policy does not link private reporting');
  assert.match(policy, /do not open a public issue/i);
});

test('the policy names no email address', () => {
  assert.deepEqual(addressesIn(read('SECURITY.md')), []);
  assert.deepEqual(addressesIn(/^## Security\n([\s\S]*?)(?=^## )/m.exec(read('README.md'))?.[1] ?? ''), []);
});

test("none of GitHub's template is left", () => {
  const policy = read('SECURITY.md');
  for (const words of TEMPLATE) {
    assert.ok(!policy.includes(words), `the policy still says ${words}`);
  }
});

test("the only version the policy names is the app's own", () => {
  const app = JSON.parse(read('apps/desktop/src-tauri/tauri.conf.json')).version;
  const named = versionsIn(read('SECURITY.md'));
  assert.ok(named.length > 0, 'the policy names no version');
  assert.deepEqual([...new Set(named)], [app]);
});

test('versions and addresses are read from the text', () => {
  assert.deepEqual(versionsIn('| 5.1.x | yes |, 0.1.0, and v10.20.30. Not 1.2 or 1.2.3.4'), ['5.1.x', '0.1.0', '10.20.30']);
  assert.deepEqual(addressesIn('mail someone@example.com or a.b+c@d-e.co.uk, not @handle'), [
    'someone@example.com',
    'a.b+c@d-e.co.uk',
  ]);
});
