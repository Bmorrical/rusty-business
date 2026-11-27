# apps/desktop/src

The page the window shows. It is Vue 3, built by Vite, and it never touches
the network: everything it needs, it asks the Rust side for.

| File           | What it is                                                          |
| -------------- | ------------------------------------------------------------------- |
| `main.js`      | Starts Vue and mounts the page                                      |
| `App.vue`      | The whole window: saved chats, the conversation, the models that answer, the message box, the context meter, the first-run picker, and the outbound gate's switch |
| `chat.js`      | Small helpers with no Vue and no Tauri in them: a chat's title, what is sent to the engine for each model, how answers to one question are grouped side by side and which one is open wide, votes on answers and their tally per model, the meter's arithmetic, when to ask for the engine to be started, the first-run picker's sizes and progress, the line under the gate's switch, and the words for a model that is on this computer and not on the engine's list |
| `chat.test.js` | Tests for `chat.js`. They run on Node alone: `npm test`             |
| `Workflow.vue` | The Workflow view: a board's road of releases and stories, the open story with its hand and your note, and the open slice with its brief, its five lanes, and the buttons that move it |
| `board.js`     | Helpers for the Workflow view, with no Vue and no Tauri in them: the five lanes, whose move a slice or a story is, a story's hand, the moves a slice offers, the road's filter and counts, and the line that says when the copy was pulled |
| `board.test.js` | Tests for `board.js`, on the sample board the app carries. They run on Node alone: `npm test` |
| `runners.js`   | Helpers for handing a slice to a model, with no Vue and no Tauri in them: the models a slice can go to, when a slice can be handed off, and the runner's clock and the line it ends with |
| `runners.test.js` | Tests for `runners.js`. They run on Node alone: `npm test` |

Logic that can be tested without a window belongs in `chat.js`, or in
`board.js` or `runners.js` for the Workflow view, with a test beside it. `App.vue` and
`Workflow.vue` keep what needs the screen.
