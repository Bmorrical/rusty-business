import assert from "node:assert/strict";
import { test } from "node:test";

import {
  answersLine,
  canRemove,
  chatModels,
  checkedLine,
  chipDetail,
  dayLabel,
  downloaded,
  downloadLine,
  firstRunOffers,
  formatCount,
  formatSize,
  formatWindow,
  fullest,
  fullness,
  gateLine,
  groups,
  isOutside,
  keptPicks,
  latestAnswers,
  listedLine,
  looksLikePath,
  meterLine,
  moreLine,
  namesLine,
  onDiskLine,
  OUTSIDE_PREFIX,
  outsideModelsLine,
  pickedMakers,
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
  withMaker,
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

// Brad, 2026-11-17: "there is a hard switch / circuit breaker to hard stop all
// outbound communication, user can turn it on to use their inference." Then:
// "off by default, everything stops".
test("the gate's line says what off and on mean", () => {
  assert.equal(gateLine(false), "Gate off. Nothing leaves this computer.");
  assert.equal(
    gateLine(true),
    "Gate on. The app may go online to check for updates, download models, start the stack, and ask Grok.",
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

test("Grok's models are told apart from the models on this computer", () => {
  assert.equal(OUTSIDE_PREFIX, "xai:");
  assert.ok(isOutside(GROK.id));
  for (const id of ["microsoft/phi-4", "", undefined, null, "grok-4", "local/xai:grok"]) {
    assert.equal(isOutside(id), false, `${id} read as Grok`);
  }
});

test("a chip says when a model is asked online", () => {
  assert.equal(chipDetail({ id: "microsoft/phi-4", maker: "Microsoft", context: 16_384 }), "Microsoft · 16K");
  assert.equal(chipDetail(GROK), "xAI · online · 256K");
  // A window xAI did not give is left out, not shown as 0K.
  assert.equal(chipDetail(GROK_UNKNOWN), "xAI · online");
});

test("Grok can answer alone, even when its window is not known", () => {
  assert.equal(answersLine([GROK]), "Grok 4 by xAI answers, with room for 262,144 tokens.");
  assert.equal(answersLine([GROK_UNKNOWN]), "Grok Mini by xAI answers.");
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
  for (const path of ["/Users/me/.config/xai/key", "~/xai-key.txt", "~\\keys\\xai", "C:\\keys\\xai.txt", "c:/keys/xai", "\\\\server\\keys", "  ~/padded  "]) {
    assert.ok(looksLikePath(path), `${path} did not read as a path`);
  }
  for (const text of ["xai-0123456789abcdefghijklmnopqrstuvwxyz", "key.txt", "~", "~key", "", undefined, "C:keys"]) {
    assert.equal(looksLikePath(text), false, `${text} read as a path`);
  }
});

test("Settings names Grok's models, or that none is found yet", () => {
  assert.equal(outsideModelsLine({ name: "Grok", models: [GROK, GROK_UNKNOWN] }), "Grok's models: Grok 4 and Grok Mini.");
  assert.equal(outsideModelsLine({ name: "Grok", models: [] }), "No Grok models found yet.");
  assert.equal(outsideModelsLine(null), "No Grok models found yet.");
});
