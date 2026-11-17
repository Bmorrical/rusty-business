# services/api/src

The API's code.

| File        | What it is                                                             |
| ----------- | ---------------------------------------------------------------------- |
| `main.rs`   | Starts the API: settings, Postgres, migrations, then listening. Also the `healthcheck` command |
| `lib.rs`    | Every route, what each one answers, and the `Probes` trait the health check asks through |
| `probes.rs` | The real probes: `SELECT 1` on Postgres and `PING` on Redis. Holds the two tests that need the running stack |

`lib.rs` knows nothing about real connections. It asks a `Probes` value
whether each dependency answers, which is how the tests cover "everything is
up", "one thing is down", and "one thing never answers" without a database.

A new route goes in `router` in `lib.rs`, with a test beside it that calls
it the way a client would.
