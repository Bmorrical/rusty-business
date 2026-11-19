import assert from "node:assert/strict";
import { test } from "node:test";

import {
  checkedLine,
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
  formatCount,
  formatSize,
  formatWindow,
  fullness,
  onDiskLine,
  quickest,
  requestMessages,
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
  { id: "mlx-community/phi-4-4bit", name: "Phi-4", maker: "Microsoft", bytes: 8256384748 },
  { id: "mlx-community/gpt-oss-20b-MXFP4-Q8", name: "gpt-oss 20B", maker: "OpenAI", bytes: 12104215835 },
  { id: "mlx-community/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-4bit", name: "Nemotron 3.5 Lightning", maker: "NVIDIA", bytes: 17792581692 },
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
  // "Until Windows has an engine, the window opens and says so." So Windows
  // gets no command to run, and the footer shows none.
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
  assert.deepEqual(requestMessages(turns, "Be brief."), [
    { role: "system", content: "Be brief." },
    { role: "user", content: "Hi" },
  ]);
  assert.deepEqual(requestMessages(turns, "  "), [{ role: "user", content: "Hi" }]);
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
  assert.equal(parseToolCall("No tools here."), null);
  assert.match(parseToolCall("```tool\n{nope}\n```").error, /not valid JSON/);
  assert.match(parseToolCall('```tool\n{"tool": "delete"}\n```').error, /no tool called delete/);
  assert.equal(toolLabel({ tool: "read", args: { path: "a.js", from: 2, to: 9 } }), "Read a.js:2-9");
  assert.equal(toolLabel({ tool: "search", args: { text: "x" } }), 'Searched for "x"');
  assert.equal(toolResult({ tool: "list" }, "src/\n"), '<tool_result tool="list">\nsrc/\n</tool_result>');
  assert.ok(WRITE_TOOLS.includes("edit") && !WRITE_TOOLS.includes("read"));
  assert.match(workspacePrompt({ folder: "/Users/me/code/site" }, "src/"), /"site"[\s\S]*<folder>\nsrc\/\n<\/folder>/);
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
