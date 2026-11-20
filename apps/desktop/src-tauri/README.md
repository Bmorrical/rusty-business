# apps/desktop/src-tauri

The Rust half of the desktop app. Tauri opens the window, loads the page
from [../src](../src), and runs the commands the page calls.

| Path                         | What it is                                               |
| ---------------------------- | -------------------------------------------------------- |
| [src](src)                   | The Rust code: the engine adapter, its start, the models on the disk and their download, the stack's start, the saved chats, and the boards |
| [capabilities](capabilities) | What the page is allowed to ask Tauri for                |
| [icons](icons)               | The app's icon in every size the systems ask for         |
| `tauri.conf.json`            | The window's size and title, the page's content security policy, and what gets bundled |
| `pinned-models.json`         | The app's models as downloads on a Mac, as MLX models: each one's revision on Hugging Face, and every file's name, size, and SHA-256 |
| `pinned-gguf-models.json`    | The same models as GGUF files, for `llama-server` on Windows and Linux, from the maker's own copy first, then ggml-org's, with the same facts |
| `stack-compose.yaml`         | The stack the app starts: [infra/compose.yaml](../../../infra/compose.yaml) with the API and the worker pulled from GitHub's container registry, pinned by digest, and only the front door on a port |
| `sample-board.json`          | The made-up board the Workflow view opens as a sample, compiled into the app |
| `build.rs`                   | Tauri's build step                                       |
| `Cargo.toml`, `Cargo.lock`   | This crate's packages, pinned                            |

This crate is not part of the workspace at the repository's root. It keeps
its own `Cargo.lock`, so a server build never needs the app's packages, and
the app never needs the servers'.

Two folders appear here when you build, and git ignores both: `target`
(compiled output) and `gen/schemas` (files Tauri writes for editors).

## Tests

```bash
cargo test
```

Tauri reads the built page when it compiles, so build the page once first
(`npm run build` in [..](..)) on a fresh checkout.
