# The desktop app

A window for chatting with models on your own computer or a configured
llama-server on your private network. The shell is Rust (Tauri 2), and the
page is Vue 3. The outbound gate controls internet features.
With it on, Grok, GPT, Claude, and Gemini can answer too, each with your
own key.

[![A chat with Phi-4 in the dark theme: two questions about rusty nails, each answer with its token count and speed, saved chats on the left, Phi-4 picked among the models at the top, and the context meter under the message box reading 184 of 16,384 tokens](../../docs/screenshots/chat.png)](../../docs/screenshots/chat.png?raw=true)

Select the picture to open it full size. The rest of the set is in
[docs/screenshots](../../docs/screenshots).

## What it does

- **The outbound gate**, a switch at the foot of the window. It is off until
  you turn it on, and it stays as you leave it. Turning it off stops active
  internet requests and asks the LAN server to pause a download the app is
  tracking. Discovery and chat work with the configured engine either way.
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
- **An Installers page**, at the foot of the window above Settings, that
  lists every model on the list, from every maker, with its license and
  download size. Install downloads one, with the gate on, then adds its
  maker so it shows at the top of the window. A model already here whose
  maker is not added has a Show button instead.
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
- **The stack, started by the app.** When the stack's front door is not
  answering and the gate is on, the app starts the stack in Docker, opening Docker Desktop first
  when it is not running. The first start downloads the stack, which takes a
  few minutes. The footer says where the stack answers, or why it is not
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
page (Vue)  --IPC-->  Rust  --http, loopback or configured private address-->  engine
                      Rust  --gate--https, on Download only-->   Hugging Face
                      Rust  --gate--Docker, when the stack is not answering-->  the stack
                      Rust  --gate--https, at open, once a day, and on Check for updates-->  GitHub's API
                      Rust  --gate--https, on Install and restart only-->  this repository's releases on GitHub
                      Rust  --gate--https, with the key from your file-->  xAI's API (Grok)
                      Rust  --gate--https, with the key from your file-->  OpenAI's API (GPT)
                      Rust  --gate--https, with the key from your file-->  Anthropic's API (Claude)
                      Rust  --gate--https, with the key from your file-->  Google's API (Gemini)
```

The page reaches the configured engine through Rust. Rust gates internet
features, including model downloads delegated to a private-network engine.

- [src-tauri/src/gate.rs](src-tauri/src/gate.rs), the outbound gate. It
  keeps the switch and provides HTTPS clients for online services. Remote
  installs check it before asking a LAN server to download a model.

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
| Windows | not chosen yet; any server with the same two calls |
| Linux   | not chosen yet; any server with the same two calls |

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

Open **Installers**, turn on the outbound gate, and select **Install**.
llama-server fetches the selected GGUF into its own cache. The app shows
its progress and refreshes the picker when the server lists the model.
**Pause** requests a pause on the server, and **Install** resumes any partial
download the server retained. A server-side download can continue if the
app closes or loses its connection. Chat with the configured LAN server
works with the gate off.

The server chooses the file from the catalog's Hugging Face repository and
filename tag. It controls the cache, network access, and download validation.
If your server supports chat only, you can install a catalog GGUF on the
server and refresh the app after llama-server lists it. Use this HTTP
connection on a trusted private network.

## What is in this folder

| Path                   | What it is                                              |
| ---------------------- | ------------------------------------------------------- |
| [src](src)             | The page: one Vue component and its helpers             |
| [src-tauri](src-tauri) | The Rust shell: the window, the engine adapter, downloads, the makers picked, the models switched off, Uninstall, the stack's start, saved chats |
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
