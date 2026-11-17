# The few commands this repository runs on. `make` alone lists them.

COMPOSE := docker compose --env-file infra/.env --file infra/compose.yaml

.DEFAULT_GOAL := help
.PHONY: help bootstrap up down logs ps health \
        test test-services test-repo test-desktop test-live e2e

help: ## List these commands
	@grep -E '^[a-z0-9-]+:.*## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN { FS = ":.*## " } { printf "  make %-14s %s\n", $$1, $$2 }'

bootstrap: ## First run: write infra/.env, build, start, wait until healthy
	@scripts/bootstrap.sh

up: ## Build and start the stack, wait until healthy
	@scripts/up.sh

down: ## Stop the stack (the data stays)
	$(COMPOSE) down

logs: ## Follow the stack's logs
	$(COMPOSE) logs --follow --tail 100

ps: ## Show each container and its health
	$(COMPOSE) ps

health: ## Ask the API how it is, through the front door
	@scripts/stack-env.sh sh -c 'curl --silent --show-error --fail-with-body "$$STACK_FRONT_DOOR/health"; echo'

test: test-services test-repo test-desktop ## Every test that needs no running stack

test-services: ## Format, lint, and unit tests for the API, worker, and shared code
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --locked -- -D warnings
	cargo test --workspace --locked

test-repo: ## The repository's own rules (a README in every folder)
	node --test scripts/*.test.mjs

test-desktop: ## The desktop app's tests (page, then Rust)
	cd apps/desktop && { [ -d node_modules ] || npm ci; } && npm test && npm run build
	cd apps/desktop/src-tauri && cargo test --locked

test-live: ## The API's tests against the running stack
	scripts/stack-env.sh cargo test --package rusty-api --locked -- --ignored

e2e: ## End to end: every part of the running stack, from outside
	scripts/stack-env.sh node --test e2e/*.test.mjs
