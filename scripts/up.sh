#!/usr/bin/env bash
# Builds and starts the stack, waits until every part reports healthy, then
# makes sure the storage bucket exists.
#
#   scripts/up.sh        (or: make up)
#
# Needs infra/.env, which scripts/bootstrap.sh writes on the first run.
set -euo pipefail
cd "$(dirname "$0")/.."

[ -f infra/.env ] || {
    echo "up: there is no infra/.env yet, run 'make bootstrap' first" >&2
    exit 1
}
compose=(docker compose --env-file infra/.env --file infra/compose.yaml)

"${compose[@]}" up --detach --build --wait
"${compose[@]}" run --rm --quiet-pull minio-bucket
