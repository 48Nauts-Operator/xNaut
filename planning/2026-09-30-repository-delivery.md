# Repository delivery and review

Each new project requires a repository URL in setup. Project settings keep the
local folder and repository URL separate. Existing imported projects keep their
files, but must configure a repository before a repository-backed exe.dev run or
notebook upload can proceed. The configured destination wins; the transport does
not guess `origin` or push to both GitHub and Forgejo.

## Task runs on exe.dev

1. Start from a clean, committed checkout. Preflight the worker's Codex login
   when applicable, Python 3, Git LFS, and repository access.
2. Push the exact source commit to `xnaut/inputs/<run-id>`. Clone real history on
   the worker, verify that commit, and create `xnaut/runs/<run-id>`. Each run gets
   a separate checkout under `~/agents/runs/`; existing runs are preserved.
3. The agent commits its authorized source changes and writes reports, notes,
   screenshots, pictures and videos under `.xnaut/runs/<run-id>/`. The launcher
   installs media LFS rules there. Other artifacts at least 8 MiB also use LFS.
4. The agent writes `handback.json` and calls
   `python3 .git/xnaut-publish.py --finish 0`. The supervisor also calls the
   publisher when the CLI exits. Only the explicit artifact directory is
   automatically staged, including when `.gitignore` ignores `.xnaut`.
5. The publisher uploads LFS objects and pushes the task branch. A failed push
   retains committed artifacts and an upload receipt on the VM. A detached tmux
   uploader retries every minute, independently of the desktop connection.
6. xNAUT fetches results for its own durable launch receipts every minute and
   after restart. It checks source ancestry, run identity and bounded JSON, then
   opens a PR using the configured Forgejo/GitHub connection. A retry looks for
   the exact branch/base PR, including closed PRs, before attempting creation.
7. A valid handback is filed through the existing evidence gate. If ownership
   still matches, the ticket goes to review and is handed to NautBot through the
   normal ticket rails. A failed run, uncommitted source, missing evidence or
   changed ownership does not silently finish the ticket. PR API availability
   and handback filing are independent retryable operations.

A PR is review, not approval: this flow never merges or pushes the default
branch. The process exit code is recorded separately from handback quality.
The run registry waits for worker process evidence before marking a dispatched
viewport as running. A worker/VM reboot preserves files but can stop tmux;
restart its publisher with the command above if necessary.

Credentials, launcher scripts, caches and local control receipts are outside
the publication directory. Do not place secrets in task reports or artifacts.
The publisher rejects artifact symlinks and refuses a changed branch or remote.
It does not sweep arbitrary untracked files into an automatic commit.

## Notebook notes

Project notebook edits and explicit chat summaries save to local SQLite first.
They also enter a durable local queue. The background worker batches pending
edits into a notebook snapshot on `xnaut/notes/<snapshot-id>`, preserving both
Markdown and JSON under `.xnaut/notes/`, and opens a PR. A newer queued edit
cannot be erased by completion of an older upload. Failure retains the queue
and checkout. Multiple snapshots may produce separate PRs; nothing auto-merges.

Notes without a project remain local and say so. This change exports notebook
snapshots; it does not replace the immediate local save or implement automatic
bidirectional merging of another device's notebook edits.

## Setup and visibility

The desktop needs Git access to the configured repository plus its corresponding
forge connection/token for PR creation. The worker needs its own repository
credentials and network access. SSH aliases are resolved to portable host/user/
port URLs; desktop private keys are never forwarded or copied by this transport.
The repository must have a default branch with an initial commit and support LFS.

Project settings show queued, prepared, running, pushed and review transfers,
errors, and PR links. Use **Refresh transfers** after changing connectivity.
Git and LFS uploads are separate from opening the PR: an API outage can leave
results safely pushed while the PR awaits the next attempt.

[Git LFS stores media outside Git's object history, with pointers in the branch](https://git-lfs.com/).
[Some LFS media does not render inline in GitHub PRs](https://docs.github.com/en/repositories/working-with-files/managing-large-files/collaboration-with-git-large-file-storage);
reviewers can fetch the branch with LFS to retrieve the original files.

This replaces rsync for exe.dev **agent launches**. The existing sandbox verify
and GitVM transfer paths remain separate. Existing runs and JOBUP-11's earlier
macOS `.git` pointer are not automatically migrated or declared recovered.
