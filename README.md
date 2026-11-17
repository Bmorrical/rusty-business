# Rusty Business

![Rusty Business: a person wobbling on one foot on the very top of a tipping stepladder, reaching for a hanging light bulb, with two rusty nails on the ground below](docs/rusty-business.svg)

A desktop window for chatting with models that run on your own computer, and
the start of the servers that will sit behind it. Nothing you type leaves the
machine.

## Screenshots

Select a picture to open it full size.

[![A chat with Phi-4 in the dark theme: two questions about rusty nails, each answer with its token count and speed, saved chats on the left, the model picker at the top right, and the context meter under the message box reading 184 of 16,384 tokens](docs/screenshots/chat.png)](docs/screenshots/chat.png?raw=true)

| A model's thinking, apart from its answer | A new chat | No engine answering |
| ----------------------------------------- | ---------- | ------------------- |
| [![A chat with gpt-oss 20B in the dark theme, with the model's thinking opened above its answer about standing on the top step of a stepladder](docs/screenshots/thinking.png)](docs/screenshots/thinking.png?raw=true) | [![A new chat in the light theme: 'Ask anything.' in the middle, Phi-4 chosen in the model picker, and the engine line reading MLX on 127.0.0.1:8080](docs/screenshots/new-chat.png)](docs/screenshots/new-chat.png?raw=true) | [![The app in the light theme with no engine answering: a red dot, the command that starts the engine, a Try again button, and the message box turned off](docs/screenshots/engine-off.png)](docs/screenshots/engine-off.png?raw=true) |

What each picture shows, and how they are made, is in
[docs/screenshots](docs/screenshots).

## What is here

This is one repository for the whole project. Every folder has a README that
says what is in it.

| Folder                               | What it holds                                                 | Today                                                         |
| ------------------------------------ | ------------------------------------------------------------- | ------------------------------------------------------------- |
| [apps/desktop](apps/desktop)         | The chat window: a Rust shell (Tauri 2) around a Vue 3 page   | Works                                                         |
| [services/api](services/api)         | The API (Rust, axum)                                          | Stub: says who it is, and whether Postgres and Redis answer   |
| [services/worker](services/worker)   | The worker (Rust)                                             | Stub: reads each job off the queue and does nothing with it   |
| [packages/shared](packages/shared)   | Code the two services share: settings and the job format      | Works                                                         |
| [infra](infra)                       | The Docker stack: Postgres, Redis, MinIO, Mailpit, and Caddy  | Works                                                         |
| [e2e](e2e)                           | End-to-end tests against the running stack                    | Works                                                         |
| [scripts](scripts)                   | Bootstrap, helpers, and the repository's own checks           | Works                                                         |
| [docs](docs)                         | Pictures the READMEs show, and the [screenshots](docs/screenshots) |                                                          |
| [.github/workflows](.github/workflows) | What GitHub runs: the test gate, and a deploy that waits for it | The gate runs; the deploy is a stub with nothing to deploy yet |

The chat window does not use the stack yet. It talks only to the model engine
on this computer. The stack is the ground the next features stand on.

## Run the chat window (macOS)

You need Rust, Node 20 or later, and Xcode's command line tools.

Start the model engine in one terminal:

```bash
mlx_lm.server --port 8080
```

Start the app in another:

```bash
cd apps/desktop && npm install && npm run tauri dev
```

More in [apps/desktop](apps/desktop).

## Run the stack

You need Docker (with Compose), `make`, and `openssl`.

```bash
make bootstrap
```

That writes `infra/.env` with new random passwords, builds the two services,
starts everything, and waits until every part reports healthy. After the
first run, `make up` and `make down` start and stop it; the data stays.

Everything listens on this computer only (127.0.0.1):

| Address                 | What answers                                |
| ----------------------- | ------------------------------------------- |
| http://127.0.0.1:4700   | The front door (Caddy), which passes to the API |
| http://127.0.0.1:4701   | The API, direct                             |
| http://127.0.0.1:4726   | Mailpit: every email the stack sends        |
| http://127.0.0.1:4791   | MinIO's console (file storage)              |
| 127.0.0.1:4732          | Postgres                                    |
| 127.0.0.1:4779          | Redis                                       |
| 127.0.0.1:4790          | MinIO's S3 address                          |
| 127.0.0.1:4725          | Mailpit's SMTP port                         |

A port that is already taken on your computer can be changed in `infra/.env`.
More in [infra](infra).

## Tests

```bash
make test
```

runs everything that needs no running stack: format, lint, and unit tests for
the services, the repository's own checks, and the desktop app's tests.

With the stack up:

```bash
make test-live
```

```bash
make e2e
```

The first runs the API's tests against the real Postgres and Redis. The second
checks every part of the stack from outside: the front door, the API, the
database migration, a job travelling from the queue to the worker, file
storage, and mail.

On GitHub, the gate runs all of the above once on each push to `main`. Nothing
is released or deployed unless the gate is green. The gate does not run on
pull requests, so run `make test` and `make e2e` before you push. More in
[.github/workflows](.github/workflows).

`make` alone lists every command.

## Roadmap

What is planned, and in what order, is in [ROADMAP.md](ROADMAP.md). It
promises no dates: a thing ships when its tests are green.

## License

Rusty Business is open source under the [MIT License](LICENSE).

Rusty Business™ is a trademark of Brad Morrical. The license covers the code,
not the name: [TRADEMARKS.md](TRADEMARKS.md) says how the name and the artwork
may be used.
