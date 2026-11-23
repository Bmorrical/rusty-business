# Security

Rusty Business™ uses the engine on your computer by default. It can also
send chat to a server you configure on your private network. The LAN gate
controls access to that server. The WAN gate controls the app’s internet
features. If you find a way around those controls or any other security
problem, I want to hear about it.

## Reporting a problem

Report it privately, on GitHub:
[Report a vulnerability](https://github.com/Bmorrical/rusty-business/security/advisories/new).
GitHub keeps the report private: only you and the repository's maintainers
can see it. Please do not open a public issue, pull request, or discussion
for it.

Say what you found, how to make it happen, and what it lets someone do. A
short report is fine.

I read every report and answer on the report itself. This is one person's
project, so there is no set time for an answer, and there is no bounty.

When the fix is in `main`, I publish an advisory on GitHub that says what was
wrong and what fixed it. It credits you by name unless you ask me not to.

## What is covered

The app's version is 0.8.4. Its downloads, for a Mac, for Windows, and for
Linux, are on the newest release on GitHub, built from `main`. Fixes go into `main`,
then into a new release, and only `main` and the newest release are
covered.

Everything in this repository is covered:

- The chat window: its page, and the Rust behind it.
- The API, the worker, and the code they share.
- The Docker stack, the scripts, and the workflows that run on GitHub.

These are the promises that matter most. A way to break one is exactly what
I want to hear about:

- The page has no way to call out.
- Both gates start off and can be turned on independently. Local model
  downloads, update checks and installs, stack startup, and provider
  requests wait for the WAN gate. Turning WAN off stops those actions.
- Discovery, chat, and install, pause, and resume requests to the configured
  private-network server wait for the LAN gate. Turning LAN off closes those
  app connections. A download already delegated to the server may continue
  there. The server operator controls the server's internet access.
- Your xAI, OpenAI, Anthropic, and Google keys stay in the files you name. The app
  keeps only each file's path, reads the key from it when it asks that
  engine, and sends the key only to its maker's API, over https, following
  no redirect: xAI's for Grok and OpenAI's for GPT, each as a bearer token,
  Anthropic's for Claude, in its own `x-api-key` header, and Google's for
  Gemini, in its own `x-goog-api-key` header, never in an address. Each way marks
  the key as sensitive, so nothing prints it. A key never reaches the page,
  a log, or a saved chat, never goes to another maker, and the app never
  changes or removes the file.
- A grade by Grok, GPT, Claude, or Gemini sends its maker the question and every answer to
  it, those from the models on this computer too, and only while the gate
  is on. The grader reads no model's name. Settings picks a model on this
  computer to grade instead, or none.
- Engine requests use HTTP with no redirects or proxy. The address must be
  loopback or an RFC 1918 IPv4 address. Engines the app starts bind to
  loopback only.
- For downloads onto this computer, the app fetches catalog models from
  Hugging Face over HTTPS and verifies each file's pinned size and checksum.
  For a LAN install, the app sends a catalog model identifier and
  llama-server handles the transfer and file validation.
- The check for a new version asks GitHub's API for this repository's newest
  release, over https, and nothing else. It sends nothing you type, follows
  no redirect, and downloads nothing. Its Download button opens only this
  repository's release files on github.com, in your browser.
- Install and restart downloads nothing until you click it, and only while
  the outbound gate is on. Then it fetches two files of the release the
  check found, from this repository's releases on github.com: `latest.json`
  and this system's file. It follows a redirect only to GitHub's own file
  hosts, over https, sends no account, token, or cookie, and reads each file
  up to a size. It installs that file only when its signature matches the
  public key built into the app, and `latest.json` names that version and
  that file. Otherwise it installs nothing and says why.
- A web search goes out only once you switch it on in Settings, where it
  starts off, and only while the outbound gate is on. It goes over https to
  one of two fixed addresses: Brave Search's API, `api.search.brave.com`,
  when you gave a key file, and DuckDuckGo's results page,
  `html.duckduckgo.com`, when you did not. No setting changes either
  address, no redirect is followed, and turning the gate off stops a search
  under way. A search carries the words the model wrote, which can come
  from your question, the app's name and version, and, to Brave only, your
  key. The app keeps the path to your key file, never the key, and the key
  never reaches the page. What comes back is cut to at most 8 results of
  plain text, with every tag taken out, before the model reads it.
- The Workflow view reads only the boards kept on this computer, in the
  app's own data folder. It asks nothing of the network and works with both
  gates off. No command it uses takes a path, a new copy of a board never
  changes or removes your notes, and nothing in the boards folder is
  deleted.
- The `board` command, run in a terminal, reads and writes one board file
  at a path you type. The path never comes from the page. The file is read
  as a plain file of at most 4 MB, never through a link, and checked
  against the file's ruleset, [BOARD.md](BOARD.md), before it is used. A push makes a new file, or replaces
  a file of the same board, and leaves any other file as it is. It writes
  first to a plain file beside that one, named for it and ending
  `.json.partial`, then moves it into place: a plain file already under
  that name is written over, and anything else there stops the push. The
  command asks nothing of the network, starts no program, deletes nothing,
  and leaves your notes as they are.
- **No direct API connection to a board's backend, for now.** The app
  holds no address, no token, and no key for one, and a change that adds
  one is refused. A board moves as a file: exported on one side, and
  imported on the other. Why: the app is offline first, and at most LAN. A
  file crosses only when a person moves it, so nothing about a board leaves
  this computer by itself, and the app has no door for a backend to be
  reached through. "For now" is meant: the rule stands until a pull request
  changes [BOARD.md](BOARD.md), which has it in full.
- Nothing in the models folder is written over, moved, or removed.
- No chat is deleted: the trash is a folder.
- Every port the stack opens listens on this computer only.

[AGENTS.md](AGENTS.md) sets the rules a change follows to keep these
promises, for people and for the AI models that work for them.

## What is not covered

Please report these to their makers:

- The model engines, `mlx_lm.server` and llama.cpp's `llama-server`, and
  the models themselves, including what a model says.
- Brave Search and DuckDuckGo. How the app reads what they send back is
  covered.
- The images the stack uses as they come: Postgres, Redis, MinIO, Mailpit,
  and Caddy.
- Docker, macOS, Windows, and Linux, with the WebKitGTK it draws with there.

One thing is known, so it needs no report: the installers are unsigned for
now, so the computer warns before it opens one. Signing them is pin 1 on the
[roadmap](ROADMAP.md).
