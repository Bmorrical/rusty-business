// The repository's rule: every folder has a README.
//
// A folder counts when it holds, at any depth, a file that git tracks or
// would track. Build output, node_modules, and anything else git ignores
// are left out because git never lists them.

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));

// Folders that are settings for a tool, not part of the project. A README
// directly in .github would be shown as the repository's front page in
// place of the real one, so that folder never gets one; the folders inside
// it still do.
const NO_README = new Set(['.github', '.vscode']);

/** Every folder that holds one of the files, from the root down. */
export function foldersOf(files) {
  const folders = new Set(['.']);
  for (const file of files) {
    let folder = path.posix.dirname(file);
    while (folder !== '.') {
      folders.add(folder);
      folder = path.posix.dirname(folder);
    }
  }
  return [...folders].sort();
}

/** The folders, of those given, that hold no README.md. */
export function missingReadmes(folders, hasFile) {
  return folders.filter(
    (folder) => !NO_README.has(folder) && !hasFile(path.posix.join(folder, 'README.md')),
  );
}

function projectFiles() {
  const listed = execFileSync(
    'git',
    ['ls-files', '--cached', '--others', '--exclude-standard', '-z'],
    { cwd: REPO, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 },
  );
  // A file that was moved or removed stays on git's list until the change
  // is committed, so only files that are really here count.
  return listed
    .split('\0')
    .filter(Boolean)
    .filter((file) => existsSync(path.join(REPO, file)));
}

test('every folder has a README', () => {
  const folders = foldersOf(projectFiles());
  assert.ok(folders.length > 10, 'the list of folders looks too short to be right');

  const missing = missingReadmes(folders, (file) => existsSync(path.join(REPO, file)));
  assert.deepEqual(missing, [], `no README.md in: ${missing.join(', ')}`);
});

test('every check in this folder is listed in its README', () => {
  // The makers check was missed once, so a check that is not listed fails.
  const listed = readFileSync(path.join(REPO, 'scripts', 'README.md'), 'utf8')
    .split('\n')
    .map((line) => line.match(/^\| `([^`]+\.test\.mjs)` \| Fails when /)?.[1])
    .filter(Boolean);
  const checks = readdirSync(path.join(REPO, 'scripts')).filter((file) => file.endsWith('.test.mjs'));
  assert.ok(checks.length > 10, 'the list of checks looks too short to be right');
  assert.deepEqual(checks.filter((file) => !listed.includes(file)), [], 'a check is missing from scripts/README.md');
  assert.deepEqual(listed.filter((file) => !checks.includes(file)), [], 'scripts/README.md lists a check that is not there');
});

test('a folder is found at every depth, once', () => {
  assert.deepEqual(foldersOf(['a/b/c.txt', 'a/d.txt', 'e.txt']), ['.', 'a', 'a/b']);
});

test('a folder with no README is named, and tool folders are let off', () => {
  const have = new Set(['README.md', 'a/README.md', '.github/workflows/README.md']);
  const folders = ['.', '.github', '.github/workflows', '.vscode', 'a', 'a/b'];
  assert.deepEqual(
    missingReadmes(folders, (file) => have.has(file)),
    ['a/b'],
  );
});
