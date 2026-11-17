#!/usr/bin/env bash
# Brings the whole stack up on a computer that has never run it.
#
#   scripts/bootstrap.sh        (or: make bootstrap)
#
# Safe to run again: an existing infra/.env is kept as it is, and Docker
# starts only what is not already running.
set -euo pipefail
cd "$(dirname "$0")/.."

env_example=infra/.env.example
env_file=infra/.env

need() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "bootstrap: '$1' is needed and was not found" >&2
        exit 1
    }
}
need docker
need openssl
docker compose version >/dev/null 2>&1 || {
    echo "bootstrap: Docker Compose is needed ('docker compose version' failed)" >&2
    exit 1
}
docker info >/dev/null 2>&1 || {
    echo "bootstrap: Docker is installed but not running" >&2
    exit 1
}

if [ -f "$env_file" ]; then
    echo "bootstrap: keeping the infra/.env that is already here"
else
    # Only this user may read the file: it holds the stack's passwords.
    umask 077
    while IFS= read -r line || [ -n "$line" ]; do
        case "$line" in
            *_PASSWORD=) printf '%s%s\n' "$line" "$(openssl rand -hex 24)" ;;
            *) printf '%s\n' "$line" ;;
        esac
    done <"$env_example" >"$env_file"
    chmod 600 "$env_file"
    echo "bootstrap: wrote infra/.env with new random passwords"
fi

echo "bootstrap: building and starting the stack (the first build takes a few minutes)"
scripts/up.sh

scripts/stack-env.sh sh -c '
    echo
    echo "The stack is up. Everything listens on this computer only:"
    echo "  Front door       $STACK_FRONT_DOOR/health"
    echo "  API, direct      $STACK_API/health"
    echo "  Mail (web page)  $STACK_MAILPIT"
    echo "  Files (console)  $STACK_MINIO_CONSOLE"
    echo "  Postgres         127.0.0.1:$STACK_POSTGRES_PORT"
    echo "  Redis            127.0.0.1:$STACK_REDIS_PORT"
    echo
    echo "Passwords are in infra/.env. Stop the stack with: make down"
'
