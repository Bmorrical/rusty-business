# The desktop app

A window for chatting with models that run on your own computer. The shell is
Rust (Tauri 2), the page is Vue 3, and nothing you type leaves the machine.

[![A chat with Phi-4 in the dark theme: two questions about rusty nails, each answer with its token count and speed, saved chats on the left, the model picker at the top right, and the context meter under the message box reading 184 of 16,384 tokens](../../docs/screenshots/chat.png)](../../docs/screenshots/chat.png?raw=true)

Select the picture to open it full size. The rest of the set is in
[docs/screenshots](../../docs/screenshots).

## What it does

- **Chat** with a streamed reply, a Stop button, and the model's thinking kept
  apart from its answer.
- **Model picker** limited to a short list: Phi-4 (Microsoft), gpt-oss 20B
  (OpenAI), and Nemotron 3.5 Lightning (NVIDIA). A model the engine holds that
  is not on the list is never shown and never accepted.
- **The models on this computer**, named when the engine is not answering.
  The app looks in the Hugging Face cache folder, where the engine keeps the
  models it has downloaded. It only reads there, it keeps no list of its own,
  and a download that stopped part way is not counted.
- **The engine, started by the app.** When no engine is answering, the app
  starts one and waits for it. It starts only what is missing: an engine that
  is already running is used as it is, and asking twice changes nothing. When
  the app quits, it stops the engine it started and no other. When it cannot
  start one, it says why in one sentence.
- **The stack, started by the app.** When the stack's front door is not
  answering, the app starts the stack in Docker, opening Docker Desktop first
  when it is not running. The first start downloads the stack, which takes a
  few minutes. The footer says where the stack answers, or why it is not
  running. As with the engine, it starts only what is missing, and when the
  app quits it stops only a stack it started. The app does not install
  Docker; the chat does not need the stack yet.
- **A first-run picker.** When none of the app's models is on this computer,
  the page lists them with their download sizes and marks the quickest to
  start. Download brings the one you pick from Hugging Face, with a progress
  bar and a Cancel button, and the app then starts the engine with it.
- **Saved chats**, one JSON file each in the app's data folder. Trash moves a
  file into a `trash` folder; the app deletes nothing.
- **A context meter** that shows how full the model's window is.
- **A notice of each new version.** When the window opens, and once a day
  while it stays open, the app asks GitHub for the newest release. When it is
  newer than this copy and holds this system's file, a notice above the chat
  says so. Download opens that file in the browser and then says how to
  install it; What changed opens the release's notes; Later hides the notice
  until the app opens again or a newer version is out.

## How it is put together

```
page (Vue)  --IPC-->  Rust  --http, this computer only-->  engine
                      Rust  --https, on Download only-->   Hugging Face
                      Rust  --Docker, when the stack is not answering-->  the stack
                      Rust  --https, at open and once a day-->  GitHub's API
```

The page never talks to the network. Rust owns seven things:

- [src-tauri/src/engine.rs](src-tauri/src/engine.rs), the engine adapter. It
  speaks one contract, an OpenAI-style server (`GET /v1/models`,
  `POST /v1/chat/completions` with a streamed reply). It refuses any address
  that is not on this computer, filters the models, and separates thinking
  from the answer (channel markers, think tags, or a separate reasoning
  field).
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
  whole; a cancelled or broken one leaves nothing behind.
- [src-tauri/src/stack.rs](src-tauri/src/stack.rs), the stack's start. It
  starts [src-tauri/stack-compose.yaml](src-tauri/stack-compose.yaml), the
  same stack as `make up`, with the API and the worker pulled from GitHub's
  container registry, pinned by digest. Only its front door has a port,
  4700, on this computer only. That is the port `make up` uses too, so one
  of the two runs at a time, and when either answers, the app starts
  nothing.
- [src-tauri/src/sessions.rs](src-tauri/src/sessions.rs), the saved chats.
- [src-tauri/src/update.rs](src-tauri/src/update.rs), the notice of a new
  version. It asks only GitHub's API, only for this repository's newest
  release, and picks this system's file. The page names only Download or
  What changed, never an address, and Rust opens only this repository's
  releases on github.com, in the browser. The app downloads and installs
  nothing itself.

Which program serves the contract is the only thing that changes per system:

| System  | Engine                                             |
| ------- | -------------------------------------------------- |
| macOS   | `mlx_lm.server` (MLX, Apple silicon)               |
| Windows | not chosen yet; any server with the same two calls |
| Linux   | not chosen yet; any server with the same two calls |

The engine's address defaults to `http://127.0.0.1:8080/v1`. Set
`RUSTY_ENGINE_URL` to use another port on this computer.

The Mac app that `make app` and `make dmg` build carries its engine inside
it, in `Contents/Resources/engine`, and looks there first. How it is built
is in [engine](engine). Run any other way, the app does not install the
engine: it looks for the engine's program in each folder on `PATH`, then in
`~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`, and
`~/Library/Python/<version>/bin`. An app opened from the Finder gets a short
`PATH`, which is why the usual folders are looked in as well. When the
program is somewhere else, set `RUSTY_ENGINE_PROGRAM` to its full path, and
that program is the only one tried, even in an app that carries its own.

## What is in this folder

| Path                   | What it is                                              |
| ---------------------- | ------------------------------------------------------- |
| [src](src)             | The page: one Vue component and its helpers             |
| [src-tauri](src-tauri) | The Rust shell: the window, the engine adapter, downloads, the stack's start, saved chats |
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
cd src-tauri && cargo test
```

The page is built before the Rust tests because Tauri reads the built page
(`dist`) when it compiles.

Five more tests are run by hand. One asks the running engine for 2+2, one
looks for the app's models on this computer, one starts a second engine on
a free port and stops it again, one starts a stack of its own on a free port
and removes it again (it needs Docker running), and one checks the small
pinned files against Hugging Face:

```bash
cd src-tauri && cargo test -- --ignored --skip a_whole_model_downloads_from_hugging_face
```

A sixth downloads Phi-4 whole, 8.3 GB, into an empty folder you name:

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
