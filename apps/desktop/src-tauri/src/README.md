# apps/desktop/src-tauri/src

The desktop app's Rust code.

| File          | What it is                                                           |
| ------------- | -------------------------------------------------------------------- |
| `main.rs`     | The program's entry. It only calls `run`                             |
| `lib.rs`      | Builds the Tauri app and lists every command the page may call       |
| `engine.rs`   | The engine adapter: finds the model engine on this computer, filters its models to the allowed list, streams a chat, and separates thinking from the answer |
| `sessions.rs` | Saved chats: one JSON file each. Trash moves a file; nothing deletes |

Three rules live in `engine.rs`, each with tests:

- Prompts never leave the computer. Only loopback addresses are accepted,
  with no proxy and no redirects.
- Only models on the list in that file are offered or accepted.
- A model's thinking never reaches the window as raw markers.

A new command is written here, added to the list in `lib.rs`, and called from
the page with `invoke`.
