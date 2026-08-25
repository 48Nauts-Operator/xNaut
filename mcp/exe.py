#!/usr/bin/env python3
"""exe.dev sandbox MCP server for xNAUT (stdio).

Persistent Linux VMs with root, a real network stack and a public HTTPS
hostname (https://<name>.exe.xyz). An alternative to GitVM: GitVM gives
ephemeral containers that self-destruct on a timeout, exe.dev gives VMs that
survive and bill down to disk when idle.

The whole exe.dev API is one endpoint: POST https://exe.dev/exec, with the SSH
command as the raw body ("the SSH API shoved into a POST body" -- exe.dev docs,
https://exe.dev/docs/https-api). So this server is one HTTP call plus argument
quoting, and it does not go stale when exe.dev adds a command.

Config (env, set in the plugin library):
  EXE_API_KEY  required   ssh exe.dev ssh-key generate-api-key --exp=30d
  EXE_URL      optional   defaults to https://exe.dev

Tools:
  exe  run any exe.dev control command (new / ls / rm / cp / share / ...)
  sh   run a shell command inside one VM, quoting handled

Token scoping is exe.dev's own: generate-api-key takes --cmds to whitelist
commands, --exp to bound replay, --ctx to tag a tenant or a run. A key minted
per run is the intended shape, not one long-lived key.

No dependencies: stdlib only, so `python3 mcp/exe.py` just runs.
"""

from __future__ import annotations

import json
import os
import shlex
import sys
import urllib.error
import urllib.request

URL = os.getenv("EXE_URL", "https://exe.dev").strip() or "https://exe.dev"
API_KEY = os.getenv("EXE_API_KEY", "").strip()

# ponytail: /exec is capped at 30s server-side with no stdin and no pty, so a
# build or a test suite must be backgrounded on the VM and polled, not awaited
# here. Raise nothing clever: say it, and let the caller nohup + tail.
TIMEOUT_S = 35

# What each status means, from https://exe.dev/docs/https-api. Mapped so a
# failure names its cause instead of surfacing a bare number.
STATUS_HELP = {
    400: "bad request: empty body or invalid command syntax (unbalanced quotes?)",
    401: "invalid token: malformed, expired, or signed with a key exe.dev does not know "
         "(check `ssh exe.dev ssh-key list`)",
    403: "the token's --cmds list does not allow this command; subcommands must be listed "
         "explicitly ('ssh-key' does not grant 'ssh-key list')",
    404: "unknown command (see `ssh exe.dev help`)",
    413: "request too large: the 64KB body limit",
    422: "the command ran and failed (non-zero exit); the message below is its output",
    429: "rate limited: the limit is per SSH key, use separate keys for independent workloads",
    504: "timed out: /exec kills a command at 30s. Background it on the VM "
         "(nohup ... &) and poll instead of waiting here.",
}


def describe(code: int, reason: str, body: str) -> str:
    """One error line: what exe.dev said, and what it means."""
    hint = STATUS_HELP.get(code, reason)
    body = (body or "").strip()
    return f"exe.dev {code}: {hint}" + (f"\n{body[:2000]}" if body else "")


def run(command: str) -> str:
    """POST one exe.dev command. The body IS the command; the reply IS its output."""
    if not API_KEY:
        raise RuntimeError("set EXE_API_KEY in the plugin's config "
                           "(ssh exe.dev ssh-key generate-api-key --exp=30d)")
    command = command.strip()
    if not command:
        raise ValueError("empty command")
    req = urllib.request.Request(
        f"{URL.rstrip('/')}/exec",
        data=command.encode("utf-8"),
        headers={"Authorization": f"Bearer {API_KEY}", "Content-Type": "text/plain"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT_S) as resp:
            return resp.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", "replace") if exc.fp else ""
        raise RuntimeError(describe(exc.code, exc.reason, body)) from None
    except urllib.error.URLError as exc:
        raise RuntimeError(f"exe.dev at {URL} is unreachable: {exc.reason}") from None


def vm_command(vm: str, command: str) -> str:
    """`ssh <vm> <command>`, quoted so the remote shell sees one intact command.

    Without the quoting a command with spaces, pipes or quotes is re-split by
    the control plane's own parser and runs as something else entirely.
    """
    vm = vm.strip()
    if not vm:
        raise ValueError("which VM? pass `vm`")
    if not command.strip():
        raise ValueError("empty command")
    return f"ssh {shlex.quote(vm)} {shlex.quote(command)}"


def do_exe(args: dict) -> str:
    return run(str(args.get("command") or ""))


def do_sh(args: dict) -> str:
    return run(vm_command(str(args.get("vm") or ""), str(args.get("command") or "")))


TOOLS = [
    {
        "name": "exe",
        "description": (
            "Run an exe.dev control command, exactly as typed after `ssh exe.dev`. "
            "Examples: `ls --json` (your VMs), `new --name=audit-1 --image=exeuntu --cpu=4 --json` "
            "(create; the VM is then reachable at https://<name>.exe.xyz), `rm audit-1` (delete), "
            "`cp ...` (copy files), `ssh-key generate-api-key --cmds='ls,new,rm,ssh' --exp=1d` "
            "(mint a scoped token). Pass --help to ANY command to get its flags as JSON. "
            "Capped at 30s: background long work on the VM instead of waiting."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {"command": {"type": "string"}},
            "required": ["command"],
        },
    },
    {
        "name": "sh",
        "description": (
            "Run a shell command inside one exe.dev VM. Quoting is handled, so pass the "
            "command as you would type it locally. Capped at 30s by exe.dev: for a build or "
            "a test suite, start it detached (`nohup make build > /tmp/b.log 2>&1 &`) and poll "
            "with `tail /tmp/b.log` rather than waiting for it here."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {"vm": {"type": "string"}, "command": {"type": "string"}},
            "required": ["vm", "command"],
        },
    },
]


def handle(method: str, params: dict):
    if method == "initialize":
        return {
            "protocolVersion": params.get("protocolVersion", "2024-11-05"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "exe", "version": "0.1.0"},
        }
    if method == "tools/list":
        return {"tools": TOOLS}
    if method == "tools/call":
        name = params.get("name")
        args = params.get("arguments") or {}
        try:
            if name == "exe":
                text = do_exe(args)
            elif name == "sh":
                text = do_sh(args)
            else:
                raise ValueError(f"unknown tool: {name}")
            return {"content": [{"type": "text", "text": text}]}
        except (ValueError, RuntimeError, OSError) as exc:
            return {"content": [{"type": "text", "text": str(exc)}], "isError": True}
    if method == "ping":
        return {}
    return None


def main() -> None:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if "id" not in msg:  # notification: nothing to answer
            continue
        result = handle(msg.get("method", ""), msg.get("params") or {})
        if result is None:
            reply = {"jsonrpc": "2.0", "id": msg["id"],
                     "error": {"code": -32601, "message": f"method not found: {msg.get('method')}"}}
        else:
            reply = {"jsonrpc": "2.0", "id": msg["id"], "result": result}
        sys.stdout.write(json.dumps(reply) + "\n")
        sys.stdout.flush()


def selftest() -> None:
    """The two things that are logic rather than plumbing: quoting, and errors.

    No network and no token: what this proves is that a command with shell
    metacharacters reaches the VM intact (the bug that silently runs something
    else), and that a failure names its cause.
    """
    assert vm_command("box", "ls") == "ssh box ls"
    # The whole point: pipes, quotes and spaces survive as ONE remote command.
    payload = "grep -r 'a b' . | wc -l"
    got = vm_command("box", payload)
    assert shlex.split(got) == ["ssh", "box", payload], shlex.split(got)
    # A VM name is quoted too, so it cannot smuggle a second command.
    assert vm_command("a; rm -rf /", "ls") == "ssh 'a; rm -rf /' ls"
    for bad in ("", "   "):
        try:
            vm_command(bad, "ls")
            raise AssertionError("a missing VM name was accepted")
        except ValueError:
            pass
    try:
        vm_command("box", "")
        raise AssertionError("an empty command was accepted")
    except ValueError:
        pass

    assert "timed out" in describe(504, "Gateway Timeout", "")
    assert "30s" in describe(504, "Gateway Timeout", "")
    assert "--cmds" in describe(403, "Forbidden", "")
    assert "boom" in describe(422, "Unprocessable", "boom")
    assert "999" in describe(999, "Weird", "")
    print("ok: shell metacharacters reach the VM intact, VM names cannot smuggle "
          "a second command, and each exe.dev status names its cause")


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest()
    else:
        main()
