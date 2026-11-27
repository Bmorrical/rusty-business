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
// left behind, when one board that cannot be opened could hide the rest,
// when a pick could land out of view, when the view shows a
// board's words as markup or gains a way to change a gate, when the sample
// board stops being made up, when a rule loses its Rust test, or when the
// READMEs, the roadmap, and SECURITY.md stop saying what the view does.
//
// A second ask, the same day: "we can start with a flat file", then "i also
// like a one command option at the cli to push pull either way", "probably
// bi-directional sync with conflict resolution", "any user can use the import
// export and build their own backend", and "this keeps it loosely coupled".
// So these also fail when the `board` command stops being one command the
// program's entry runs, when the command or the merge behind it gains a way
// past this computer, a way to delete, a way to start a program, or a file it
// reads or writes without the store, when a path could come from the page,
// when a pull could write the file or a push the copy here, when a conflict
// could write anything, or when the docs stop saying how to keep a board in
// step and how to build the other side.
//
// A third set, the day after: "it should have a general meta-data and data
// struct, versioning so someone doesn't have to remap when the next party
// wants to iterate it's logic ahead", "anyone should be able to read a
// ruleset and import their own road", and "direct API connections are refused
// as rule for now, use the import / export to help enforce the offline / LAN
// model", with "security.md articulates that and anywhere else". So these
// also fail when BOARD.md, the file's ruleset, loses a section or one of its
// version rules, when its example stops being checked by Rust, when the rule
// against a direct API connection is missing or reworded in any place that
// states it, when it loses "for now", or when the roadmap promises a source
// that is an address on a network.
//
// "they build the import export on their own back", so the ruleset also has
// to say what a backend may do that this app's own side does not: keep its
// own ids, underscores and all, retire a status, apply part of a file, and
// leave things out of one. These fail when one of those rules goes.
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
const KEPT = 'apps/desktop/src-tauri/src/store.rs';
const LIB = 'apps/desktop/src-tauri/src/lib.rs';
const SAMPLE = 'apps/desktop/src-tauri/sample-board.json';
const MERGE = 'apps/desktop/src-tauri/src/sync.rs';
const COMMAND = 'apps/desktop/src-tauri/src/cli.rs';
const ENTRY = 'apps/desktop/src-tauri/src/main.rs';
const CONF = 'apps/desktop/src-tauri/tauri.conf.json';

/** Everything the `board` command does. */
const WORDS = ['list', 'import', 'export', 'pull', 'push', 'sync'];

/** The file's ruleset, at the root of the repository. */
const RULESET = 'BOARD.md';

/**
 * The rule against a direct API connection, in one wording wherever it is
 * stated. "For now" is part of the rule.
 */
const NO_API = [
  "No direct API connection to a board's backend, for now.",
  'The app holds no address, no token, and no key for one, and a change that adds one is refused.',
  'A board moves as a file: exported on one side, and imported on the other.',
].join(' ');

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
  'the_sample_board_is_a_board_file_on_this_apps_line',
  'a_pulled_board_comes_back_whole',
  'a_pull_replaces_the_snapshot_and_keeps_the_note',
  'a_note_is_kept_apart_from_the_snapshot',
  'an_empty_note_clears_it_and_a_long_one_is_refused',
  'a_note_needs_a_story_on_the_board_unless_it_already_has_one',
  'a_note_that_would_take_the_notes_past_the_size_cap_is_refused',
  'the_sample_never_writes_over_a_board_already_there',
  'a_broken_file_under_the_samples_name_is_said_and_left_as_it_is',
  'an_id_cannot_reach_outside_the_folder',
  'a_file_that_is_not_a_board_is_named_and_left_alone',
  'a_board_too_large_is_not_read',
  'a_link_to_a_file_somewhere_else_is_not_read',
  'a_board_that_breaks_the_format_is_refused_with_the_reason',
  'notes_that_cannot_be_read_are_never_written_over',
  'the_folder_and_its_files_are_the_owners_alone',
  'a_rev_and_a_history_are_checked',
  'two_releases_cannot_share_a_name',
  'the_last_synced_copy_is_kept_apart_from_the_snapshot',
  'a_boards_file_is_remembered',
  'the_sync_files_cannot_reach_outside_the_folder',
  'a_file_somewhere_else_is_read_as_any_board_file_is',
  'a_link_somewhere_else_is_neither_read_nor_written_through',
  'a_send_makes_a_new_file_or_replaces_its_own_board_and_nothing_else',
  'the_rulesets_example_is_a_board_file',
  'a_file_is_a_meta_block_and_a_data_block',
  'a_board_v1_file_is_still_read_and_is_written_back_as_board_v1',
  'what_a_reader_does_not_know_is_kept_and_written_back',
  'a_file_on_a_line_above_this_apps_is_the_one_refusal',
  'statuses_are_data_and_are_checked',
  'a_retired_status_stays_on_the_list_and_is_written_back',
  'a_board_this_app_writes_says_so',
];

/** The Rust tests that keep what two copies of a board come to. */
const MERGE_KEPT_BY = [
  'a_side_that_changed_nothing_takes_the_other_whole',
  'changes_to_different_things_are_both_kept',
  'the_same_thing_changed_two_ways_is_a_conflict_settled_for_one_side',
  'what_each_side_added_is_kept',
  'what_one_side_removed_is_removed_unless_the_other_changed_it',
  'a_story_moved_to_another_release_moves',
  'notes_on_the_board_are_added_to_and_taken_from_by_both_sides',
  'the_side_that_moved_things_sets_the_order',
  'with_nothing_to_compare_against_nothing_is_taken_away',
  'a_release_goes_with_its_stories_and_stays_while_one_remains',
  'a_file_comes_from_a_copy_when_the_copy_is_in_its_history',
  'a_stamped_board_names_its_state_and_lists_where_it_came_from',
  'a_key_this_app_does_not_know_is_merged_like_any_other',
  'a_slices_number_is_never_taken_away_by_a_side_that_has_none',
  'a_status_id_and_the_lists_of_statuses_are_merged_three_ways',
  'a_file_on_a_lower_line_takes_away_nothing_it_has_no_place_for',
  'a_backend_that_moves_up_a_line_and_names_its_statuses_is_no_conflict',
  'the_merged_board_is_on_the_newer_of_the_two_versions',
];

/** The Rust tests that keep what the `board` command does. */
const COMMAND_KEPT_BY = [
  'only_the_board_command_is_taken',
  'help_is_shown_and_a_command_that_cannot_be_read_is_refused',
  'a_first_import_makes_this_computers_copy_and_leaves_the_file_as_it_is',
  'one_command_pushes_and_one_pulls_and_neither_writes_the_other_side',
  'sync_brings_together_what_each_side_changed',
  'a_conflict_writes_nothing_until_a_side_is_picked',
  'a_push_is_refused_while_the_file_has_changes_that_are_not_here',
  'an_export_makes_a_new_file_and_writes_over_nothing_but_its_own_board',
  'with_no_id_every_board_with_a_file_is_done_and_the_worst_is_left_with',
  'list_names_each_board_and_the_file_it_has',
  'what_the_file_took_away_is_taken_away_here_only_when_it_came_from_this_copy',
  'a_backend_written_by_hand_stays_in_step_through_one_file',
  'a_backend_that_changed_before_it_imported_loses_nothing',
  'the_help_says_a_board_moves_as_a_file_and_never_over_an_api',
  'a_board_v1_file_goes_back_out_as_board_v1',
  'a_file_a_step_ahead_comes_back_with_everything_this_app_does_not_know',
  'a_file_on_a_newer_line_moves_this_computers_copy_up_and_never_back',
  'a_file_on_a_line_above_this_apps_is_refused_and_nothing_is_changed',
];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** A Rust file without its tests. */
function code(text, file = STORE) {
  const tests = text.indexOf('#[cfg(test)]');
  assert.ok(tests >= 0, `${file} has no tests`);
  return text.slice(0, tests);
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
  // All four files of a board are found through the id check, and nowhere else.
  for (const name of ['snapshot', 'drafts_file', 'base_file', 'source_file']) {
    const start = store.indexOf(`fn ${name}(&self, id: &str)`);
    assert.ok(start >= 0, `${STORE} has no ${name}`);
    const body = store.slice(start, store.indexOf('\n    }', start));
    assert.ok(body.indexOf('if !valid_id(id)') >= 0, `${name} skips the id check`);
    assert.ok(body.indexOf('if !valid_id(id)') < body.indexOf('.join('), `${name} builds the path before it checks the id`);
  }
  assert.equal(count(store, 'format!("{id}.json")'), 4, 'a board file is named somewhere the id check does not cover');
});

test('the view and the store have no way past this computer, delete nothing, and start nothing', () => {
  for (const file of [VIEW, HELPERS]) {
    const text = read(file);
    for (const way of WAYS_OUT) assert.ok(!text.includes(way), `${file} holds ${way}`);
  }

  // The tests too: not one of them deletes, or reaches out.
  const store = read(STORE);
  for (const never of NEVER) assert.ok(!store.includes(never), `${STORE} holds ${never}`);
  // The store stands alone, but for its database. It asks nothing of the
  // network, so it needs no gate.
  const reaches = code(store).match(/crate::[a-z_]+/g) ?? [];
  assert.deepEqual([...new Set(reaches)], ['crate::store'], `${STORE} reaches into another part of the app`);
  assert.ok(!code(store).includes('std::env'), `${STORE} reads the environment`);

  // The database is held to the same: the tests too.
  const kept = read(KEPT);
  for (const never of NEVER) assert.ok(!kept.includes(never), `${KEPT} holds ${never}`);
  assert.ok(!code(kept, KEPT).includes('crate::'), `${KEPT} reaches into another part of the app`);
  assert.ok(!code(kept, KEPT).includes('std::env'), `${KEPT} reads the environment`);
});

// The road's line for this: "Boards kept in an encrypted SQLite database,
// with its key in the system's key store".
test('the boards are kept in an encrypted database, its key in the key store', () => {
  const kept = code(read(KEPT), KEPT);
  const cargo = read('apps/desktop/src-tauri/Cargo.toml');
  assert.match(cargo, /^rusqlite = \{ version = "[^"]+", features = \["bundled-sqlcipher"\] \}$/m, 'rusqlite is not built with SQLCipher');
  assert.match(cargo, /^rusqlite = \{ version = "[^"]+", features = \["bundled-sqlcipher-vendored-openssl"\] \}$/m, 'Windows has no SQLCipher');
  assert.ok(!/features = \["bundled"\]/.test(cargo), 'a plain SQLite is built beside SQLCipher');
  // The key goes to SQLCipher before anything is read, and a build that
  // cannot encrypt is refused.
  assert.ok(kept.indexOf('pragma_update(None, "key"') >= 0, `${KEPT} never gives the database its key`);
  assert.ok(kept.indexOf('pragma_update(None, "key"') < kept.indexOf('PRAGMA cipher_version'), `${KEPT} reads before it gives the key`);
  assert.ok(kept.includes('This build cannot encrypt'), `${KEPT} would open a database it cannot encrypt`);
  assert.ok(kept.includes('keyring::Entry::new'), `${KEPT} does not keep its key in the system's key store`);
  // The app and the command both open the boards through it.
  assert.ok(read(LIB).includes('board::Boards::open('), `${LIB} keeps the boards as files`);
  assert.ok(read('apps/desktop/src-tauri/src/cli.rs').includes('Boards::open('), 'the board command keeps the boards as files');
  assert.ok(read(LIB).includes('mod store;'), `${LIB} does not hold the database`);
});

test('a file is written beside itself, then moved into place', () => {
  const store = code(read(STORE));
  assert.equal(count(store, 'fs::write('), 0, 'the store writes a file in one step');
  assert.equal(count(store, 'fs::File::create('), 1, 'the store makes a file somewhere other than its one helper');
  assert.equal(count(store, 'fs::rename('), 1, 'the store moves a file somewhere other than its one helper');
  const start = store.indexOf('\nfn write(path: &Path, text: &str, what: &str)');
  assert.ok(start >= 0, `${STORE} has no write helper`);
  const write = store.slice(start, store.indexOf('\n}', start));
  assert.ok(write.includes('path.with_extension("json.partial")'), 'the half-written file has another name');
  // Made, made private, filled, put on the disk, then moved: in that order.
  // A file moved before it is on the disk can be empty after a power cut.
  const steps = [
    'fs::File::create(&partial)',
    'own(&partial, 0o600);',
    'file.write_all(text.as_bytes())',
    'file.sync_all()',
    'fs::rename(&partial, path)',
  ];
  let last = -1;
  for (const step of steps) {
    const at = write.indexOf(step);
    assert.ok(at > last, `the write helper has no "${step}", or has it out of order`);
    last = at;
  }

  // The folders are the person's alone, and one write runs at a time.
  assert.match(store, /fs::create_dir_all\(&dir\)[^;]+;\s+own\(&dir, 0o700\);/, 'the boards folder is open to others');
  for (const name of ['pull', 'note', 'keep_base', 'remember']) {
    const from = store.indexOf(`pub fn ${name}(&self`);
    const body = store.slice(from, store.indexOf('\n    }', from));
    assert.ok(body.includes('self.writing.lock()'), `${name} does not wait its turn to write`);
    assert.ok(body.indexOf('self.writing.lock()') < body.indexOf('self.put('), `${name} writes before it has its turn`);
  }
});

// One board that cannot be opened must not hide the boards that can.
test('the view opens the first board that opens, and names one it passed over', () => {
  const view = read(VIEW);
  const open = functionOf(view, 'openFirst');
  assert.ok(
    open.includes('await openFirstOf(good.value, (entry) => invoke("load_board", { id: entry.id }))'),
    'the view stops at the first board it cannot open',
  );
  assert.ok(open.includes('if (opened.local) show(opened.local);'), 'the board that opened is not shown');
  assert.ok(open.includes('problem.value = opened.problem;'), 'a board that was passed over is not named');
  assert.ok(
    view.includes('{{ good.length ? "No board could be opened." : "No board on this computer yet." }}'),
    'the view says there is no board when there are boards it could not open',
  );
  assert.match(read(HELPERS), /\nexport async function openFirstOf\(entries, load\) \{/, `${HELPERS} has no openFirstOf`);

  // Rust refuses a board where two releases share a name, so the name alone
  // tells two releases apart. A title is free text and may be left out.
  assert.ok(view.includes('v-for="row in rows" :key="row.epic.release"'), 'a release on the road is not told apart by its name');
});

// Brad, 2026-11-20: "privately".
test('a pull writes the snapshot alone, and a note the drafts alone', () => {
  const store = code(read(STORE));
  const body = (name) => {
    const from = store.indexOf(`pub fn ${name}(&self`);
    assert.ok(from >= 0, `${STORE} has no ${name}`);
    return store.slice(from, store.indexOf('\n    }', from));
  };
  assert.match(body('pull'), /self\.put\(\s*Kind::Board,\s*&board\.id,/, 'a pull writes somewhere other than the snapshot');
  assert.ok(!body('pull').includes('drafts'), 'a pull touches the notes');
  assert.match(body('note'), /self\.put\(\s*Kind::Drafts,\s*id,/, 'a note is written somewhere other than the drafts');
  assert.equal(count(body('note'), 'self.put('), 1, 'a note writes more than the drafts');
  assert.equal(count(body('pull'), 'self.put('), 1, 'a pull writes more than the snapshot');
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
  for (const [file, names] of [[MERGE, MERGE_KEPT_BY], [COMMAND, COMMAND_KEPT_BY]]) {
    const text = read(file);
    for (const name of names) {
      assert.match(text, new RegExp(`#\\[test\\]\\s+fn ${name}\\(`), `${file} lost its test ${name}`);
    }
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
  // A file on line 2 of the format: a meta block, a data block, and nothing else.
  assert.deepEqual(Object.keys(sample), ['meta', 'data'], 'the sample is not a meta block and a data block');
  const { meta } = sample;
  const { releases } = sample.data;
  assert.equal(meta.format, 'board');
  assert.equal(meta.version, '2.0');
  assert.equal(meta.id, 'sample');
  assert.equal(meta.name, 'Sample board');
  assert.ok(!meta.pulledAt, 'the sample says it was pulled');
  assert.ok(!meta.rev && !meta.writtenBy && !meta.writtenAt, 'the sample says a program wrote it');
  assert.ok(releases[0].detail.includes('made-up'), 'the sample does not say it is made up');
  assert.ok(!/https?:|www\.|@|\b\d{1,3}(?:\.\d{1,3}){3}\b/.test(text), 'the sample holds an address');

  // Brad, 2026-11-21: "dynamic would be good rather than static". The statuses are the sample's own list,
  // and every row names one of them by its id.
  for (const [list, rows] of [
    ['releases', releases],
    ['stories', releases.flatMap((release) => release.stories)],
  ]) {
    const statuses = meta.statuses[list];
    const ids = statuses.map((status) => status.id);
    assert.ok(ids.length > 1, `the sample has no list of statuses for its ${list}`);
    assert.equal(new Set(ids).size, ids.length, `two statuses for the sample's ${list} share an id`);
    for (const row of rows) {
      const status = statuses.find((entry) => entry.id === row.statusId);
      assert.ok(status, `"${row.title}" names a status the sample's list does not hold`);
      assert.equal(row.status, status.name, `"${row.title}" does not read as its status is named`);
    }
  }

  const numbers = releases.flatMap((epic) => epic.stories.map((story) => story.number));
  assert.equal(new Set(numbers).size, numbers.length, 'two stories in the sample share a number');
  for (const story of releases.flatMap((epic) => epic.stories)) {
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
  assert.ok(boards.includes('"format": "board"') && boards.includes('"version": "2.0"'), 'apps/desktop/README.md does not show the format and its version');
  assert.ok(boards.includes('[BOARD.md](../../BOARD.md)'), 'apps/desktop/README.md does not point to the ruleset');
  assert.match(boards, /A\s+board v1 file, the format before this one, is line 1 and is still read/, 'apps/desktop/README.md does not say a board v1 file is still read');
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

// Brad, 2026-11-20: "i also like a one command option at the cli to push pull either way".
test('one command in a terminal does it, and the program leaves from its entry alone', () => {
  const entry = read(ENTRY);
  const asked = entry.indexOf('rusty_business_lib::command(std::env::args().skip(1).collect())');
  assert.ok(asked >= 0, `${ENTRY} does not look for the board command`);
  assert.ok(asked < entry.indexOf('rusty_business_lib::run()'), 'the app opens before the board command is looked for');
  assert.equal(count(entry, 'std::process::exit('), 1, `${ENTRY} leaves in more than one place`);

  const lib = read(LIB);
  assert.ok(lib.includes('\nmod cli;\n') && lib.includes('\nmod sync;\n'), `${LIB} does not hold the command and the merge`);
  assert.match(lib, /pub fn command\(args: Vec<String>\) -> Option<i32> \{\s+cli::run\(&args\)\s+\}/, `${LIB} does not hand the command its words`);

  const command = code(read(COMMAND), COMMAND);
  assert.ok(
    command.includes('if args.first().map(String::as_str) != Some("board") {\n        return None;'),
    'the app does not open when something other than board is asked for',
  );
  // It runs without the app, so it finds the app's data folder by the app's own name.
  const identifier = JSON.parse(read(CONF)).identifier;
  assert.ok(command.includes(`pub const IDENTIFIER: &str = "${identifier}";`), `${COMMAND} and ${CONF} name two data folders`);
  assert.ok(/Boards::open\(\s+data\.join\(IDENTIFIER\)\.join\("boards"\),/.test(command), 'the command reads another boards folder than the app');

  for (const word of WORDS) {
    assert.ok(command.includes(`["${word}"`), `the command does not take ${word}`);
    assert.ok(command.includes(`\n  rusty-business board ${word}`), `the command's help does not show ${word}`);
  }
  assert.match(
    command,
    /const DONE: i32 = 0;\s+const REFUSED: i32 = 1;\s+const CONFLICTS: i32 = 2;/,
    'the command leaves with something other than 0, 1, and 2',
  );
});

// Brad, 2026-11-20: "this keeps it loosely coupled".
test('the command and the merge have no way past this computer, and touch no file themselves', () => {
  for (const file of [MERGE, COMMAND]) {
    // The tests too: not one of them deletes, or reaches out.
    const text = read(file);
    for (const never of NEVER) assert.ok(!text.includes(never), `${file} holds ${never}`);

    const own = code(text, file);
    // The page cannot call either, so a path can only come from the person at the terminal.
    assert.ok(!own.includes('tauri::'), `${file} offers the page a command`);
    // Every read and write is the store's.
    for (const direct of ['fs::', 'File::', 'std::env']) assert.ok(!own.includes(direct), `${file} holds ${direct}`);
  }
  // The merge only compares what it is handed.
  const merge = code(read(MERGE), MERGE);
  for (const reach of ['std::path', 'std::io', 'Boards']) assert.ok(!merge.includes(reach), `${MERGE} holds ${reach}`);

  const lib = read(LIB);
  assert.equal(count(lib, 'cli::'), 1, `${LIB} uses the command somewhere other than the program's start`);
  assert.equal(count(lib, 'sync::'), 0, `${LIB} offers the merge to the page`);
});

// Brad, 2026-11-20: "probably bi-directional sync with conflict resolution".
test('a pull never writes the file, a push never writes the copy here, and a conflict writes nothing', () => {
  const command = code(read(COMMAND), COMMAND);
  const body = (name) => {
    const from = command.indexOf(`\nfn ${name}(`);
    assert.ok(from >= 0, `${COMMAND} has no ${name}`);
    return command.slice(from, command.indexOf('\n}\n', from));
  };

  const pulled = body('pulled');
  const pushed = body('pushed');
  assert.ok(!pulled.includes('.send('), 'a pull writes the file');
  assert.ok(!pushed.includes('boards.pull('), 'a push writes the copy on this computer');
  assert.equal(count(command, '.send('), 1, 'the file is written somewhere other than a push');
  assert.equal(count(command, 'boards.pull('), 1, 'the copy here is written somewhere other than a pull');

  // With conflicts and no side picked, the pull stops before its first write.
  const stop = pulled.indexOf('if count > 0 && keep.is_none() {');
  assert.ok(stop >= 0, 'a pull goes on past its conflicts');
  assert.match(pulled.slice(stop), /^if count > 0 && keep\.is_none\(\) \{\s+conflicts\([^;]+;\s+return Ok\(CONFLICTS\);/, 'conflicts do not stop a pull');
  for (const write of ['boards.pull(', 'boards.keep_base(', 'boards.remember(']) {
    assert.ok(pulled.indexOf(write) > stop, `a pull calls ${write} before it has looked for conflicts`);
  }
  // The last synced copy counts only when the file came from it.
  assert.ok(pulled.includes('.filter(|base| sync::descends(&theirs, base))'), 'a pull compares against a copy the file did not come from');

  // A file is replaced only when everything in it is already here.
  const guard = pushed.indexOf('if !base.as_ref().is_some_and(|base| sync::same(base, theirs)) {');
  assert.ok(guard >= 0, 'a push replaces a file whose changes are not here');
  assert.ok(guard < pushed.indexOf('boards.send('), 'a push writes before it has looked at the file');
  assert.ok(pushed.includes('Pull first, or run sync.'), 'a refused push does not say what to do');

  // Sync pushes only when the pull is done.
  assert.match(body('both'), /if code != DONE \{\s+return code;\s+\}\s+push\(boards, id, file, out\)/, 'sync pushes after a pull that was not done');
});

// Brad, 2026-11-20: "we can start with a flat file".
test('the file somewhere else is read and written as any board file is', () => {
  const store = code(read(STORE));
  const body = (name) => {
    const from = store.indexOf(`pub fn ${name}(`);
    assert.ok(from >= 0, `${STORE} has no ${name}`);
    return store.slice(from, store.indexOf('\n    }', from));
  };

  const outside = body('outside');
  assert.ok(outside.includes('!= Some("json")'), 'a file with any name is read as a board');
  assert.ok(outside.includes('Self::parse(&Self::text(path, "Could not read the file")?)?'), 'the file skips the checks every board file gets');
  assert.ok(outside.includes('if !valid_id(&board.id)'), 'a file may say it is a board no id can name');

  const send = body('send');
  const turn = send.indexOf('self.writing.lock()');
  const looked = send.indexOf('Self::outside(path)?');
  const written = send.indexOf('write(path, &text,');
  assert.ok(turn >= 0 && looked > turn && written > looked, 'a send does not take its turn, look at the file, then write, in that order');
  assert.ok(send.includes('Some(there) if there.id != board.id => {'), 'a send writes over a file that holds another board');
  assert.ok(send.includes('path.with_extension("json.partial")'), 'a send does not look at where the half-written file goes');
  assert.equal(count(store, 'write(path, &text,'), 1, 'the store writes to a typed path somewhere other than a send');

  // The last synced copy and the remembered file each have their own folder.
  assert.match(body('keep_base'), /self\.put\(\s*Kind::Synced,\s*&board\.id,/, 'the last synced copy is written somewhere else');
  assert.match(body('remember'), /self\.put\(\s*Kind::Source,\s*id,/, 'the file is remembered somewhere else');
  assert.ok(!body('keep_base').includes('self.snapshot(') && !body('remember').includes('self.snapshot('), 'the sync files write the snapshot');
});

// Brad, 2026-11-20: "any user can use the import export and build their own backend".
test('the docs say how to keep a board in step, and how to build the other side', () => {
  const desktop = read('apps/desktop/README.md');
  const boards = desktop.slice(desktop.indexOf('## Boards'));
  assert.ok(boards.includes('### A board in step with a file'), 'apps/desktop/README.md does not say how to keep a board in step');
  for (const word of WORDS) {
    assert.ok(boards.includes(`\nrusty-business board ${word}`), `apps/desktop/README.md does not show board ${word}`);
  }
  for (const piece of ['`--mine`', '`--theirs`', '`synced/<id>.json`', '`sources/<id>.json`', '"rev": ', '"history": ', '**`rev`**', '**`history`**']) {
    assert.ok(boards.includes(piece), `apps/desktop/README.md does not name ${piece}`);
  }
  assert.match(boards, /leaves with 0 when it is done, 1 when it refused or failed, and 2 when\s+there are conflicts to settle/, 'apps/desktop/README.md does not say what the command leaves with');
  assert.match(boards, /lists each conflict,[^.]+,\s+and\s+writes\s+nothing/, 'apps/desktop/README.md does not say a conflict writes nothing');
  assert.match(boards, /Your notes stay on this computer/, 'apps/desktop/README.md does not say the notes stay');

  const own = boards.slice(boards.indexOf('### A backend of your own'));
  assert.ok(boards.includes('### A backend of your own'), 'apps/desktop/README.md does not say how to build the other side');
  assert.match(own, /The board file is an open format, and the file is the whole of the join/, 'the docs do not say the file is all two sides share');
  assert.match(own, /It needs none of this\s+app's code/, 'the docs do not say a backend needs none of this app');
  assert.ok(own.includes('- **To export**') && own.includes('- **To import**'), 'the docs do not say what a backend does to export and to import');
  assert.match(own, /A test in `cli\.rs` runs a backend written by hand/, 'the docs do not point at the test that runs a backend');

  const rust = read('apps/desktop/src-tauri/src/README.md');
  for (const file of ['sync.rs', 'cli.rs', 'main.rs']) {
    assert.ok(rust.includes(`\n| \`${file}\``), `apps/desktop/src-tauri/src/README.md has no row for ${file}`);
  }
  assert.match(rust, /No command the page can call takes a path/, 'apps/desktop/src-tauri/src/README.md does not say where a path can come from');

  const roadmap = read('ROADMAP.md');
  const done = roadmap.slice(roadmap.indexOf('\n## Done'));
  assert.ok(done.includes('- **A board in step with a file.**'), 'the roadmap does not list a board in step with a file as done');

  const security = read('SECURITY.md');
  const promises = security.slice(security.indexOf('These are the promises that matter most.'));
  assert.match(promises, /The `board` command, run in a terminal, reads and writes one board file\s+at a path you type/, 'SECURITY.md does not say what the command reads and writes');
  assert.match(promises, /The path never comes from the page/, 'SECURITY.md does not say where the path comes from');
  assert.match(promises, /leaves any other file as it is/, 'SECURITY.md does not promise a push leaves other files');
  assert.match(promises, /asks nothing of the network, starts no program, deletes nothing/, 'SECURITY.md does not say what the command never does');
});

/** A page as one line of plain words: no bold marks, and no line breaks. */
function flat(text) {
  return text.replace(/\*\*/g, '').replace(/\s+/g, ' ');
}

/** One version's note in RELEASES.md. */
function note(version) {
  const notes = read('RELEASES.md');
  const from = notes.indexOf(`\n## ${version}\n`);
  assert.notEqual(from, -1, `RELEASES.md has no note for ${version}`);
  const to = notes.indexOf('\n## ', from + 1);
  return notes.slice(from, to === -1 ? notes.length : to);
}

// "anyone should be able to read a ruleset and import their own road", and
// "versioning so someone doesn't have to remap when the next party wants to
// iterate it's logic ahead".
test('the board file has a ruleset, and it says what a version may change', () => {
  const page = read(RULESET);
  const headings = [...page.matchAll(/^## (.+)$/gm)].map((found) => found[1]);
  assert.deepEqual(
    headings,
    [
      'The rule above the rest',
      'The file',
      'The meta block',
      'The data block',
      'Statuses',
      'Lanes and moves',
      'Versions',
      'Reading a board v1 file',
      'When both sides changed',
      'Limits',
      'A backend of your own',
    ],
    'BOARD.md gained, lost, or moved a section',
  );
  for (const heading of headings) {
    const address = heading.toLowerCase().replace(/ /g, '-');
    assert.ok(page.includes(`](#${address})`), `BOARD.md's index has no link to "${heading}"`);
  }

  const versions = page.slice(page.indexOf('\n## Versions'), page.indexOf('\n## Reading a board v1 file'));
  for (const rule of [
    '**Within a line, a file only gains.**',
    '**A reader keeps what it does not know.**',
    '**A change that is not an addition starts a new line**',
    '**A reader says so when the line is above its own.**',
  ]) {
    assert.ok(versions.includes(rule), `BOARD.md lost a version rule: ${rule}`);
  }

  const words = flat(page);
  for (const key of ['format', 'version', 'id', 'name', 'writtenBy', 'writtenAt', 'rev', 'history', 'statuses', 'statusId', 'finished', 'moves']) {
    assert.ok(page.includes(`\`${key}\``), `BOARD.md does not name ${key}`);
  }
  assert.match(words, /needs none of this app's code/, 'BOARD.md does not say a backend needs none of this app');

  // A backend's ids are its own, and so is how much of a file it takes.
  const statuses = flat(page.slice(page.indexOf('\n## Statuses'), page.indexOf('\n## Lanes and moves')));
  assert.match(statuses, /Its stable id: up to 64 letters, digits, hyphens, and underscores/, 'BOARD.md does not say a status id may hold an underscore');
  assert.match(statuses, /should not have to rename one to write a board file/, 'BOARD.md does not say why a status id may hold an underscore');
  assert.match(statuses, /The board's `id`, a `rev`, and a slice's `id` stay letters, digits, and hyphens/, 'BOARD.md does not say which ids stay as they were');
  assert.match(statuses, /`archived` \| `true` on a status that is retired/, 'BOARD.md does not name archived');
  assert.match(statuses, /A retired status stays on its list/, 'BOARD.md does not say a retired status stays on its list');
  assert.ok(read(STORE).includes('fn valid_status_id('), 'board.rs lost the rule for a status id');
  const section = page.slice(page.indexOf('\n## A backend of your own'));
  const own = flat(section);
  for (const [rule, why] of [
    ['**What "changed" means.**', /comparing what it would write now with what the last file held, not by watching its own tables/],
    ['**An import may apply part of a file.**', /lists every difference it did not apply for the person importing/],
    ['**An import may apply part of a file.**', /its next export has a new `rev`, with the file's `rev` at the head of `history`/],
    ['**A status.**', /An import need not add, change, or retire a status/],
    ['**A row the file does not have.**', /a row's absence from a file means nothing by itself/],
    ['**A text over its limit.**', /is left out whole, and the writer tells the person exporting how many it left out\. It is never cut short/],
    ['**Rows that are not for the other side.**', /reads a row that was in the last file and is not in this one as removed on the backend's side/],
    ['**`moves`, where no list was set.**', /writes `moves` only for a status that has a list/],
  ]) {
    assert.ok(section.includes(rule), `BOARD.md lost a rule for a backend: ${rule}`);
    assert.match(own, why, `BOARD.md no longer says what ${rule} comes to`);
  }
  for (const key of ['writtenBy', 'writtenAt', 'pulledAt']) {
    assert.ok(own.includes(`\`${key}\``), `BOARD.md does not keep ${key} out of what a backend compares`);
  }

  // The example in the ruleset is a real file: Rust reads it in a test.
  const store = read(STORE);
  assert.ok(store.includes('include_str!("../../../../BOARD.md")'), 'board.rs no longer reads the example out of BOARD.md');
  assert.ok(store.includes('fn the_rulesets_example_is_a_board_file()'), 'board.rs lost the test that reads the example in BOARD.md');

  // The page describes the file, and nothing on the other side of it.
  assert.doesNotMatch(page, /https?:\/\//, 'BOARD.md names an address');
});

// "direct API connections are refused as rule for now, use the import / export
// to help enforce the offline / LAN model", and "security.md articulates that
// and anywhere else".
test('no direct API connection to a backend, for now, in one wording wherever it is said', () => {
  const places = {
    [RULESET]: read(RULESET),
    'SECURITY.md': read('SECURITY.md'),
    'AGENTS.md': read('AGENTS.md'),
    'README.md': read('README.md'),
    'apps/desktop/README.md': read('apps/desktop/README.md'),
    "RELEASES.md's 0.8.3 note": note('0.8.3'),
    [`${COMMAND}'s help`]: code(read(COMMAND), COMMAND),
  };
  for (const [place, text] of Object.entries(places)) {
    assert.ok(flat(text).includes(NO_API), `${place} does not state the rule against a direct API connection, in its one wording`);
  }
  assert.ok(NO_API.includes(', for now.'), 'the rule lost "for now"');

  // Why the rule is there, wherever there is room to say it.
  for (const place of [RULESET, 'SECURITY.md', 'AGENTS.md', 'README.md', "RELEASES.md's 0.8.3 note"]) {
    assert.match(flat(places[place]), /offline first, and at most LAN/, `${place} does not say why a board moves as a file`);
  }
  assert.match(flat(places[RULESET]), /A file crosses only when a person moves it/, 'BOARD.md does not say a file crosses only when a person moves it');
  assert.match(flat(places[RULESET]), /loosely coupled/, 'BOARD.md does not say the two sides stay loosely coupled');

  // Every other place points at the ruleset, which has the rule in full.
  for (const place of ['SECURITY.md', 'AGENTS.md', 'README.md', 'apps/desktop/README.md']) {
    assert.match(places[place], /\]\((\.\.\/\.\.\/)?BOARD\.md\)/, `${place} does not link BOARD.md`);
  }
  assert.ok(places[`${COMMAND}'s help`].includes('BOARD.md, at the root of the repository, is the file\'s ruleset.'), 'the help does not say where the ruleset is');
  const security = places['SECURITY.md'];
  const promises = security.slice(security.indexOf('These are the promises that matter most.'));
  assert.ok(flat(promises).includes(NO_API), 'the rule is not among the promises in SECURITY.md');

  // The roadmap promises nothing the rule refuses.
  const roadmap = read('ROADMAP.md');
  const next = roadmap.slice(roadmap.indexOf('\n## Next'), roadmap.indexOf('\n## Later'));
  const source = next.slice(next.indexOf('- **A source for boards.**'));
  assert.ok(next.includes('- **A source for boards.**'), 'the roadmap lost "A source for boards"');
  assert.match(flat(source), /A source that is an address on a network is refused for now/, 'the roadmap does not say a source on a network is refused for now');
  assert.doesNotMatch(flat(next), /reached through the gates/, 'the roadmap promises a board source reached over the network');
});
