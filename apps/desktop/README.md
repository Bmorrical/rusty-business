# The desktop app

A window for chatting with models on your own computer or a configured
llama-server on your private network. The shell is Rust (Tauri 2), and the
page is Vue 3. The LAN gate controls access to your private-network engine.
The WAN gate controls the app's internet features, including Grok, GPT,
Claude, and Gemini with your own keys.

[![A chat with Phi-4 in the dark theme: two questions about rusty nails, each answer with its token count and speed, saved chats on the left, Phi-4 picked among the models at the top, and the context meter under the message box reading 184 of 16,384 tokens](../../docs/screenshots/chat.png)](../../docs/screenshots/chat.png?raw=true)

Select the picture to open it full size. The rest of the set is in
[docs/screenshots](../../docs/screenshots).

## What it does

- **The LAN and WAN gates**, two switches at the foot of the window, each with
  a tip that says what it means. Both start off and keep their settings independently. LAN controls discovery,
  chat, and model install, pause, and resume requests to the configured
  server. WAN controls direct downloads onto this computer, updates, stack
  startup, and outside providers. Turning either gate off stops the app's
  work through that gate. On-device chat works with both gates off.
- **Chat** with a streamed reply, a Stop button, and the model's thinking kept
  apart from its answer.
- **One model or several.** The models sit at the top of the window, one
  button each, and any number of them can be picked; one always stays
  picked. Each picked model answers the same question, and the answers sit
  side by side. The models on this computer take turns, since the engine
  holds one at a time, and each sees only its own side of the chat. Stop
  ends the answer under way and asks no more models. A reopened chat picks
  the models that answered its latest question.
- **A short list of models:** Phi-4 (Microsoft), gpt-oss 20B (OpenAI),
  Nemotron 3.5 Lightning (NVIDIA), Gemma 4 26B (Google), and Llama 3.1 8B
  (Meta). The window shows the models of the makers picked. A model the
  engine holds that is not on the list is never shown and never accepted.
- **Model makers**, picked on a first run, Microsoft to begin with, and in
  Settings, which adds or removes one with one click. One maker always stays
  picked. Removing a maker hides its models and deletes nothing, and saved
  chats still name them.
- **Install, in Settings under Models**, where every model on the list,
  from every maker, shows with its license and download size. Install
  downloads one, with the gate on, then adds its maker so it shows at the
  top of the window. A model already here whose maker is not added has a
  Show button instead. The Installers tabs beside it hold the picture
  models, the picture check, and the outside engines.
- **A Workflow view**, under the gear at the foot of the side, that
  shows a board: a road of releases, the stories and bugs under each, and
  the slices each story is cut into. A slice goes through five lanes,
  Sliced, Written, Checked, Reviewed, and Main, and the view says whose
  move each slice and each story is. A story shows five marks, one for
  each lane all of its slices have been through, and all five make a royal
  flush. The view reads only a copy kept on this computer, so it works
  with both gates off, and it keeps a note of your own on any story. The
  sample board is made up, and shows how one reads. [Boards](#boards)
  says where a board is kept and what is in one.
- **A switch for each model**, in Settings under Models, for the models on
  this computer and the outside engines' alike. A model switched off is left
  out of questions, grading, and comparisons, and its files stay. Saved
  chats still name it.
- **Uninstall**, beside the switch for a model on this computer. It asks
  first, then moves the model's files to the Trash (the Recycle Bin on
  Windows), where you can take them back. On a Mac, the question also says
  that other apps that use the Hugging Face cache lose the model too.
- **Grok, by xAI**, added on the Installers page under Outside engines with
  the path to a file that holds your own xAI key. With the gate on, the app
  finds Grok's models, and they join the buttons at the top of the window,
  marked online, to answer alone or beside the models here. The key never
  reaches the page. Remove forgets the path, leaves the file as it is, and
  saved chats still name Grok's models.
- **GPT, by OpenAI**, added the same way on a card of its own, with the path
  to a file that holds your own OpenAI key. That key goes only to OpenAI's
  API. Each engine keeps its own path and models, and removing one leaves
  the others as they were.
- **Claude, by Anthropic**, added the same way on a card of its own, with
  the path to a file that holds your own Anthropic key. That key goes only
  to Anthropic's API, in Anthropic's own header. A chat for Claude starts
  and ends with a message of yours, and a question Claude declines says so.
- **Gemini, by Google**, added the same way on a card of its own, with the
  path to a file that holds your own Google key. That key goes only to
  Google's API, in Google's own header, never in an address. A chat for
  Gemini starts and ends with a message of yours, and a question Gemini
  declines says so.
- **Grading**, once several models have answered. One model, an outside
  engine (Grok first, then GPT, then Claude, then Gemini) until another is picked in
  Settings under Grading, grades each answer from A to F and sums them up. It reads the question and the answers, numbered and
  without the models' names. Each answer shows its letter, the summary sits
  below them, and a round not graded yet has a button to grade it. Where the
  grader wrote "Answer 2", the page adds the name of the model that wrote it.
  No grading turns it off.
- **Web search**, off until you switch it on in Settings under Web. A model
  asked alone can then look something up while it answers: it writes the
  search, the app runs it, and the model reads at most 8 results as plain
  text, with every tag taken out. A search goes out only while the gate is
  on, over https, to DuckDuckGo's results page, or to Brave Search's API
  when you give the path to a file that holds your own Brave key. That key
  goes only to Brave and never reaches the page.
- **The models on this computer**, named when the engine is not answering.
  On a Mac, the app looks in the Hugging Face cache folder, where the engine
  keeps the models it has downloaded. On Windows and Linux, it looks in the
  app's own models folder. It only reads there, it keeps no list of its own,
  and a download that stopped part way is not counted.
- **The engine, started by the app.** When no engine is answering, the app
  starts one and waits for it: `mlx_lm.server` on a Mac, and llama.cpp's
  `llama-server` on Windows and Linux. It starts only what is missing: an
  engine that is already running is used as it is, and asking twice changes
  nothing. When the app quits, it stops the engine it started and no other.
  When it cannot start one, it says why in one sentence.
- **A model the engine leaves out.** A question goes only to a model the
  engine lists. An engine started earlier, by another copy of the app or by
  hand, can list fewer models than this computer holds. The app then names
  each model that is on this computer and not on the engine's list, beside
  the models a question goes to, at the foot of the side, and on the
  model's row in Settings and on the Installers page. It also says whether
  this window started the engine that answers. It does not stop or restart
  an engine it did not start: it says how to, and Check again looks once
  more.
- **The stack, started by the app.** When the stack's front door is not
  answering and the gate is on, the app starts the stack in Docker, opening Docker Desktop first
  when it is not running. The first start downloads the stack, which takes a
  few minutes. A dot at the foot of the side shows the stack, as another
  shows the engine, and its tip says where it answers, or why it is not
  running. As with the engine, it starts only what is missing, and when the
  app quits it stops only a stack it started. The app does not install
  Docker; the chat does not need the stack yet.
- **A first-run picker.** When none of the app's models is on this computer,
  the page shows a box for each maker, Microsoft ticked, and lists the models
  of the makers ticked with their download sizes, marking the quickest to
  start. With the gate on, Download brings the one you pick from Hugging
  Face, with a progress bar and a Cancel button, and the app then starts the
  engine with it. With it off, the picker offers to turn it on.
- **Saved chats**, one JSON file each in the app's data folder. Trash moves a
  file into a `trash` folder; the app deletes nothing.
- **A context meter** that shows how full the model's window is. When
  several models answer, it shows the fullest window and names its model.
- **A notice of each new version.** When the window opens, once a day while
  it stays open, and when you click Check for updates under the saved chats,
  the app asks GitHub for the newest release. When it is
  newer than this copy and holds this system's file, a notice above the chat
  says so. Where this copy came from the `.dmg`, the `.exe`, the `.deb`, or
  the `.rpm`, Install and restart downloads the new version, checks its
  signature, puts it in place of this copy, and opens it. Download opens
  that file in the browser and then says how to install it; What changed
  opens the release's notes; Later hides the notice
  until the app opens again, a newer version is out, or you click Check for
  updates. Check for updates says what it found, or why the check failed;
  while the gate is off, it says to turn the gate on.

## How it is put together

```
page (Vue)  --IPC-->  Rust  --HTTP, loopback-->  on-device engine
                      Rust  --LAN gate--HTTP, configured private address-->  LAN engine
                      Rust  --WAN gate--HTTPS, on model download-->  Hugging Face
                      Rust  --WAN gate--Docker, when the stack is not answering-->  the stack
                      Rust  --WAN gate--HTTPS, on update checks and installs-->  GitHub
                      Rust  --WAN gate--HTTPS, with the key from your file-->  xAI, OpenAI, Anthropic, or Google
```

- [src-tauri/src/gate.rs](src-tauri/src/gate.rs) keeps both switches, provides
  HTTPS clients for WAN requests, and cancels LAN requests when LAN is
  turned off.

- [src-tauri/src/engine.rs](src-tauri/src/engine.rs), the engine adapter. It
  speaks one contract, an OpenAI-style server (`GET /v1/models`,
  `POST /v1/chat/completions` with a streamed reply). It accepts loopback
  and RFC 1918 IPv4 addresses, filters the models, and separates thinking
  from the answer (channel markers, think tags, or a reasoning field).
- [src-tauri/src/models.rs](src-tauri/src/models.rs), the search for the
  app's models on the disk. It works with the engine off, and it changes
  nothing in the folder it reads.
- [src-tauri/src/starter.rs](src-tauri/src/starter.rs), the engine's start.
  It checks the whole plan before anything starts: the engine's program, a
  model for it to serve, the port, and a place for its log. The engine it
  starts listens on this computer only, and is told to stay offline, so it
  downloads nothing.
- [src-tauri/src/downloads.rs](src-tauri/src/downloads.rs), the models'
  download. It brings only a model on the list, at a pinned revision, and
  only from Hugging Face over https, and it checks each file's size and
  SHA-256 as it arrives. A finished model moves into the models folder
  whole. [src-tauri/src/remote.rs](src-tauri/src/remote.rs) delegates LAN
  installs to llama-server and tracks when the model is ready.
- [src-tauri/src/stack.rs](src-tauri/src/stack.rs), the stack's start. It
  starts [src-tauri/stack-compose.yaml](src-tauri/stack-compose.yaml), the
  same stack as `make up`, with the API and the worker pulled from GitHub's
  container registry, pinned by digest. Only its front door has a port,
  4700, on this computer only. That is the port `make up` uses too, so one
  of the two runs at a time, and when either answers, the app starts
  nothing.
- [src-tauri/src/makers.rs](src-tauri/src/makers.rs), the makers picked.
  It keeps them in the app's data folder, accepts only makers on the list,
  always keeps one, and starts with Microsoft and the maker of any model
  already on this computer. The models shown, on the disk, and offered for
  download are only those of the makers picked.
- [src-tauri/src/deactivated.rs](src-tauri/src/deactivated.rs), the models
  switched off. It keeps their ids in the app's data folder, and only ids of
  models the app could offer.
- [src-tauri/src/uninstall.rs](src-tauri/src/uninstall.rs), Uninstall. It
  moves one model's folder or file from the folder the engine reads to the
  Trash, and deletes nothing. A model not on the list, or a name that steps
  out of the folder, is refused.
- [src-tauri/src/outside.rs](src-tauri/src/outside.rs), the engines outside
  this computer: Grok, by xAI, GPT, by OpenAI, Claude, by Anthropic, and
  Gemini, by Google. It
  keeps the path to each of the person's keys in the app's data folder,
  never a key. It reads a key from its file only when it asks that engine's
  maker, after the gate says yes, and sends it only to that maker's API, at
  an address fixed in the code, over https, with no proxy and no redirect.
  The page gets the paths and the models, never a key.
- [src-tauri/src/sessions.rs](src-tauri/src/sessions.rs), the saved chats.
- [src-tauri/src/board.rs](src-tauri/src/board.rs), the boards the Workflow
  view shows. It reads and writes only the app's own `boards` folder, takes
  no path from the page, asks nothing of the network, and deletes nothing.
- [src-tauri/src/update.rs](src-tauri/src/update.rs), the notice of a new
  version. It asks only GitHub's API, only for this repository's newest
  release, and picks this system's file. The page names only Download or
  What changed, never an address, and Rust opens only this repository's
  releases on github.com, in the browser. Install and restart fetches the
  release's `latest.json` and this system's file through the gate, and
  Tauri's updater installs the file only when its signature matches the
  public key in `tauri.conf.json`.

Which program serves the contract is the only thing that changes per system:

| System  | Engine                                             |
| ------- | -------------------------------------------------- |
| macOS   | `mlx_lm.server` (MLX, Apple silicon)               |
| Windows | `llama-server`, installed on the computer          |
| Linux   | `llama-server`, carried in the packages            |

The engine's address defaults to `http://127.0.0.1:8080/v1`. Set
`RUSTY_ENGINE_URL` to use another port on this computer or an RFC 1918
IPv4 address on your private network.

The Mac app that `make app` and `make dmg` build carries its engine inside
it, in `Contents/Resources/engine`, and looks there first. How it is built
is in [engine](engine). Run any other way, the app does not install the
engine: it looks for the engine's program in each folder on `PATH`, then in
`~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`, and
`~/Library/Python/<version>/bin`. An app opened from the Finder gets a short
`PATH`, which is why the usual folders are looked in as well. When the
program is somewhere else, set `RUSTY_ENGINE_PROGRAM` to its full path, and
that program is the only one tried, even in an app that carries its own.

## Install models on a LAN server

Run a [llama-server build with the model management API](https://github.com/ggml-org/llama.cpp/pull/23976)
in router mode on the server. Start it without `--model` or `--hf-repo`,
bind it to your private interface, and allow it to reach Hugging Face.
For example, on the server at `192.168.1.50`:

```sh
llama-server --host 192.168.1.50 --port 8080
```

On the Mac, run this from `apps/desktop` after installing the desktop
dependencies:

```sh
RUSTY_ENGINE_URL=http://192.168.1.50:8080/v1 npm run tauri dev
```

For a packaged build of this version installed in Applications:

```sh
RUSTY_ENGINE_URL=http://192.168.1.50:8080/v1 \
  /Applications/Rusty\ Business.app/Contents/MacOS/rusty-business
```

Open **Installers**, turn on the **LAN gate**, and select **Install**.
llama-server fetches the selected GGUF into its own cache. The app shows
its progress and refreshes the picker when the server lists the model.
**Pause** requests a pause on the server, and **Install** resumes any partial
download the server retained. Use **Pause** before turning off LAN if you
want the server to pause. A server-side download can continue when the app
closes, loses its connection, or turns the LAN gate off. Discovery, chat,
and installs through the server work with the WAN gate off.

The server chooses the file from the catalog's Hugging Face repository and
filename tag. It controls the cache, network access, and download validation.
To keep inference offline, give the inference process no internet access and
run a separate downloader with internet access and a shared model cache.
If your server supports chat only, you can install a catalog GGUF on the
server and refresh the app after llama-server lists it. Use this HTTP
connection on a trusted private network.

## Boards

The Workflow view shows a board: releases, the stories and bugs under each,
and the slices each story is cut into. A board is one JSON file in a
`boards` folder, beside the `sessions` folder:

- macOS: `~/Library/Application Support/net.morrical.rusty-business/boards`
- Windows: `%APPDATA%\net.morrical.rusty-business\boards`
- Linux: `~/.local/share/net.morrical.rusty-business/boards`

A board gets there when you import its file with the `board` command
below, when you put its file in that folder, or when you open the sample
board, which is made up. The app does not reach the network for one. The
file is named `<id>.json`, and its name is the board's id: up to 64 letters,
digits, and hyphens. The view lists every board in the folder. A file it
cannot read is listed with what is wrong, and left as it is.

The file's ruleset is [BOARD.md](../../BOARD.md), at the root of the
repository: what the file holds, each rule, and why the rule is there. In
short:

```json
{
  "meta": {
    "format": "board",
    "version": "2.0",
    "id": "list-app",
    "name": "A list app",
    "rev": "export-12",
    "history": ["export-11", "export-10"],
    "statuses": {
      "stories": [
        { "id": "building", "name": "Building", "finished": false },
        { "id": "done", "name": "Done", "finished": true }
      ]
    }
  },
  "data": {
    "releases": [
      {
        "release": "1.2.0",
        "title": "Lists that sort",
        "detail": "",
        "status": "Building",
        "stories": [
          {
            "number": 412,
            "kind": "story",
            "title": "Sortable column headers",
            "detail": "Click a column's header to sort the list by it.",
            "status": "Building",
            "statusId": "building",
            "notes": ["Numbers sort as numbers, not as text."],
            "slices": [
              {
                "id": "a",
                "title": "A sort helper for text and numbers",
                "detail": "Goal: one function that sorts rows by a column.",
                "lane": "written"
              }
            ]
          }
        ]
      }
    ]
  }
}
```

- **`meta`** says what the file is, and **`data`** is the board.
- **`format`** is `board`, and **`version`** is the ruleset's version the
  file follows, as a line and a step: `2.0`. Within a line a file only
  gains, so the app reads every step of lines 1 and 2, keeps a key it does
  not know, and writes it back. A file on a line above 2 is not read. A
  board v1 file, the format before this one, is line 1 and is still read.
- **`statuses`** are the board's own: a list for stories and bugs, and one
  for releases. Each status has an `id` that never changes, a `name`, and
  whether it is `finished`. A row's **`statusId`** names one. The app shows
  that status's name, and the row's `status` as written when the id is not
  on the list.
- **`id`**, on the board, says which board a file kept somewhere else is.
  In the boards folder, the file's name says it.
- **`rev`** names the state a file holds, and **`history`** lists the
  states it came from, the newest first, up to 50. Each is up to 64
  letters, digits, and hyphens. Both are for keeping a board in step with a
  file, below, and a board that is only read needs neither.
- **`number`** is a story's number, from 1 up, and no two stories on a
  board share one. **`kind`** is `story` or `bug`, and a story with no
  `kind` is a story.
- **`lane`** is the last lane a slice has been through: `sliced`,
  `written`, `checked`, `reviewed`, or `main`, in that order. A slice with
  no `lane` has been through none.
- **`move`**, on a story or a slice, says whose move it is: `you`, `model`,
  `check`, or `review`. Leave it out, and the move goes to whoever takes
  the slice into its next lane: you, a local model, the check, a reviewer,
  and you again.
- **`detail`** on a slice is its brief: what to do, in which files, and
  how to tell it is done. **`notes`** are the board's own notes on a story
  or a slice.
- **`id`** on a slice is up to 16 letters, digits, and hyphens, and no two
  slices of a story share one.

A story holds a lane when it has slices and every one of them has been
through that lane. All five lanes make a royal flush: every slice is on
main.

A board is at most 4 MB, with up to 500 releases, 5,000 stories, 100 slices
a story, and 200 notes on a story or a slice. The view lists up to 100
boards.

Your own notes are kept apart from the board, in `drafts/<id>.json` in the
same folder: one note a story, up to 4,000 characters. A new copy of a
board never changes or removes your notes. A note on a story that has left
the board stays, and the view lists it under No longer on this board.

### A board in step with a file

A board can have one file kept somewhere else: a folder you share, a
checkout of a repository, or wherever another program writes its export.
One command, run in a terminal, keeps that file and this computer's copy in
step, either way:

```sh
rusty-business board import <file>       # a first copy here, from a file
rusty-business board export <id> <file>  # a first file, from the copy here
rusty-business board sync                # pull, then push
rusty-business board pull
rusty-business board push
rusty-business board list
```

On a Mac the program is inside the app, at
`/Applications/Rusty\ Business.app/Contents/MacOS/rusty-business`. From
this repository, it is `cargo run -- board list` in `apps/desktop/src-tauri`.

- **`import`** reads a board file, keeps a copy of it here, and remembers
  where the file is. **`export`** writes a board that is here to a new
  file, and remembers that. After either, `pull`, `push`, and `sync` need no
  path.
- **`pull`** brings the file's changes here, and never writes the file.
  **`push`** writes what is here to the file, and never writes the copy
  here. **`sync`** pulls, then pushes.
- With no id, `pull`, `push`, and `sync` do every board that has a file.
  With one, as in `board sync list-app`, they do that board.
- It leaves with 0 when it is done, 1 when it refused or failed, and 2 when
  there are conflicts to settle.

The app keeps each board as it stood at the last sync, and compares both
sides against that. Changes to different things are both kept. What one
side added is kept. What one side took away is taken away, unless the
other side changed it. A release is known by its name, a story by its
number, and a slice by its id within its story.

A conflict is the same thing changed two ways, such as one story's title.
The command lists each conflict, with what is here and what the file has,
and writes nothing. Run it again with `--mine` to keep what is here, or
`--theirs` to keep what the file has.

A push is refused while the file has changes that are not here yet: pull
first, or run `sync`. A push makes a new file, or replaces a file of the
same board, and leaves any other file as it is.

Beside the board and your notes, the boards folder keeps two more files
for each board in step with a file: `synced/<id>.json`, the board as it
stood at the last sync, and `sources/<id>.json`, where its file is.

What it does not do yet:

- Your notes stay on this computer. They are never written to the file.
- The Workflow view cannot change a board yet, so the copy here changes
  only when a pull brings something in, or when you edit its file.
- The view does not show a key the app does not know. The key is kept, and
  written back.
- When both sides moved stories or slices, the order on the side that is
  kept stands, and no conflict is listed for it.
- The command reaches no network. The file is a path on this computer,
  which can be a folder that something else shares or syncs.
- The app shows a board the command changed the next time the view loads
  it.
- On Windows, the command has not been tried, and the installed app may
  print nothing in a terminal.

### A backend of your own

The board file is an open format, and the file is the whole of the join.
Any program that can read and write one can be the other side: a script,
an export from a tracker, or a server of your own. It needs none of this
app's code, and this app does not need to know what it is.
[BOARD.md](../../BOARD.md) is the file's ruleset.

**No direct API connection to a board's backend, for now.** The app holds no
address, no token, and no key for one, and a change that adds one is
refused. A board moves as a file: exported on one side, and imported on the
other. BOARD.md says why.

A backend keeps the `rev` and `history` of the last file it exported or
imported, and whether its board changed since.

- **To export**, when its board changed since that file, or it has never
  exported, it gives the board a new `rev`, one it has never used, and
  writes `history` as the last `rev` followed by the last `history`, at
  most 50. When nothing changed, it writes the same `rev` and `history`
  again.
- **To import**, when its own board changed since that file, it refuses:
  export first, run `board sync` here, then import. When the file's `rev`
  is the last one, there is nothing to do. When the last `rev` is in the
  file's `history`, it takes the file whole, and the file's `rev` and
  `history` become its last. Otherwise it refuses, because the file does
  not come from what it holds.
- A release keeps its name, a story its number, and a slice its id, so
  each can be found again.

The merging is done here, by `board sync`. A backend that changes its
board before it imports a file pushed from here has gone its own way: when
it next exports, nothing here can say what each side changed, so nothing
is taken away, and every difference is listed as a conflict to settle for
one side. A backend that also keeps the last file's content can do its own
merge by the rules above, and avoid that.

BOARD.md has the rest of what a backend may do: apply part of a file and
list what it did not, leave an over-long text out of an export, and keep
rows of its own out of a file.

A test in `cli.rs` runs a backend written by hand, this way, through each
of these cases.

## What is in this folder

| Path                   | What it is                                              |
| ---------------------- | ------------------------------------------------------- |
| [src](src)             | The page: the window, the Workflow view, and their helpers |
| [src-tauri](src-tauri) | The Rust shell: the window, the engine adapter, downloads, the makers picked, the models switched off, Uninstall, the stack's start, saved chats, boards |
| [engine](engine)       | The engine the Mac app carries: the one package asked for, and every package under it, pinned by hash |
| `index.html`           | The page's single HTML file                             |
| `vite.config.js`       | Vite's settings; the dev server stays on 127.0.0.1:1420 |
| `package.json`         | The page's packages and the `npm` commands below        |

## Run it (macOS)

From the repository's root, one command builds the app with its engine
inside, and opens it:

```bash
make app
```

It needs a Mac with Apple silicon, macOS 14 or later, and Apple's command
line tools. It fetches its own Rust, Node, and Python into
`src-tauri/target/mac`, which git ignores, and installs nothing anywhere
else. `make dmg` packs the same app as a `.dmg`, with its checksum beside
it, in that folder. Both run [scripts/mac-app.sh](../../scripts/mac-app.sh).

To work on the app, with the page reloading as you edit it, you need Rust,
Node 20 or later, Xcode's command line tools, and the engine,
`mlx_lm.server`. The stack also needs Docker; the chat works without it.

From this folder:

```bash
npm install
```

```bash
npm run tauri dev
```

When no engine is answering, the app starts one. To start one yourself, in
another terminal:

```bash
mlx_lm.server --port 8080
```

## Tests

From the repository's root, `make test-desktop` runs all of these:

```bash
npm test
```

```bash
npm run build
```

```bash
cd src-tauri && cargo fmt --check
```

```bash
cd src-tauri && cargo clippy --all-targets --locked -- -D warnings
```

```bash
cd src-tauri && cargo test --locked
```

`npm test` runs every `src/*.test.js`. The page is built before the Rust
checks because Tauri reads the built page (`dist`) when it compiles.

Eleven more tests are run by hand, because each needs an engine, a model,
Docker, a key, or the internet. Six need only this computer and the
internet. One asks the running engine for 2+2, one looks for the app's
models on this computer, one starts a second engine on a free port and
stops it again, one starts a stack of its own on a free port and removes it
again (it needs Docker running), one checks the small pinned files against
Hugging Face, and one asks GitHub for the newest release:

```bash
cd src-tauri && cargo test -- --ignored --skip a_whole_model_downloads_from_hugging_face --skip answers_two_plus_two
```

Four ask an outside engine for 2+2, each with the key in a file you name in
`RUSTY_XAI_KEY_FILE`, `RUSTY_OPENAI_KEY_FILE`, `RUSTY_ANTHROPIC_KEY_FILE`,
or `RUSTY_GOOGLE_KEY_FILE`. Run one at a time, for example:

```bash
cd src-tauri && RUSTY_ANTHROPIC_KEY_FILE=<the key file> cargo test -- --ignored claude_answers_two_plus_two
```

The last downloads Phi-4 whole, 8.3 GB, into an empty folder you name:

```bash
cd src-tauri && RUSTY_DOWNLOAD_TO=<an empty folder> cargo test -- --ignored a_whole_model_downloads_from_hugging_face
```

## Where chats are kept

- macOS: `~/Library/Application Support/net.morrical.rusty-business/sessions`
- Windows: `%APPDATA%\net.morrical.rusty-business\sessions`
- Linux: `~/.local/share/net.morrical.rusty-business/sessions`

## Where the engine's log is kept

An engine the app starts writes its log to `engine.log`, beside the
`sessions` folder. The file holds the latest start only, and only this
computer's user can read it. The engine is never asked to log what is typed.

## Where the stack's settings and log are kept

The app keeps the stack's files in a `stack` folder beside `sessions`: its
compose file, the front door's Caddyfile, and `stack.env`, which holds the
passwords made on the first start. Only this computer's user can read
`stack.env`. What Docker said during the latest start is in `stack.log`,
beside the folder. The stack's data stays in Docker volumes named
`rusty-business-app_*`, which the app never removes.
