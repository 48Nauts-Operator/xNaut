#!/usr/bin/env python3
"""Run-local outbox. Installed under .git, never committed or downloaded/executed.

Only this run's artifact directory is staged. Source changes must be committed
by the agent. Git/LFS authentication belongs to the worker's own environment.
"""

import datetime
import fcntl
import json
import os
import shlex
import shutil
import signal
from pathlib import Path
import subprocess
import sys
import time


def git(*args, check=True):
    process = subprocess.Popen(
        ["git", *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env={**os.environ, "GIT_TERMINAL_PROMPT": "0"},
        start_new_session=True,
    )
    # Videos can legitimately take several minutes. Kill the entire Git/LFS
    # process group on a stalled upload, preserving the commit for retry.
    timeout = 1800 if args[0] in ("lfs", "push") else 180
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.communicate()
        raise RuntimeError("Git upload timed out; data is retained for retry")
    result = subprocess.CompletedProcess(
        process.args, process.returncode, stdout, stderr
    )
    if check and result.returncode:
        # Git diagnostics can contain credential-helper output or signed URLs.
        raise RuntimeError(
            "git " + args[0] + " failed; check repository access and LFS storage"
        )
    return result


def atomic(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def publish(exit_code=None):
    metadata = json.loads(Path(".git/xnaut-transfer.json").read_text())
    branch = metadata["branch"]
    if git("branch", "--show-current").stdout.strip() != branch:
        raise RuntimeError("run branch changed; refusing to publish another branch")
    if git("remote", "get-url", "origin").stdout.strip() != (
        metadata.get("worker_remote") or metadata["remote"]
    ):
        raise RuntimeError("run repository changed; refusing to publish elsewhere")
    git("merge-base", "--is-ancestor", metadata["source_sha"], "HEAD")
    artifacts = Path(metadata["artifacts"])
    # Refuse symlinks (including parents), so uploads cannot follow a link
    # out of the run and publish some other project's files.
    for parent in [artifacts, *artifacts.parents]:
        if parent.is_symlink():
            raise RuntimeError("artifact directories must not be symlinks")
    for path in artifacts.rglob("*"):
        if path.is_symlink():
            raise RuntimeError("artifact symlinks cannot be published")
    # Track unusually large artifacts too, even when their extension is new.
    # Git LFS --filename escapes literal names instead of treating them as globs.
    for path in artifacts.rglob("*"):
        if path.is_file() and path.stat().st_size >= 8 * 1024 * 1024:
            subprocess.run(
                ["git", "lfs", "track", "--filename", str(path.relative_to(artifacts))],
                cwd=artifacts,
                check=True,
                capture_output=True,
                timeout=30,
            )
    result_path = artifacts / "result.json"
    if exit_code is not None and not result_path.exists():
        dirty = git(
            "status",
            "--porcelain",
            "--untracked-files=normal",
            "--",
            ".",
            ":(exclude)" + str(artifacts),
        ).stdout.strip()
        atomic(
            result_path,
            {
                "run_id": metadata["run_id"],
                "source_sha": metadata["source_sha"],
                "exit_code": exit_code,
                "uncommitted_source": bool(dirty),
                "finished_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            },
        )
    # Do not sweep arbitrary source files or other staged changes into the
    # artifact commit. The dedicated path is an explicit publication area.
    git("add", "-f", "--", str(artifacts))
    if git("diff", "--cached", "--quiet", "--", str(artifacts), check=False).returncode:
        git(
            "-c",
            "user.name=xNAUT",
            "-c",
            "user.email=xnaut@localhost",
            "commit",
            "--only",
            "-m",
            "chore(xnaut): preserve run " + metadata["run_id"],
            "--",
            str(artifacts),
        )
    git("lfs", "push", "origin", branch)
    git("push", "origin", "HEAD:refs/heads/" + branch)
    atomic(
        Path(".git/xnaut-upload.json"),
        {"state": "pushed", "head": git("rev-parse", "HEAD").stdout.strip()},
    )


def main():
    os.chdir(Path(__file__).resolve().parent.parent)
    retry = "--retry" in sys.argv
    exit_code = (
        int(sys.argv[sys.argv.index("--finish") + 1])
        if "--finish" in sys.argv
        else None
    )
    with open(".git/xnaut-publish.lock", "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        while True:
            try:
                publish(exit_code)
                print("xNAUT: results pushed to the configured repository", flush=True)
                return
            except Exception as error:
                atomic(
                    Path(".git/xnaut-upload.json"),
                    {"state": "pending", "error": str(error)},
                )
                print(
                    "xNAUT: upload pending; results retained on this worker. "
                    + str(error),
                    flush=True,
                )
                if not retry:
                    # Keep retrying even when an interactive agent remains open
                    # after its final answer. tmux owns this outbox, not the app.
                    if exit_code is not None and shutil.which("tmux"):
                        meta = json.loads(Path(".git/xnaut-transfer.json").read_text())
                        session = "xnaut-upload-" + meta["run_id"]
                        command = " ".join(
                            shlex.quote(v)
                            for v in [
                                sys.executable,
                                str(Path(__file__).resolve()),
                                "--finish",
                                str(exit_code),
                                "--retry",
                            ]
                        )
                        subprocess.run(
                            [
                                "tmux",
                                "new-session",
                                "-d",
                                "-s",
                                session,
                                "-c",
                                str(Path.cwd()),
                                command,
                            ],
                            capture_output=True,
                            timeout=10,
                        )
                    sys.exit(1)
                time.sleep(60)


if __name__ == "__main__":
    main()
