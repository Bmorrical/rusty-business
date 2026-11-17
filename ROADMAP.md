# Roadmap

What is planned for Rusty Business™, and in what order. There are no dates
here: a thing ships when its tests are green.

## Pinned

Decided, and parked until it is picked up again. A pin keeps its number.

1. **Signed installers.** The first installers will ship unsigned, so macOS
   and Windows will both warn before they open one. Signing and notarizing
   the macOS app with an Apple Developer ID, then signing the Windows
   installer, takes the warnings away.

## Next

In this order:

- **Installers.** A `.dmg` and an `.exe`, linked from the README: download,
  open, and go. The Mac's carries the model engine inside, so there is
  nothing else to install. Unsigned at first; see pin 1.

## Later

- **An engine for Windows.** MLX runs on Apple silicon only, so Windows
  needs an engine of its own. It is not chosen yet.

## Done

- **The chat window.** Saved chats, a model picker, and a token meter, in
  front of a model engine on the same computer.
- **One repository for everything.** A Rust API and worker, a Docker stack,
  and a README in every folder. The two services are stubs.
- **Open source.** The MIT license, and a page on how the name may be used.
- **The test gate.** Every test runs on GitHub on each push to `main`.
  Nothing is released or deployed unless that run is green.
- **The app finds its models.** It names the models already on the computer,
  and it starts the model engine by itself when the engine's program is
  installed, with nothing to type in a terminal first. MLX on Apple silicon.
- **A first-run picker.** On a computer with none of its models, the app
  lists them with their download sizes and points to the quickest to start.
  Download brings the one you pick from Hugging Face, each file checked
  against its pinned size and checksum.
- **The app starts the stack.** When Docker is on the computer, the app
  starts the Docker stack too, opening Docker Desktop when it is not running.
  The API and the worker come from GitHub's container registry, pinned by
  digest. When the app quits, it stops the stack it started.
