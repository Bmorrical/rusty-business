# scripts

Helpers the Makefile calls, and the repository's own checks.

| File               | What it does                                                        | Run it with       |
| ------------------ | ------------------------------------------------------------------- | ----------------- |
| `bootstrap.sh`     | First run: writes `infra/.env` with new random passwords, then starts the stack. Safe to run again; it keeps an `infra/.env` that is already there | `make bootstrap`  |
| `up.sh`            | Builds and starts the stack, waits until every part is healthy, then makes sure the storage bucket exists | `make up` |
| `stack-env.sh`     | Runs any command with the stack's addresses in its environment, read from `infra/.env` | `make test-live`, `make e2e` |
| `readmes.test.mjs` | Fails when a folder in this repository has no README                | `make test-repo`  |

`stack-env.sh` is how a program on this computer finds the stack:

```bash
scripts/stack-env.sh env
```

prints, among others, `DATABASE_URL`, `REDIS_URL`, and the `STACK_` addresses
the end-to-end suite uses. It reads `infra/.env` as text and never runs it.

Scripts here are Bash or plain Node with no packages to install, start by
moving to the repository's root, and stop at the first error.
