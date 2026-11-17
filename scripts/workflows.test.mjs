// The rules for the GitHub workflows, checked on every test run. The one
// they all serve: nothing deploys until the gate is green.
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

function workflow(name) {
  const found = workflows.find((each) => each.name === name);
  assert.ok(found, `${name} is missing from .github/workflows`);
  return found.text;
}

test('the gate and the deploy workflow are both here', () => {
  assert.deepEqual(
    workflows.map((each) => each.name),
    ['deploy.yml', 'gate.yml'],
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

test('every workflow can only read the repository', () => {
  for (const { name, text } of workflows) {
    const plain = withoutComments(text);
    assert.match(plain, /^permissions:\n {2}contents: read\n(?! )/m, `${name}: top-level permissions`);
    assert.doesNotMatch(plain, /:\s*write\b/, `${name}: something is given write access`);
    assert.equal(
      plain.match(/^\s*permissions:/gm).length,
      1,
      `${name}: a job sets its own permissions`,
    );
  }
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

test('a deploy starts only on request, and every job waits for the gate', () => {
  const text = workflow('deploy.yml');
  assert.deepEqual(triggersOf(text), ['workflow_dispatch']);

  const jobs = jobsOf(text);
  const gate = jobs.find((job) => job.id === 'gate');
  assert.ok(gate, 'deploy.yml has no gate job');
  assert.match(gate.body, /^ {4}uses: \.\/\.github\/workflows\/gate\.yml$/m);
  assert.match(gate.body, /^ {4}if: github\.ref == 'refs\/heads\/main'$/m);

  const others = jobs.filter((job) => job.id !== 'gate');
  assert.ok(others.length > 0, 'deploy.yml has no deploy job');
  for (const job of others) {
    assert.match(
      job.body,
      /^ {4}needs: (gate|\[(.*, )?gate(, .*)?\])$/m,
      `the ${job.id} job does not wait for the gate`,
    );
    assert.doesNotMatch(job.body, /^ {4}if: .*always\(\)/m, `the ${job.id} job runs past a red gate`);
  }
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
