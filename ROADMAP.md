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

- **A first-run picker.** On a computer with no models, the app lists the
  models it knows with their download sizes, and points to a small one to
  get talking quickly. On a computer with no engine, it installs the engine
  too.
- **The app starts the stack.** When Docker is installed, the app brings the
  Docker stack up too.
- **Installers.** A `.dmg` and an `.exe`, linked from the README: download,
  open, and go. Unsigned at first; see pin 1.

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
