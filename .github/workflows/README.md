# .github/workflows

What GitHub runs for this repository.

| File         | When it runs                                                        | What it does                                             |
| ------------ | ------------------------------------------------------------------- | -------------------------------------------------------- |
| `gate.yml`   | Once on each push to `main`, on request, and when `deploy.yml` calls it | Runs every test this repository has                  |
| `deploy.yml` | Only on request, only from `main`                                   | Runs the gate, then deploys. There is nothing to deploy yet |

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
- Every action is pinned to a full commit, with its version in a comment
  beside it. A tag can be moved; a commit cannot.
- No workflow uses `pull_request_target`.
- The checkout step keeps no credentials behind.

`scripts/workflows.test.mjs` checks these rules, and that every job in
`deploy.yml` waits for the gate. It runs with `make test-repo`, so the gate
checks its own rules.

There is no README directly in `.github`: GitHub would show it as the
repository's front page in place of the real one.
