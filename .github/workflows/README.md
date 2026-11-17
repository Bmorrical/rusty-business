# .github/workflows

What GitHub runs for this repository.

| File          | When it runs                                                        | What it does                                             |
| ------------- | ------------------------------------------------------------------- | -------------------------------------------------------- |
| `gate.yml`    | Once on each push to `main`, on request, and when Deploy or Release calls it | Runs every test this repository has             |
| `deploy.yml`  | Only on request, only from `main`                                   | Runs the gate, then puts the API and worker images on GitHub's container registry |
| `release.yml` | Only on request, only from `main`                                   | Runs the gate, then builds the Mac app on a Mac, the Windows installer on Windows, and the Linux packages on Linux, and puts them all on a GitHub release, where the README's download links find them |

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

## What Release publishes

Release puts the app on a GitHub release named for its version, such as
`v0.3.0`: `Rusty-Business-mac-arm64.dmg`, with the model engine inside,
`Rusty-Business-windows-x64-setup.exe`, and for Linux
`Rusty-Business-linux-x64.deb`, `Rusty-Business-linux-x64.rpm`, and
`Rusty-Business-linux-x64.tar.gz`, those four with no engine yet, and a
checksum for each. It is marked as the newest release, which is the one the README's
download links find.

| Job       | Machine                  | What it does                                                   |
| --------- | ------------------------ | -------------------------------------------------------------- |
| `gate`    | as above                 | The whole gate. Nothing below starts until it is green         |
| `mac`     | macOS, Apple silicon     | Runs `make dmg`, the command a person runs, and keeps the `.dmg` and its checksum for the next job |
| `windows` | Windows, Intel or AMD    | Runs `scripts/windows-app.sh` in Git Bash, the command a person runs, and keeps the `.exe` and its checksum for the next job |
| `linux`   | Linux, Intel or AMD      | Works in Ubuntu 22.04's image, pinned by digest, so the packages start on Ubuntu 22.04 and Debian 12 too (a program built against a newer C library does not start on an older one). Installs WebKitGTK, OpenSSL, and a C compiler with `apt-get`, runs `scripts/linux-app.sh`, the command a person runs, and keeps the `.deb`, the `.rpm`, the `.tar.gz`, and their checksums for the next job |
| `publish` | Linux                    | Checks all five checksums and that the app's three version numbers agree, then makes the release and its tag at this commit |

A release that is out is never changed. When the version's tag is already
on GitHub, the run stops, so publishing again means raising the version in
`apps/desktop/package.json`, `apps/desktop/src-tauri/tauri.conf.json`, and
`apps/desktop/src-tauri/Cargo.toml`.

To run it: the Actions tab, then Release, then Run workflow, on `main`.

`scripts/release.sh` makes the release, and `scripts/release.test.mjs`
checks what it asks GitHub to do, with a stand-in `gh`, so nothing is
released by a test.

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
  There are two exceptions. Deploy's `build` and `publish` jobs may also
  write packages, which is how they push the images. Release's `publish`
  job may write the repository's contents, which is how it makes the
  release and its tag. No job may write anything else.
- Every action is pinned to a full commit, with its version in a comment
  beside it. A tag can be moved; a commit cannot.
- No workflow uses `pull_request_target`.
- Every checkout keeps no credentials behind.
- No `${{ }}` expression is written into a command. A value reaches a
  command through `env:`, so it is never read as shell.
- The registry token reaches Docker only on its standard input, never on a
  command line. In Release, the token reaches `gh` only through the release
  step's own `env:`. No other secret is used.

`scripts/workflows.test.mjs` checks these rules, that every job in
`deploy.yml` and `release.yml` waits for the gate, that Deploy builds each
kind of image on its own kind of computer before it joins them, and that
Release builds the Mac app on a Mac with `make dmg`, the Windows
installer on Windows with `scripts/windows-app.sh`, and the Linux packages
on Linux with `scripts/linux-app.sh`. It runs with
`make test-repo`, so the gate checks its own rules.

There is no README directly in `.github`: GitHub would show it as the
repository's front page in place of the real one.
