# services/worker

The worker. It waits on a queue in Redis, reads each job that arrives, and
today does nothing with it. The first real kind of job gets its branch in
[src/lib.rs](src/lib.rs).

## The queue

The queue is the Redis list `rusty:jobs`. Each entry is one JSON object:

```json
{"id": "0001", "kind": "hello", "payload": {"any": "json"}}
```

`id` and `kind` are 1 to 64 letters, digits, dots, underscores, colons, or
hyphens. `payload` is optional and may be any JSON. An entry over 64 KB, or
one that is not a job, is dropped with a warning and the worker carries on.

The worker logs a job's `id` and `kind`. It never logs the payload.

## Settings

| Name        | Needed | What it is                       |
| ----------- | ------ | -------------------------------- |
| `REDIS_URL` | yes    | `redis://:password@host:port/0`  |

## How it stays alive

- It refreshes the key `rusty:worker:heartbeat` every few seconds; the key
  lasts 30 seconds. `rusty-worker healthcheck` exits 0 only while that key
  exists, which is how Docker knows the worker is well.
- If Redis goes away, it waits 2 seconds and connects again.
- It stops cleanly on Ctrl-C or Docker's stop signal.

## Try it

With the stack up, from the repository's root, put a job on the queue:

```bash
docker compose --env-file infra/.env --file infra/compose.yaml exec redis redis-cli RPUSH rusty:jobs '{"id":"0001","kind":"hello"}'
```

Then read what the worker made of it:

```bash
docker compose --env-file infra/.env --file infra/compose.yaml logs --tail 5 worker
```

## Tests

```bash
cargo test --package rusty-worker
```

The path from the queue to the worker is covered end to end by `make e2e`.

| Path       | What it is |
| ---------- | ---------- |
| [src](src) | The code   |
