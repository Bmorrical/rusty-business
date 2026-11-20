import assert from "node:assert/strict";
import { test } from "node:test";

import {
  answerRoom,
  answersLine,
  canGrade,
  canRemove,
  chatModels,
  checkedLine,
  chipDetail,
  CUT_MARK,
  dayLabel,
  downloaded,
  downloadLine,
  firstRunOffers,
  modelShelf,
  changeLines,
  parseToolCall,
  findToolCall,
  withAttachments,
  attachmentTokens,
  attachmentLine,
  shownReply,
  toolLabel,
  toolResult,
  WRITE_TOOLS,
  workspacePrompt,
  webPrompt,
  WEB_TOOL,
  FOLDER_TOOLS,
  describesWork,
  withoutToolFences,
  formatCount,
  formatSize,
  formatWindow,
  fullest,
  fullness,
  gateLine,
  GRADE_INSTRUCTIONS,
  GRADE_ROLE,
  gradedAnswers,
  gradeMessages,
  graderFor,
  gradesLine,
  groups,
  installButton,
  installLine,
  isOutside,
  keptPicks,
  latestAnswers,
  listedLine,
  looksLikePath,
  meterLine,
  moreLine,
  nameAnswers,
  namesLine,
  onDiskLine,
  OUTSIDE_PREFIXES,
  outsideModelsLine,
  parseGrades,
  pickedMakers,
  quickest,
  requestMessages,
  roundGrades,
  savedTurn,
  shouldStart,
  showsStartHint,
  STACK_STARTING,
  stackLine,
  showsUpdate,
  UPDATE_EVERY,
  updateDetail,
  updateLine,
  startLine,
  titleFrom,
  tokensPerSecond,
  tokensUsed,
  arrangeSessions,
  greeting,
  memoryRequest,
  parseFacts,
  systemPrompt,
  withMaker,
  isOn,
  modelLine,
  switchedOn,
  uninstallAsk,
  uninstalledLine,
  withPick,
} from "./chat.js";

test("a title is the first line with text, cut to fit", () => {
  assert.equal(titleFrom("\n\n  What   is 2+2?  \nSecond line"), "What is 2+2?");
  assert.equal(titleFrom("   "), "New chat");
  const long = titleFrom("word ".repeat(40));
  assert.equal(Array.from(long).length <= 60, true);
  assert.equal(long.endsWith("..."), true);
});

test("thinking never goes back to the engine", () => {
  const messages = requestMessages([
    { role: "user", content: "What is 2+2?" },
    { role: "assistant", content: "4.", thinking: "Add them.", promptTokens: 12 },
    { role: "user", content: "And 3+3?" },
  ]);
  assert.deepEqual(messages, [
    { role: "user", content: "What is 2+2?" },
    { role: "assistant", content: "4." },
    { role: "user", content: "And 3+3?" },
  ]);
});

test("a reply stopped before it began does not break the turn order", () => {
  const messages = requestMessages([
    { role: "user", content: "First" },
    { role: "assistant", content: "", stopped: true },
    { role: "user", content: "Second" },
  ]);
  assert.deepEqual(messages, [{ role: "user", content: "First\n\nSecond" }]);
});

test("a turn with a role the engine does not take is left out", () => {
  assert.deepEqual(requestMessages([{ role: "tool", content: "x" }]), []);
  assert.deepEqual(requestMessages(undefined), []);
});

test("tokens used come from the latest measured reply", () => {
  const turns = [
    { role: "user", content: "a" },
    { role: "assistant", content: "b", promptTokens: 10, completionTokens: 5 },
    { role: "user", content: "c" },
    { role: "assistant", content: "d", promptTokens: 30, completionTokens: 7 },
    { role: "user", content: "e" },
  ];
  assert.equal(tokensUsed(turns), 37);
  assert.equal(tokensUsed([{ role: "user", content: "a" }]), null);
});

test("fullness stays between nothing and full", () => {
  assert.equal(fullness(null, 16384), 0);
  assert.equal(fullness(8192, 16384), 0.5);
  assert.equal(fullness(99999, 16384), 1);
});

test("numbers read the way a person writes them", () => {
  assert.equal(formatCount(16384), "16,384");
  assert.equal(formatWindow(16384), "16K");
  assert.equal(formatWindow(262144), "256K");
  // Outside engines count their windows in round thousands. Read in 1,024s,
  // Claude's 1,000,000 showed as 977K and Grok's 256,000 as 250K.
  assert.equal(formatWindow(1_000_000), "1M");
  assert.equal(formatWindow(200_000), "200K");
  assert.equal(formatWindow(256_000), "256K");
  assert.equal(formatWindow(400_000), "400K");
  assert.equal(formatWindow(1_048_576), "1M");
  assert.equal(formatWindow(0), "0K");
});

test("speed needs both a count and a time", () => {
  assert.equal(tokensPerSecond({ completionTokens: 100, seconds: 2 }), 50);
  assert.equal(tokensPerSecond({ completionTokens: 100, seconds: 0 }), null);
  assert.equal(tokensPerSecond({ seconds: 2 }), null);
});

test("days are named from the calendar, not from hours", () => {
  const now = new Date(2026, 10, 16, 0, 30).getTime();
  assert.equal(dayLabel(new Date(2026, 10, 16, 0, 5).getTime(), now), "Today");
  assert.equal(dayLabel(new Date(2026, 10, 15, 23, 55).getTime(), now), "Yesterday");
  assert.equal(dayLabel(new Date(2026, 10, 2, 12, 0).getTime(), now), "Nov 2");
  assert.equal(dayLabel(0, now), "");
});

test("a saved turn drops window-only and empty fields", () => {
  const saved = savedTurn({
    role: "assistant",
    content: "4.",
    thinking: "",
    model: "phi",
    promptTokens: 12,
    completionTokens: 3,
    seconds: 0.4,
    stopped: false,
    cutShort: false,
    pending: true,
  });
  assert.deepEqual(saved, {
    role: "assistant",
    content: "4.",
    model: "phi",
    promptTokens: 12,
    completionTokens: 3,
    seconds: 0.4,
  });
});

test("a reply that ran into the length limit is saved that way", () => {
  const saved = savedTurn({ role: "assistant", content: "One, two", cutShort: true, stopped: false });
  assert.deepEqual(saved, { role: "assistant", content: "One, two", cutShort: true });
});

// Brad, on what the app does by itself: "it should search and up the local
// models and stack." With the engine off, the app says which of its models it
// found on the disk.
test("the models on this computer are named in one sentence", () => {
  const phi = { id: "mlx-community/phi-4-4bit", name: "Phi-4" };
  const gptOss = { id: "mlx-community/gpt-oss-20b-MXFP4-Q8", name: "gpt-oss 20B" };
  const nemotron = { id: "mlx-community/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-4bit", name: "Nemotron 3.5 Lightning" };

  assert.equal(onDiskLine([phi]), "On this computer: Phi-4.");
  assert.equal(onDiskLine([phi, gptOss]), "On this computer: Phi-4 and gpt-oss 20B.");
  assert.equal(
    onDiskLine([phi, gptOss, nemotron]),
    "On this computer: Phi-4, gpt-oss 20B, and Nemotron 3.5 Lightning.",
  );
});

test("a model kept in two sizes is named once", () => {
  const models = [
    { id: "mlx-community/phi-4-4bit", name: "Phi-4" },
    { id: "mlx-community/phi-4-8bit", name: "Phi-4" },
    { id: "mlx-community/gpt-oss-20b-MXFP4-Q8", name: "gpt-oss 20B" },
  ];
  assert.equal(onDiskLine(models), "On this computer: Phi-4 and gpt-oss 20B.");
});

test("with none of its models on the disk, the app says so", () => {
  const none = "None of this app's models is on this computer yet.";
  assert.equal(onDiskLine([]), none);
  // An older report has no list at all.
  assert.equal(onDiskLine(undefined), none);
  assert.equal(onDiskLine(null), none);
});

// Brad, on what the app does by itself: "it should search and up the local
// models and stack." and "install and go, one click ready". The page asks for
// the engine to be started only when no engine answers.
test("the engine is asked for only when none answers", () => {
  assert.equal(shouldStart({ reachable: false, error: "The engine is not answering." }), true);
  // One that answers is left alone.
  assert.equal(shouldStart({ reachable: true }), false);
  // Before the first report there is nothing to go on.
  assert.equal(shouldStart(null), false);
  assert.equal(shouldStart(undefined), false);
  // A report that does not say is not a reason to start anything.
  assert.equal(shouldStart({}), false);
});

test("when the app cannot start the engine, it says why, once", () => {
  const off = { reachable: false, error: "The engine is not answering." };
  const notFound = "The app looked for mlx_lm.server on this computer and did not find it.";

  assert.equal(startLine({ answering: false, started: false, reason: notFound }, off), notFound);
  assert.equal(startLine({ answering: false, started: true, reason: ` ${notFound}\n` }, off), notFound);

  // An engine that answers leaves nothing to explain.
  assert.equal(startLine({ answering: true, started: true, reason: null }, { reachable: true }), "");
  assert.equal(startLine({ answering: true, started: false, reason: notFound }, off), "");
  assert.equal(startLine({ answering: false, started: false, reason: notFound }, { reachable: true }), "");

  // No reason, or no report at all.
  assert.equal(startLine({ answering: false, started: false, reason: null }, off), "");
  assert.equal(startLine({ answering: false, started: false }, off), "");
  assert.equal(startLine(undefined, off), "");

  // The same sentence is not said twice, one line under the other.
  const badAddress = "The engine address must be on this computer (for example http://127.0.0.1:8080/v1).";
  assert.equal(startLine({ answering: false, started: false, reason: badAddress }, { reachable: false, error: badAddress }), "");
});

// Brad, on the first run: "I should be able to just pull the .dmg or .exe from
// read me and install and go, one click ready." He picked "Picker with sizes".
// On a computer with none of the app's models, the page lists the ones it can
// download, each with its size, and points to the quickest.
const OFFERS = [
  { id: "mlx-community/phi-4-4bit", name: "Phi-4", maker: "Microsoft", license: "MIT", bytes: 8256384748 },
  { id: "mlx-community/gpt-oss-20b-MXFP4-Q8", name: "gpt-oss 20B", maker: "OpenAI", license: "Apache 2.0", bytes: 12104215835 },
  {
    id: "mlx-community/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-4bit",
    name: "Nemotron 3.5 Lightning",
    maker: "NVIDIA",
    license: "OpenMDW 1.1",
    bytes: 17792581692,
  },
];

test("the picker shows only on a first run, with none of the app's models anywhere", () => {
  assert.deepEqual(firstRunOffers({ reachable: false, onDisk: [], models: [], offers: OFFERS }), OFFERS);
  // One model on the disk is enough to start from, so nothing is offered.
  assert.deepEqual(firstRunOffers({ reachable: false, onDisk: [OFFERS[0]], models: [], offers: OFFERS }), []);
  // An engine that already serves a model needs no download either.
  assert.deepEqual(firstRunOffers({ reachable: true, onDisk: [], models: [{ id: "x" }], offers: OFFERS }), []);
  // Before the first report, or from an older one, there is nothing to offer.
  assert.deepEqual(firstRunOffers(null), []);
  assert.deepEqual(firstRunOffers({ reachable: false, onDisk: [] }), []);
});

test("the Models settings list every download, marking the ones already here", () => {
  const shelf = modelShelf({ onDisk: [OFFERS[0]], models: [{ id: OFFERS[1].id }], offers: OFFERS });
  assert.deepEqual(shelf.map((offer) => offer.here), [true, true, false]);
  assert.equal(shelf[2].name, "Nemotron 3.5 Lightning");
  assert.deepEqual(modelShelf(null), []);
});

test("the quickest to start is the smallest download", () => {
  assert.equal(quickest(OFFERS), "mlx-community/phi-4-4bit");
  assert.equal(quickest([OFFERS[2], OFFERS[1]]), "mlx-community/gpt-oss-20b-MXFP4-Q8");
  assert.equal(quickest([]), "");
  assert.equal(quickest(undefined), "");
});

test("sizes read in gigabytes, the way the app's sentences give them", () => {
  assert.deepEqual(OFFERS.map((offer) => formatSize(offer.bytes)), ["8.3 GB", "12.1 GB", "17.8 GB"]);
  assert.equal(formatSize(0), "0.0 GB");
  assert.equal(formatSize(undefined), "0.0 GB");
});

test("a download's progress stays between nothing and done", () => {
  assert.equal(downloaded({ done: 0, total: 8256384748 }), 0);
  assert.equal(downloaded({ done: 4128192374, total: 8256384748 }), 0.5);
  assert.equal(downloaded({ done: 9e9, total: 8256384748 }), 1);
  assert.equal(downloaded({ done: 5, total: 0 }), 0);
  assert.equal(downloaded(null), 0);

  assert.equal(downloadLine({ done: 3.1e9, total: 8256384748 }), "3.1 of 8.3 GB");
  assert.equal(downloadLine({ done: 9e9, total: 8256384748 }), "8.3 of 8.3 GB");
  assert.equal(downloadLine(null), "0.0 of 0.0 GB");
});

// Brad: "it should search and up the local models and stack." The footer says
// the stack is starting, then where it answers, or why it is not running in
// the app's own words, so a person knows whether to select Try again.
test("the stack's line says it is starting, where it answers, or why it is not running", () => {
  const running = { running: true, started: true, address: "127.0.0.1:4700", reason: null };
  assert.equal(stackLine(running, false), "Stack on 127.0.0.1:4700");
  assert.equal(stackLine({ ...running, started: false }, false), "Stack on 127.0.0.1:4700");

  // While the app starts it, that is all the line says, even over an older report.
  assert.equal(stackLine(null, true), STACK_STARTING);
  assert.equal(stackLine({ running: false, started: false, address: "127.0.0.1:4700", reason: "Docker is not running." }, true), STACK_STARTING);
  assert.match(STACK_STARTING, /first start downloads it/);

  const noDocker = "Docker is not on this computer, so the stack is not running; the chat does not need it yet.";
  assert.equal(stackLine({ running: false, started: false, address: "127.0.0.1:4700", reason: `  ${noDocker}\n` }, false), noDocker);
  assert.equal(stackLine({ running: false, started: false, address: "127.0.0.1:4700", reason: null }, false), "The stack is not running.");

  // Before the app has asked, there is nothing to say.
  assert.equal(stackLine(null, false), "");
  assert.equal(stackLine(undefined, false), "");
});

test("the footer shows a command to start the engine only where there is one", () => {
  // Brad, 2026-11-16: "I should be able to just pull the .dmg or .exe from
  // read me and install and go, one click ready." The roadmap, of the .exe:
  // "Until Windows has an engine, the window opens and says so." Then, on
  // 2026-11-19: "add an engine to linux and windows distro's." There the app
  // starts llama-server itself, with its own models folder, so no one command
  // fits: Windows and Linux get none, and the footer shows none.
  const mac = { startHint: "mlx_lm.server --port 8080" };
  assert.equal(showsStartHint(mac, false), true);
  assert.equal(showsStartHint(mac, true), false, "not while the app starts the engine");
  assert.equal(showsStartHint({ startHint: "" }, false), false, "Windows");
  assert.equal(showsStartHint({ startHint: "  " }, false), false);
  assert.equal(showsStartHint({}, false), false);
  assert.equal(showsStartHint(null, false), false);
});

// "when a new build is ready, the user is notified in app, and it gives them
// the download"
const update = {
  version: "0.5.0",
  current: "0.4.0",
  file: "Rusty-Business-mac-arm64.dmg",
  steps: "When it is downloaded, quit Rusty Business™, open the .dmg, and drag the app onto Applications to replace this one.",
  command: null,
  page: "github.com/Bmorrical/rusty-business/releases/tag/v0.5.0",
};

test("a new version is shown until Later is chosen for it", () => {
  assert.equal(showsUpdate(update, ""), true);
  assert.equal(showsUpdate(update, "0.5.0"), false);
  // Later for one version does not hide the next.
  assert.equal(showsUpdate({ ...update, version: "0.6.0" }, "0.5.0"), true);
  assert.equal(showsUpdate(null, ""), false);
  assert.equal(showsUpdate(undefined, ""), false);
  assert.equal(showsUpdate({}, ""), false);
});

test("the notice says which version is ready and what Download opens", () => {
  assert.equal(updateLine(update), "Rusty Business™ 0.5.0 is ready.");
  assert.equal(updateDetail(update), "This is 0.4.0. Download opens Rusty-Business-mac-arm64.dmg in your browser.");
  // A copy that does not install new versions itself keeps Download alone.
  assert.equal(
    updateDetail({ ...update, install: false }),
    "This is 0.4.0. Download opens Rusty-Business-mac-arm64.dmg in your browser.",
  );
});

// "when can we do an in app install rather than a new dmg download? is that
// after the signing? if we can do it now, then roll that next", then "Click to
// install": the notice shows Install and restart, nothing downloads until it
// is clicked, and Download stays beside it.
test("where this copy installs new versions, the notice names Install and restart first", () => {
  const installs = { ...update, install: true };
  assert.equal(
    updateDetail(installs),
    "This is 0.4.0. Install and restart puts 0.5.0 in place of this copy and opens it. Download opens Rusty-Business-mac-arm64.dmg in your browser.",
  );
  assert.equal(installButton(false), "Install and restart");
  assert.equal(installButton(true), "Installing...");
});

// "is the user able to click a refresh button? If not, I want that.", then
// "check for updates I imply"
test("Check for updates says what it found", () => {
  assert.equal(checkedLine(update, "0.4.0"), "Rusty Business™ 0.5.0 is ready.");
  assert.equal(checkedLine(null, "0.4.0"), "Rusty Business™ 0.4.0 is the newest version.");
  assert.equal(checkedLine({}, "0.4.0"), "Rusty Business™ 0.4.0 is the newest version.");
  assert.equal(checkedLine(null, ""), "This is the newest version.");
});

test("the page asks once a day while it stays open", () => {
  assert.equal(UPDATE_EVERY, 86_400_000);
});

test("the system message comes first, and is left out when empty", () => {
  const turns = [{ role: "user", content: "Hi" }];
  assert.deepEqual(requestMessages(turns, null, "Be brief."), [
    { role: "system", content: "Be brief." },
    { role: "user", content: "Hi" },
  ]);
  assert.deepEqual(requestMessages(turns, null, "  "), [{ role: "user", content: "Hi" }]);
});

test("the system prompt carries memory and the project, and nothing when there is none", () => {
  assert.equal(systemPrompt("", null), "");
  const global = systemPrompt("Lives in Jacksonville.", null);
  assert.match(global, /What you remember about the user:\n<memory>\nLives in Jacksonville\.\n<\/memory>/);
  assert.doesNotMatch(global, /project/);
  const project = systemPrompt("", { name: "Gledit", instructions: "Be terse.", memory: "- For small businesses" });
  assert.match(project, /"Gledit" project/);
  assert.match(project, /<project_instructions>\nBe terse\.\n<\/project_instructions>/);
  assert.match(project, /What you remember about this project:\n<memory>\n- For small businesses/);
  assert.doesNotMatch(project, /about the user/);
});

test("the memory request names the scopes and what is already known", () => {
  const [system, user] = memoryRequest("I moved to Denver", "Likes Rust", { name: "Odd Stereo", memory: "A podcast" });
  assert.equal(user.content, "I moved to Denver");
  assert.match(system.content, /"Odd Stereo" project/);
  assert.match(system.content, /Likes Rust\nA podcast/);
  assert.match(memoryRequest("x", "", null)[0].content, /Use "user" for every fact/);
});

test("facts are read from JSON, even wrapped in words, and nothing from anything else", () => {
  const answer = 'Sure: {"memories":[{"content":" Lives in Denver ","scope":"user"},{"content":"Ships Friday","scope":"project"},{"content":""}]}';
  assert.deepEqual(parseFacts(answer, true), { user: ["Lives in Denver"], project: ["Ships Friday"] });
  assert.deepEqual(parseFacts(answer, false), { user: ["Lives in Denver", "Ships Friday"], project: [] });
  for (const bad of ["", "no facts", "{not json}", '{"memories":"x"}', null]) {
    assert.deepEqual(parseFacts(bad, true), { user: [], project: [] });
  }
});

test("the side lists projects' chats, then pinned, then the rest by age", () => {
  const now = new Date(2026, 10, 18, 20, 0).getTime();
  const day = 86_400_000;
  const sessions = [
    { id: "a", updatedAt: now - 1000, project: "p1" },
    { id: "b", updatedAt: now - 2000, pinned: true },
    { id: "c", updatedAt: now - 3000 },
    { id: "d", updatedAt: now - 3 * day },
    { id: "e", updatedAt: now - 20 * day, project: "gone" },
    { id: "f", updatedAt: now - 90 * day },
  ];
  const side = arrangeSessions(sessions, [{ id: "p1" }, { id: "p2" }], now);
  assert.deepEqual(side.byProject.p1.map((entry) => entry.id), ["a"]);
  assert.deepEqual(side.byProject.p2, []);
  assert.deepEqual(side.pinned.map((entry) => entry.id), ["b"]);
  assert.deepEqual(
    side.ages.map((age) => [age.label, age.items.map((entry) => entry.id)]),
    [["Today", ["c"]], ["Previous 7 days", ["d"]], ["Previous 30 days", ["e"]], ["Older", ["f"]]],
  );
});

test("the greeting follows the hour", () => {
  assert.equal(greeting(8), "Good morning");
  assert.equal(greeting(13), "Good afternoon");
  assert.equal(greeting(20), "Good evening");
  assert.equal(greeting(2), "Good evening");
});

test("a reply's tool block is read, and shown without the block", () => {
  const reply = 'Let me look.\n\n```tool\n{"tool": "read", "path": "src/app.js", "from": 2}\n```';
  assert.deepEqual(parseToolCall(reply), { tool: "read", args: { path: "src/app.js", from: 2 }, before: "Let me look." });
  assert.equal(shownReply(reply), "Let me look.");
  assert.equal(shownReply("Half way ```to"), "Half way ```to");
  // An example of the format, with words after it, is not a call to hide.
  const example = 'Like this:\n```tool\n{"tool": "list", "path": "."}\n```\nThen wait.';
  assert.equal(shownReply(example), example);
  assert.equal(shownReply('Looking.\n```tool\n{"tool": "li'), "Looking.");
  // A fence inside the JSON is not the block's end.
  const readme = '```tool\n{"tool": "write", "path": "README.md", "content": "Run:\\n```\\nmake\\n```"}\n```';
  assert.deepEqual(parseToolCall(readme).args, { path: "README.md", content: "Run:\n```\nmake\n```" });
  assert.equal(parseToolCall("No tools here."), null);
  assert.match(parseToolCall("```tool\n{nope}\n```").error, /not valid JSON/);
  assert.match(parseToolCall('```tool\n{"tool": "delete"}\n```').error, /no tool called delete/);
  assert.equal(toolLabel({ tool: "read", args: { path: "a.js", from: 2, to: 9 } }), "Read a.js:2-9");
  assert.equal(toolLabel({ tool: "search", args: { text: "x" } }), 'Searched for "x"');
  assert.equal(toolResult({ tool: "list" }, "src/\n"), '<tool_result tool="list">\nsrc/\n</tool_result>');
  assert.ok(WRITE_TOOLS.includes("edit") && !WRITE_TOOLS.includes("read"));
  assert.match(workspacePrompt({ folder: "/Users/me/code/site" }, "src/"), /"site"[\s\S]*<folder>\nsrc\/\n<\/folder>/);
});

test("a model can search the web with one tool", () => {
  const call = parseToolCall('Let me look.\n```tool\n{"tool": "web_search", "query": "plumber Jacksonville FL"}\n```');
  assert.deepEqual(call, { tool: WEB_TOOL, args: { query: "plumber Jacksonville FL" }, before: "Let me look." });
  assert.equal(findToolCall("", '{"query":"weather Jacksonville"}').tool, WEB_TOOL);
  assert.equal(toolLabel({ tool: WEB_TOOL, args: { query: "plumbers" } }), 'Searched the web for "plumbers"');
  assert.ok(!FOLDER_TOOLS.includes(WEB_TOOL) && !WRITE_TOOLS.includes(WEB_TOOL));
  // Alone, the prompt says how to call a tool; after a folder's, it does not repeat it.
  assert.match(webPrompt("Friday, November 20, 2026"), /```tool[\s\S]*Today is Friday, November 20, 2026\./);
  assert.doesNotMatch(webPrompt("today", true), /end your message with exactly one tool block/);
  assert.match(webPrompt("today", true), /"web_search"/);
});

test("a change reads as lines gone and lines new", () => {
  assert.deepEqual(changeLines({ tool: "edit", args: { find: "a\nb", replace: "c\n" } }), [
    { kind: "gone", text: "a" },
    { kind: "gone", text: "b" },
    { kind: "new", text: "c" },
  ]);
  assert.deepEqual(changeLines({ tool: "write", args: { content: "x" } }), [{ kind: "new", text: "x" }]);
  assert.deepEqual(changeLines({ tool: "read", args: {} }), []);
});

test("a call left in the thinking, as gpt-oss does, is still found", () => {
  const thinking = 'Let\'s list src folder.\n\n{"path":"src"}';
  assert.deepEqual(findToolCall("", thinking), {
    tool: "list",
    args: { path: "src" },
    before: "",
    block: '```tool\n{"tool":"list","path":"src"}\n```',
  });
  assert.equal(findToolCall("", '{"path":"src/app.js"}').tool, "read");
  assert.equal(findToolCall("", '{"text":"main("}').tool, "search");
  assert.equal(findToolCall("", '{"path":"a.js","find":"x","replace":"y"}').tool, "edit");
  // A real answer wins, and thinking with no call finds nothing.
  assert.equal(findToolCall("The entry point is main.js.", '{"path":"src"}'), null);
  assert.equal(findToolCall("", "Just thinking."), null);
  assert.equal(findToolCall('Look.\n```tool\n{"tool":"list","path":"."}\n```', "").block, "");
});

test("attached files go to the model before the message, and say when they were cut", () => {
  const turn = {
    role: "user",
    content: "Sum the totals.",
    attachments: [{ name: "sales.csv", kind: "CSV", text: "west,10", chars: 7, cut: false }],
  };
  assert.equal(withAttachments(turn), '<attachment name="sales.csv">\nwest,10\n</attachment>\n\nSum the totals.');
  assert.match(withAttachments({ content: "", attachments: [{ name: "a", text: "x", chars: 9000, cut: true }] }), /first 1 characters of 9,000\.\)\n<\/attachment>\n\nThe user attached the files above\.$/);
  assert.equal(requestMessages([turn])[0].content, withAttachments(turn));
  assert.equal(withAttachments({ content: " hi " }), "hi");
  assert.equal(attachmentTokens([{ text: "1234567" }]), 2);
  assert.equal(attachmentLine({ kind: "PDF", chars: 12400, cut: true }), "PDF · 12K characters, cut short");
  assert.equal(savedTurn(turn).attachments.length, 1);
});

// Brad, 2026-11-17: "there is a hard switch / circuit breaker to hard stop all
// outbound communication, user can turn it on to use their inference." Then:
// "off by default, everything stops".
test("the gate's line says what off and on mean", () => {
  assert.equal(gateLine(false), "Gate off. Nothing leaves this computer.");
  assert.equal(
    gateLine(true),
    "Gate on. The app may go online to check for updates, download models, start the stack, and ask the outside engines you add.",
  );
  // Anything but a plain yes reads as off.
  for (const open of [undefined, null, 0, ""]) assert.equal(gateLine(open), gateLine(false));
});

// Brad, 2026-11-17: "user can pick google, gemini, meta from the app if they
// want those instead. I think the user can pick one or more at install with
// microsoft default, and in settings they can one click more of them if they
// want."
const MAKERS = [
  { name: "Microsoft", picked: true, models: [] },
  { name: "OpenAI", picked: false, models: [] },
  { name: "NVIDIA", picked: false, models: [] },
  { name: "Google", picked: true, models: [] },
  { name: "Meta", picked: false, models: [] },
];

test("one click adds or removes a maker, and the list keeps its order", () => {
  assert.deepEqual(pickedMakers(MAKERS), ["Microsoft", "Google"]);
  assert.deepEqual(withMaker(MAKERS, "Meta", true), ["Microsoft", "Google", "Meta"]);
  assert.deepEqual(withMaker(MAKERS, "OpenAI", true), ["Microsoft", "OpenAI", "Google"]);
  assert.deepEqual(withMaker(MAKERS, "Microsoft", false), ["Google"]);
  // Adding one already picked, or removing one that is not, changes nothing.
  assert.deepEqual(withMaker(MAKERS, "Google", true), ["Microsoft", "Google"]);
  assert.deepEqual(withMaker(MAKERS, "Meta", false), ["Microsoft", "Google"]);
  assert.deepEqual(pickedMakers(null), []);
});

test("the last maker picked stays picked", () => {
  const one = MAKERS.map((maker) => ({ ...maker, picked: maker.name === "Microsoft" }));
  assert.deepEqual(withMaker(one, "Microsoft", false), ["Microsoft"]);
  assert.equal(canRemove(one, "Microsoft"), false);
  assert.equal(canRemove(MAKERS, "Microsoft"), true);
  assert.equal(canRemove(MAKERS, "Google"), true);
  // A maker not picked has nothing to remove.
  assert.equal(canRemove(MAKERS, "Meta"), false);
});

test("each model in Settings gives its license, and its size or that it is here", () => {
  assert.equal(listedLine({ name: "Phi-4", license: "MIT", onDisk: true, offer: OFFERS[0] }), "MIT · on this computer");
  assert.equal(listedLine({ name: "gpt-oss 20B", license: "Apache 2.0", onDisk: false, offer: OFFERS[1] }), "Apache 2.0 · 12.1 GB");
  assert.equal(
    listedLine({ name: "Llama 3.1 8B", license: "Llama 3.1 Community License", onDisk: false, offer: null }),
    "Llama 3.1 Community License · no download for this computer",
  );
});

test("each model on the Installers page says the same, and when it waits for its maker", () => {
  // Brad, 2026-11-17: "one click from an installers page would be good".
  const google = { name: "Google", picked: false };
  const gemma = { name: "Gemma 4 26B", license: "Apache 2.0", onDisk: true, offer: OFFERS[1] };
  // On this computer, with its maker not added: Show adds the maker.
  assert.equal(installLine(google, gemma), "Apache 2.0 · on this computer, shown once Google is added");
  // With its maker added, it reads as in Settings.
  assert.equal(installLine({ ...google, picked: true }, gemma), listedLine(gemma));
  // Not here yet, it gives its size, whether its maker is added or not.
  const away = { ...gemma, onDisk: false };
  assert.equal(installLine(google, away), "Apache 2.0 · 12.1 GB");
  assert.equal(installLine({ ...google, picked: true }, away), "Apache 2.0 · 12.1 GB");
  // And one with no download says so.
  assert.equal(installLine(google, { ...away, offer: null }), "Apache 2.0 · no download for this computer");
});

const PHI = "mlx-community/phi-4-4bit";
const GPT = "mlx-community/gpt-oss-20b-MXFP4-Q8";
const NEMO = "mlx-community/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-4bit";
const ASKED = [
  { role: "user", content: "Why do nails rust?" },
  { role: "assistant", content: "Oxygen and water.", model: PHI, promptTokens: 17, completionTokens: 20 },
  { role: "assistant", content: "Iron oxidizes.", model: GPT, promptTokens: 80, completionTokens: 300 },
  { role: "user", content: "How do I stop it?" },
  { role: "assistant", content: "Paint them.", model: GPT, promptTokens: 400, completionTokens: 40 },
  { role: "assistant", content: "", model: NEMO },
];

// Brad, 2026-11-17: "the next step is user can run the same question in
// 1-nth models and checks the results of each one".
test("one question, many models: each answer sits with the question it answers", () => {
  const rounds = groups(ASKED);
  assert.equal(rounds.length, 2);
  assert.equal(rounds[0].question, ASKED[0]);
  assert.deepEqual(rounds[0].answers, [ASKED[1], ASKED[2]]);
  assert.deepEqual(rounds[1].answers, [ASKED[4], ASKED[5]]);
  // The chat's own turns, so a reply still arriving shows as it grows.
  assert.equal(rounds[1].answers[1], ASKED[5]);
  assert.deepEqual(groups([{ role: "assistant", content: "Hi." }])[0].question, null);
  assert.deepEqual(groups(undefined), []);
});

test("each model reads one answer to each question, its own when it gave one", () => {
  assert.deepEqual(requestMessages(ASKED, PHI), [
    { role: "user", content: "Why do nails rust?" },
    { role: "assistant", content: "Oxygen and water." },
    { role: "user", content: "How do I stop it?" },
    { role: "assistant", content: "Paint them." },
  ]);
  assert.deepEqual(requestMessages(ASKED.slice(0, 4), GPT).at(1), { role: "assistant", content: "Iron oxidizes." });
  // Not asked the first time, a model reads the first answer; an empty one is skipped.
  assert.deepEqual(requestMessages(ASKED.slice(0, 4), NEMO).at(1), { role: "assistant", content: "Oxygen and water." });
  assert.deepEqual(requestMessages(ASKED, NEMO).at(-1), { role: "assistant", content: "Paint them." });
  // The question being asked goes to each model with no other model's answer to it.
  assert.deepEqual(requestMessages(ASKED.slice(0, 4), GPT).at(-1), { role: "user", content: "How do I stop it?" });
});

test("a model is picked or let go with one click, and one always stays", () => {
  const ids = [PHI, GPT, NEMO];
  assert.deepEqual(withPick([PHI], NEMO, true, ids), [PHI, NEMO]);
  assert.deepEqual(withPick([NEMO, PHI], GPT, true, ids), [PHI, GPT, NEMO], "in the order of the models on hand");
  assert.deepEqual(withPick([PHI, GPT], PHI, false, ids), [GPT]);
  assert.deepEqual(withPick([PHI], PHI, false, ids), [PHI], "the last one picked stays");
  assert.deepEqual(withPick(["gone"], PHI, true, ids), [PHI], "a model no longer on hand drops out");
});

test("a chat opens with the models that answered its latest question", () => {
  assert.deepEqual(latestAnswers(ASKED), [GPT, NEMO]);
  assert.deepEqual(latestAnswers(ASKED.slice(0, 3)), [PHI, GPT]);
  assert.deepEqual(latestAnswers(ASKED.slice(0, 4)), [], "a question with no answer yet names none");
  assert.deepEqual(latestAnswers(undefined), []);
  // A model that answered twice, as after a retry, is named once.
  const twice = [...ASKED, { role: "assistant", content: "Again.", model: GPT }];
  assert.deepEqual(latestAnswers(twice), [GPT, NEMO]);
  assert.deepEqual(chatModels({ model: PHI, messages: ASKED }), [GPT, NEMO]);
  assert.deepEqual(chatModels({ model: PHI, messages: [{ role: "user", content: "Hi." }] }), [PHI]);
  assert.deepEqual(chatModels(null), []);
  // The first list that names a model on hand wins, kept to those on hand.
  assert.deepEqual(keptPicks([PHI, GPT], [NEMO], [GPT, "gone", PHI], [PHI]), [PHI, GPT]);
  assert.deepEqual(keptPicks([PHI], [], undefined), []);
});

test("the meter shows the fullest window of the models asked", () => {
  const phi = { id: PHI, name: "Phi-4", context: 16_384 };
  const gpt = { id: GPT, name: "gpt-oss 20B", context: 131_072 };
  const nemo = { id: NEMO, name: "Nemotron 3.5 Lightning", context: 262_144 };
  assert.equal(tokensUsed(ASKED, PHI), 37);
  assert.equal(tokensUsed(ASKED, GPT), 440);
  assert.equal(tokensUsed(ASKED, NEMO), null);
  // 440 of 131,072 is fuller than 37 of 16,384.
  const top = fullest(ASKED, [phi, gpt, nemo]);
  assert.equal(top.model, gpt);
  assert.equal(top.used, 440);
  assert.equal(fullest(ASKED, [phi]).used, 37);
  // A model that has not answered is counted by the chat's latest reply.
  assert.equal(fullest(ASKED, [nemo]).used, 440);
  assert.equal(fullest([], [phi]), null);
  assert.equal(meterLine(top, false), "440 of 131,072 tokens");
  assert.equal(meterLine(top, true), "Fullest window, gpt-oss 20B: 440 of 131,072 tokens");
});

test("the models asked are named in one line, with the Oxford comma", () => {
  assert.equal(namesLine(["Phi-4"]), "Phi-4");
  assert.equal(namesLine(["Phi-4", "gpt-oss 20B"]), "Phi-4 and gpt-oss 20B");
  assert.equal(namesLine(["Phi-4", "gpt-oss 20B", "Nemotron 3.5 Lightning"]), "Phi-4, gpt-oss 20B, and Nemotron 3.5 Lightning");
  assert.equal(namesLine([]), "");
  assert.equal(moreLine(["Phi-4", "gpt-oss 20B", "Nemotron 3.5 Lightning"]), "Phi-4 and 2 more");
  assert.equal(moreLine(["Phi-4", ""]), "Phi-4");
  assert.equal(moreLine([]), "");
  const phi = { name: "Phi-4", maker: "Microsoft", context: 16_384 };
  assert.equal(answersLine([phi]), "Phi-4 by Microsoft answers, with room for 16,384 tokens.");
  // Brad, 2026-11-17: "if they can't run asychronos then the models can
  // iterate on the question".
  assert.equal(answersLine([phi, { name: "gpt-oss 20B" }]), "Phi-4 and gpt-oss 20B answer in turn, side by side.");
  assert.equal(answersLine([]), "");
});

// Brad, 2026-11-17: "user can add their outside inference, start with Grok and
// Grok ai. user adds it manually, they will need to provide or specify a path
// to a key, now, the Rusty Business™ can query Grok for the same question. ...
// you can use grok by it's self too."
const GROK = { id: "xai:grok-4", name: "Grok 4", maker: "xAI", context: 262_144 };
const GROK_UNKNOWN = { id: "xai:grok-mini", name: "Grok Mini", maker: "xAI", context: 0 };
// Brad, 2026-11-17: "and claude and codex/gpt and gemmini". OpenAI's list
// gives no windows, so a GPT model's is never known.
const OPENAI_GPT = { id: "openai:gpt-5", name: "GPT-5", maker: "OpenAI", context: 0 };
const CLAUDE = { id: "anthropic:claude-opus-5-5", name: "Claude Opus 5.5", maker: "Anthropic", context: 1_000_000 };
const GEMINI = { id: "google:gemini-3.5-flash", name: "Gemini 3.5 Flash", maker: "Google", context: 1_048_576 };

test("the outside engines' models are told apart from the models on this computer", () => {
  assert.deepEqual(OUTSIDE_PREFIXES, ["xai:", "openai:", "anthropic:", "google:"]);
  assert.ok(isOutside(GROK.id));
  assert.ok(isOutside(OPENAI_GPT.id));
  assert.ok(isOutside(CLAUDE.id));
  assert.ok(isOutside(GEMINI.id));
  for (const id of [
    "microsoft/phi-4",
    "",
    undefined,
    null,
    "grok-4",
    "gpt-5",
    "claude-opus-5-5",
    "gemini-3.5-flash",
    "google/gemma-4-26b",
    "local/xai:grok",
    "local/openai:gpt-5",
    "local/anthropic:claude",
    "local/google:gemini",
    "openai",
    "anthropic",
    "google",
    "OPENAI:gpt-5",
    "Anthropic:claude-opus-5-5",
    "Google:gemini-3.5-flash",
  ]) {
    assert.equal(isOutside(id), false, `${id} read as an outside engine's`);
  }
});

test("a chip says when a model is asked online", () => {
  assert.equal(chipDetail({ id: "microsoft/phi-4", maker: "Microsoft", context: 16_384 }), "Microsoft · 16K");
  assert.equal(chipDetail(GROK), "xAI · online · 256K");
  // A window xAI did not give is left out, not shown as 0K.
  assert.equal(chipDetail(GROK_UNKNOWN), "xAI · online");
  assert.equal(chipDetail(OPENAI_GPT), "OpenAI · online");
  // Google counts Gemini's window in powers of two, 1,048,576, as 1M.
  assert.equal(chipDetail(GEMINI), "Google · online · 1M");
});

test("an outside engine can answer alone, even when its window is not known", () => {
  assert.equal(answersLine([GROK]), "Grok 4 by xAI answers, with room for 262,144 tokens.");
  assert.equal(answersLine([GROK_UNKNOWN]), "Grok Mini by xAI answers.");
  assert.equal(answersLine([OPENAI_GPT]), "GPT-5 by OpenAI answers.");
  assert.equal(answersLine([GROK, OPENAI_GPT]), "Grok 4 and GPT-5 answer in turn, side by side.");
  assert.equal(
    answersLine([{ name: "Phi-4", maker: "Microsoft", context: 16_384 }, GROK]),
    "Phi-4 and Grok 4 answer in turn, side by side.",
  );
  // A window not known is never the fullest, and never divides by nothing.
  const asked = [{ role: "user", content: "Hi." }, { role: "assistant", content: "Hello.", model: GROK_UNKNOWN.id, promptTokens: 9, completionTokens: 3 }];
  assert.equal(fullest(asked, [GROK_UNKNOWN]), null);
  assert.equal(fullest(asked, [GROK_UNKNOWN, GROK]).model, GROK);
});

test("a key pasted where the path goes is told from a path", () => {
  for (const path of ["/Users/me/.config/xai/key", "~/.config/openai/key", "~/xai-key.txt", "~\\keys\\xai", "C:\\keys\\xai.txt", "c:/keys/xai", "\\\\server\\keys", "  ~/padded  "]) {
    assert.ok(looksLikePath(path), `${path} did not read as a path`);
  }
  for (const text of ["xai-0123456789abcdefghijklmnopqrstuvwxyz", "sk-proj-not-a-real-key-0123456789abcdefghij", "key.txt", "~", "~key", "", undefined, "C:keys"]) {
    assert.equal(looksLikePath(text), false, `${text} read as a path`);
  }
});

test("the Installers page names each engine's models, or that none is found yet", () => {
  assert.equal(outsideModelsLine({ name: "Grok", models: [GROK, GROK_UNKNOWN] }), "Grok's models: Grok 4 and Grok Mini.");
  assert.equal(outsideModelsLine({ name: "GPT", models: [OPENAI_GPT] }), "GPT's models: GPT-5.");
  assert.equal(outsideModelsLine({ name: "Grok", models: [] }), "No Grok models found yet.");
  assert.equal(outsideModelsLine({ name: "GPT" }), "No GPT models found yet.");
  assert.equal(outsideModelsLine(null), "No models found yet.");
});

// Brad, 2026-11-17: "The results are graded and summarized by grok after all
// the results are in", and "I should be able to pick which engine does the
// grading".
const GRADED = [
  { role: "user", content: "Why do nails rust?" },
  { role: "assistant", content: "Oxygen and water.", model: PHI },
  { role: "assistant", content: "Iron oxidizes.", model: GPT },
  { role: GRADE_ROLE, content: "Answer 1: B. Right.\nAnswer 2: A. Names the reaction.\nSummary: Both agree.", model: GROK.id },
  { role: "user", content: "How do I stop it?" },
];

test("a round's grade sits with its answers and is never one of them", () => {
  const rounds = groups(GRADED);
  assert.deepEqual(rounds[0].answers, [GRADED[1], GRADED[2]]);
  assert.equal(rounds[0].grade, GRADED[3]);
  assert.equal(rounds[1].grade, null);
  // A second grade of the same round is not read.
  assert.equal(groups([...GRADED.slice(0, 4), { role: GRADE_ROLE, content: "Again.", model: PHI }])[0].grade, GRADED[3]);
  // The models that answered are the chat's models, never the grader.
  assert.deepEqual(latestAnswers(GRADED.slice(0, 4)), [PHI, GPT]);
  // A grade is saved as a turn of its own, with the model that graded.
  assert.deepEqual(savedTurn({ ...GRADED[3], thinking: "", pending: true }), { role: GRADE_ROLE, content: GRADED[3].content, model: GROK.id });
});

test("a grade is never sent to a model as part of the chat", () => {
  for (const id of [PHI, GPT, NEMO, undefined]) {
    const sent = requestMessages(GRADED, id);
    assert.ok(sent.every((message) => !message.content.includes("Answer 1: B")), `${id} was sent the grade`);
    assert.equal(sent.at(-1).content, "How do I stop it?");
  }
});

test("only answers from several models, not graded yet, are graded", () => {
  const [round] = groups(GRADED);
  assert.equal(canGrade(round), false, "a graded round is graded again");
  const ungraded = { ...round, grade: null };
  assert.equal(canGrade(ungraded), true);
  assert.deepEqual(gradedAnswers(ungraded), [GRADED[1], GRADED[2]]);
  assert.equal(canGrade({ ...ungraded, answers: [GRADED[1]] }), false, "one answer is graded");
  // An answer that never began is not one to grade.
  const empty = { role: "assistant", content: "  ", model: NEMO };
  assert.deepEqual(gradedAnswers({ ...ungraded, answers: [GRADED[1], empty, GRADED[2]] }), [GRADED[1], GRADED[2]]);
  assert.equal(canGrade({ ...ungraded, answers: [GRADED[1], empty] }), false);
  assert.equal(canGrade(null), false);
});

test("the grader reads the question and the answers, numbered, with no model's name", () => {
  const [system, user] = gradeMessages(" Why do nails rust? ", ["Oxygen and water.", " Iron oxidizes. "]);
  assert.deepEqual(system, { role: "system", content: GRADE_INSTRUCTIONS });
  assert.deepEqual(user, { role: "user", content: "Question:\nWhy do nails rust?\n\nAnswer 1:\nOxygen and water.\n\nAnswer 2:\nIron oxidizes." });
  assert.equal(gradeMessages("Q", ["a", "b"]).length, 2);
  for (const words of ["A to F", "Summary", "names of the models", CUT_MARK]) {
    assert.ok(GRADE_INSTRUCTIONS.includes(words), `the grader is not told about ${words}`);
  }
});

test("each answer is cut to fit the grader's window, evenly", () => {
  // A window not known, as with some of Grok's, cuts nothing.
  assert.equal(answerRoom(2, 0), Infinity);
  assert.equal(answerRoom(0, 16_384), Infinity);
  // Room for 6,000 tokens, less 5,120 kept free, at 3 characters a token,
  // less the question, shared by 2.
  assert.equal(answerRoom(2, 6_000, 40), 1_300);
  // However small the window, each answer keeps its first 600 characters.
  assert.equal(answerRoom(4, 4_096), 600);
  const long = "x".repeat(5_000);
  const [, user] = gradeMessages("Q", [long, "Short."], 6_000);
  assert.equal(answerRoom(2, 6_000, 1), 1_319);
  assert.equal(user.content, `Question:\nQ\n\nAnswer 1:\n${"x".repeat(1_319)}\n${CUT_MARK}\n\nAnswer 2:\nShort.`);
  // A cut never splits a character in two, even at an odd count.
  const [, emoji] = gradeMessages("Q", ["🙂".repeat(2_000), "Fine."], 6_000);
  assert.ok(emoji.content.includes(`${"🙂".repeat(1_319)}\n${CUT_MARK}`), "the cut is not counted in characters");
  assert.doesNotMatch(emoji.content, /[\uD800-\uDBFF](?![\uDC00-\uDFFF])/);
  assert.equal(gradeMessages("Q", [long, "b"], 0)[1].content.includes(CUT_MARK), false);
});

test("the grades and the summary are read from the grader's reply", () => {
  assert.deepEqual(parseGrades("Answer 1: B. Right, but short.\nAnswer 2: A-. Clear and complete.\nSummary: Both name oxygen. Answer 2 is best.", 2), {
    grades: [
      { letter: "B", why: "Right, but short." },
      { letter: "A-", why: "Clear and complete." },
    ],
    summary: "Both name oxygen. Answer 2 is best.",
    summed: true,
  });
  // Bold, bullets, list numbers, dashes, and the word Grade.
  const marked = parseGrades("Here you go.\n**Answer 1:** **C+** — Too long.\n- Answer 2 - Grade: F, Wrong.\n3. Answer #3 (B): Fine.\n\n## Summary\nThey differ.\n\nAnswer 3 is best.", 3);
  assert.deepEqual(marked.grades, [
    { letter: "C+", why: "Too long." },
    { letter: "F", why: "Wrong." },
    { letter: "B", why: "Fine." },
  ]);
  assert.equal(marked.summary, "They differ.\n\nAnswer 3 is best.");
  // A word is not a grade, a number with no answer is left out, and the first grade stands.
  const odd = parseGrades("Answer 1: Accurate and clear.\nAnswer 1: a fine one\nAnswer 3: A. None.\nAnswer 0: A.\nAnswer 2: D.\nAnswer 2: A. Again.", 2);
  assert.deepEqual(odd.grades, [null, { letter: "D", why: "" }]);
  assert.equal(odd.summary, "Answer 1: Accurate and clear.\nAnswer 1: a fine one", "what was not a grade is lost");
  // After the Summary line, everything is the summary, even a line like a grade.
  assert.equal(parseGrades("Summary:\nAnswer 1: A clear winner.", 1).summary, "Answer 1: A clear winner.");
  // A reply in no form the page knows is shown whole.
  assert.deepEqual(parseGrades("Both are fine.", 2), { grades: [null, null], summary: "Both are fine.", summed: false });
  assert.deepEqual(parseGrades(undefined, undefined), { grades: [], summary: "", summed: false });
});

test("each answer shows its own grade, and the summary waits for its line", () => {
  const empty = { role: "assistant", content: "", model: NEMO };
  const round = { ...groups(GRADED)[0] };
  round.answers = [round.answers[0], empty, round.answers[1]];
  const read = roundGrades(round);
  assert.deepEqual(read.grades, [{ letter: "B", why: "Right." }, null, { letter: "A", why: "Names the reaction." }]);
  assert.equal(read.summary, "Both agree.");
  assert.deepEqual(roundGrades({ answers: [empty] }), { grades: [null], summary: "" });
  // While the grade arrives, half a line is not shown as the summary.
  const arriving = { ...round, grade: { role: GRADE_ROLE, content: "Answer 1: B. Right.\nAnsw", pending: true } };
  assert.equal(roundGrades(arriving).summary, "");
  assert.deepEqual(roundGrades(arriving).grades[0], { letter: "B", why: "Right." });
  const summing = { ...round, grade: { role: GRADE_ROLE, content: "Answer 1: B.\nSummary: Bo", pending: true } };
  assert.equal(roundGrades(summing).summary, "Bo");
  assert.equal(roundGrades({ ...arriving, grade: { ...arriving.grade, pending: false } }).summary, "Answ");
});

test("the page names the model behind each answer the grader numbered", () => {
  const names = { [PHI]: "Phi-4", [GPT]: "gpt-oss 20B" };
  const nameOf = (id) => names[id] ?? "";
  const empty = { role: "assistant", content: "", model: NEMO };
  const round = { ...groups(GRADED)[0] };
  // The empty answer was not numbered, so Answer 2 is still gpt-oss's.
  round.answers = [round.answers[0], empty, round.answers[1]];
  round.grade = {
    role: GRADE_ROLE,
    content: "Answer 1: B. Shorter than Answer 2.\nAnswer 2: A. Names the reaction.\nSummary: Answer 2 is best; answer #1 agrees. Answer 3 and Answer 12 are not here. Answer 1 (Phi-4) was named already.",
  };
  const read = roundGrades(round, nameOf);
  assert.equal(
    read.summary,
    "Answer 2 (gpt-oss 20B) is best; answer #1 (Phi-4) agrees. Answer 3 and Answer 12 are not here. Answer 1 (Phi-4) was named already.",
  );
  assert.deepEqual(read.grades, [{ letter: "B", why: "Shorter than Answer 2 (gpt-oss 20B)." }, null, { letter: "A", why: "Names the reaction." }]);
  // A model not on hand has no name to add, and with no names nothing changes.
  assert.equal(nameAnswers("Answer 1 is fine.", [{ model: "gone" }], nameOf), "Answer 1 is fine.");
  assert.equal(roundGrades(round).summary, "Answer 2 is best; answer #1 agrees. Answer 3 and Answer 12 are not here. Answer 1 (Phi-4) was named already.");
  // The grader itself is never sent a name.
  assert.doesNotMatch(gradeMessages("Q", ["One.", "Two."], 0)[1].content, /Phi|gpt/);
});

test("the first outside engine grades until another model is picked, and none when No grading is", () => {
  const phi = { id: PHI, name: "Phi-4" };
  assert.equal(graderFor(null, [phi, GROK, GROK_UNKNOWN]), GROK);
  // In the Installers page's order: Grok's first, then GPT's, Claude's, and
  // Gemini's.
  assert.equal(graderFor(null, [phi, GROK, OPENAI_GPT, CLAUDE, GEMINI]), GROK);
  assert.equal(graderFor(null, [phi, OPENAI_GPT, CLAUDE]), OPENAI_GPT, "GPT does not grade when Grok is not on hand");
  assert.equal(graderFor(null, [phi, CLAUDE, GEMINI]), CLAUDE, "Claude does not grade when Grok and GPT are not on hand");
  assert.equal(graderFor(null, [phi, GEMINI]), GEMINI, "Gemini does not grade when it is the only outside engine on hand");
  assert.equal(graderFor(OPENAI_GPT.id, [phi, GROK, OPENAI_GPT]), OPENAI_GPT);
  assert.equal(graderFor(CLAUDE.id, [phi, GROK, OPENAI_GPT, CLAUDE]), CLAUDE, "Claude does not grade when picked");
  assert.equal(graderFor(GEMINI.id, [phi, GROK, GEMINI]), GEMINI, "Gemini does not grade when picked");
  assert.equal(graderFor(undefined, [phi]), null, "a model on this computer grades without being picked");
  assert.equal(graderFor("", [phi, GROK]), null, "No grading still grades");
  assert.equal(graderFor(PHI, [phi, GROK]), phi);
  // A grader picked that is not on hand, as Grok with the gate off, grades nothing.
  assert.equal(graderFor(GROK.id, [phi]), null);
  assert.equal(graderFor(null, undefined), null);
  assert.equal(gradesLine(GROK), "Grok 4 grades the answers once all are in.");
});

test("a reply that shows code instead of using the tools is caught", () => {
  assert.ok(describesWork("Here is the file:\n\n```yaml\nname: Dog Trainer\n```"));
  assert.ok(!describesWork("Done. The game is stubbed in games/dog-trainer."));
  assert.ok(!describesWork('Looking.\n```tool\n{"tool": "list", "path": "."}\n```'));
});

test("a tool call in a plain fence is read too", () => {
  const reply = 'I will list the files.\n\n```\n{"tool": "list", "path": "."}\n```\n\nThen make the game.\n\n```js\nfunction go() {}\n```';
  assert.deepEqual(parseToolCall(reply), { tool: "list", args: { path: "." }, before: "I will list the files." });
  assert.equal(parseToolCall("```\n{\"name\": \"x\"}\n```"), null);
});

test("a tool call in a plain fence is not shown as code", () => {
  assert.equal(
    withoutToolFences('Listing.\n\n```\n{"tool": "list", "path": "."}\n```\n\n```js\ngo();\n```').replace(/\n+/g, "|"),
    "Listing.|```js|go();|```",
  );
});

test("a call with raw line breaks in its strings is still read", () => {
  const reply = 'Editing.\n\n```\n{"tool": "edit", "path": "a.js", "find": "<a>\n  old\n</a>", "replace": "<a>new</a>"}\n```';
  assert.deepEqual(parseToolCall(reply).args, { path: "a.js", find: "<a>\n  old\n</a>", replace: "<a>new</a>" });
  assert.match(parseToolCall('```\n{"tool": "edit", "path": \n```').error, /not valid JSON/);
});

// Brad, 2026-11-19: "next story, I should be able to deactivate and uninstall
// a model. I can't do that currently from what I can see."
test("a model switched off is left out of the questions, grading, and comparisons", () => {
  const phi = { id: PHI, name: "Phi-4", maker: "Microsoft" };
  const oss = { id: "mlx-community/gpt-oss-20b-MXFP4-Q8", name: "gpt-oss 20B", maker: "OpenAI" };
  const onHand = [phi, oss, GROK, OPENAI_GPT];
  assert.deepEqual(switchedOn(onHand, []), onHand, "every model starts on");
  assert.deepEqual(switchedOn(onHand, undefined), onHand);
  const shown = switchedOn(onHand, [oss.id, GROK.id]);
  assert.deepEqual(shown, [phi, OPENAI_GPT], "the rest keep the list's order");
  // A question, and a comparison of several, can go only to models switched on.
  const ids = shown.map((entry) => entry.id);
  // As the page picks: the picks kept, else the first model switched on.
  assert.deepEqual(keptPicks(ids, [oss.id, GROK.id], ids.slice(0, 1)), [phi.id], "a pick switched off falls back to the first on");
  assert.deepEqual(keptPicks(ids, [oss.id, phi.id, OPENAI_GPT.id]), [phi.id, OPENAI_GPT.id]);
  // The grader comes from the same list: Grok switched off does not grade.
  assert.equal(graderFor(null, shown), OPENAI_GPT);
  assert.equal(graderFor(GROK.id, shown), null, "a grader switched off is not on hand");
  assert.equal(graderFor(null, switchedOn([phi, GROK], [GROK.id])), null);
});

test("a model with several copies here is on only while every copy is", () => {
  const copies = ["someone/Phi-4", "someone/Phi-4-8bit"];
  assert.equal(isOn(copies, []), true);
  assert.equal(isOn(copies, ["someone/Phi-4-8bit"]), false);
  assert.equal(isOn([], ["someone/Phi-4"]), true, "a model not here has nothing to switch");
  assert.equal(isOn(undefined, undefined), true);
  const phi = { name: "Phi-4", license: "MIT", onDisk: true, offer: null, ids: copies };
  assert.equal(modelLine(phi, []), "MIT · on this computer");
  assert.equal(modelLine(phi, ["someone/Phi-4"]), "MIT · on this computer, switched off");
  const away = { name: "Gemma 4 26B", license: "Apache 2.0", onDisk: false, offer: null, ids: [] };
  assert.equal(modelLine(away, ["someone/Phi-4"]), listedLine(away));
});

test("Uninstall asks first, names where the model goes, and warns of the shared cache", () => {
  assert.equal(
    uninstallAsk("Phi-4", { trash: "the Trash", sharedCache: true }),
    "Move Phi-4 to the Trash? Other apps that use the Hugging Face cache lose it too.",
  );
  assert.equal(uninstallAsk("Phi-4", { trash: "the Recycle Bin", sharedCache: false }), "Move Phi-4 to the Recycle Bin?");
  assert.equal(uninstallAsk("Phi-4", null), "Move Phi-4 to the Trash?", "before Rust answers, it says the Trash");
  assert.equal(uninstalledLine("gpt-oss 20B", { trash: "the Recycle Bin" }), "gpt-oss 20B is in the Recycle Bin.");
  assert.equal(uninstalledLine("gpt-oss 20B", undefined), "gpt-oss 20B is in the Trash.");
});
