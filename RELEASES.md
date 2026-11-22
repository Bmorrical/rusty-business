# Release notes

What changed in each version of Rusty Business™, newest first.

Each version gets one note, written here before that version is built. The
gate fails without it, and the release run stops before it asks GitHub for
anything. A merge into `main` gets no note of its own: what it changed is in
the note of the version it goes out in.

From 0.8.3 on, a version's note is also at the top of its page under
[Releases](https://github.com/Bmorrical/rusty-business/releases), above the
steps to install it. The pages of the versions before 0.8.3 hold those steps
alone. A release that is out is never changed, so their notes are here.

No version went out as 0.1.0, 0.4.0, or 0.8.2.

## 0.8.4

**Fixed**

- **You keep your place while models answer.** Scroll up to read an earlier answer and the chat stays where you put it, even while another model is still answering. Scroll back to the end and it follows the answers again.

**New**

- **The Utilities page.** Small tools that run on this computer alone, so they work with both gates off. Opened from the side, next to Installers.
- **HEIC to JPEG.** The first utility, on a Mac for now. Drop pictures or a folder, or choose them, and each one gets a JPEG copy saved beside it. The original is never changed and a file already there is never replaced: a taken name gets `-1`, `-2`, and so on. Where a picture was taken is left out of its copy unless you keep it with the switch. Quality starts at 90, and Show opens the copy in Finder.
- **Picture check, if you want it.** An optional small model, nsfw-image-detection-384 by Marqo, installed in one click from Installers and uninstalled to the Trash the same way. Once installed, drop a JPEG, PNG, or WebP on Utilities and it shows a score on screen, with the caution that the model can make mistakes. It is a score, never a verdict. The picture is read on this computer and never copied or saved, and the score is never kept, logged, or put in a chat. Nothing installs it but you, and checking works with both gates off.
- **Open one answer wide.** When several models answer one question, a button on each answer opens it across the whole width, at the top of that question, to read more of it, and the same button puts it back side by side. Opening another answer in that question closes the first.
- **Votes on answers.** Rate any answer up or down, and when several models answer one question, vote one of them the best. The Votes tab in Settings keeps a tally per model, with the questions each one won, and each question opens its chat. Votes stay with their chats on this computer and are never sent to a model. They start on, and the same tab turns them off, which hides the buttons and keeps the votes already given.

## 0.8.3

Released 2026-11-21.

One release for everything merged since 0.8.1.

**New**

- **The Workflow view.** Opened from the side, it shows a board kept on this computer: its releases, the stories and bugs under each, the slices each story is cut into, and whose move each one is. A slice goes through five lanes: Sliced, Written, Checked, Reviewed, and Main. A made-up sample board comes inside the app. Notes of your own on a story are kept apart from the board. It reads only this computer, so it works with both gates off.
- **The `board` command.** `rusty-business board list | import | export | pull | push | sync` keeps a board in step with one file kept somewhere else, both ways. When the same thing changed on both sides, it lists the conflict and writes nothing until you pick a side with `--mine` or `--theirs`.
- **A ruleset for the board file.** `BOARD.md`, at the root of the repository, says what the file holds, each rule, and why, so any program that reads and writes one can be the other side. The file has a `meta` block and a `data` block, and a version: within a line a file only gains, and the app keeps a key it does not know and writes it back. A board's statuses are its own, listed in the file, each with an id that never changes.
- **No direct API connection to a board's backend, for now.** The app holds no address, no token, and no key for one, and a change that adds one is refused. A board moves as a file: exported on one side, and imported on the other. That keeps the app offline first, and at most LAN.
- **Web search.** In Settings, Web lets a model search the web while it answers. It is off to start, and it works with one model asked at a time, while the WAN gate is on. Only the model's search words go out: to DuckDuckGo, or to Brave Search with your own key.
- **Linux carries its engine.** The `.deb`, the `.rpm`, and the `.tar.gz` now have llama.cpp's `llama-server` inside, the Vulkan build, pinned by checksum.
- **A model the engine leaves out is named.** When an engine that was already running lists fewer models than this computer holds, the app names the missing ones, says whether this window started that engine, and says what to do. It still never stops an engine it did not start.
- **AGENTS.md.** One page at the root sets every rule of the project, for people and for the AI models that work for them: how to behave here, how to send a change, what a good slice of work looks like, and what is refused. The README, `CONTRIBUTING.md`, `SECURITY.md`, and `BOARD.md` point to it.
- **Release notes.** Every version has its note in `RELEASES.md`, back to 0.2.0, and from this version on at the top of its release page, which is what the What changed button opens.

**Fixed**

- "Open the sample board" never writes over a board of your own.
- One board file that cannot be read no longer hides the others. The view opens the next one and names the one it passed over.
- Notes on a board are held to the size the app can read back.
- A board's file is on the disk before it is moved into place.

**Good to know**

- The Workflow view only reads for now. A board changes when a file is pulled.
- A board v1 file, the format before this one, is still read, and a copy that came from one is written back as one.
- Windows still carries no engine: the app starts `llama-server` when it finds it on the computer.
- A copy of 0.8.0 or later offers Install and restart.

## 0.8.1

Released 2026-11-20.

- **A llama.cpp server on your own network.** The app can chat with a `llama-server` you configure on a private network, and Install can ask that server to fetch a model from the app's list.
- **Two gates in place of one.** The outbound gate is now a LAN gate, for that server, and a WAN gate, for everything past your network. Each is off until you turn it on.

## 0.8.0

Released 2026-11-20.

- **Install and restart.** Where a copy came from the `.dmg`, the `.exe`, the `.deb`, or the `.rpm`, the notice of a new version offers Install and restart, with Download beside it. The app installs the new file only when its signature matches the key built into the app, then opens the new version. A copy unpacked from the `.tar.gz` still updates by download.
- **Signed update files.** The release run signs each file Install and restart puts in place, and publishes `latest.json`, the list the app reads to find them.
- **A `.dmg` that does not pack says why**, and the build tries again.

## 0.7.0

Released 2026-11-20.

- **A switch for each model.** In Settings, under Models, every model has a switch, the outside engines' too. A model switched off is left out of questions, grading, and comparisons, and its files stay.
- **Uninstall.** It asks first, then moves a model's files to the Trash, or the Recycle Bin on Windows. Nothing is deleted.
- **A release waits on every test.** The gate now runs all of them.
- The README gives the Terminal command that opens the Mac app.
- Three of the app's libraries moved to newer versions: KaTeX, zip, and calamine.

## 0.6.1

Released 2026-11-19.

- **Projects, memory, and search.** Chats group into projects, each with its own instructions and memory. A global memory goes with every chat. Chats can be pinned and searched.
- **Projects that work in a folder.** A project can point at a folder on this computer. The model lists, reads, and searches its files, and edits or writes them only after you press Apply.
- **Attached files.** PDF, Word, PowerPoint, spreadsheets, CSV, and text or code files are read into text on this computer and go with the message.
- **Formatted replies.** Replies show their Markdown, tables, code, and math, and every message has a Copy button.
- **A Models page.** Settings lists every model the app can download, and marks the ones already on this computer.
- **A new look** for the window.
- `CONTRIBUTING.md`: the rules every change follows, and the gate on every pull request.

## 0.6.0

Released 2026-11-19.

- **The app starts llama.cpp on Windows and Linux.** MLX runs on Apple silicon only, so on Windows and Linux the app starts llama.cpp's `llama-server` by itself when it is installed.
- **GGUF models for Windows and Linux**, kept in the app's own models folder, each file checked against its pinned size and checksum.
- **Signing for the Mac app, behind a switch.** The release run can sign the Mac app and have Apple notarize it. The switch starts off, so the Mac app still went out unsigned.
- The README opens with three ways to install the app.

## 0.5.0

Released 2026-11-19.

- **A notice of each new version.** The window says when a newer version is out. Download opens this system's file in the browser, What changed opens the release's page, and Later hides the notice for that version. A Check for updates button asks now.
- **The outbound gate.** A switch at the foot of the window, off until you turn it on. While it is off, nothing leaves the computer.
- **Model makers you choose.** Pick them on a first run, and add or remove them in Settings.
- **One question, many models.** Pick several at the top of the window, and each answers the same question, side by side.
- **Grading and a summary.** Once every answer is in, a model you pick grades each from A to F and sums them up.
- **An installers page.** Every model on the list, from every maker, is one Install away.
- **Four engines outside this computer.** Grok, GPT, Claude, and Gemini, each with the path to a file that holds your own key, and only while the outbound gate is on.

## 0.3.0

Released 2026-11-17.

- **Downloads for Linux.** A `.deb`, an `.rpm`, and a `.tar.gz`, built after a green gate.
- The README lists every download at the top, with an index of the page.

## 0.2.0

Released 2026-11-17. The first release.

- **The chat window.** Saved chats, a model picker, and a token meter, in front of a model engine on the same computer.
- **The app finds its models.** It names the models already on the computer, and it starts the model engine by itself. MLX on Apple silicon.
- **A first-run picker.** On a computer with none of its models, the app lists them with their download sizes, and Download brings the one you pick.
- **The app starts the stack.** When Docker is on the computer, the app starts the Docker stack, and stops it when the app quits.
- **A download for the Mac.** A `.dmg` with the model engine inside. `make app` builds the same app from a fresh copy of the repository.
- **An installer for Windows.** An `.exe`, with no model engine yet.
- **The test gate.** Every test runs on each push to `main`, and nothing is released or deployed unless that run is green.
- **Open source.** The MIT license, a page on how the name may be used, a security policy, and a roadmap.
