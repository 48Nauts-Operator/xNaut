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

test:
    cd . && pytest

feature name:
    git checkout -b feature/{{name}}

fix-branch name:
    git checkout -b fix/{{name}}
