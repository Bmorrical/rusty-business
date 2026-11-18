# Security

Rusty Business™ is built local first, to keep what you type on your
computer, with an outbound gate that holds everything else back until you
turn it on. If you find a way around that, or any other security problem, I
want to hear about it.

## Reporting a problem

Report it privately, on GitHub:
[Report a vulnerability](https://github.com/Bmorrical/rusty-business/security/advisories/new).
GitHub keeps the report private: only you and the repository's maintainers
can see it. Please do not open a public issue, pull request, or discussion
for it.

Say what you found, how to make it happen, and what it lets someone do. A
short report is fine.

I read every report and answer on the report itself. This is one person's
project, so there is no set time for an answer, and there is no bounty.

When the fix is in `main`, I publish an advisory on GitHub that says what was
wrong and what fixed it. It credits you by name unless you ask me not to.

## What is covered

The app's version is 0.5.0. Its downloads, for a Mac, for Windows, and for
Linux, are on the newest release on GitHub, built from `main`. Fixes go into `main`,
then into a new release, and only `main` and the newest release are
covered.

Everything in this repository is covered:

- The chat window: its page, and the Rust behind it.
- The API, the worker, and the code they share.
- The Docker stack, the scripts, and the workflows that run on GitHub.

These are the promises that matter most. A way to break one is exactly what
I want to hear about:

- The page has no way to call out.
- While the outbound gate is off, which it is until you turn it on, the app
  makes no connection past this computer: no check for a new version, no
  download, no start of the stack, and no question to Grok. Turning it off
  stops what is under way. With it on, every such connection is https only,
  through no proxy.
- Your xAI key stays in the file you name. The app keeps only the file's
  path, reads the key from it when it asks Grok, and sends the key only to
  xAI's API, over https, following no redirect. The key never reaches the
  page, a log, or a saved chat, and the app never changes or removes the
  file.
- The app refuses an engine address that is not on this computer, and an
  engine it starts listens on this computer only.
- The app downloads only the models on its list, only from Hugging Face over
  https, and checks every file against its pinned size and checksum.
- The check for a new version asks GitHub's API for this repository's newest
  release, over https, and nothing else. It sends nothing you type, follows
  no redirect, and downloads and installs nothing. Its Download button opens
  only this repository's release files on github.com, in your browser.
- Nothing in the models folder is written over, moved, or removed.
- No chat is deleted: the trash is a folder.
- Every port the stack opens listens on this computer only.

## What is not covered

Please report these to their makers:

- The model engine, `mlx_lm.server`, and the models themselves, including
  what a model says.
- The images the stack uses as they come: Postgres, Redis, MinIO, Mailpit,
  and Caddy.
- Docker, macOS, Windows, and Linux, with the WebKitGTK it draws with there.

One thing is known, so it needs no report: the installers are unsigned for
now, so the computer warns before it opens one. Signing them is pin 1 on the
[roadmap](ROADMAP.md).
