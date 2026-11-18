# Roadmap

What is planned for Rusty Business™, and in what order. There are no dates
here: a thing ships when its tests are green.

## Pinned

Decided, and parked until it is picked up again. A pin keeps its number.

1. **Signed installers.** The installers ship unsigned at first, so macOS
   and Windows both warn before they open one. Signing and notarizing
   the macOS app with an Apple Developer ID, then signing the Windows
   installer, takes the warnings away.

## Next

- **Grading and a summary.** Once every answer is in, an engine you pick
  grades them and sums them up.
- **An installers page.** One click installs a model to run here, or sets up
  Claude or OpenAI's models.

## Later

- **Grok through xAI's newer API.** The app asks Grok through the chat API
  that xAI now calls legacy. Its Responses API is the one to move to, with
  the same key and the same rules.
- **An engine for Windows and Linux.** MLX runs on Apple silicon only, so
  Windows and Linux need an engine of their own. It is not chosen yet.
- **Pictures.** Add an image to a question and ask about it.
- **A look at the desktop.** Select text, or point at the screen, and ask
  about what is there.
- **Skills.** Tell the app how you like its answers, such as short unless
  you ask for more, and it keeps that.
- **Learning on this computer.** The app learns from how you use it, here,
  and reaches outside sparingly, only when you say so.

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
- **One command for the Mac app.** `make app` builds the app with the model
  engine inside and opens it, from a fresh copy of this repository, with
  nothing to install first but Apple's command line tools. `make dmg` packs
  the same app as a `.dmg`.
- **A download for the Mac.** The README links the newest `.dmg`, with the
  model engine inside: download, open, and go. The Release workflow builds
  it on a Mac with `make dmg`, after a green gate, and a release that is out
  is never changed. Unsigned for now; see pin 1.
- **An installer for Windows.** The README links the newest `.exe`, beside
  the Mac's `.dmg`: download, open, and go. The Release workflow builds it
  on Windows with `scripts/windows-app.sh`, after a green gate. Until Windows
  has an engine, the window opens and says so. Unsigned for now; see pin 1.
- **Downloads for Linux.** A `.deb`, an `.rpm`, and a `.tar.gz`, linked
  from the README beside the Mac's `.dmg` and the Windows `.exe`: download,
  install, and go. The Release workflow builds them on Ubuntu with
  `scripts/linux-app.sh`, after a green gate. Until Linux has an engine, the
  window opens and says so.
- **A notice of each new version.** When a new version is released, the
  window says so, and Download opens this system's file in the browser: the
  `.dmg`, the `.exe`, or the Linux file this copy came from. The app asks
  GitHub when the window opens, once a day, and when you click Check for
  updates. It installs nothing itself.
- **The outbound gate.** A switch at the foot of the window, off until you
  turn it on. While it is off, nothing leaves the computer: no check for a
  new version, no download, and no start of the stack. Turning it off stops
  what is under way. A chat with a model here works either way.
- **Model makers you choose.** On a first run, you tick one maker or more,
  with Microsoft ticked to start. Settings adds or removes a maker with one
  click. The list now holds five models, Google's Gemma 4 26B and Meta's
  Llama 3.1 8B among them. Removing a maker hides its models and deletes
  nothing, and saved chats still name them.
- **One question, many models.** Pick one model or several at the top of
  the window, and each answers the same question, side by side. The models
  on this computer take turns, since the engine holds one at a time, and
  Stop asks no more of them.
- **Grok, the first engine outside this computer.** In Settings, you give
  the path to a file that holds your own xAI key. The app reads the key
  there when it asks Grok, never shows it to the page, and sends it over
  https only to xAI's API, and only while the outbound gate is on. Grok
  answers alone or as one of several, and a chip at the top says it is
  asked online.
