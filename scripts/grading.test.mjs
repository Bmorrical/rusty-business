// Grading and a summary, by the engine picked, checked on every test run.
//
// It answers two asks: "The results are graded and summarized by grok after
// all the results are in", and "I should be able to pick which engine does
// the grading".
//
// chat.test.js checks the page's helpers: what the grader reads, how its
// reply is read, and which model grades. These fail when grading starts
// before every answer is in, runs after Stop, sends the models' names or
// anything but the question and the answers, goes anywhere but the app's own
// way of asking a model, keeps a grade that failed or was stopped, loses the
// pick in Settings or its line on what goes to an outside grader's maker,
// keeps something new in the page's storage, or when Rust loses the test that keeps a grade apart from
// the answers.
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
const CHAT = 'apps/desktop/src/chat.js';
const SESSIONS = 'apps/desktop/src-tauri/src/sessions.rs';

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** One function's text in App.vue, up to the next one. */
function functionOf(app, name) {
  const start = app.search(new RegExp(`\\n(?:async )?function ${name}\\(`));
  assert.ok(start >= 0, `${APP} has no ${name}`);
  const end = app.slice(start + 1).search(/\n(?:async )?function |\n\/\*\*|\nonMounted\(/);
  return app.slice(start, end < 0 ? undefined : start + 1 + end);
}

test('Rust keeps the test that keeps a grade apart from the answers', () => {
  assert.match(read(SESSIONS), /#\[test\]\s+fn a_grade_is_kept_and_names_no_model_as_an_answer\(\)/, `${SESSIONS} has lost its grading test`);
});

test('the answers are graded once all are in, and never after Stop', () => {
  const send = functionOf(read(APP), 'send');
  const loop = send.indexOf('for (const [index, entry] of asked.entries()) {');
  const kept = send.indexOf('session.messages = session.messages.filter((turn) => !replies.includes(turn) || turn.content || turn.thinking);');
  const graded = send.indexOf('const error = await grade(session, round, judge);');
  assert.ok(loop >= 0 && kept > loop, 'the answers are no longer asked for and kept as before');
  assert.ok(graded > kept, 'grading starts before every answer is in');
  assert.match(
    send.slice(kept),
    /const judge = grader\.value;\s+if \(!stopAll && judge\) \{\s+const round = groups\(session\.messages\)\.at\(-1\);\s+if \(canGrade\(round\)\) \{\s+const error = await grade\(session, round, judge\);/,
    'the answers are graded after Stop, with no grader picked, or when there is nothing to grade',
  );
  assert.equal(send.match(/await grade\(/g)?.length, 1, 'the answers are graded more than once');
  assert.ok(graded < send.indexOf('busy.value = false;'), 'the page lets a new question in while the grade arrives');
});

test('the grader reads the question and the answers alone, and asks the way every model is asked', () => {
  const grade = functionOf(read(APP), 'grade');
  assert.ok(grade.includes('const answers = gradedAnswers(round).map((turn) => turn.content);'), 'the grader reads more of an answer than its words');
  assert.ok(grade.includes('const messages = gradeMessages(withAttachments(round.question), answers, judge.context);'), 'the grader reads more than the question and the answers');
  assert.ok(grade.includes('const error = await ask(reply, judge, messages);'), 'the grader is asked some other way');
  assert.ok(!grade.includes('invoke('), 'the grader is asked around the app\'s own way of asking a model');
  assert.ok(grade.includes('{ role: GRADE_ROLE, content: "", thinking: "", model: judge.id, pending: true }'), 'the grade is no longer kept as a turn of its own');
  assert.match(grade, /if \(error \|\| reply\.stopped \|\| !reply\.content\.trim\(\)\) \{\s+session\.messages = session\.messages\.filter\(\(turn\) => turn !== reply\);/, 'a grade that failed, was stopped, or never began is kept');
  const round = functionOf(read(APP), 'gradeRound');
  assert.ok(round.includes('if (busy.value || !judge || !canGrade(round)) return;'), 'a round is graded while another answer arrives, twice, or with no grader');

  // No model's name reaches the grader.
  const chat = read(CHAT);
  const messages = chat.slice(chat.indexOf('export function gradeMessages('), chat.indexOf('\n}\n', chat.indexOf('export function gradeMessages(')));
  assert.ok(messages, `${CHAT} has no gradeMessages`);
  assert.ok(!/\bmodel|\bname/.test(messages), "the grader can read the models' names");
});

test("Settings lets the user pick the grader, or none, and says what goes to an outside grader's maker", () => {
  const app = read(APP);
  const start = app.indexOf('<h3 class="gap-above">Grading</h3>');
  assert.ok(start >= 0, 'Settings has lost the pick of the grader');
  const settings = app.slice(start, app.indexOf('</section>', start));
  for (const part of [
    'for="grader"',
    '<select id="grader" v-model="graderChoice" :disabled="busy">',
    '<option value="">No grading</option>',
    'v-for="entry in models"',
    'numbered and without the models\' names',
    'Until you pick one, the first outside engine on hand grades, in the order the Installers page lists them.',
    'The model picked is not on hand, so the answers are not graded.',
    'The model picked is switched off, so the answers are not graded.',
    'Outside engines are on hand only while the gate is on.',
    'v-if="grader && isOutside(grader.id)"',
    "{{ grader.name }} grades on {{ grader.maker }}'s computers, so the question and every answer, the answers",
    'from this computer too, go to {{ grader.maker }} each time it grades. That happens only while the gate is on.',
  ]) {
    assert.ok(settings.includes(part), `Settings' grading has lost ${part}`);
  }
  // A pick is kept between runs, and "No grading" is kept as a pick.
  assert.ok(app.includes('window.localStorage.setItem("grader", id);'), 'the pick is not kept');
  assert.ok(app.includes('const graderPick = ref(rememberedGrader());'), 'the pick kept is not read back');
  assert.ok(app.includes('const grader = computed(() => graderFor(graderPick.value, models.value));'), 'the grader can be a model not on hand');

  // Each answer shows its grade, the summary has its card, and an ungraded
  // round can be graded by hand.
  for (const part of [
    '<p v-if="gradeOf(round, index)" class="grade small">',
    '<section v-if="round.grade" class="grade-card">',
    'Summary, by {{ modelName(round.grade.model) || "a model not on hand" }}',
    '<button type="button" @click="gradeRound(round)">Grade with {{ grader.name }}</button>',
    '{{ gradesLine(grader) }}',
    // The grader numbers the answers; the page adds the names.
    'roundGrades(round, modelName)',
  ]) {
    assert.ok(app.includes(part), `the page has lost ${part}`);
  }

  // The line on who grades stands apart, after the empty chat's other lines,
  // so it never takes their place or shows one that is not true.
  assert.match(
    app,
    /<p v-if="chosen\.length" class="quiet">\{\{ answersLine\(chosen\) \}\}<\/p>\s+<p v-else-if="allOff" class="quiet">[\s\S]*?<\/p>\s+<p v-else-if="starting" class="quiet">Starting the engine\.\.\.<\/p>\s+<p v-else-if="engine\?\.reachable" class="quiet">The engine has none of this app's models on hand\.<\/p>\s+<p v-if="many && grader" class="quiet">\{\{ gradesLine\(grader\) \}\}<\/p>/,
    'the line on who grades breaks the empty chat\'s other lines',
  );
});
