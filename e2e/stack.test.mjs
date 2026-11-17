// End to end: the running stack, checked from outside, the way a person at
// this computer would meet it. Run with `make e2e` after `make bootstrap`.
//
// Anything that takes time is asked for again and again until a deadline,
// never after a fixed pause, so a slow machine only makes this slower.

import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

import { redis, sendMail } from './wire.mjs';

const run = promisify(execFile);
const REPO = fileURLToPath(new URL('..', import.meta.url));

// The same names packages/shared/src/settings.rs gives the queue and the
// worker's heartbeat. If they change there, this suite should fail here.
const JOBS_QUEUE = 'rusty:jobs';
const WORKER_HEARTBEAT = 'rusty:worker:heartbeat';
const HEARTBEAT_SECONDS = 30;

const LONG_RUNNING = ['api', 'caddy', 'mailpit', 'minio', 'postgres', 'redis', 'worker'];

function setting(name) {
  const value = process.env[name];
  assert.ok(value, `${name} is not set: run this suite with "make e2e"`);
  return value;
}

async function compose(...args) {
  const { stdout } = await run(
    'docker',
    ['compose', '--env-file', 'infra/.env', '--file', 'infra/compose.yaml', ...args],
    { cwd: REPO, maxBuffer: 16 * 1024 * 1024 },
  );
  return stdout;
}

// `docker compose ps` prints one JSON object per line, or one list,
// depending on its version.
async function containers() {
  const out = (await compose('ps', '--all', '--format', 'json')).trim();
  if (out.startsWith('[')) return JSON.parse(out);
  return out
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

async function until(what, check, { seconds = 45 } = {}) {
  const deadline = Date.now() + seconds * 1000;
  let last;
  for (;;) {
    try {
      const found = await check();
      if (found) return found;
    } catch (reason) {
      last = reason;
    }
    if (Date.now() > deadline) {
      throw new Error(`${what}: not seen within ${seconds} seconds`, { cause: last });
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}

async function getJson(url) {
  const reply = await fetch(url, { signal: AbortSignal.timeout(10_000) });
  return { status: reply.status, body: await reply.json() };
}

function job(kind) {
  return { id: `e2e-${randomUUID()}`, kind, payload: { from: 'the end-to-end suite' } };
}

async function workerLogs() {
  return compose('logs', '--no-color', '--since', '10m', 'worker');
}

test('every container is up and healthy', async () => {
  const seen = await containers();
  const byService = new Map(seen.map((container) => [container.Service, container]));

  assert.deepEqual([...byService.keys()].sort(), LONG_RUNNING);
  for (const service of LONG_RUNNING) {
    const container = byService.get(service);
    assert.equal(container.State, 'running', `${service} is ${container.State}`);
    assert.equal(container.Health, 'healthy', `${service} is ${container.Health}`);
  }
});

test('every published port is on this computer only', async () => {
  const seen = await containers();
  let published = 0;
  for (const container of seen) {
    for (const port of container.Publishers ?? []) {
      if (!port.PublishedPort) continue;
      published += 1;
      assert.equal(
        port.URL,
        '127.0.0.1',
        `${container.Service} publishes port ${port.PublishedPort} on ${port.URL}`,
      );
    }
  }
  assert.equal(published, 8, 'the stack publishes eight ports');
});

test('the front door answers by itself', async () => {
  const reply = await fetch(`${setting('STACK_FRONT_DOOR')}/front-door`);
  assert.equal(reply.status, 200);
  assert.equal(await reply.text(), 'ok');
});

test('the API says who it is, through the front door', async () => {
  const { status, body } = await getJson(`${setting('STACK_FRONT_DOOR')}/`);
  assert.equal(status, 200);
  assert.equal(body.service, 'rusty-business-api');
  assert.match(body.version, /^\d+\.\d+\.\d+$/);
});

test('health is ok through the front door and straight from the API', async () => {
  for (const base of [setting('STACK_FRONT_DOOR'), setting('STACK_API')]) {
    const { status, body } = await getJson(`${base}/health`);
    assert.equal(status, 200, `${base}/health`);
    assert.deepEqual(body, { status: 'ok', checks: { postgres: 'ok', redis: 'ok' } });
  }
});

test('the starter migration has run in Postgres', async () => {
  const out = await compose(
    'exec',
    '--no-TTY',
    'postgres',
    'sh',
    '-c',
    'psql --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" --tuples-only --no-align ' +
      '--command "SELECT note FROM bootstrap_notes"',
  );
  assert.deepEqual(out.trim().split('\n'), ['The database is ready.']);
});

test('a job put on the queue reaches the worker', async () => {
  const sent = job('hello');
  await redis(setting('REDIS_URL'), ['RPUSH', JOBS_QUEUE, JSON.stringify(sent)]);

  await until('the worker logging the job', async () => (await workerLogs()).includes(sent.id));
  const [waiting] = await redis(setting('REDIS_URL'), ['LLEN', JOBS_QUEUE]);
  assert.equal(waiting, 0, 'the queue is empty again');
});

test('a queue entry that is not a job does not stop the worker', async () => {
  const after = job('after-nonsense');
  await redis(
    setting('REDIS_URL'),
    ['RPUSH', JOBS_QUEUE, 'this is not a job'],
    ['RPUSH', JOBS_QUEUE, JSON.stringify(after)],
  );

  await until('the worker logging the job behind the bad entry', async () =>
    (await workerLogs()).includes(after.id),
  );
});

test('the worker keeps its heartbeat fresh', async () => {
  const [exists, secondsLeft] = await redis(
    setting('REDIS_URL'),
    ['EXISTS', WORKER_HEARTBEAT],
    ['TTL', WORKER_HEARTBEAT],
  );
  assert.equal(exists, 1);
  assert.ok(
    secondsLeft > 0 && secondsLeft <= HEARTBEAT_SECONDS,
    `the heartbeat has ${secondsLeft} seconds left`,
  );
});

test('Redis refuses a caller with no password', async () => {
  const open = new URL(setting('REDIS_URL'));
  open.password = '';
  await assert.rejects(redis(open.href, ['PING']), /NOAUTH/);
});

test('file storage answers, and the bucket is there', async () => {
  const reply = await fetch(`${setting('STACK_MINIO')}/minio/health/live`);
  assert.equal(reply.status, 200);

  const listing = await compose('exec', '--no-TTY', 'minio', 'mc', 'ls', 'local');
  assert.match(listing, new RegExp(`\\b${setting('STACK_MINIO_BUCKET')}/`));
});

test('mail sent to the stack lands in Mailpit', async () => {
  const subject = `e2e ${randomUUID()}`;
  await sendMail({
    port: setting('STACK_MAILPIT_SMTP_PORT'),
    from: 'e2e@rusty.localhost',
    to: 'inbox@rusty.localhost',
    subject,
    body: 'Sent by the end-to-end suite.',
  });

  await until('the message showing in Mailpit', async () => {
    const { body } = await getJson(`${setting('STACK_MAILPIT')}/api/v1/messages?limit=50`);
    return body.messages.some((message) => message.Subject === subject);
  });
});
