# apps/desktop/src-tauri/capabilities

What the page is allowed to ask Tauri for. Tauri reads every `.json` and
`.toml` file in this folder when it builds.

| File           | What it grants                                                    |
| -------------- | ----------------------------------------------------------------- |
| `default.json` | The main window gets Tauri's `core:default` set and nothing more  |

That means the page has no file system access, no shell, and no network
plugin. It reaches the engine and the saved chats only through the app's own
commands in [../src](../src).

Add a permission here only when a feature needs it, and name the feature in
the same pull request.
