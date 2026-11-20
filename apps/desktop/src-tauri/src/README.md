# apps/desktop/src-tauri/src

The desktop app's Rust code.

| File          | What it is                                                           |
| ------------- | -------------------------------------------------------------------- |
| `main.rs`     | The program's entry. It only calls `run`                             |
| `lib.rs`      | Builds the Tauri app and lists every command the page may call       |
| `gate.rs`     | The outbound gate: the switch, off until the person turns it on, and every client that reaches past this computer |
| `engine.rs`   | The engine adapter: finds the model engine on this computer, filters its models to the allowed list, streams a chat, and separates thinking from the answer |
| `models.rs`   | The models already on this computer: reads the folder the engine reads, the Hugging Face cache on a Mac or the app's own models folder on Windows and Linux, and reports the app's models that are whole there |
| `starter.rs`  | The engine's start: when no engine answers, finds the engine's program on this computer and starts it, `mlx_lm.server` on a Mac and llama.cpp's `llama-server` on Windows and Linux |
| `downloads.rs` | The models' download: on a first run, brings the model picked from Hugging Face into the models folder the engine reads |
| `makers.rs`   | The model makers picked: Microsoft until the person picks others, and always one |
| `deactivated.rs` | The models switched off: left out of questions, grading, and comparisons, with their files kept |
| `uninstall.rs` | Uninstall: moves a model's folder or file to the Trash, never deletes |
| `outside.rs`  | The engines outside this computer, Grok, GPT, Claude, and Gemini: keeps the path to each of the person's keys, never a key, and asks xAI's, OpenAI's, Anthropic's, or Google's API through the gate |
| `stack.rs`    | The stack's start: when its front door does not answer, opens Docker Desktop if it is not running, then starts the stack in `../stack-compose.yaml` |
| `sessions.rs` | Saved chats: one JSON file each, a grade kept apart from the answers. Trash moves a file; nothing deletes |
| `update.rs`   | The notice of a new version: asks GitHub for the newest release, opens this system's file in the browser, and installs it when its signature matches |

Three rules live in `gate.rs`, each with tests:

- The gate is off until the person turns it on, and it stays as they left
  it. Anything but a kept "on" reads as off.
- Every connection past this computer is made there, and only while the gate
  is on, over https, with no proxy. The one other client in the app speaks
  to the model engine on this computer.
- Turning the gate off stops what is under way: the check for a new version,
  a model download, a start of the stack, and an outside engine's answer.

Three more live in `engine.rs`, each with tests:

- Prompts never leave the computer. Only loopback addresses are accepted,
  with no proxy and no redirects.
- Only models on the list in that file are offered or accepted.
- A model's thinking never reaches the window as raw markers.

Three more live in `models.rs`, each with tests:

- It only reads. Nothing in the models folder is written, moved, or removed.
  Uninstall, in `uninstall.rs`, is the one way a model leaves it.
- The app keeps no list of its own. The disk is asked every time.
- Only models on the list are reported, and only when whole: a download that
  stopped part way, or a GGUF model missing one of its parts, is not counted.
  A projector or a draft model alone is not a model.

Five more live in `starter.rs`, each with tests:

- The whole plan is checked before anything starts: the program, a model for
  it to serve, the port, and a place for its log.
- Only what is missing is started. An engine that answers is left alone, and
  so is anything else that holds the engine's port.
- Asking twice changes nothing. There is one engine at most.
- The app stops only an engine it started, and only when the app quits. It
  asks the engine to stop first, and stops whatever the engine started with it.
- `llama-server` is started on 127.0.0.1, offline, one model at a time, with
  the app's models folder, no chat page of its own, and none of its settings
  taken from the app's own.

Six more live in `downloads.rs`, each with tests:

- Only a model on the list is downloaded, at the revision pinned in
  `../pinned-models.json` on a Mac, or in `../pinned-gguf-models.json` on
  Windows and Linux, and only the files listed there.
- Only from Hugging Face, over https. A redirect anywhere else is refused,
  and no proxy, account, token, or cookie is used.
- Every file's size and SHA-256 are checked as it arrives, and a mismatch
  throws the whole download away.
- The models folder is only added to: a finished model moves in whole, and
  only where there is no folder for it yet.
- One download at a time, and Cancel stops it and leaves nothing behind.
- Once a model is whole on Windows or Linux, `llama-server` is asked to read
  its models folder again, so the new model is listed without a restart.

Two more live in `deactivated.rs`, each with tests:

- A model switched off stays off, the next time the app opens too, until
  the person switches it on. Its files stay where they are.
- Only a model the app could offer is kept off: one on the list, or one of
  an outside engine's. Anything else is passed over, and a switch that
  cannot be kept changes nothing.

Three more live in `uninstall.rs`, each with tests:

- Nothing deletes. A model's folder, or its one GGUF file, goes to the
  Trash (the Recycle Bin on Windows), where the person can take it back.
  On a Mac, the file manager moves it, so the app never asks to control
  Finder.
- Only a model on the list, only from the folder the engine reads, and only
  that model's own folder or file. A name that steps out of the folder is
  refused, and nothing else is touched.
- Nothing moves while a model downloads, and a move the Trash refuses leaves
  the model where it was. A model moved is switched back on, so it is not
  kept off if it comes back.

Four more live in `stack.rs`, each with tests:

- The whole plan is checked before anything starts: this system, the port,
  Docker, and a place for the settings and the log.
- Only what is missing is started. A front door that answers is left alone,
  and so is anything else that holds its port.
- Asking twice changes nothing. There is one start at a time.
- The app stops only a stack it started, and only when the app quits.
  Stopping keeps every container and its data. Nothing removes a volume or
  an image, and Docker Desktop is left running.

Seven more live in `update.rs`, each with tests:

- Only GitHub's API is asked, over https, and only for this repository's
  newest release. No redirect is followed, no proxy, account, token, or
  cookie is used, and the answer is read up to a size and no further.
- A release is a new version only when its tag is a plain version newer than
  the app's, it is neither a draft nor a pre-release, and it holds this
  system's file.
- Each system gets its own file: the `.dmg`, the `.exe`, or, on Linux, the
  `.deb`, the `.rpm`, or the `.tar.gz`, by where the program is installed.
- The page names only the download or the notes, never an address. Rust
  opens only this repository's releases on github.com, in the browser.
- A check that fails keeps what the last one found. Check for updates says
  why it failed; the checks at open and once a day show nothing.
- Install and restart downloads nothing until it is clicked, and only while
  the gate is on; turning the gate off stops its download. It fetches only
  the found release's `latest.json` and this system's file, follows a
  redirect only to GitHub's own file hosts, reads each up to a size, and
  runs one install at a time.
- A file is installed only when its signature matches the public key in
  `tauri.conf.json`, and `latest.json` names the version found and that
  file. It installs only over a copy put in place from the `.dmg`, the
  `.exe`, the `.deb`, or the `.rpm`; any other copy keeps Download. When it
  fails, it says why and that Download still works.

Five more live in `outside.rs`, each with tests, for Grok, GPT, Claude, and
Gemini alike:

- A key never leaves Rust. The app keeps only the path to its file, and
  the page, a log, an error, and a saved chat never hold a key. A key
  pasted where the path goes is refused and not kept.
- With the gate off, no outside engine is asked and no key is even read.
  Turning the gate off stops an engine part way through, whether it is
  finding models or answering.
- Each engine is asked only at its maker's API, xAI's for Grok, OpenAI's
  for GPT, Anthropic's for Claude, and Google's for Gemini, at an address
  fixed in the code,
  over https, with no proxy and no redirect, and an answer from anywhere
  else is refused.
- Only the models an engine's own list gave are asked of it, and a model
  off that list, or from another engine, is refused.
- Each engine keeps its own path and models. Remove forgets the path and
  keeps the models found, so saved chats still name them, and leaves the
  other engines as they were. The key file is the person's, and the app never
  changes or removes it.

An engine the app starts listens on this computer only, is told to stay
offline, and is never asked to log what is typed. A stack the app starts
has its front door on this computer only, and its passwords are made on its
first start and kept where only this computer's user can read them.

A new command is written here, added to the list in `lib.rs`, and called from
the page with `invoke`.
