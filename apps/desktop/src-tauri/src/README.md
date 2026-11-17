# apps/desktop/src-tauri/src

The desktop app's Rust code.

| File          | What it is                                                           |
| ------------- | -------------------------------------------------------------------- |
| `main.rs`     | The program's entry. It only calls `run`                             |
| `lib.rs`      | Builds the Tauri app and lists every command the page may call       |
| `engine.rs`   | The engine adapter: finds the model engine on this computer, filters its models to the allowed list, streams a chat, and separates thinking from the answer |
| `models.rs`   | The models already on this computer: reads the Hugging Face cache folder and reports the app's models that are whole there |
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

A new command is written here, added to the list in `lib.rs`, and called from
the page with `invoke`.
