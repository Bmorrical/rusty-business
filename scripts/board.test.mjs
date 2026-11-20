// The Workflow view and the boards it reads, checked on every test run.
//
// It answers the asks that started it: "I need to see my "Road"", and to see
// it "privately"; "each subtask is a small slice that needs to be done"; and
// "offline first, security second".
//
// Rust keeps the boards, and its tests in board.rs check how it behaves. The
// page's reading of a board has its tests in board.test.js. These fail when
// the side loses its Workflow button, when the view asks Rust for anything
// but its four commands or a command takes a path, when the view or the store
// gains a way to reach past this computer, to delete a file, or to start a
// program, when a file could be written in one step, when a note could be
// left behind, when a pick could land out of view, when the view shows a
// board's words as markup or gains a way to change a gate, when the sample
// board stops being made up, when a rule loses its Rust test, or when the
// READMEs, the roadmap, and SECURITY.md stop saying what the view does.
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const APP = 'apps/desktop/src/App.vue';
const VIEW = 'apps/desktop/src/Workflow.vue';
const HELPERS = 'apps/desktop/src/board.js';
const STORE = 'apps/desktop/src-tauri/src/board.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';
const SAMPLE = 'apps/desktop/src-tauri/sample-board.json';

/** Everything the view asks of Rust. */
const COMMANDS = ['list_boards', 'load_board', 'open_sample_board', 'save_board_note'];

/** What a command may take from the page: a board's id, a story's number, and a note. */
const TAKEN = ['id: String', 'story: u64', 'text: String'];

/** Ways for a page to reach past this computer. */
const WAYS_OUT = ['fetch(', 'XMLHttpRequest', 'WebSocket', 'EventSource', 'sendBeacon', '<iframe', 'http://', 'https://'];

/** What the store never does: reach the network, delete, or start a program. */
const NEVER = [
  'remove_file',
  'remove_dir',
  'reqwest',
  'ureq',
  'std::net',
  'TcpStream',
  'TcpListener',
  'UdpSocket',
  'http://',
  'https://',
  'std::process',
  'Command::new',
  'OpenOptions',
  'set_len',
];

/** The Rust tests that keep the rules. */
const KEPT_BY = [
  'the_sample_board_is_a_board_v1_file',
  'a_pulled_board_comes_back_whole',
  'a_pull_replaces_the_snapshot_and_keeps_the_note',
  'a_note_is_kept_apart_from_the_snapshot',
  'an_empty_note_clears_it_and_a_long_one_is_refused',
  'a_note_needs_a_story_on_the_board_unless_it_already_has_one',
  'an_id_cannot_reach_outside_the_folder',
  'a_file_that_is_not_a_board_is_named_and_left_alone',
  'a_board_too_large_is_not_read',
  'a_link_to_a_file_somewhere_else_is_not_read',
  'a_board_that_breaks_the_format_is_refused_with_the_reason',
  'notes_that_cannot_be_read_are_never_written_over',
  'the_folder_and_its_files_are_the_owners_alone',
];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** The store without its tests. */
function code(store) {
  const tests = store.indexOf('#[cfg(test)]');
  assert.ok(tests >= 0, `${STORE} has no tests`);
  return store.slice(0, tests);
}

/** One function's text in the view. */
function functionOf(view, name) {
  const start = view.search(new RegExp(`\\n(?:async )?function ${name}\\(`));
  assert.ok(start >= 0, `${VIEW} has no ${name}`);
  return view.slice(start, view.indexOf('\n}', start) + 2);
}

/** How many times a text holds a piece. */
function count(text, piece) {
  return text.split(piece).length - 1;
}

// Brad, 2026-11-20: "I need to see my "Road"".
test('the side has a Workflow button, and it opens the view in place of the chat', () => {
  const app = read(APP);
  assert.match(app, /import Workflow from "\.\/Workflow\.vue";/, 'the page does not load the view');

  const button = app.indexOf(`@click="togglePage('workflow')"`);
  assert.ok(button >= 0, 'the side has no Workflow button');
  assert.ok(button < app.indexOf(`@click="togglePage('installers')"`), 'the Workflow button is not above Installers');
  assert.match(app.slice(button, button + 80), />\s+Workflow\s+<\/button>/, 'the button does not say Workflow');

  const start = app.indexOf('<Workflow');
  assert.ok(start >= 0, 'the page does not show the view');
  const tag = app.slice(start, app.indexOf('/>', start));
  assert.ok(tag.includes(`v-if="page === 'workflow'"`), 'the view shows beside another page');
  assert.ok(tag.includes('@close="closePage"'), 'the view cannot go back to the chat');
  assert.ok(tag.includes(':wan="gate"') && tag.includes(':lan="lanGate"'), 'the view is not told where the gates stand');

  // Opening it asks the app for nothing: the view reads its own board.
  const open = functionOf(app, 'openPage');
  const early = open.indexOf('if (name === "workflow") return;');
  assert.ok(early >= 0, 'opening Workflow goes on to ask Rust for the makers');
  assert.ok(early < open.indexOf('await '), 'opening Workflow asks Rust before it stops');
});

test('the view asks Rust for four things, and each is a command the app offers', () => {
  const view = read(VIEW);
  const asked = [...view.matchAll(/invoke\(\s*"([a-z_]+)"/g)].map((match) => match[1]);
  assert.equal(count(view, 'invoke('), asked.length, 'the view calls a command it does not name');
  assert.deepEqual([...new Set(asked)].sort(), [...COMMANDS].sort(), 'the view asks Rust for something else');

  const lib = read(LIB);
  const store = code(read(STORE));
  assert.ok(lib.includes('mod board;'), `${LIB} does not hold the store`);
  for (const name of COMMANDS) {
    assert.ok(lib.includes(`board::${name},`), `the app does not offer ${name}`);
    assert.match(store, new RegExp(`#\\[tauri::command\\]\\s+pub fn ${name}\\(`), `${STORE} has no ${name}`);
  }
  assert.equal(count(store, '#[tauri::command]'), COMMANDS.length, 'the store offers a command the view does not use');
  assert.equal(count(lib, 'board::'), COMMANDS.length + 1, `${LIB} offers a board command this check does not know`);
});

// Brad, 2026-11-20: "offline first, security second".
test('no command takes a path from the page, and an id cannot name one', () => {
  const store = code(read(STORE));
  for (const name of COMMANDS) {
    const start = store.indexOf(`pub fn ${name}(`) + `pub fn ${name}(`.length;
    const taken = store
      .slice(start, store.indexOf(') ->', start))
      .replace("boards: tauri::State<'_, Boards>", '')
      .split(',')
      .map((each) => each.trim())
      .filter(Boolean);
    for (const each of taken) assert.ok(TAKEN.includes(each), `${name} takes ${each} from the page`);
  }

  assert.match(
    store,
    /fn valid_id\(id: &str\) -> bool \{\s+!id\.is_empty\(\) && id\.len\(\) <= 64 && id\.chars\(\)\.all\(\|c\| c\.is_ascii_alphanumeric\(\) \|\| c == '-'\)\s+\}/,
    'an id may hold more than letters, digits, and hyphens',
  );
  // Both files of a board are found through the id check, and nowhere else.
  for (const name of ['snapshot', 'drafts_file']) {
    const start = store.indexOf(`fn ${name}(&self, id: &str)`);
    assert.ok(start >= 0, `${STORE} has no ${name}`);
    const body = store.slice(start, store.indexOf('\n    }', start));
    assert.ok(body.indexOf('if !valid_id(id)') >= 0, `${name} skips the id check`);
    assert.ok(body.indexOf('if !valid_id(id)') < body.indexOf('.join('), `${name} builds the path before it checks the id`);
  }
  assert.equal(count(store, 'format!("{id}.json")'), 2, 'a board file is named somewhere the id check does not cover');
});

test('the view and the store have no way past this computer, delete nothing, and start nothing', () => {
  for (const file of [VIEW, HELPERS]) {
    const text = read(file);
    for (const way of WAYS_OUT) assert.ok(!text.includes(way), `${file} holds ${way}`);
  }

  // The tests too: not one of them deletes, or reaches out.
  const store = read(STORE);
  for (const never of NEVER) assert.ok(!store.includes(never), `${STORE} holds ${never}`);
  // The store stands alone. It asks nothing of the network, so it needs no gate.
  assert.ok(!code(store).includes('crate::'), `${STORE} reaches into another part of the app`);
  assert.ok(!code(store).includes('std::env'), `${STORE} reads the environment`);
});

test('a file is written beside itself, then moved into place', () => {
  const store = code(read(STORE));
  assert.equal(count(store, 'fs::write('), 1, 'the store writes a file somewhere other than its one helper');
  const start = store.indexOf('\nfn write(path: &Path, text: &str, what: &str)');
  assert.ok(start >= 0, `${STORE} has no write helper`);
  const write = store.slice(start, store.indexOf('\n}', start));
  const partial = write.indexOf('fs::write(&partial, text)');
  const owned = write.indexOf('own(&partial, 0o600);');
  const moved = write.indexOf('fs::rename(&partial, path)');
  assert.ok(write.includes('path.with_extension("json.partial")'), 'the half-written file has another name');
  assert.ok(partial >= 0 && owned > partial && moved > owned, 'a file is not written, made private, then moved, in that order');

  // The folders are the person's alone, and one write runs at a time.
  assert.match(store, /fs::create_dir_all\(&dir\)[^;]+;\s+own\(&dir, 0o700\);/, 'the boards folder is open to others');
  for (const name of ['pull', 'note']) {
    const from = store.indexOf(`pub fn ${name}(&self`);
    const body = store.slice(from, store.indexOf('\n    }', from));
    assert.ok(body.includes('self.writing.lock()'), `${name} does not wait its turn to write`);
    assert.ok(body.indexOf('self.writing.lock()') < body.indexOf('write(&path'), `${name} writes before it has its turn`);
  }
});

// Brad, 2026-11-20: "privately".
test('a pull writes the snapshot alone, and a note the drafts alone', () => {
  const store = code(read(STORE));
  const body = (name) => {
    const from = store.indexOf(`pub fn ${name}(&self`);
    assert.ok(from >= 0, `${STORE} has no ${name}`);
    return store.slice(from, store.indexOf('\n    }', from));
  };
  assert.ok(body('pull').includes('let path = self.snapshot(&board.id)?;'), 'a pull writes somewhere other than the snapshot');
  assert.ok(!body('pull').includes('drafts'), 'a pull touches the notes');
  assert.ok(body('note').includes('let path = self.drafts_file(id)?;'), 'a note is written somewhere other than the drafts');
  assert.ok(!body('note').includes('self.snapshot('), 'a note writes the snapshot');
  // A file that is a link, or larger than a board may be, is not read.
  assert.match(store, /fs::symlink_metadata\(path\)\s+\.map\(\|found\| found\.file_type\(\)\.is_file\(\)\)/, 'a linked file is read');
  assert.match(store, /\.take\(SIZE_LIMIT \+ 1\)\s+\.read_to_string\(&mut text\)/, 'a file is read past the size a board may be');
});

test('every rule has its Rust test', () => {
  const store = read(STORE);
  for (const name of KEPT_BY) {
    assert.match(store, new RegExp(`#\\[test\\]\\s+fn ${name}\\(`), `${STORE} lost its test ${name}`);
  }
});

test('a note is never left behind', () => {
  const view = read(VIEW);
  // Esc and "Back to the chat" both wait for the note.
  assert.ok(view.includes('@keydown.esc="leave"'), 'Esc leaves without saving the note');
  assert.ok(view.includes('@click="leave">Back to the chat</button>'), '"Back to the chat" leaves without saving the note');
  assert.match(functionOf(view, 'leave'), /if \(await saveNote\(\)\) emit\("close"\);/, 'the view closes on a note it could not save');
  assert.equal(count(view, 'emit("close")') + count(view, "emit('close')"), 1, 'the view closes some other way');
  assert.match(view, /onBeforeUnmount\(\(\) => \{\s+saveNote\(\);\s+\}\);/, 'a note is not saved when the view goes');

  // Another story, another board, or the sample opens only once the note is safe.
  const pick = functionOf(view, 'pickStory');
  assert.ok(pick.indexOf('if (!(await saveNote())) return;') >= 0, 'another story opens over an unsaved note');
  assert.ok(pick.indexOf('if (!(await saveNote())) return;') < pick.indexOf('openNumber.value = number;'), 'the story changes before the note is saved');
  assert.match(functionOf(view, 'pickBoard'), /if \(await saveNote\(\)\) show\(await invoke\("load_board", \{ id \}\)\);/, 'another board opens over an unsaved note');
  const sample = functionOf(view, 'openSample');
  assert.ok(sample.indexOf('if (!(await saveNote())) return;') >= 0, 'the sample opens over an unsaved note');
  assert.ok(sample.indexOf('if (!(await saveNote())) return;') < sample.indexOf('invoke('), 'the sample opens before the note is saved');

  // What was typed during a save is saved after it, and a failed save says so.
  const save = functionOf(view, 'saveNote');
  assert.ok(save.includes('if (saving) return saving.then((safe) => (safe ? saveNote() : false));'), 'a note typed during a save is skipped');
  assert.match(save, /\.catch\(\(error\) => \{[^}]*noteProblem\.value = String\(error\);\s+return false;/, 'a failed save reads as a saved one');
});

// Brad, 2026-11-20: "each subtask is a small slice that needs to be done".
test('a picked story or slice is in view, with the keyboard on its heading', () => {
  const view = read(VIEW);
  assert.ok(view.includes('@click="pickSlice(entry.id)"'), 'a slice does not open through pickSlice');
  // Each pane has more than one heading, one for each thing it can show. Every one takes the keyboard.
  for (const [id, pane] of [['wf-story-title', 'story'], ['wf-slice-title', 'slice']]) {
    assert.ok(count(view, `id="${id}"`) >= 1, `the ${pane} pane has no heading`);
    assert.equal(count(view, `id="${id}"`), count(view, new RegExp(`id="${id}"[^>]* tabindex="-1"`, 'g')), `a ${pane} heading cannot take the keyboard`);
  }

  const story = functionOf(view, 'pickStory');
  assert.ok(story.includes('toTop(work, storyPane, slicePane);'), 'a picked story opens part-way down');
  assert.ok(story.includes('if (stacked(cols.value)) bring(storyPane);'), 'a picked story stays under the road on a narrow window');
  const slice = functionOf(view, 'pickSlice');
  assert.ok(slice.includes('toTop(slicePane);'), 'a picked slice opens part-way down');
  assert.ok(slice.includes('if (stacked(work.value)) bring(slicePane);'), 'a picked slice stays under the story when the two are stacked');

  const bring = functionOf(view, 'bring');
  assert.ok(bring.includes('?.focus({ preventScroll: true });'), 'the heading does not take the keyboard');
  assert.ok(bring.includes('?.scrollIntoView({ block: "start" });'), 'the pane is not brought into view');
});

test('the view shows a board as text, says where the gates stand, and changes neither', () => {
  const view = read(VIEW);
  assert.ok(!view.includes('v-html'), 'the view shows a board\'s words as markup');
  assert.ok(!view.includes('innerHTML'), 'the view writes markup itself');
  assert.ok(view.includes('<style scoped>'), 'the view\'s styles reach the rest of the page');

  assert.ok(view.includes('WAN {{ wan ? "on" : "off" }}'), 'the view does not say where the WAN gate stands');
  assert.ok(view.includes('LAN {{ lan ? "on" : "off" }}'), 'the view does not say where the LAN gate stands');
  assert.ok(view.includes('This view reads only that copy.'), 'the view does not say it reads the local copy alone');
  // Its only words to the rest of the page: close me, and show the side.
  assert.ok(view.includes('const emit = defineEmits(["close", "side"]);'), 'the view can ask the page for something else');
  assert.ok(view.includes('pulledLine(board.pulledAt)'), 'the view does not say when the copy was pulled');
});

test('the sample board is made up, and travels inside the app', () => {
  assert.ok(code(read(STORE)).includes('const SAMPLE: &str = include_str!("../sample-board.json");'), 'the sample is read from somewhere else');
  const text = read(SAMPLE);
  const sample = JSON.parse(text);
  assert.equal(sample.format, 'board v1');
  assert.equal(sample.id, 'sample');
  assert.equal(sample.name, 'Sample board');
  assert.equal(sample.pulledAt, 0, 'the sample says it was pulled');
  assert.ok(sample.epics[0].detail.includes('made-up'), 'the sample does not say it is made up');
  assert.ok(!/https?:|www\.|@|\b\d{1,3}(?:\.\d{1,3}){3}\b/.test(text), 'the sample holds an address');

  const numbers = sample.epics.flatMap((epic) => epic.stories.map((story) => story.number));
  assert.equal(new Set(numbers).size, numbers.length, 'two stories in the sample share a number');
  for (const story of sample.epics.flatMap((epic) => epic.stories)) {
    const ids = story.slices.map((slice) => slice.id);
    assert.equal(new Set(ids).size, ids.length, `two slices of #${story.number} share an id`);
  }

  const view = read(VIEW);
  assert.ok(view.includes('No board on this computer yet.'), 'the view has no words for an empty folder');
  assert.match(view, /The sample\s+board is made up, and shows how one reads\./, 'the view does not say the sample is made up');
});

test('the READMEs, the roadmap, and SECURITY.md say what the view does', () => {
  const desktop = read('apps/desktop/README.md');
  assert.ok(desktop.includes('## Boards'), 'apps/desktop/README.md has no Boards section');
  const boards = desktop.slice(desktop.indexOf('## Boards'));
  for (const folder of [
    '~/Library/Application Support/net.morrical.rusty-business/boards',
    '%APPDATA%\\net.morrical.rusty-business\\boards',
    '~/.local/share/net.morrical.rusty-business/boards',
  ]) {
    assert.ok(boards.includes(folder), `apps/desktop/README.md does not say boards are kept in ${folder}`);
  }
  assert.ok(boards.includes('"format": "board v1"'), 'apps/desktop/README.md does not show the format');
  for (const lane of ['sliced', 'written', 'checked', 'reviewed', 'main']) {
    assert.ok(boards.includes(`\`${lane}\``), `apps/desktop/README.md does not name the lane ${lane}`);
  }

  const source = read('apps/desktop/src/README.md');
  // A row starts its line: a file named in a sentence further down is not one.
  const row = (text, file) => text.includes(`\n| \`${file}\``);
  for (const file of ['Workflow.vue', 'board.js', 'board.test.js']) {
    assert.ok(row(source, file), `apps/desktop/src/README.md has no row for ${file}`);
  }
  assert.ok(row(read('apps/desktop/src-tauri/src/README.md'), 'board.rs'), 'apps/desktop/src-tauri/src/README.md has no row for board.rs');
  assert.ok(row(read('apps/desktop/src-tauri/README.md'), 'sample-board.json'), 'apps/desktop/src-tauri/README.md has no row for the sample board');
  assert.ok(row(read('scripts/README.md'), 'board.test.mjs'), 'scripts/README.md has no row for this check');

  const roadmap = read('ROADMAP.md');
  const done = roadmap.slice(roadmap.indexOf('\n## Done'));
  assert.ok(done.includes('- **The Workflow view.**'), 'the roadmap does not list the Workflow view as done');

  const security = read('SECURITY.md');
  const promises = security.slice(security.indexOf('These are the promises that matter most.'));
  assert.match(promises, /The Workflow view reads only the boards kept on this computer/, 'SECURITY.md does not promise the view reads the local copy alone');
  assert.match(promises, /with both\s+gates\s+off/, 'SECURITY.md does not say the view works with both gates off');
  assert.match(promises, /never\s+changes\s+(?:or\s+removes\s+)?your\s+notes/, 'SECURITY.md does not promise a new copy leaves the notes');
});
