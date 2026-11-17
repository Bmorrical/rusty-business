# services/api

The API. Rust, on axum. Today it answers two questions.

| Request       | Answer                                                                    |
| ------------- | ------------------------------------------------------------------------- |
| `GET /`       | `{"service": "rusty-business-api", "version": "0.1.0"}`                   |
| `GET /health` | `200` with `{"status": "ok", "checks": {"postgres": "ok", "redis": "ok"}}`, or `503` with `"down"` beside whatever did not answer |

`/health` says "ok" or "down" and nothing more. The reason goes to the log,
never to the caller.

## Settings

All from the environment. There is no default address and no default
password.

| Name           | Needed | What it is                                              |
| -------------- | ------ | ------------------------------------------------------- |
| `DATABASE_URL` | yes    | `postgres://user:password@host:port/database`           |
| `REDIS_URL`    | yes    | `redis://:password@host:port/0`                         |
| `API_BIND`     | no     | The address to listen on. `127.0.0.1` unless set        |
| `API_PORT`     | no     | The port to listen on. `8000` unless set                |

## What happens at start

1. The settings are read. A missing or unusable one stops the program.
2. Postgres is tried up to 15 times, 2 seconds apart, in case it is still
   starting.
3. Every migration in [migrations](migrations) that has not run yet is run.
4. The API listens. It stops cleanly on Ctrl-C or Docker's stop signal.

## Run it

In the stack, `make up` builds and runs it. To run it from source against
the stack's Postgres and Redis, from the repository's root:

```bash
API_PORT=8010 scripts/stack-env.sh cargo run --package rusty-api
```

`rusty-api healthcheck` asks the API on this machine for `/health` and exits
0 only on a `200`.

## Tests

```bash
cargo test --package rusty-api
```

The unit tests stand in fakes for Postgres and Redis, so they need nothing
running. Two more tests use the real ones; with the stack up:

```bash
make test-live
```

| Path                     | What it is                                    |
| ------------------------ | --------------------------------------------- |
| [src](src)               | The code                                      |
| [migrations](migrations) | The database's history, one file per change   |
