# services/worker/src

The worker's code.

| File      | What it is                                                               |
| --------- | ------------------------------------------------------------------------ |
| `main.rs` | The loop: refresh the heartbeat, wait on the queue, hand each entry to `handle`, reconnect when Redis goes away. Also the `healthcheck` command |
| `lib.rs`  | `handle`: decides what to do with one queue entry. Today that is "note it" for a job and "drop it" for anything else |

`lib.rs` has no Redis in it, so what the worker does with a job can be tested
with a plain string. A new kind of job is a new branch in `handle`, with a
test beside it.
