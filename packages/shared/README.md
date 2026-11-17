# packages/shared

The Rust crate `rusty-shared`: what the API and the worker must agree on.

- **Settings.** Read from the environment and nowhere else. No address and
  no password has a default; a missing value is an error that names it. A
  setting that holds a password never prints it: `Debug` and `redacted` show
  `***` in its place.
- **The job format.** What one entry on the queue looks like, and the checks
  that decide whether a line is a job.
- **Shared names.** The queue (`rusty:jobs`), the worker's heartbeat key
  (`rusty:worker:heartbeat`), and how long the heartbeat lasts.

A service uses it through the workspace:

```toml
[dependencies]
rusty-shared = { workspace = true }
```

## Tests

```bash
cargo test --package rusty-shared
```

| Path       | What it is |
| ---------- | ---------- |
| [src](src) | The code   |
