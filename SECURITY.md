# Security

Rusty Business™ is built to keep what you type on your computer. If you find
a way around that, or any other security problem, I want to hear about it.

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

There is no release yet. The app's version is 0.1.0, and it is built from
source. Fixes go into `main`, and only `main` is covered.

Everything in this repository is covered:

- The chat window: its page, and the Rust behind it.
- The API, the worker, and the code they share.
- The Docker stack, the scripts, and the workflows that run on GitHub.

These are the promises that matter most. A way to break one is exactly what
I want to hear about:

- The page has no way to call out.
- The app refuses an engine address that is not on this computer, and an
  engine it starts listens on this computer only.
- The app downloads only the models on its list, only from Hugging Face over
  https, and checks every file against its pinned size and checksum.
- Nothing in the models folder is written over, moved, or removed.
- No chat is deleted: the trash is a folder.
- Every port the stack opens listens on this computer only.

## What is not covered

Please report these to their makers:

- The model engine, `mlx_lm.server`, and the models themselves, including
  what a model says.
- The images the stack uses as they come: Postgres, Redis, MinIO, Mailpit,
  and Caddy.
- Docker, macOS, and Windows.

One thing is known, so it needs no report: the installers, when they come,
will be unsigned at first, and the computer will warn before it opens one.
Signing them is pin 1 on the [roadmap](ROADMAP.md).
