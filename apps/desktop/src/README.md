# apps/desktop/src

The page the window shows. It is Vue 3, built by Vite, and it never touches
the network: everything it needs, it asks the Rust side for.

| File           | What it is                                                          |
| -------------- | ------------------------------------------------------------------- |
| `main.js`      | Starts Vue and mounts the page                                      |
| `App.vue`      | The whole window: saved chats, the conversation, the model picker, the message box, and the context meter |
| `chat.js`      | Small helpers with no Vue and no Tauri in them: a chat's title, what is sent to the engine, the meter's arithmetic, when to ask for the engine to be started |
| `chat.test.js` | Tests for `chat.js`. They run on Node alone: `npm test`             |

Logic that can be tested without a window belongs in `chat.js`, with a test
beside it. `App.vue` keeps what needs the screen.
