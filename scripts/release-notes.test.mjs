// The release notes' rules, checked on every test run.
//
// Brad, 2026-11-21: "each tag gets a release note", "merges to main do not",
// "and it should be enforced that the notes were published before the build
// can go out", and "the root readme clearly shows user where to find the
// notes".
//
// RELEASES.md holds one note for each version, under "## <version>", newest
// first. The app's own version must have one, so a pull request that raises
// the version without a note is red, and the Release workflow, whose every
// job waits on the gate, builds nothing. scripts/release.sh then puts that
// note at the top of the version's page, and release.test.mjs checks that it
// stops without one.
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const NOTES = 'RELEASES.md';
const RELEASES = 'https://github.com/Bmorrical/rusty-business/releases';

/**
 * The versions that went out before notes were kept, and the day each did,
 * from its page on GitHub. Brad, 2026-11-21: "release history since
 * beginning if possible". Their notes stay.
 */
const OUT = {
  '0.8.3': '2026-11-21',
  '0.8.1': '2026-11-20',
  '0.8.0': '2026-11-20',
  '0.7.0': '2026-11-20',
  '0.6.1': '2026-11-19',
  '0.6.0': '2026-11-19',
  '0.5.0': '2026-11-19',
  '0.3.0': '2026-11-17',
  '0.2.0': '2026-11-17',
};

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** A Markdown page's `## ` sections, in order: each heading and the text under it. */
export function sectionsOf(markdown) {
  const sections = [];
  for (const line of markdown.split('\n')) {
    const heading = /^## (.*)$/.exec(line);
    if (heading) sections.push({ heading: heading[1], body: '' });
    else if (sections.length) sections.at(-1).body += `${line}\n`;
  }
  return sections;
}

const VERSION_SHAPE = /^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/;

/** Below zero when version `a` is the older, above when it is the newer. */
export function compare(a, b) {
  const [x, y] = [a, b].map((version) => version.split('.').map(Number));
  for (let i = 0; i < 3; i += 1) {
    if (x[i] !== y[i]) return x[i] - y[i];
  }
  return 0;
}

/**
 * Everything wrong with a notes page, for an app at `version`, when the
 * versions in `out` went out on the days given. An empty list means the page
 * keeps every rule.
 */
export function problemsIn(page, version, out = {}) {
  const problems = [];
  if (!page.startsWith('# Release notes\n')) problems.push('the page does not open with "# Release notes"');
  const sections = sectionsOf(page);
  const versions = [];

  for (const { heading, body } of sections) {
    // "merges to main do not": a note is for a version, never for a pull
    // request, a day, or work that has no version yet.
    if (!VERSION_SHAPE.test(heading)) {
      problems.push(`"## ${heading}" is not a version: every note is for one version, such as "## ${version}"`);
      continue;
    }
    if (versions.includes(heading)) problems.push(`${heading} has two notes`);
    else if (versions.length && compare(heading, versions.at(-1)) > 0) problems.push(`${heading} is below ${versions.at(-1)}: the newest goes first`);
    versions.push(heading);
    if (compare(heading, version) > 0) problems.push(`${heading} has a note, and the app is at ${version}`);

    const lines = body.split('\n');
    const words = lines.filter((line) => line.trim());
    if (!words.some((line) => /[A-Za-z]/.test(line))) problems.push(`${heading}'s note says nothing`);
    if (heading !== version && !/^Released \d{4}-\d{2}-\d{2}\.(?: |$)/.test(words[0] ?? '')) {
      problems.push(`${heading}'s note does not open with the day it went out, as "Released 2026-11-20."`);
    }
    // The note is shown on the version's page, where a new line in the text
    // is a new line on the page, and where "## Install" follows it.
    lines.forEach((line, at) => {
      if (/^[ \t]/.test(line)) problems.push(`${heading}'s note has a line that starts with a space: "${line.trim().slice(0, 40)}"`);
      else if (line.startsWith('#')) problems.push(`${heading}'s note has a heading of its own: "${line.slice(0, 40)}"`);
      else if (line && at > 0 && lines[at - 1].trim() && !line.startsWith('- ')) {
        problems.push(`${heading}'s note has a wrapped line: "${line.slice(0, 40)}". One line to a paragraph and to a list item`);
      }
    });
  }

  // "each tag gets a release note".
  if (!versions.includes(version)) problems.push(`${version} has no note: write one under "## ${version}" before this version is built`);
  for (const [past, day] of Object.entries(out)) {
    const section = sections.find(({ heading }) => heading === past);
    if (!section) problems.push(`${past} went out, and its note is gone`);
    else if (!section.body.trimStart().startsWith(`Released ${day}.`)) problems.push(`${past} went out on ${day}, and its note does not open with "Released ${day}."`);
  }
  return problems;
}

/** The jobs of a workflow, each with its own text. */
export function jobsOf(workflow) {
  const jobs = [];
  let inside = false;
  for (const line of workflow.split('\n')) {
    if (/^\S/.test(line)) inside = line === 'jobs:';
    else if (inside) {
      const name = /^ {2}([A-Za-z_][\w-]*):\s*$/.exec(line);
      if (name) jobs.push({ name: name[1], text: '' });
      else if (jobs.length) jobs.at(-1).text += `${line}\n`;
    }
  }
  return jobs;
}

/** The jobs a job waits on. */
export function needsOf(text) {
  const needs = /^ {4}needs:[ \t]*(.*)$/m.exec(text)?.[1] ?? '';
  return needs.replace(/[[\]]/g, '').split(',').map((name) => name.trim()).filter(Boolean);
}

const { version: VERSION } = JSON.parse(read('apps/desktop/package.json'));

test('each tag gets a release note: the app\'s version has one, and so does every version that went out', () => {
  assert.deepEqual(problemsIn(read(NOTES), VERSION, OUT), []);
  const versions = sectionsOf(read(NOTES)).map(({ heading }) => heading);
  assert.equal(versions[0], VERSION, `the first note on the page is not ${VERSION}'s`);
  for (const past of Object.keys(OUT)) assert.ok(versions.includes(past), `${past} has no note`);
});

test('the rules catch a page that breaks them', () => {
  const page = (...sections) => `# Release notes\n\nWhat changed.\n\n${sections.join('\n')}`;
  const now = '## 1.2.0\n\n- A made-up change.\n';
  const past = '## 1.1.0\n\nReleased 2001-02-03.\n\n- An older change.\n';
  const out = { '1.1.0': '2001-02-03' };
  assert.deepEqual(problemsIn(page(now, past), '1.2.0', out), []);
  assert.deepEqual(problemsIn(page('## 1.2.0\n\nOne made-up release.\n\n**New**\n\n- One.\n- Two.\n', past), '1.2.0', out), []);

  for (const [name, text, expected] of [
    ['no note for the version', page(past), /^1\.2\.0 has no note/],
    ['a note with nothing in it', page('## 1.2.0\n', past), /^1\.2\.0's note says nothing$/],
    ['a note of marks alone', page('## 1.2.0\n\n- \n', past), /^1\.2\.0's note says nothing$/],
    // "merges to main do not".
    ['a note for work with no version', page('## Unreleased\n\n- A change.\n', now, past), /^"## Unreleased" is not a version/],
    ['a note for a pull request', page(now, '## #62\n\n- A merge.\n', past), /^"## #62" is not a version/],
    ['a note for a day', page(now, '## 2001-02-03\n\n- A merge.\n', past), /^"## 2001-02-03" is not a version/],
    ['a v before the version', page('## v1.2.0\n\n- A change.\n', past), /^"## v1\.2\.0" is not a version/],
    ['a version with a date beside it', page('## 1.2.0 (soon)\n\n- A change.\n', past), /^"## 1\.2\.0 \(soon\)" is not a version/],
    ['two notes for one version', page(now, now, past), /^1\.2\.0 has two notes$/],
    ['the older one first', page(past, now), /^1\.2\.0 is below 1\.1\.0/],
    ['ten after nine', page('## 1.9.0\n\n- A change.\n', '## 1.10.0\n\nReleased 2001-02-03.\n\n- A change.\n'), /^1\.10\.0 is below 1\.9\.0/],
    ['a note ahead of the app', page('## 1.3.0\n\n- A change.\n', now.replace('\n\n', '\n\nReleased 2001-02-04.\n\n'), past), /^1\.3\.0 has a note, and the app is at 1\.2\.0$/],
    ['an older note with no day', page(now, '## 1.1.0\n\n- An older change.\n'), /^1\.1\.0's note does not open with the day it went out/],
    ['a version that went out, gone', page(now), /^1\.1\.0 went out, and its note is gone$/],
    ['a version that went out, on another day', page(now, past.replace('2001-02-03', '2001-02-04')), /^1\.1\.0 went out on 2001-02-03/],
    ['a wrapped list item', page('## 1.2.0\n\n- A made-up change that runs\n  on to a second line.\n', past), /^1\.2\.0's note has a line that starts with a space/],
    ['a wrapped paragraph', page('## 1.2.0\n\nA made-up change that runs\non to a second line.\n', past), /^1\.2\.0's note has a wrapped line/],
    ['a heading inside a note', page('## 1.2.0\n\n### New\n\n- A change.\n', past), /^1\.2\.0's note has a heading of its own/],
    ['another title', `# Changes\n\n${now}\n${past}`, /^the page does not open with "# Release notes"$/],
  ]) {
    const problems = problemsIn(text, '1.2.0', out);
    assert.ok(problems.some((problem) => expected.test(problem)), `${name}: not caught. Found: ${JSON.stringify(problems)}`);
  }

  assert.ok(compare('1.10.0', '1.9.0') > 0 && compare('0.8.3', '0.8.3') === 0 && compare('0.8.1', '0.8.3') < 0);
  assert.deepEqual(needsOf('    runs-on: x\n    needs: [gate, mac]\n'), ['gate', 'mac']);
  assert.deepEqual(needsOf('    needs: gate\n'), ['gate']);
  assert.deepEqual(needsOf('    runs-on: x\n'), []);
  assert.deepEqual(
    jobsOf('on: push\njobs:\n  gate:\n    uses: x\n  mac:\n    needs: gate\nother:\n  not-a-job:\n').map(({ name }) => name),
    ['gate', 'mac'],
  );
});

// Brad, 2026-11-21: "and it should be enforced that the notes were published
// before the build can go out". This file is one of the repository checks,
// the gate runs them, and every job of the Release workflow waits on the
// gate, so a version with no note is never built. The note is on `main`
// before the run starts, and the release script puts the same note on the
// version's page.
test('a version with no note cannot be built: the gate runs this check, and every job of the Release workflow waits on the gate', () => {
  assert.match(read('Makefile'), /^test-repo:.*\n\tnode --test scripts\/\*\.test\.mjs$/m, 'make test-repo no longer runs every check in scripts/');
  assert.match(read('.github/workflows/gate.yml'), /^ {6}- run: make test-repo$|^ {8}run: make test-repo$/m, 'the gate no longer runs make test-repo');

  const jobs = jobsOf(read('.github/workflows/release.yml'));
  const gate = jobs.find(({ name }) => name === 'gate');
  assert.ok(gate, 'the Release workflow has no gate job');
  assert.match(gate.text, /^ {4}uses: \.\/\.github\/workflows\/gate\.yml$/m, "the Release workflow's gate job runs something else");
  assert.ok(jobs.length > 1, 'the Release workflow has no job but the gate');
  for (const { name, text } of jobs) {
    if (name === 'gate') continue;
    assert.ok(needsOf(text).includes('gate'), `the ${name} job of the Release workflow does not wait on the gate`);
  }

  assert.equal(/^NOTES_FILE=(\S+)$/m.exec(read('scripts/release.sh'))?.[1], NOTES, 'release.sh reads its notes from another file');
  assert.match(read('CONTRIBUTING.md'), /\[RELEASES\.md\]\(RELEASES\.md\)/, 'CONTRIBUTING.md does not say where a version\'s note goes');
});

// Brad, 2026-11-21: "the root readme clearly shows user where to find the
// notes".
test('the root README says where the release notes are: in its index, beside its status, and in a section of their own', () => {
  const readme = read('README.md');
  const top = readme.slice(0, readme.indexOf('\n## '));
  assert.ok(top.includes('[Release notes](#release-notes)'), "the README's index does not link the release notes");
  assert.equal(top.split('(#release-notes)').length - 1, 2, 'the top of the README does not point to the release notes twice: in the index, and beside the status');
  assert.match(top, /^\*\*What changed in each version\*\* is under \[Release notes\]\(#release-notes\)\.$/m, 'the README does not say beside its status where the notes are');

  const section = sectionsOf(readme).filter(({ heading }) => heading === 'Release notes');
  assert.equal(section.length, 1, 'the README does not have one "## Release notes" section');
  assert.ok(section[0].body.includes(`[${NOTES}](${NOTES})`), `the README's section does not link ${NOTES}`);
  assert.ok(section[0].body.includes(`[Releases](${RELEASES})`), "the README's section does not link the releases on GitHub");

  const download = sectionsOf(readme).find(({ heading }) => heading === 'Download the built app');
  assert.ok(download?.body.includes('(#release-notes)'), 'the README does not point to the notes beside the downloads');

  // The page itself says where else its notes are shown.
  assert.ok(read(NOTES).includes(`[Releases](${RELEASES})`), `${NOTES} does not link the releases on GitHub`);
});
