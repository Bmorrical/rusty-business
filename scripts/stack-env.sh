#!/usr/bin/env bash
# Runs a command with the stack's addresses, as seen from this computer, in
# its environment. The values come from infra/.env.
#
#   scripts/stack-env.sh cargo test --package rusty-api -- --ignored
#   scripts/stack-env.sh node --test e2e/*.test.mjs
set -euo pipefail
cd "$(dirname "$0")/.."

env_file=infra/.env
[ -f "$env_file" ] || {
    echo "stack-env: there is no infra/.env yet, run 'make bootstrap' first" >&2
    exit 1
}
[ "$#" -gt 0 ] || {
    echo "stack-env: give a command to run, for example: scripts/stack-env.sh env" >&2
    exit 1
}

# value NAME DEFAULT: the last NAME= line in infra/.env, or DEFAULT. The
# file is read as text, never run.
value() {
    local line
    line=$(grep -E "^$1=" "$env_file" | tail -n 1 || true)
    line=${line#*=}
    printf '%s' "${line:-$2}"
}

postgres_port=$(value PORT_POSTGRES 4732)
redis_port=$(value PORT_REDIS 4779)

export DATABASE_URL="postgres://$(value POSTGRES_USER rusty):$(value POSTGRES_PASSWORD '')@127.0.0.1:${postgres_port}/$(value POSTGRES_DB rusty)"
export REDIS_URL="redis://:$(value REDIS_PASSWORD '')@127.0.0.1:${redis_port}/0"
export STACK_FRONT_DOOR="http://127.0.0.1:$(value PORT_FRONT_DOOR 4700)"
export STACK_API="http://127.0.0.1:$(value PORT_API 4701)"
export STACK_MAILPIT="http://127.0.0.1:$(value PORT_MAILPIT_UI 4726)"
export STACK_MAILPIT_SMTP_PORT="$(value PORT_MAILPIT_SMTP 4725)"
export STACK_MINIO="http://127.0.0.1:$(value PORT_MINIO 4790)"
export STACK_MINIO_CONSOLE="http://127.0.0.1:$(value PORT_MINIO_CONSOLE 4791)"
export STACK_MINIO_BUCKET="$(value MINIO_BUCKET rusty-business)"
export STACK_POSTGRES_PORT="$postgres_port"
export STACK_REDIS_PORT="$redis_port"

exec "$@"
