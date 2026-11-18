# Screenshots

Five pictures of the chat window. Each is 1,840 pixels wide, taken from a
window 920 points wide, so the words are large enough to read. Select a
picture to open it full size.

## A chat

[![A chat with Phi-4 in the dark theme: two questions about rusty nails, each answer with its token count and speed, saved chats on the left, the model picker at the top right, and the context meter under the message box reading 184 of 16,384 tokens](chat.png)](chat.png?raw=true)

Two questions to Phi-4 and its answers. Under each answer is the model's
name, how many tokens the answer took, and how fast they came. Saved chats
are on the left, the model picker is at the top right, and the meter under
the message box shows how full the model's window is.

## Thinking, apart from the answer

[![A chat with gpt-oss 20B in the dark theme, with the model's thinking opened above its answer about standing on the top step of a stepladder](thinking.png)](thinking.png?raw=true)

gpt-oss 20B thinks before it answers. The app keeps that thinking folded away
above the answer. Here it is opened.

## A new chat

[![A new chat in the light theme: 'Ask anything.' in the middle, Phi-4 chosen in the model picker, the engine line reading MLX on 127.0.0.1:8080, the stack line reading Stack on 127.0.0.1:4700, and the outbound gate off at the foot of the window](new-chat.png)](new-chat.png?raw=true)

What the app shows before the first question. The app follows the system's
light or dark setting; this is light.

## No engine answering

[![The app in the light theme with no engine answering: a red dot, the three models already on this computer, the reason the app did not start the engine, the command that starts one by hand, a Try again button, a green dot for the stack, and the message box turned off](engine-off.png)](engine-off.png?raw=true)

What the app shows when nothing answers at the engine's address and it could
not start an engine itself: which of its models are already on this computer,
why it did not start the engine, the command that starts one by hand, and a
button to try again.

## The first run

[![The first run in the dark theme, with none of the app's models on this computer: Phi-4 at 8.3 GB, gpt-oss 20B at 12.1 GB, and Nemotron 3.5 Lightning at 17.8 GB, each with a Download button that waits for the outbound gate, a Turn on the gate button above them, and Phi-4 marked as the quickest to start](first-run.png)](first-run.png?raw=true)

What the app shows when none of its models is on this computer: each one it
can download, with its maker and its download size, and the smallest marked
as the quickest to start. Nothing downloads until the outbound gate is on and
Download is pressed.

## How these were made

```bash
make screenshots
```

builds the app's page and runs
[scripts/screenshots.mjs](../../scripts/screenshots.mjs), which opens the page
in Google Chrome with no window on screen and saves each picture here.

These are pictures of the app's page, not captures of the app's window:

- There is no title bar, and the model picker is drawn by Chrome. The app
  uses the system's web view, which draws that control its own way.
- Rust is not running. The page's calls to it are answered from
  [chats.json](chats.json): what the app reported about the engine on one
  Mac, the models it offers with their sizes, and replies the three models
  gave there, recorded once. The words,
  token counts, and speeds are real. Nothing is sent to a model while the
  pictures are made, and no engine is started: in the last two pictures, the
  page is told why the app did not start one. For the first run, it is also
  told that none of the models is on this computer, and nothing downloads.
  No stack is started either: in every picture, the page is told the stack's
  front door already answers. The outbound gate is off in every picture, as it
  is until it is turned on.
- "Today" and "Yesterday" are counted from the day the pictures are made.
- Made on another system, the letters come out in that system's font.

## The first picture

[![The first picture of the Rusty Business™ window, wide and in the dark theme: one saved chat on the left, a long answer from Phi-4 about string theory in the middle, and the model picker at the top right](../chat-window.webp)](../chat-window.webp?raw=true)

The one capture of the app's real window, title bar and all. The window was
wide when it was taken, so its words are small. It is kept here and shown
nowhere else.

## What is in this folder

| File             | What it is                                                     |
| ---------------- | -------------------------------------------------------------- |
| `chat.png`       | A chat with two questions answered                             |
| `thinking.png`   | A chat with the model's thinking opened                        |
| `new-chat.png`   | A new chat, before the first question                          |
| `engine-off.png` | The app with no engine answering                               |
| `first-run.png`  | The first run, with none of the app's models on this computer  |
| `chats.json`     | What the pictures are made from: the engine's report, the models offered, and four recorded chats |
