"""Stop an accepted repository task's CLI; never forge publisher/exit proof."""

import fcntl
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def process(pid):
    try:
        p = Path("/proc") / str(pid)
        raw = (p / "stat").read_text()
        fields = raw[raw.rfind(")") + 2 :].split()
        if fields[0] == "Z":
            return None
        try:
            cwd = os.readlink(p / "cwd")
        except PermissionError:
            cwd = None
        return dict(
            pid=int(pid),
            parent=int(fields[1]),
            session=int(fields[3]),
            birth=fields[19],
            cwd=cwd,
        )
    except FileNotFoundError:
        return None


def processes():
    return [
        row
        for p in Path("/proc").iterdir()
        if p.name.isdigit()
        if (row := process(int(p.name))) is not None
    ]


def git(root, *args):
    return subprocess.run(
        ["git", "-C", str(root), *args],
        check=True,
        capture_output=True,
        text=True,
        timeout=15,
    ).stdout.strip()


def atomic(path, value):
    with tempfile.NamedTemporaryFile(
        mode="w", dir=path.parent, prefix=path.name + ".", delete=False
    ) as output:
        temp = Path(output.name)
        json.dump(value, output)
        output.flush()
        os.fsync(output.fileno())
    try:
        os.replace(temp, path)
    finally:
        if temp.exists():
            temp.unlink()


def members(rows, supervisor):
    ids = {supervisor["pid"]}
    while True:
        expanded = ids | {r["pid"] for r in rows if r["parent"] in ids}
        if expanded == ids:
            break
        ids = expanded
    return [r for r in rows if r["pid"] in ids or r["session"] == supervisor["session"]]


def worker_root(expected, home):
    run_id = expected["run_id"]
    if not run_id or any(
        c not in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
        for c in run_id
    ):
        raise ValueError("Invalid run identity")
    environment = expected["environment"]
    if environment == "exe-dev":
        workdir = "agents/runs/" + run_id
        root = Path(home) / workdir
    elif environment == "gitvm":
        workdir = "/workspace/.xnaut-runs/" + run_id
        root = Path(workdir)
    else:
        raise ValueError("Unknown repository worker environment")
    if expected["workdir"] != workdir:
        raise ValueError("Unexpected worker directory")
    return root


def handoff(expected, home=None):
    home = Path.home() if home is None else Path(home)
    run_id = expected["run_id"]
    root = worker_root(expected, home)
    if (
        root.resolve() != root.absolute()
        or not (root / ".git").is_dir()
        or (root / ".git").is_symlink()
    ):
        raise ValueError("Worker directory is missing or redirected")
    for name in [
        "xnaut-transfer.json",
        "xnaut-phase",
        "xnaut-supervisor.pid",
        "xnaut-upload.json",
        "xnaut-publish.lock",
        "xnaut-handoff.lock",
        "xnaut-handoff.json",
    ]:
        if (root / ".git" / name).is_symlink():
            raise ValueError("Worker control file is redirected")
    meta = json.loads((root / ".git/xnaut-transfer.json").read_text())
    for key in [
        "run_id",
        "project",
        "ticket",
        "handle",
        "workdir",
        "artifacts",
        "branch",
        "source_sha",
        "remote",
    ]:
        if meta.get(key) != expected.get(key):
            raise ValueError("Worker transfer identity changed: " + key)
    if meta.get("worker", {"kind": "exe-dev"}).get("kind") != expected["environment"]:
        raise ValueError("Worker execution environment changed")
    head = expected["head"]
    # Final Git/LFS publication is provisional; its dirty index is not failure.
    receipt_path = root / ".git/xnaut-handoff.json"
    if (
        receipt_path.exists()
        and (root / ".git/xnaut-phase").read_text().strip() == "uploading"
    ):
        waiting = json.loads(receipt_path.read_text())
        if waiting.get("run_id") != run_id or waiting.get("head") != head:
            raise ValueError("Publisher handoff identity changed")
        if time.time() - waiting["requested_at"] > 600:
            raise ValueError(
                "Completed task publisher deadline exceeded; evidence retained"
            )
        waiting["state"] = "pending"
        return waiting
    if git(root, "rev-parse", "HEAD") != head or git(root, "status", "--porcelain"):
        raise ValueError("Completed worker head or clean checkout changed")
    result = json.loads(
        git(root, "show", head + ":" + expected["artifacts"] + "/result.json")
    )
    if (
        result.get("run_id") != run_id
        or result.get("source_sha") != expected["source_sha"]
        or result.get("exit_code") != 0
        or result.get("uncommitted_source") is not False
    ):
        raise ValueError("Accepted successful publication is not intact")
    upload = json.loads((root / ".git/xnaut-upload.json").read_text())
    if upload.get("state") != "pushed" or upload.get("head") != head:
        raise ValueError("Publisher has not confirmed the accepted revision")
    with (root / ".git/xnaut-handoff.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        receipt = root / ".git/xnaut-handoff.json"
        previous = json.loads(receipt.read_text()) if receipt.exists() else None
        if previous and (previous["run_id"] != run_id or previous["head"] != head):
            raise ValueError("A different handoff request already exists")
        supervisor_pid = int((root / ".git/xnaut-supervisor.pid").read_text())
        phase = (root / ".git/xnaut-phase").read_text().strip()
        rows = processes()
        supervisor = next((r for r in rows if r["pid"] == supervisor_pid), None)
        if previous:
            owned = previous["processes"]
            outstanding = [
                r
                for r in rows
                if r["session"] == previous["supervisor"]["session"]
                or any(
                    r["pid"] == old["pid"] and r["birth"] == old["birth"]
                    for old in owned
                )
                or r["cwd"] == str(root)
            ]
            if phase == "finished" and not outstanding:
                previous["state"] = "finished"
                atomic(receipt, previous)
                return previous
            previous["state"] = "pending"
            atomic(receipt, previous)
            if time.time() - previous["requested_at"] > 600:
                raise ValueError(
                    "Completed task handoff timed out; retain worker and publication evidence"
                )
            # Replaying a persisted request never signals a new or reused PID.
            child = next(
                (
                    r
                    for r in rows
                    if r["pid"] == previous["child"]["pid"]
                    and r["birth"] == previous["child"]["birth"]
                ),
                None,
            )
            if child is None:
                return previous
            if child != previous["child"] or supervisor != previous["supervisor"]:
                raise ValueError("Recorded writer identity changed during handoff")
        else:
            if (
                phase == "finished"
                and supervisor is None
                and not any(
                    r["cwd"] == str(root) or r["session"] == supervisor_pid
                    for r in rows
                )
            ):
                return dict(state="finished", run_id=run_id, head=head)
            if phase != "running" or supervisor is None:
                raise ValueError(
                    "Supervisor state is uncertain; no writer was signaled"
                )
            if (
                supervisor["cwd"] != str(root)
                or supervisor["session"] != supervisor_pid
            ):
                raise ValueError("Supervisor does not own this task session")
            children = [r for r in rows if r["parent"] == supervisor_pid]
            if len(children) != 1 or children[0]["pid"] != expected.get("agent_pid"):
                raise ValueError(
                    "The registered completed CLI is not the supervisor child"
                )
            child = children[0]
            owned = members(rows, supervisor)
            if child["cwd"] != str(root) or any(r["cwd"] != str(root) for r in owned):
                raise ValueError(
                    "A task process changed workspace; no writer was signaled"
                )
            previous = dict(
                state="pending",
                run_id=run_id,
                head=head,
                requested_at=time.time(),
                supervisor=supervisor,
                child=child,
                processes=owned,
            )
        # pidfd keeps the signal bound to this process even if its numeric PID
        # is reused after inspection. The supervisor and publisher stay alive.
        publisher = (root / ".git/xnaut-publish.lock").open("a")
        try:
            fcntl.flock(publisher, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            publisher.close()
            atomic(receipt, previous)
            return previous
        try:
            fd = os.pidfd_open(child["pid"])
        except ProcessLookupError:
            publisher.close()
            atomic(receipt, previous)
            return previous
        try:
            current = process(child["pid"])
            if current is None:
                atomic(receipt, previous)
                return previous
            if current != child or process(supervisor_pid) != supervisor:
                raise ValueError("Writer changed before graceful handoff")
            atomic(receipt, previous)
            try:
                signal.pidfd_send_signal(fd, signal.SIGTERM)
            except ProcessLookupError:
                pass  # A later observation still has to prove every writer gone.
        finally:
            os.close(fd)
            publisher.close()
        return previous


if __name__ == "__main__":
    try:
        print(json.dumps(handoff(json.load(sys.stdin))))
    except Exception as error:
        print(json.dumps({"state": "refused", "reason": str(error)}))
        sys.exit(1)
