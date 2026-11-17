# packages/shared/src

The shared crate's code.

| File          | What it is                                                           |
| ------------- | -------------------------------------------------------------------- |
| `lib.rs`      | Names what the crate offers. No logic                                |
| `settings.rs` | `ApiSettings` and `WorkerSettings`, the checks on each value, the shared names, and `redacted`, which hides the password in an address |
| `job.rs`      | `Job`, how a queue entry is read and written, and why an entry is refused |

Each settings type can be built from any lookup, not only the real
environment, so the tests never change the process's environment.
