# Rusty Business

A desktop window for chatting with models that run on your own computer. The
shell is Rust (Tauri 2), the page is Vue 3, and nothing you type leaves the
machine.

## What it does

- **Chat** with a streamed reply, a Stop button, and the model's thinking kept
  apart from its answer.
- **Model picker** limited to a short list: Phi-4 (Microsoft), gpt-oss 20B
  (OpenAI), and Nemotron 3.5 Lightning (NVIDIA). A model the engine holds that
  is not on the list is never shown and never accepted.
- **Saved chats**, one JSON file each in the app's data folder. Trash moves a
  file into a `trash` folder; the app deletes nothing.
- **A context meter** that shows how full the model's window is.

## How it is put together

```
page (Vue)  --IPC-->  Rust  --http, this computer only-->  engine
```

The page never talks to the network. Rust owns two things:

- `src-tauri/src/engine.rs`, the engine adapter. It speaks one contract, an
  OpenAI-style server (`GET /v1/models`, `POST /v1/chat/completions` with a
  streamed reply). It refuses any address that is not on this computer,
  filters the models, and separates thinking from the answer (channel
  markers, think tags, or a separate reasoning field).
- `src-tauri/src/sessions.rs`, the saved chats.

Which program serves the contract is the only thing that changes per system:

| System  | Engine                                             |
| ------- | -------------------------------------------------- |
| macOS   | `mlx_lm.server` (MLX, Apple silicon)               |
| Windows | not chosen yet; any server with the same two calls |

The engine's address defaults to `http://127.0.0.1:8080/v1`. Set
`RUSTY_ENGINE_URL` to use another port on this computer.

## Run it (macOS)

You need Rust, Node 20 or later, and Xcode's command line tools.

Start the engine in one terminal:

```bash
mlx_lm.server --port 8080
```

Start the app in another:

```bash
npm install
npm run tauri dev
```

## Tests

```bash
npm test
cd src-tauri && cargo test
```

One more test talks to the running engine and asks it for 2+2:

```bash
cd src-tauri && cargo test -- --ignored
```

## Where chats are kept

- macOS: `~/Library/Application Support/net.morrical.rusty-business/sessions`
- Windows: `%APPDATA%\net.morrical.rusty-business\sessions`
