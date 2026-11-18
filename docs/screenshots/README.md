# Screenshots

Eight pictures of the chat window. Each is 1,840 pixels wide, taken from a
window 920 points wide, so the words are large enough to read. Select a
picture to open it full size.

## A chat

[![A chat with Phi-4 in the dark theme: two questions about rusty nails, each answer with its token count and speed, saved chats on the left, Phi-4 picked among the models at the top, and the context meter under the message box reading 184 of 16,384 tokens](chat.png)](chat.png?raw=true)

Two questions to Phi-4 and its answers. Under each answer is the model's
name, how many tokens the answer took, and how fast they came. Saved chats
are on the left, the models are at the top with Phi-4 picked, and the meter
under the message box shows how full the model's window is.

## Thinking, apart from the answer

[![A chat with gpt-oss 20B in the dark theme, with the model's thinking opened above its answer about standing on the top step of a stepladder](thinking.png)](thinking.png?raw=true)

gpt-oss 20B thinks before it answers. The app keeps that thinking folded away
above the answer. Here it is opened.

## One question, three models

[![One question put to three models in the light theme: Phi-4, gpt-oss 20B, and Nemotron 3.5 Lightning picked at the top, and their answers side by side under the question, each with its token count and speed, and the meter naming the fullest window](compare.png)](compare.png?raw=true)

Phi-4, gpt-oss 20B, and Nemotron 3.5 Lightning are picked, so each answers
the same question, and the answers sit side by side. The models on this
computer take turns, since the engine holds one at a time. gpt-oss and
Nemotron think first, and their thinking is folded away. The meter shows
the fullest of the three windows, and names its model.

## A new chat

[![A new chat in the light theme: 'Ask anything.' in the middle, Phi-4 picked among the models at the top, the engine line reading MLX on 127.0.0.1:8080, the stack line reading Stack on 127.0.0.1:4700, and the outbound gate off at the foot of the window](new-chat.png)](new-chat.png?raw=true)

What the app shows before the first question. The app follows the system's
light or dark setting; this is light.

## No engine answering

[![The app in the light theme with no engine answering: a red dot, the three models already on this computer, the reason the app did not start the engine, the command that starts one by hand, a Try again button, a green dot for the stack, and the message box turned off](engine-off.png)](engine-off.png?raw=true)

What the app shows when nothing answers at the engine's address and it could
not start an engine itself: which of its models are already on this computer,
why it did not start the engine, the command that starts one by hand, and a
button to try again.

## The first run

[![The first run in the dark theme, with none of the app's models on this computer: Microsoft ticked under Show models from, and OpenAI, NVIDIA, Google, and Meta not, a Turn on the gate button, Microsoft's Phi-4 at 8.3 GB with a Download button that waits for the outbound gate, and a line saying more models, and the outside engines, are on the Installers page](first-run.png)](first-run.png?raw=true)

What the app shows when none of its models is on this computer. Microsoft
is ticked to begin with; ticking another maker adds its models below, each
with its maker, license, and download size, and the smallest marked as the
quickest to start. Microsoft cannot be unticked while it is the only maker
ticked. Nothing downloads until the outbound gate is on and Download is
pressed. More models, and the outside engines, are a click away on the
Installers page.

## The Installers page

[![The Installers page in the dark theme, with the five model makers: Microsoft's Phi-4, OpenAI's gpt-oss 20B, and NVIDIA's Nemotron 3.5 Lightning on this computer, Google's Gemma 4 26B and Meta's Llama 3.1 8B marked Not added, each with its license, its download size, and an Install button that waits for the outbound gate, and Outside engines below](installers.png)](installers.png?raw=true)

The Installers page, opened from the foot of the window above Settings,
lists every model on the app's list, from every maker, with its license and
download size. Install downloads one, once the outbound gate is on, then
adds its maker so the model shows at the top of the window. Here the gate is
off, so Install waits. A model already on this computer whose maker is not
added has a Show button instead. Grok and GPT are added below, under
Outside engines, each on its own card.

## Settings: the model makers

[![Settings in the light theme, with the five model makers: Microsoft, OpenAI, and NVIDIA picked, each with a Remove button and its model on this computer, and Google and Meta not picked, each with an Add button and its model's license and download size](settings.png)](settings.png?raw=true)

Settings, opened from the foot of the window, lists every maker on the app's
list with its models. Add and Remove pick a maker with one click. Here the
makers picked are the three whose models are on this computer, as the app
picks them before you do. Removing one hides its models and deletes nothing,
and saved chats still name them. To download a model, Settings points to
the Installers page.

## How these were made

```bash
make screenshots
```

builds the app's page and runs
[scripts/screenshots.mjs](../../scripts/screenshots.mjs), which opens the page
in Google Chrome with no window on screen and saves each picture here.

These are pictures of the app's page, not captures of the app's window:

- There is no title bar, and the first run's boxes are drawn by Chrome. The
  app uses the system's web view, which draws them its own way.
- Rust is not running. The page's calls to it are answered from
  [chats.json](chats.json): what the app reported about the engine on one
  Mac with every maker picked, the five models it offers with their sizes,
  and replies the three models there gave, recorded once. One of the chats
  puts the same question to all three. Each picture picks
  makers as the app does before you pick any: Microsoft, and the maker of
  each model already on the computer. The words, token counts, and speeds
  are real. Nothing is sent to a model while the pictures are made, and no
  engine is started: in the pictures of no engine and of the first run, the
  page is told why the app did not start one. For the first run, it is also
  told that none of the models is on this computer, and nothing downloads.
  No stack is started either: in every picture, the page is told the stack's
  front door already answers. Grok and GPT are not added, and no key is
  read. The outbound gate is off in every picture, as it is until it is
  turned on, and no maker can be changed.
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
| `compare.png`    | One question answered by three models, side by side            |
| `new-chat.png`   | A new chat, before the first question                          |
| `engine-off.png` | The app with no engine answering                               |
| `first-run.png`  | The first run, with none of the app's models on this computer  |
| `installers.png` | The Installers page, with every maker's models                 |
| `settings.png`   | Settings, with the five model makers                           |
| `chats.json`     | What the pictures are made from: the engine's report, the models offered, and five recorded chats |
