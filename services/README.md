# services

The servers. Each is a Rust program in the workspace at the repository's
root, built into its own Docker image by
[infra/docker/service.Dockerfile](../infra/docker/service.Dockerfile).

| Folder           | What it is                                        | Today                                              |
| ---------------- | ------------------------------------------------- | -------------------------------------------------- |
| [api](api)       | Answers HTTP requests                             | Says who it is, and whether Postgres and Redis answer |
| [worker](worker) | Does work that should not hold up a request       | Reads each job off the queue and does nothing with it |

Both are stubs on purpose: the wiring is real (settings, health checks,
migrations, the queue, graceful stop) and the features are not written yet.

Rules both follow:

- Settings come from the environment only, through
  [packages/shared](../packages/shared). A missing value stops the program at
  start and names what is missing.
- Each program can check itself: `service healthcheck` exits 0 only when it
  is well. Docker uses that, so the images need no curl.
- Logs never carry a password, and never carry a job's payload.

## Tests

From the repository's root:

```bash
make test-services
```

A new service is a folder here named for what it does, a package named
`rusty-<folder>`, a line in the root `Cargo.toml`, and a block in
[infra/compose.yaml](../infra/compose.yaml).
