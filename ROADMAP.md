# Roadmap

What is planned for Rusty Business™, and in what order. There are no dates
here: a thing ships when its tests are green.

## Pinned

Decided, and parked until it is picked up again. A pin keeps its number.

1. **Signed installers.** The installers ship unsigned at first, so macOS
   and Windows both warn before they open one. Signing and notarizing
   the macOS app with an Apple Developer ID, then signing the Windows
   installer, takes the warnings away. Each system's signing goes behind a
   switch of its own, off to start: the Mac's is being built first, and
   Windows's follows. Until its switch is on, a download ships as it does
   today, unsigned.

## Next

- **Kits.** A kit is a small file, kept outside the app, that names what a
  board needs before work on it can start, such as a folder on this
  computer or a repository to fetch. The app reads a kit and says what is
  missing.
- **A source for boards.** The `board` command keeps a board in step with
  a file on this computer. This adds a source a kit names: an address
  reached through the gates, pulled from the Workflow view. A pull leaves
  your notes.
- **Slices you can write.** Add and edit a story's slices in the Workflow
  view, each with a brief small enough for a local model, a meter that
  says whether the brief fits the model's window, and a way to copy your
  slices out as text.
- **An engine for Windows.** The Windows installer carries llama.cpp's
  `llama-server` inside the download, so nothing else has to be installed
  first: the Vulkan build from llama.cpp's own releases, pinned to one
  release and checked by its SHA-256, which uses the Vulkan loader. Until
  then, the app starts `llama-server` when it is installed on the computer.
- **Reading images.** Images dropped on a message, and scanned PDFs, are
  read by a model that can see. That means mlx-vlm in the engine and a
  vision model on the Models list, such as Qwen2.5-VL 7B at about 5 GB.
- **Making images.** Ask for a picture and it appears in the chat, made on
  this computer: mflux in the engine and a FLUX model of 6 to 12 GB.

## Later

- **A router over specialist models.** A small, fast classifier picks the
  best model for each message: one base model with an adapter per task,
  such as web coding, UI design, or project planning, so switching costs
  next to nothing. It ships first with the general model alone, noting what
  it would have picked, so real use shows which specialists to train.
- **Grok through xAI's newer API.** The app asks Grok through the chat API
  that xAI now calls legacy. Its Responses API is the one to move to, with
  the same key and the same rules.
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
  `scripts/linux-app.sh`, after a green gate.
- **A notice of each new version.** When a new version is released, the
  window says so, and Download opens this system's file in the browser: the
  `.dmg`, the `.exe`, or the Linux file this copy came from. The app asks
  GitHub when the window opens, once a day, and when you click Check for
  updates. The check itself downloads nothing.
- **The outbound gate.** A switch at the foot of the window, off until you
  turn it on. While it is off, nothing leaves the computer: no check for a
  new version, no download, and no start of the stack. Turning it off stops
  what is under way. A chat with a model here works either way.
- **Model makers you choose.** On a first run, you tick one maker or more,
  with Microsoft ticked to start. Settings adds or removes a maker with one
  click, and Install or Show on the Installers page adds one. The list now
  holds five models, Google's Gemma 4 26B and Meta's Llama 3.1 8B among
  them. Removing a maker hides its models and deletes nothing, and saved
  chats still name them.
- **One question, many models.** Pick one model or several at the top of
  the window, and each answers the same question, side by side. The models
  on this computer take turns, since the engine holds one at a time, and
  Stop asks no more of them.
- **Grok, the first engine outside this computer.** On the Installers page,
  you give the path to a file that holds your own xAI key. The app reads
  the key there when it asks Grok, never shows it to the page, and sends it
  over https only to xAI's API, and only while the outbound gate is on.
  Grok answers alone or as one of several, and a chip at the top says it is
  asked online.
- **GPT, by OpenAI, beside Grok.** GPT is added on the Installers page the
  same way, with the path to your own OpenAI key, which goes only to
  OpenAI's API. Each engine keeps its own key path and models, and removing
  one leaves the other as it was.
- **Claude, by Anthropic, beside them.** Claude is added on the Installers
  page the same way, with the path to your own Anthropic key, which goes
  only to Anthropic's API. Claude answers alone or among several, and can
  grade.
- **Gemini, by Google, beside them.** Gemini is added on the Installers
  page the same way, with the path to your own Google key, which goes only
  to Google's API, in a header and never in an address. Gemini answers
  alone or among several, and can grade.
- **Grading and a summary.** Once every answer is in, one model grades each
  from A to F and sums them up, reading the question and the answers,
  numbered and without the models' names. An outside engine grades, Grok
  first, then GPT, then Claude, then Gemini, until another model is picked
  in Settings, where No grading turns it off. A round not graded yet has a
  button to grade it.
- **An installers page.** Every model on the list, from every maker, is one
  Install away, with its license and download size. Install downloads it,
  with the outbound gate on, then adds its maker, so it shows at the top of
  the window. A model already here whose maker is not added has a Show
  button instead. Grok, GPT, Claude, and Gemini are added there too.
- **The app starts llama.cpp on Windows and Linux.** MLX runs on Apple
  silicon only, so on Windows and Linux the app starts llama.cpp's
  `llama-server` by itself when it is installed, the way the Mac's engine
  starts: on 127.0.0.1, offline, one model at a time. The five models come
  as GGUF files, from the maker's own copy first, then ggml-org's, each file
  checked against its pinned size and checksum.
- **An engine for Linux.** The `.deb`, the `.rpm`, and the `.tar.gz` carry
  llama.cpp's `llama-server` inside them, the Vulkan build from llama.cpp's
  own releases, pinned to one release and checked by its SHA-256. The
  packages ask for the Vulkan loader, so there is nothing to install first.
- **Projects, memory, and search.** Chats group into projects, each with
  its own instructions and memory; a global memory goes with every chat and
  grows as you talk. Chats can be pinned and searched.
- **A Models page.** Settings lists every model the app can download, and
  marks the ones already on this computer.
- **Projects that work in a folder.** A project can point at a folder on
  this computer. The model lists, reads, and searches its files, and edits
  or writes them only after you press Apply.
- **Formatted replies.** Replies show their Markdown, tables, code, and math.
- **Attached files.** PDF, Word, PowerPoint, spreadsheets, CSV, and text or
  code files are read into text on this computer and go with the message.
- **Switching a model off, and uninstalling one.** In Settings, under
  Models, each model has a switch, the outside engines' too. A model
  switched off is left out of questions, grading, and comparisons, and its
  files stay. Uninstall asks first, then moves a model on this computer to
  the Trash, or the Recycle Bin on Windows. Nothing is deleted.
- **Install and restart.** Where this copy came from the `.dmg`, the
  `.exe`, the `.deb`, or the `.rpm`, the notice of a new version offers
  Install and restart, with Download beside it. Nothing downloads until you
  click it. Then the app fetches the release's `latest.json` and this
  system's file through the outbound gate, installs the file only when its
  signature matches the key built into the app, and opens the new version.
  The Release workflow signs each of those files and writes `latest.json`.
  A copy unpacked from the `.tar.gz` updates by download.
- **Web search.** A model asked alone can look something up on the web
  while it answers, once you switch it on in Settings, where it starts
  off. The model writes the search, and its words go out through the
  outbound gate, over https, to DuckDuckGo, or to Brave Search when you
  give the path to a file that holds your own key. The model reads at most
  8 results, as plain text.
- **The Workflow view.** A board is a road of releases, the stories and
  bugs under each, and the slices each story is cut into. The view shows
  one, with the five lanes a slice goes through, Sliced, Written, Checked,
  Reviewed, and Main, and whose move each slice and each story is. It
  reads only a copy kept on this computer, so it works with both gates
  off, and it keeps a note of your own on any story. A made-up sample
  board shows how one reads.
- **A board in step with a file.** One command in a terminal,
  `rusty-business board`, keeps a board on this computer in step with one
  file kept somewhere else: `pull`, `push`, or `sync` for both ways.
  Changes to different things are both kept. The same thing changed two
  ways is a conflict: it is listed, and nothing is written until you pick
  a side. The file is board v1, an open format, so any program that reads
  and writes one can be the other side.
- **A model the engine leaves out.** An engine that was already running
  can list fewer models than this computer holds. The app names each
  model that is on the disk and not on the engine's list, says whether
  this window started that engine, and says what to do. It still never
  stops an engine it did not start.
