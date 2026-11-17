# infra

The stack, as Docker runs it on one computer.

| Path                           | What it is                                              |
| ------------------------------ | ------------------------------------------------------- |
| [compose.yaml](compose.yaml)   | Every container, its health check, and its ports        |
| [.env.example](.env.example)   | The settings, with every password left empty            |
| [docker](docker)               | The recipe that builds the two services                 |
| [caddy](caddy)                 | The front door's settings                               |

## What runs

| Service      | Image                             | What it is for                                   | Port on this computer |
| ------------ | --------------------------------- | ------------------------------------------------ | --------------------- |
| `caddy`      | `caddy:2-alpine`                  | The front door. Passes requests to the API       | 4700                  |
| `api`        | built here                        | [services/api](../services/api)                  | 4701                  |
| `worker`     | built here                        | [services/worker](../services/worker)            | none                  |
| `postgres`   | `postgres:18-alpine`              | The database                                     | 4732                  |
| `redis`      | `redis:8-alpine`                  | The job queue and the worker's heartbeat         | 4779                  |
| `minio`      | `cgr.dev/chainguard/minio:latest` | File storage that speaks the S3 protocol         | 4790, console 4791    |
| `mailpit`    | `axllent/mailpit:v1.33`           | Catches every email and shows it on a web page   | SMTP 4725, page 4726  |

`minio-bucket` is a one-off job, not a running service: it makes the bucket
if it is not there, then exits. `make up` runs it once the stack is healthy.

The model engine is not here. It needs the computer's own GPU, so it runs
outside Docker.

## Start and stop

From the repository's root:

```bash
make bootstrap
```

writes `infra/.env` with new random passwords (only on the first run), then
builds, starts, and waits until every part reports healthy. After that:

```bash
make up
```

```bash
make down
```

`make down` keeps the data. No command in this repository removes a volume.
`make ps` shows each container's health, and `make logs` follows the logs.

## How it is locked down

- **This computer only.** Every published port binds to `127.0.0.1`. Nothing
  can be reached from another machine. `make e2e` checks this.
- **No shared passwords.** `.env.example` holds none. Bootstrap makes a new
  random one for Postgres, Redis, and MinIO, and writes `infra/.env` so only
  your user can read it. Git ignores that file.
- **The two services** run as a user that owns nothing, on a read-only file
  system, with every Linux capability dropped and no way to gain one. So does
  the front door.
- **Compose refuses to start** when a needed value is missing, and says which.

## Changing a port

Edit the `PORT_` line in `infra/.env`, then `make up`. The scripts and the
tests read the same file, so they follow.

## Notes

- MinIO's own images are no longer published on Docker Hub, so the stack
  uses Chainguard's build of the same server. Only its `latest` tag is free
  to pull, so that one image is not pinned to a version.
- Postgres 18 keeps its data under `/var/lib/postgresql`, one level above
  where earlier versions did. The volume is mounted there.
