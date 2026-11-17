# engine

The model engine the Mac app carries inside it: `mlx_lm.server`, from MLX
LM, on a Python of its own.

| File               | What it is                                                     |
| ------------------ | -------------------------------------------------------------- |
| `requirements.in`  | The one package asked for, at an exact version                 |
| `requirements.txt` | Every package that one needs, each at an exact version and pinned by hash, as wheels for macOS 14 on Apple silicon. Made from `requirements.in`; never edited by hand |

## How the app gets it

`make app` and `make dmg` run [scripts/mac-app.sh](../../../scripts/mac-app.sh),
which:

1. fetches a Python of its own, pinned there by address and checksum;
2. installs `requirements.txt` into it, wheels only, each one refused unless
   its hash matches;
3. checks that every program and library in it runs on macOS 14.0, the
   oldest macOS the app allows;
4. compiles it once, so the app never writes inside itself;
5. adds `bin/mlx_lm.server`, which runs that Python with this computer's own
   Python settings kept out;
6. checks that it starts, with no home folder and no network;
7. copies it into the app, at `Contents/Resources/engine`.

The engine is built again only when `requirements.txt` or the script
changes. It all happens in `apps/desktop/src-tauri/target/mac`, which git
ignores.

The app looks there first for the engine's program, unless
`RUSTY_ENGINE_PROGRAM` names another. The engine it starts listens on this
computer only and stays offline, as described in [the app's
README](../README.md).

## Moving to a newer MLX LM

Change the version in `requirements.in`, then pin everything under it
again from the repository's root. This needs [uv](https://docs.astral.sh/uv/):

```bash
make engine-pins
```

Then `make dmg`, and open the app it builds to check the engine starts.
