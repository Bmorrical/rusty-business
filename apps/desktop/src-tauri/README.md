# apps/desktop/src-tauri

The Rust half of the desktop app. Tauri opens the window, loads the page
from [../src](../src), and runs the commands the page calls.

| Path                         | What it is                                               |
| ---------------------------- | -------------------------------------------------------- |
| [src](src)                   | The Rust code: the engine adapter, its start, the models on the disk and their download, and the saved chats |
| [capabilities](capabilities) | What the page is allowed to ask Tauri for                |
| [icons](icons)               | The app's icon in every size the systems ask for         |
| `tauri.conf.json`            | The window's size and title, the page's content security policy, and what gets bundled |
| `pinned-models.json`         | The app's models as downloads: each one's revision on Hugging Face, and every file's name, size, and SHA-256 |
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
