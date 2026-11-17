# scripts

Helpers the Makefile calls, and the repository's own checks.

| File               | What it does                                                        | Run it with       |
| ------------------ | ------------------------------------------------------------------- | ----------------- |
| `bootstrap.sh`     | First run: writes `infra/.env` with new random passwords, then starts the stack. Safe to run again; it keeps an `infra/.env` that is already there | `make bootstrap`  |
| `up.sh`            | Builds and starts the stack, waits until every part is healthy, then makes sure the storage bucket exists | `make up` |
| `stack-env.sh`     | Runs any command with the stack's addresses in its environment, read from `infra/.env` | `make test-live`, `make e2e` |
| `readmes.test.mjs` | Fails when a folder in this repository has no README                | `make test-repo`  |
| `license.test.mjs` | Fails when the MIT license, the trademark page, or the banner is missing or changed, when a nail in the banner turns its point away from the ladder, or when a package does not say MIT | `make test-repo`  |
| `workflows.test.mjs` | Fails when a GitHub workflow breaks a rule: an action not pinned to a commit, more than read access, a run on a pull request, or a deploy that does not wait for the gate | `make test-repo`  |
| `roadmap.test.mjs` | Fails when the roadmap loses a section, a pin loses its number, a date is promised, or the README stops linking the roadmap | `make test-repo`  |
| `screenshots.mjs`  | Remakes the pictures in [docs/screenshots](../docs/screenshots): opens the app's built page in Google Chrome with no window on screen, and answers the page from recorded chats. It starts no engine | `make screenshots` |
| `screenshots.test.mjs` | Fails when a picture stops opening full size, is too small to read, or is missing from the set, when the first wide picture is shown outside the gallery, when the recorded chats name a model the app does not allow, when the recorded engine report and the app's own differ in their fields, when the page stops naming the models on this computer while the engine is off, or when the page stops asking for the engine to be started or stops saying why it could not be | `make test-repo`  |
| `comparison.test.mjs` | Fails when the README's comparison with Ollama is missing or gives fewer than three reasons either way, leaves out a maker on the app's list of models, or links anywhere but Ollama's own site, when the README stops listing what the app cannot do yet, or when the chat window gains a way to reach past this computer | `make test-repo`  |

`stack-env.sh` is how a program on this computer finds the stack:

```bash
scripts/stack-env.sh env
```

prints, among others, `DATABASE_URL`, `REDIS_URL`, and the `STACK_` addresses
the end-to-end suite uses. It reads `infra/.env` as text and never runs it.

Scripts here are Bash or plain Node with no packages to install, start by
moving to the repository's root, and stop at the first error.
`screenshots.mjs` is the one that needs something more: Google Chrome or
Chromium on the computer. No test starts the browser.
