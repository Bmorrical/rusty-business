// The LAN and WAN gates, as the page shows them, checked on every test run.
//
// It answers the asks that started it: "there is a hard switch / circuit
// breaker to hard stop all outbound communication, user can turn it on to use
// their inference", and "off by default, everything stops".
//
// Rust keeps the gate, and its tests in gate.rs check how it behaves. These
// fail when the page stops reading the gate as Rust keeps it before it asks
// for anything, reads anything but a plain yes as on, loses the switch at the
// foot of the window, or lets a short window hide it, or when the comparison
// stops naming the Rust tests that keep the gate.
//
// The checks read the files as text, so nothing has to be installed to run
// them.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const APP = 'apps/desktop/src/App.vue';
const COMPARISON = 'scripts/comparison.test.mjs';

/** The Rust tests that keep the gate, each named in the comparison's list. */
const GATE_TESTS = [
  ['GATE', 'the_gate_is_off_by_default'],
  ['GATE', 'a_closed_gate_makes_no_connection'],
  ['GATE', 'every_connection_past_this_computer_goes_through_the_gate'],
  ['GATE', 'lan_and_wan_keep_independent_preferences'],
  ['GATE', 'lan_closure_cancels_an_in_flight_request_even_if_reopened'],
  ['DOWNLOADS', 'a_closed_lan_gate_never_starts_a_server_download'],
  ['DOWNLOADS', 'wan_shutdown_only_cancels_downloads_onto_this_computer'],
  ['UPDATE', 'with_the_gate_off_nothing_is_asked_and_the_check_says_to_turn_it_on'],
  ['DOWNLOADS', 'with_the_gate_off_nothing_is_downloaded'],
  ['STACK', 'with_the_gate_off_the_stack_is_not_started'],
  ['OUTSIDE', 'with_the_gate_off_grok_is_not_asked_and_the_key_is_not_read'],
  ['OUTSIDE', 'turning_the_gate_off_stops_grok_part_way_through'],
];

function read(file) {
  return readFileSync(path.join(REPO, file), 'utf8');
}

/** One function's text in App.vue. */
function functionOf(app, name) {
  const start = app.search(new RegExp(`\\n(?:async )?function ${name}\\(`));
  assert.ok(start >= 0, `${APP} has no ${name}`);
  return app.slice(start, app.indexOf('\n}', start) + 2);
}

test('the page reads the gate as Rust keeps it, before it asks for anything', () => {
  const app = read(APP);
  const readGate = functionOf(app, 'readGate');
  assert.match(readGate, /gate\.value = \(await invoke\("gate_status"\)\) === true;/, 'the page reads more than a plain yes as on');
  assert.match(readGate, /catch \{\s+gate\.value = false;/, 'the page reads a failed answer as on');

  const mounted = app.slice(app.indexOf('onMounted(async () => {'), app.indexOf('onBeforeUnmount('));
  const first = mounted.indexOf('await Promise.all([readGate(), readLanGate()]);');
  assert.ok(first >= 0, 'the page does not read the gate when it opens');
  for (const ask of ['checkStack();', 'checkUpdate();', 'checkEngine()']) {
    assert.ok(mounted.indexOf(ask) > first, `the page runs ${ask} before it knows whether the gate is on`);
  }
});

test('a switch at the foot of the window turns the gate on and off', () => {
  const app = read(APP);
  const footer = app.slice(app.indexOf('<div class="gate">'), app.indexOf('</aside>'));
  assert.ok(footer, 'the switch is gone from the foot of the window');
  for (const part of ['role="switch"', ':aria-checked="gate"', 'aria-describedby="gate-line"', ':disabled="gateBusy"', '@click="setGate(!gate)"', 'WAN gate', ':aria-checked="lanGate"', 'aria-describedby="lan-gate-line"', ':disabled="lanGateBusy"', '@click="setLanGate(!lanGate)"', 'LAN gate']) {
    assert.ok(footer.includes(part), `the gate's switch has lost ${part}`);
  }
  assert.ok(footer.includes('{{ gateLine(gate) }}'), "the switch no longer says what the gate's state means");

  const setGate = functionOf(app, 'setGate');
  assert.match(setGate, /gate\.value = \(await invoke\("set_gate", \{ open \}\)\) === true;/, 'the switch does not ask Rust to change the gate');

  // On a short window, such as a first run at 680 by 480, the footer is
  // taller than the room under the saved chats. It scrolls, so the switch can
  // still be reached, and the saved chats keep some room of their own.
  const rule = (selector) => new RegExp(`\\n${selector.replace('.', '\\.')} \\{([^}]*)\\}`).exec(app)?.[1] ?? '';
  assert.match(rule('.engine'), /min-height: 0;[\s\S]*overflow-y: auto;/, 'the footer cannot scroll, so a short window hides the switch');
  assert.match(rule('.sessions'), /min-height: 5rem;/, 'the footer can squeeze the saved chats out of sight');
});

// Todd, 2026-11-24: the foot of the side "takes up so much space". The
// gates stay one click away, each line moves into a tip, and the rest goes
// under one gear.
test('the gates stay in sight, each says what it means in a tip, and the rest sits under a gear', () => {
  const app = read(APP);
  const foot = app.slice(app.indexOf('<div class="side-foot">'), app.indexOf('</aside>'));
  assert.ok(foot, 'the side has lost its foot');
  assert.ok(app.indexOf('</footer>') < app.indexOf('<div class="side-foot">'), 'the gates have moved back into the part that scrolls');
  const rule = (selector) => new RegExp(`\\n${selector.replace('.', '\\.')} \\{([^}]*)\\}`).exec(app)?.[1] ?? '';
  assert.doesNotMatch(rule('.side-foot'), /overflow/, 'the foot scrolls, so a short window can hide a gate');

  for (const [id, name] of [['gate-line', 'WAN'], ['lan-gate-line', 'LAN']]) {
    assert.ok(foot.includes(`aria-label="About the ${name} gate"`), `the ${name} gate has lost its tip`);
    assert.ok(foot.includes(`<span id="${id}" class="tip" role="tooltip">`), `the ${name} gate's line is not in its tip, so the switch no longer describes it`);
  }
  assert.match(rule('.info:hover .tip,\n.info:focus .tip'), /visibility: visible;/, 'a tip never shows');

  const gear = foot.slice(foot.indexOf('<div class="gear-wrap">'));
  for (const part of ['aria-haspopup="menu"', ':aria-expanded="gearOpen"', '@click="gearOpen = !gearOpen"', 'aria-label="Settings and tools"']) {
    assert.ok(gear.includes(part), `the gear has lost ${part}`);
  }
  // Installers and Utilities are tabs of Settings, so the gear holds three.
  const order = ["togglePage('workflow')", "togglePage('settings')", '@click="checkNow"'].map((part) => gear.indexOf(part));
  assert.ok(order.every((at) => at > 0), 'the gear no longer holds every page and Check for updates');
  assert.deepEqual([...order].sort((a, b) => a - b), order, 'the gear lists its items out of order');
  assert.ok(!foot.slice(0, foot.indexOf('<div class="gear-wrap">')).includes('togglePage('), 'a page button sits outside the gear again');

  assert.match(functionOf(app, 'onWindowKey'), /gearOpen\.value = false;/, 'Escape no longer shuts the gear');
  assert.match(functionOf(app, 'onWindowClick'), /if \(gearOpen\.value && !event\.target\.closest\?\.\("\.gear"\)\) gearOpen\.value = false;/, 'a click elsewhere no longer shuts the gear');
});

test('LAN and WAN switches independently control model discovery, providers, and installs', async () => {
  const app = read(APP);
  const lanModel = { id: 'microsoft/phi-4-gguf:Q4_K' };
  const localModel = { id: 'phi-4' };
  const provider = { id: 'outside:grok/test' };
  const refs = Object.fromEntries(
    ['gate', 'gateBusy', 'gateNote', 'lanGate', 'lanGateBusy', 'lanGateNote', 'engine', 'vision', 'outside', 'picked', 'updateChecked']
      .map((name) => [name, { value: false }]),
  );
  refs.engine.value = { remote: true, reachable: true, models: [lanModel] };
  refs.outside.value = [{ added: true, models: [provider] }];
  const calls = [];
  const saved = { wan: false, lan: false };
  const context = vm.createContext({
    ...refs,
    computed: (get) => ({ get value() { return get(); } }),
    async invoke(command, args) {
      calls.push(command);
      switch (command) {
        case 'gate_status': return saved.wan;
        case 'lan_gate_status': return saved.lan;
        case 'set_gate': return (saved.wan = args.open);
        case 'set_lan_gate': return (saved.lan = args.open);
        default: throw new Error(command);
      }
    },
    rig: { value: { models: [] } },
    rigOffered: () => [],
    pickModels() {},
    checkEngine() { calls.push('engine'); },
    checkStack() { calls.push('stack'); },
    checkUpdate() { calls.push('update'); },
  });
  const values = ['outsideListed', 'outsideModels', 'lanEngine', 'engineModels', 'visionModels', 'rigModels', 'onHand', 'installGateOpen', 'canDownload'];
  const formulas = values.map((name) => app.match(new RegExp(`^const ${name} = .+;`, 'm'))?.[0]);
  assert.ok(formulas.every(Boolean));
  vm.runInContext([
    ...formulas,
    ...['readGate', 'readLanGate', 'setGate', 'setLanGate', 'turnOnInstallGate'].map((name) => functionOf(app, name)),
  ].join('\n'), context);
  const view = () => JSON.parse(vm.runInContext('JSON.stringify({ models: onHand.value.map(({id}) => id), canDownload: canDownload.value })', context));
  await vm.runInContext('Promise.all([readGate(), readLanGate()])', context);
  assert.deepEqual(view(), { models: [], canDownload: false });

  await context.turnOnInstallGate();
  assert.deepEqual(calls.splice(0), ['gate_status', 'lan_gate_status', 'set_lan_gate', 'engine']);
  assert.deepEqual(view(), { models: [lanModel.id], canDownload: true });

  await context.setGate(true);
  assert.deepEqual(calls.splice(0), ['set_gate', 'stack', 'update']);
  assert.deepEqual(view(), { models: [lanModel.id, provider.id], canDownload: true });

  await context.setLanGate(false);
  assert.deepEqual(calls.splice(0), ['set_lan_gate', 'engine']);
  assert.deepEqual(view(), { models: [provider.id], canDownload: false });

  await context.setGate(false);
  assert.deepEqual(view(), { models: [], canDownload: false });
  refs.engine.value = { remote: false, reachable: true, models: [localModel] };
  assert.deepEqual(view(), { models: [localModel.id], canDownload: false });
  await context.turnOnInstallGate();
  assert.deepEqual(saved, { wan: true, lan: false });
  assert.deepEqual(view(), { models: [localModel.id, provider.id], canDownload: true });
});

test('the comparison names every Rust test that keeps the gate', () => {
  const comparison = read(COMPARISON);
  const kept = comparison.slice(comparison.indexOf('const KEPT_BY = ['), comparison.indexOf('];', comparison.indexOf('const KEPT_BY = [')));
  for (const [file, name] of GATE_TESTS) {
    assert.ok(kept.includes(`{ file: ${file}, test: '${name}' }`), `the comparison no longer names ${name}`);
  }
});
