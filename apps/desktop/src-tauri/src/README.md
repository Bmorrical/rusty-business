# apps/desktop/src-tauri/src

The desktop app's Rust code.

| File          | What it is                                                           |
| ------------- | -------------------------------------------------------------------- |
| `main.rs`     | The program's entry. It only calls `run`                             |
| `lib.rs`      | Builds the Tauri app and lists every command the page may call       |
| `engine.rs`   | The engine adapter: finds the model engine on this computer, filters its models to the allowed list, streams a chat, and separates thinking from the answer |
| `models.rs`   | The models already on this computer: reads the Hugging Face cache folder and reports the app's models that are whole there |
| `starter.rs`  | The engine's start: when no engine answers, finds the engine's program on this computer and starts it |
| `sessions.rs` | Saved chats: one JSON file each. Trash moves a file; nothing deletes |

Three rules live in `engine.rs`, each with tests:

- Prompts never leave the computer. Only loopback addresses are accepted,
  with no proxy and no redirects.
- Only models on the list in that file are offered or accepted.
- A model's thinking never reaches the window as raw markers.

Three more live in `models.rs`, each with tests:

- It only reads. Nothing in the cache folder is written, moved, or removed.
- The app keeps no list of its own. The disk is asked every time.
- Only models on the list are reported, and only when whole: a download that
  stopped part way is not counted.

Four more live in `starter.rs`, each with tests:

- The whole plan is checked before anything starts: the program, a model for
  it to serve, the port, and a place for its log.
- Only what is missing is started. An engine that answers is left alone, and
  so is anything else that holds the engine's port.
- Asking twice changes nothing. There is one engine at most.
- The app stops only an engine it started, and only when the app quits.

An engine the app starts listens on this computer only, is told to stay
offline, and is never asked to log what is typed.

A new command is written here, added to the list in `lib.rs`, and called from
the page with `invoke`.
