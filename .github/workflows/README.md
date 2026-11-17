# .github/workflows

What GitHub runs for this repository.

| File         | When it runs                                                        | What it does                                             |
| ------------ | ------------------------------------------------------------------- | -------------------------------------------------------- |
| `gate.yml`   | Once on each push to `main`, on request, and when `deploy.yml` calls it | Runs every test this repository has                  |
| `deploy.yml` | Only on request, only from `main`                                   | Runs the gate, then puts the API and worker images on GitHub's container registry |

## The rule

**Nothing is released or deployed unless the gate is green.** A red gate is
fixed in its own pull request, and the gate runs again. There is no way
around it and no "just this once".

## What the gate runs

| Job        | Machine | What it runs                                                              |
| ---------- | ------- | ------------------------------------------------------------------------- |
| `services` | Linux   | `make test-services` (format, lint, unit tests) and `make test-repo`      |
| `desktop`  | macOS   | `make test-desktop` (page tests, page build, the app's Rust tests)        |
| `e2e`      | Linux   | `scripts/bootstrap.sh`, then `make test-live` and `make e2e` against the whole stack in Docker |
| `gate`     | Linux   | Green only when the three above are green. This is the one result to read |

Each job runs the same `make` command a person runs, so a red job can be
reproduced on any computer with the same command.

## What Deploy publishes

Deploy puts the two services on GitHub's container registry, as
`ghcr.io/bmorrical/rusty-business/api` and
`ghcr.io/bmorrical/rusty-business/worker`, for the desktop app to pull when
it starts the stack. They are built from the same recipe `make up` uses,
[infra/docker](../../infra/docker).

| Job       | Machine                  | What it does                                                   |
| --------- | ------------------------ | -------------------------------------------------------------- |
| `gate`    | as above                 | The whole gate. Nothing below starts until it is green         |
| `build`   | Linux, one Intel and one Arm | Builds both services for its own kind of computer and pushes them, tagged `sha-<commit>-amd64` or `sha-<commit>-arm64`. Nothing is emulated |
| `publish` | Linux                    | Joins each service's two images under `sha-<commit>`, checks that the result holds exactly those two, and writes each one's digest in the run's summary |

The digest is what the app pins. A tag can be moved; a digest cannot. No
image is tagged `latest`.

To run it: the Actions tab, then Deploy, then Run workflow, on `main`. The
first time, GitHub may list the two packages as private. Each one is made
public once, in its package settings, so the app can pull it without signing
in.

`scripts/publish-images.sh` does the building and the joining, and
`scripts/publish-images.test.mjs` checks what it asks Docker to do, with a
stand-in Docker, so nothing is built or pushed by a test.

## Pull requests

The gate does not run on pull request pushes. Before you push, run:

```bash
make test
```

and, with the stack up (`make bootstrap`):

```bash
make e2e
```

The gate then judges what actually landed on `main`.

## How the workflows are kept safe

- Each workflow can only read the repository (`permissions: contents: read`).
  Deploy's `build` and `publish` jobs are the one exception: they may also
  write packages, which is how they push the images. No job may write
  anything else.
- Every action is pinned to a full commit, with its version in a comment
  beside it. A tag can be moved; a commit cannot.
- No workflow uses `pull_request_target`.
- Every checkout keeps no credentials behind.
- No `${{ }}` expression is written into a command. A value reaches a
  command through `env:`, so it is never read as shell.
- The registry token reaches Docker only on its standard input, never on a
  command line, and no other secret is used.

`scripts/workflows.test.mjs` checks these rules, that every job in
`deploy.yml` waits for the gate, and that Deploy builds each kind of image on
its own kind of computer before it joins them. It runs with
`make test-repo`, so the gate checks its own rules.

There is no README directly in `.github`: GitHub would show it as the
repository's front page in place of the real one.
