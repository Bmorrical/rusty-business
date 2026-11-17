# e2e

End-to-end tests: the running stack, checked from outside, the way a person
at this computer would meet it.

```bash
make bootstrap
```

```bash
make e2e
```

| File             | What it is                                                         |
| ---------------- | ------------------------------------------------------------------ |
| `stack.test.mjs` | The tests                                                          |
| `wire.mjs`       | A tiny Redis client and a tiny mail sender, so nothing is installed |

What is checked:

- Every container is up and healthy, and every published port is on
  127.0.0.1 only.
- The front door answers by itself, and passes requests to the API.
- `/health` says Postgres and Redis are well, through the front door and
  straight from the API.
- The starter migration has run.
- A job put on the queue reaches the worker, and an entry that is not a job
  does not stop it.
- The worker's heartbeat is fresh.
- Redis refuses a caller with no password.
- File storage answers and the bucket exists.
- An email sent to the stack lands in Mailpit.

The suite is plain Node (20 or later) with no packages. It reads the stack's
addresses from the environment, which `make e2e` fills from `infra/.env`.

Anything that takes time is asked for again and again until a deadline,
never after a fixed pause. A slow machine makes the suite slower, not red.
