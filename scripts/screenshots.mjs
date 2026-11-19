// Remakes the pictures in docs/screenshots.
//
// Each picture is the app's own page, built and opened in Chrome with no
// window on screen, in a small window drawn at twice the pixels so the words
// can be read and the picture can be enlarged.
//
// Rust is not running here, so the page's calls to it are answered from
// docs/screenshots/chats.json: what the app reported about the engine on one
// computer with every maker picked, and replies the three models there gave,
// recorded once. One of the chats puts the same question to all three.
// Nothing is sent to a model while the pictures are made, and no engine is
// started.
//
// Needs Google Chrome or Chromium (CHROME names the binary when it is not in
// a usual place) and the page built first. `make screenshots` does both.

import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { latestAnswers, titleFrom } from '../apps/desktop/src/chat.js';

const REPO = fileURLToPath(new URL('..', import.meta.url));
const PAGE = path.join(REPO, 'apps/desktop/dist');
const PICTURES = path.join(REPO, 'docs/screenshots');

// The window, in points, and how many pixels are drawn for each point. A
// small window keeps the words large; twice the pixels keeps them sharp when
// the picture is enlarged.
export const WIDTH = 920;
export const HEIGHT = 640;
export const SCALE = 2;

/**
 * One picture each. `open` is the id of the saved chat to show, `thinking`
 * opens that chat's thinking, `top` shows the chat from its first question
 * rather than its last answer, `page` opens the Installers or Settings page,
 * `engine: 'off'` is the app with no engine answering, and
 * `engine: 'first-run'` is the app on a computer with none of its models yet.
 */
export const SCENES = [
  { file: 'chat.png', scheme: 'dark', engine: 'on', open: 'nails' },
  { file: 'thinking.png', scheme: 'dark', engine: 'on', open: 'ladder', thinking: true, top: true },
  { file: 'compare.png', scheme: 'light', engine: 'on', open: 'compare', top: true },
  { file: 'new-chat.png', scheme: 'light', engine: 'on' },
  { file: 'engine-off.png', scheme: 'light', engine: 'off' },
  { file: 'first-run.png', scheme: 'dark', engine: 'first-run' },
  { file: 'installers.png', scheme: 'dark', engine: 'on', page: 'installers' },
  { file: 'settings.png', scheme: 'light', engine: 'on', page: 'settings' },
];

/** The button in the side bar that opens each page a picture shows. */
export const PAGE_BUTTONS = { installers: 'Installers', settings: 'Settings' };

/** The outside engines as outside_status gives them before anyone adds one. */
export const OUTSIDE_NOT_ADDED = [
  { id: 'xai', name: 'Grok', maker: 'xAI', host: 'api.x.ai', added: false, keyPath: null, models: [], warning: null, error: null },
  { id: 'openai', name: 'GPT', maker: 'OpenAI', host: 'api.openai.com', added: false, keyPath: null, models: [], warning: null, error: null },
  { id: 'anthropic', name: 'Claude', maker: 'Anthropic', host: 'api.anthropic.com', added: false, keyPath: null, models: [], warning: null, error: null },
  { id: 'google', name: 'Gemini', maker: 'Google', host: 'generativelanguage.googleapis.com', added: false, keyPath: null, models: [], warning: null, error: null },
];

const BROWSERS = [
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/Applications/Chromium.app/Contents/MacOS/Chromium',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
  '/usr/bin/chromium-browser',
];

const TYPES = {
  '.css': 'text/css; charset=utf-8',
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.svg': 'image/svg+xml',
};

/** The saved chats as Rust would hand them over, newest first. */
export function sessionsFrom(chats, now = new Date()) {
  return chats.map((chat, index) => {
    // Noon on the chat's day, so "Today" and "Yesterday" hold at any hour.
    const day = new Date(now.getFullYear(), now.getMonth(), now.getDate() - chat.daysAgo, 12, 0, -index);
    return {
      id: chat.id,
      title: titleFrom(chat.messages[0].content),
      model: chat.model,
      createdAt: day.getTime(),
      updatedAt: day.getTime(),
      messages: chat.messages,
    };
  });
}

/** The maker picked when nothing else is, as in makers.rs. */
export const FIRST_MAKER = 'Microsoft';

/** The makers on the app's list, in its order, as the recorded downloads name them. */
export function makersOf(report) {
  return [...new Set(report.offers.map((offer) => offer.maker))];
}

/**
 * The makers picked before the person picks any, as makers.rs has it:
 * Microsoft, and the maker of every model of the app's already on the disk.
 */
export function pickedAtFirst(report) {
  const wanted = [FIRST_MAKER, ...report.onDisk.map((model) => model.maker)];
  return makersOf(report).filter((maker) => wanted.includes(maker));
}

/**
 * The report with only the picked makers' models, as engine.rs gives it: the
 * engine's other models are set aside, so saved chats still name them. The
 * stand-in carries this function into the page, so it uses nothing else.
 */
export function forMakers(report, picked) {
  const mine = (list) => list.filter((model) => picked.includes(model.maker));
  const setAside = report.models.filter((model) => !picked.includes(model.maker));
  return { ...report, models: mine(report.models), setAside, onDisk: mine(report.onDisk), offers: mine(report.offers) };
}

/**
 * Every maker on the app's list with its models, as makers_status gives it.
 * The recording offers every model on the list, so its offers are the list.
 * The stand-in carries this function into the page, so it uses nothing else.
 */
export function makersStatus(report, picked) {
  const names = [...new Set(report.offers.map((offer) => offer.maker))];
  return names.map((name) => ({
    name,
    picked: picked.includes(name),
    models: report.offers
      .filter((offer) => offer.maker === name)
      .map((offer) => ({
        name: offer.name,
        license: offer.license,
        onDisk: report.onDisk.some((model) => model.name === offer.name),
        ids: report.onDisk.filter((model) => model.name === offer.name).map((model) => model.id),
        offer,
      })),
  }));
}

/** What the app reports when nothing answers at the engine's address. */
export function engineOff(engine) {
  return { ...engine, reachable: false, models: [], setAside: [], hidden: 0, error: 'The engine is not answering.' };
}

/**
 * What the app reports on a computer with none of its models: no engine
 * answers, nothing is on the disk, and the models it can download are offered.
 */
export function firstRun(engine) {
  return { ...engineOff(engine), onDisk: [] };
}

/**
 * What the app reports when it went to start the engine and could not find
 * the engine's program. The sentence is the app's own, from starter.rs.
 */
export const NOT_STARTED = {
  answering: false,
  started: false,
  reason: 'The app looked for mlx_lm.server on this computer and did not find it.',
};

/**
 * What the app reports when it went to start the engine with no model on the
 * disk to serve. The sentence is the app's own, from starter.rs.
 */
export const NO_MODEL = {
  answering: false,
  started: false,
  reason: 'With no model to serve, the app did not start the engine.',
};

/**
 * What the app reports when the stack's front door already answers, as it
 * does once the stack runs. Nothing is started while the pictures are made.
 */
export const STACK_RUNNING = {
  running: true,
  started: false,
  address: '127.0.0.1:4700',
  reason: null,
};

/**
 * What a scene's page is given: the report about the engine, the report about
 * starting it, and the saved chats. A first run has no saved chats yet.
 */
export function engineFor(scene, engine, sessions) {
  if (scene.engine === 'first-run') return { report: firstRun(engine), start: NO_MODEL, saved: [] };
  if (scene.engine === 'off') return { report: engineOff(engine), start: NOT_STARTED, saved: sessions };
  return { report: engine, start: NOT_STARTED, saved: sessions };
}

/**
 * The script that stands in for Rust inside the page. It answers the ten
 * calls the pictures need and refuses the rest, a download and a change of
 * makers among them. Grok is not added. Asked to start the engine, it starts
 * nothing and gives the reason it was handed, by default that the engine's
 * program was not found. Asked to start the stack, it starts nothing and says
 * the stack is running. The outbound gate is off, as it is on a first run.
 * The makers are picked as the app picks them before the person does, and
 * the engine's report holds only their models. No model is switched off, and
 * Uninstall would move one to the Trash, as on a Mac.
 */
export function standIn(engine, sessions, start = NOT_STARTED, stack = STACK_RUNNING) {
  return `(() => {
    const engine = ${JSON.stringify(engine)};
    const picked = ${JSON.stringify(pickedAtFirst(engine))};
    const sessions = ${JSON.stringify(sessions)};
    const notStarted = ${JSON.stringify(start)};
    const stack = ${JSON.stringify(stack)};
    const outside = ${JSON.stringify(OUTSIDE_NOT_ADDED)};
    const forMakers = ${forMakers};
    const makersStatus = ${makersStatus};
    const latestAnswers = ${latestAnswers};
    const answers = {
      engine_status: () => forMakers(engine, picked),
      makers_status: () => makersStatus(engine, picked),
      start_engine: () => notStarted,
      start_stack: () => stack,
      gate_status: () => false,
      deactivated_models: () => [],
      uninstall_place: () => ({ trash: 'the Trash', sharedCache: true }),
      outside_status: () => outside,
      list_sessions: () =>
        sessions.map((entry) => ({
          id: entry.id,
          title: entry.title,
          model: entry.model,
          models: latestAnswers(entry.messages),
          updatedAt: entry.updatedAt,
          turns: entry.messages.length,
        })),
      load_session: ({ id }) => sessions.find((entry) => entry.id === id),
    };
    try {
      window.localStorage.clear();
    } catch {
      // A blank page has no storage to clear.
    }
    window.__TAURI_INTERNALS__ = {
      invoke: async (command, args) => {
        if (!(command in answers)) throw new Error(command + ' is not part of the pictures.');
        return structuredClone(answers[command](args ?? {}));
      },
    };
  })();`;
}

function findBrowser() {
  const named = process.env.CHROME;
  const found = named ? [named].find(existsSync) : BROWSERS.find(existsSync);
  if (!found) {
    throw new Error(
      named
        ? `CHROME names ${named}, and nothing is there.`
        : 'Google Chrome was not found. Set CHROME to the browser binary.',
    );
  }
  return found;
}

/** Serves the built page on this computer only, on a port the system picks. */
function serve(root) {
  const server = createServer((request, response) => {
    const asked = decodeURIComponent(new URL(request.url, 'http://page').pathname);
    const file = path.join(root, asked === '/' ? 'index.html' : asked);
    const inside = file.startsWith(root + path.sep);
    if (!inside || !existsSync(file) || !statSync(file).isFile()) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { 'content-type': TYPES[path.extname(file)] ?? 'application/octet-stream' });
    response.end(readFileSync(file));
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(server));
  });
}

/**
 * Starts Chrome with no window and its own empty profile, and returns a
 * function that sends it one command (the DevTools protocol, over a pipe).
 */
function startBrowser(binary, profile) {
  const browser = spawn(
    binary,
    [
      '--headless=new',
      '--remote-debugging-pipe',
      `--user-data-dir=${profile}`,
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-extensions',
      '--hide-scrollbars',
      '--force-color-profile=srgb',
      'about:blank',
    ],
    { stdio: ['ignore', 'ignore', 'ignore', 'pipe', 'pipe'] },
  );
  const [, , , toBrowser, fromBrowser] = browser.stdio;
  const waiting = new Map();
  let last = 0;
  let unread = '';

  fromBrowser.setEncoding('utf8');
  fromBrowser.on('data', (piece) => {
    unread += piece;
    for (;;) {
      const end = unread.indexOf('\0');
      if (end < 0) break;
      const message = JSON.parse(unread.slice(0, end));
      unread = unread.slice(end + 1);
      const asked = waiting.get(message.id);
      if (!asked) continue;
      waiting.delete(message.id);
      if (message.error) asked.reject(new Error(`${asked.method}: ${message.error.message}`));
      else asked.resolve(message.result);
    }
  });
  const closed = new Promise((resolve) => {
    browser.once('exit', () => {
      for (const asked of waiting.values()) asked.reject(new Error(`${asked.method}: the browser closed.`));
      waiting.clear();
      resolve();
    });
  });

  const send = (method, params = {}, sessionId = undefined) =>
    new Promise((resolve, reject) => {
      last += 1;
      waiting.set(last, { method, resolve, reject });
      toBrowser.write(`${JSON.stringify({ id: last, method, params, sessionId })}\0`);
    });
  return { browser, closed, send };
}

async function inPage(send, tab, expression) {
  const { result, exceptionDetails } = await send(
    'Runtime.evaluate',
    { expression, awaitPromise: true, returnByValue: true },
    tab,
  );
  if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
  return result.value;
}

async function until(send, tab, expression, what) {
  for (let tries = 0; tries < 100; tries += 1) {
    if (await inPage(send, tab, expression)) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`The page never showed ${what}.`);
}

async function takePicture(send, address, scene, engine, sessions) {
  const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
  const { sessionId: tab } = await send('Target.attachToTarget', { targetId, flatten: true });

  await send('Page.enable', {}, tab);
  await send(
    'Emulation.setDeviceMetricsOverride',
    { width: WIDTH, height: HEIGHT, deviceScaleFactor: SCALE, mobile: false },
    tab,
  );
  await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: scene.scheme }] }, tab);
  await send('Emulation.setFocusEmulationEnabled', { enabled: true }, tab);
  const { report, start, saved } = engineFor(scene, engine, sessions);
  await send('Page.addScriptToEvaluateOnNewDocument', { source: standIn(report, saved, start) }, tab);
  await send('Page.navigate', { url: address }, tab);
  await until(send, tab, `document.querySelector('.engine .dot') !== null`, 'the engine line');
  if (scene.engine !== 'on') {
    // With no engine answering, the page asks for one to be started. The
    // picture is of the page once it has its answer, not of the wait.
    await until(send, tab, `document.querySelector('.engine button.wide') !== null`, 'the Try again button');
  }
  if (scene.engine === 'first-run') {
    await until(send, tab, `document.querySelectorAll('.offer').length > 0`, 'the models to download');
  }

  if (scene.open) {
    const row = sessions.findIndex((entry) => entry.id === scene.open);
    if (row < 0) throw new Error(`${scene.file}: no recorded chat is called ${scene.open}.`);
    await until(send, tab, `document.querySelectorAll('button.session').length > ${row}`, 'the saved chats');
    await inPage(send, tab, `document.querySelectorAll('button.session')[${row}].click()`);
    await until(send, tab, `document.querySelector('.turn.assistant .note') !== null`, 'the chat');
  }
  if (scene.thinking) {
    await inPage(send, tab, `document.querySelector('details.thinking').open = true`);
  }
  if (scene.top) {
    await inPage(send, tab, `document.querySelector('.thread').scrollTop = 0`);
  }
  if (scene.page) {
    const label = JSON.stringify(PAGE_BUTTONS[scene.page]);
    await inPage(send, tab, `[...document.querySelectorAll('button')].find((button) => button.textContent.trim() === ${label}).click()`);
    await until(send, tab, `document.querySelectorAll('.makers > li').length > 0`, 'the model makers');
    // Scrolled to the first maker, the picture holds every maker on the list.
    await inPage(send, tab, `document.querySelector('.makers > li').scrollIntoView({ block: 'start' })`);
  }

  // Wait for the fonts, then for two frames, so the picture is of a settled page.
  await inPage(
    send,
    tab,
    `document.fonts.ready.then(
      () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => done(true)))),
    )`,
  );
  const { data } = await send('Page.captureScreenshot', { format: 'png' }, tab);
  writeFileSync(path.join(PICTURES, scene.file), Buffer.from(data, 'base64'));
  await send('Target.closeTarget', { targetId });
}

async function main() {
  if (!existsSync(path.join(PAGE, 'index.html'))) {
    throw new Error('The page is not built. Run `make screenshots`, which builds it first.');
  }
  const { engine, chats } = JSON.parse(readFileSync(path.join(PICTURES, 'chats.json'), 'utf8'));
  const sessions = sessionsFrom(chats);
  const profile = mkdtempSync(path.join(tmpdir(), 'rusty-business-pictures-'));
  const server = await serve(PAGE);
  const { browser, closed, send } = startBrowser(findBrowser(), profile);
  try {
    const address = `http://127.0.0.1:${server.address().port}/`;
    for (const scene of SCENES) {
      await takePicture(send, address, scene, engine, sessions);
      console.log(`docs/screenshots/${scene.file}`);
    }
  } finally {
    browser.kill();
    await closed;
    await new Promise((resolve) => server.close(resolve));
    // The profile is this run's own, made a few lines up. The browser's
    // helpers can still be writing to it for a moment, so try a few times.
    rmSync(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.chdir(REPO);
  await main();
}
