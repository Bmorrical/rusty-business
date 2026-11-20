// CONTRIBUTING.md: the agreement every change follows before it merges.
//
// Brad, 2026-11-19: "give me a simple contibuting.md we can agree before
// anything merges, then he is granted like me some authority to merge on
// main, that is all." Then the rules:
//
//   "offline first
//    all checks pass pr's before merge
//    copilot reviews and cod when credits exists
//    deploys only on green"
//
// "test coverage can't slip, always must compliement the work and cover
// well, backend and e2e". "so I'd put two splits.  contributors and
// maintainers". "maintainers are contributors too, but not the other way
// around.  I decide or the mass of maintainers decide later". For the first
// maintainer: "no chains but the project and repo I still have admin control,
// he's in lieutenant position."

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const read = (file) => readFileSync(path.join(REPO, file), 'utf8');

/** The page's text with its line breaks and runs of spaces made single spaces. */
const flat = (text) => text.replace(/\s+/g, ' ');

/** The text of one `## ` section, flattened, or null when it is missing. */
function section(text, heading) {
  const start = text.indexOf(`\n## ${heading}\n`);
  if (start === -1) return null;
  const next = text.indexOf('\n## ', start + 1);
  return flat(text.slice(start, next === -1 ? undefined : next));
}

const page = read('CONTRIBUTING.md');

test('the page has the rules, then contributors, maintainers, and who decides, in that order', () => {
  const headings = [...page.matchAll(/^## (.+)$/gm)].map((match) => match[1]);
  assert.deepEqual(headings, ['The rules for every change', 'Contributors', 'Maintainers', 'Who decides']);
});

test('maintainers are contributors too, and not the other way around', () => {
  const top = flat(page.slice(0, page.indexOf('\n## ')));
  assert.ok(top.includes('Every maintainer is a contributor too and follows the same rules'), 'the page stops saying maintainers are contributors');
  assert.ok(top.includes('but a contributor is not a maintainer'), 'the page stops saying a contributor is not a maintainer');
});

test('every change is offline first, brings its tests, passes every check, answers every review, and ships only on green', () => {
  const rules = section(page, 'The rules for every change');
  assert.ok(rules, 'the rules are missing');
  for (const words of [
    '**Offline first.** On-device chat works with both network gates off.',
    'LAN gate allows model discovery, chat, and model install, pause, and resume requests to the configured private-network engine.',
    'WAN gate allows direct internet features, which use each service\'s own fixed hosts.',
    'Both gates start off and can be enabled independently.',
    '**Tests come with the work.** Every change brings tests that cover it, in the backend and end to end. Coverage never slips.',
    '**All checks pass before a pull request merges.** The gate runs on every pull request',
    'When Copilot has credits, it reviews each pull request.',
    'Each comment, from Copilot or a person, is fixed, or declined with the reason, before the merge.',
    '**Deploys and releases only on green.** Nothing is deployed or released unless the gate is green on `main`.',
  ]) {
    assert.ok(rules.includes(words), `the rules stop saying: ${words}`);
  }
});

test('a contributor sends one change per pull request, tested first, under the MIT License', () => {
  const contributors = section(page, 'Contributors');
  assert.ok(contributors, 'the contributors part is missing');
  for (const words of [
    'Anyone can contribute.',
    'one change per pull request, with a title and a description',
    'run `make test`, and, with the stack up (`make bootstrap`), `make e2e`',
    'under the [MIT License](LICENSE)',
    '[TRADEMARKS.md](TRADEMARKS.md)',
    'as [SECURITY.md](SECURITY.md) says, never in an issue or a pull request',
  ]) {
    assert.ok(contributors.includes(words), `the contributors part stops saying: ${words}`);
  }
});

test('a maintainer approves a contributor\'s pull request, merges their own once every rule is met, keeps a straight line, and releases only on green', () => {
  const maintainers = section(page, 'Maintainers');
  assert.ok(maintainers, 'the maintainers part is missing');
  for (const words of [
    'Maintainers may also review and merge.',
    "A contributor's pull request merges once it meets every rule above and a maintainer has approved it.",
    'A maintainer may merge their own pull request once it meets every rule above, without waiting for another approval.',
    '"Squash and merge" or "Rebase and merge"',
    'Never force-push to `main`, and never change a release once it is published.',
    'only after the gate there is green',
  ]) {
    assert.ok(maintainers.includes(words), `the maintainers part stops saying: ${words}`);
  }
});

test('the owner decides, and later a majority of the maintainers may', () => {
  const decides = section(page, 'Who decides');
  assert.ok(decides, 'who decides is missing');
  assert.ok(decides.includes("The project's owner decides who becomes a maintainer and what this page says."));
  assert.ok(decides.includes('Later, a majority of the maintainers may decide instead.'));
});

test('the page names no one, and the README links it', () => {
  assert.doesNotMatch(page, /Brad|Morrical/, 'the page names a person');
  assert.doesNotMatch(page, /(^|\s)@[A-Za-z0-9-]+/, 'the page names an account');
  assert.ok(read('README.md').includes('[CONTRIBUTING.md](CONTRIBUTING.md)'), 'the README stops linking the page');
});
