# xnaut — common dev tasks. Run `just` to see all recipes.

REPO := "48Nauts/xnaut"
FORGEJO_BASE := "http://cosmos.tail138398.ts.net:3000"

default:
    @just --list

push:
    git push forgejo

pull:
    git pull --rebase

open:
    open "{{FORGEJO_BASE}}/{{REPO}}"

ci:
    open "{{FORGEJO_BASE}}/{{REPO}}/actions"

issues:
    open "{{FORGEJO_BASE}}/{{REPO}}/issues"

prs:
    open "{{FORGEJO_BASE}}/{{REPO}}/pulls"

lint:
    cd . && ruff check .
    cd . && ruff format --check .

fix:
    cd . && ruff check --fix .
    cd . && ruff format .

# Backend suite + hygiene. `pytest` was here and this repo has no Python tests.
test:
    cargo test --manifest-path src-tauri/Cargo.toml
    node scripts/hygiene-check.mjs

# The checks a green suite cannot make about itself: does the suite pollute the
# vault, is every hook wired at both ends, can every canvas be reopened.
hygiene:
    node scripts/hygiene-check.mjs

feature name:
    git checkout -b feature/{{name}}

fix-branch name:
    git checkout -b fix/{{name}}

# Standalone pre-flight health check → preflight-report.html
preflight *args:
    node scripts/preflight.mjs {{args}}
