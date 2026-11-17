# infra/caddy

Settings for Caddy, the stack's front door.

| File        | What it is                                  |
| ----------- | ------------------------------------------- |
| `Caddyfile` | Mounted read-only into the `caddy` container |

What the front door does today:

- Listens on port 8080 inside its container, published as
  `127.0.0.1:4700` on this computer.
- Answers `/front-door` by itself with `ok`, so it can be checked without
  the API behind it.
- Passes every other request to the API.

It speaks plain HTTP on purpose. It is published on 127.0.0.1 only, so there
is no network between the browser and Caddy to protect. Its admin interface
is off.

After a change to the Caddyfile:

```bash
docker compose --env-file infra/.env --file infra/compose.yaml restart caddy
```
