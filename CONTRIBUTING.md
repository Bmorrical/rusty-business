# Contributing to Rusty Business™

This page is the agreement every change follows before it merges. There are
two roles. Contributors send changes, and maintainers review and merge them.
Every maintainer is a contributor too and follows the same rules, but a
contributor is not a maintainer.

[AGENTS.md](AGENTS.md) sets every rule of the project in one place, for
people and for the AI models that work for them. Read it first.

## The rules for every change

- **Offline first.** On-device chat works with both network gates off. The
  LAN gate allows model discovery, chat, and model install, pause, and
  resume requests to the configured private-network engine. The WAN gate
  allows direct internet features, which use each service's own fixed hosts.
  Both gates start off and can be enabled independently.
- **Tests come with the work.** Every change brings tests that cover it, in
  the backend and end to end. Coverage never slips.
- **All checks pass before a pull request merges.** The gate runs on every
  pull request: the services, the desktop app, and the whole stack end to end.
- **Every review comment gets an answer.** When Copilot has credits, it
  reviews each pull request. Each comment, from Copilot or a person, is
  fixed, or declined with the reason, before the merge.
- **Deploys and releases only on green.** Nothing is deployed or released
  unless the gate is green on `main`.

## Contributors

Anyone can contribute.

- Work in a fork, or in a branch if you have access.
- Make one change per pull request, with a title and a description: what
  changes, why, and how it was checked.
- Before you push, run `make test`, and, with the stack up
  (`make bootstrap`), `make e2e`.
- Your work comes in under the [MIT License](LICENSE), like the rest of the
  code. Opening a pull request means you agree to that.
- The name and the artwork follow [TRADEMARKS.md](TRADEMARKS.md).
- Report a security problem privately, as [SECURITY.md](SECURITY.md) says,
  never in an issue or a pull request.

## Maintainers

Maintainers may also review and merge.

- A contributor's pull request merges once it meets every rule above and a
  maintainer has approved it.
- A maintainer may merge their own pull request once it meets every rule
  above, without waiting for another approval.
- `main` keeps a straight line of commits, so use "Squash and merge" or
  "Rebase and merge".
- Never force-push to `main`, and never change a release once it is
  published.
- A release runs from `main`, and only after the gate there is green.
- Every version gets one release note, written in [RELEASES.md](RELEASES.md)
  before the version is built. The gate fails without it, and the release
  run stops. A merge into `main` gets no note of its own: what it changed
  goes in the note of the version it goes out in.

## Who decides

The project's owner decides who becomes a maintainer and what this page
says. Later, a majority of the maintainers may decide instead. A change to
this page is a pull request like any other.
