#!/usr/bin/env python3
"""Task bootstrap, transported by xNAUT, never read from a task repository.

Input (including optional enrollment credentials) comes only over SSH stdin.
Forge tokens stay on the desktop. A worker keeps one deploy key per repository.
CLI references: https://tailscale.com/docs/reference/tailscale-cli/up and
https://github.com/git-lfs/git-lfs/blob/main/docs/api/server-discovery.md.
"""

import fcntl
import hashlib
import http.client
import json
import os
import re
from pathlib import Path
import shlex
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request
import urllib.error


class SetupError(Exception):
    pass


def run(args, timeout=60, env=None):
    try:
        return subprocess.run(
            args,
            capture_output=True,
            text=True,
            timeout=timeout,
            env={**os.environ, "GIT_TERMINAL_PROMPT": "0", **(env or {})},
        )
    except (OSError, subprocess.TimeoutExpired):
        raise SetupError("worker_command_failed")


def endpoint(remote):
    if "://" not in remote:
        host, path = remote.split(":", 1)
        remote = "ssh://" + host + "/" + path
    parsed = urllib.parse.urlparse(remote)
    if (
        parsed.scheme != "ssh"
        or not parsed.hostname
        or not parsed.username
        or parsed.password
    ):
        raise SetupError("invalid_ssh_remote")
    parts = parsed.path.strip("/").removesuffix(".git").split("/")
    if len(parts) != 2 or any(
        not p
        or any(
            c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-"
            for c in p
        )
        for p in parts
    ):
        raise SetupError("invalid_ssh_remote")
    return parsed


def reachable(host, port):
    try:
        with socket.create_connection((host, port), timeout=8):
            return True
    except OSError:
        proxy = userspace_config()
        if not proxy:
            return False
        connection = http.client.HTTPConnection("127.0.0.1", proxy["port"], timeout=8)
        try:
            connection.set_tunnel(host, port)
            connection.connect()
            return True
        except OSError:
            return False
        finally:
            connection.close()


def network_dir():
    return Path.home() / ".local/share/xnaut/worker-network"


def userspace_config():
    path = network_dir() / "userspace.json"
    if not path.exists():
        return None
    try:
        value = json.loads(path.read_text())
        if not isinstance(value["port"], int) or not 1024 <= value["port"] <= 65535:
            raise ValueError("invalid proxy port")
        return value
    except (OSError, ValueError, KeyError):
        raise SetupError("network_daemon_unavailable")


def tailscale_command():
    if userspace_config():
        return ["tailscale", "--socket=" + str(network_dir() / "tailscaled.sock")]
    return ["tailscale"]


def start_userspace_network():
    # Tailscale's documented container mode needs neither systemd nor /dev/net/tun:
    # https://tailscale.com/docs/concepts/userspace-networking
    directory = network_dir()
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    directory.chmod(0o700)
    with (directory / "lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if (
            userspace_config()
            and run(tailscale_command() + ["status", "--json"]).returncode == 0
        ):
            return
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        command = [
            shutil.which("tailscaled") or "/usr/sbin/tailscaled",
            "--tun=userspace-networking",
            "--socket=" + str(directory / "tailscaled.sock"),
            "--state=" + str(directory / "tailscaled.state"),
            "--outbound-http-proxy-listen=127.0.0.1:" + str(port),
        ]
        # Keep private daemon state and logs outside all task checkouts.
        fd = os.open(
            directory / "daemon.log", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600
        )
        with os.fdopen(fd, "ab") as log:
            process = subprocess.Popen(
                command,
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=log,
                start_new_session=True,
            )
        config = directory / "userspace.json"
        config.write_text(json.dumps({"port": port}))
        config.chmod(0o600)
        for _ in range(40):
            if process.poll() is not None:
                break
            status = run(tailscale_command() + ["status", "--json"])
            if status.returncode == 0:
                return
            time.sleep(0.25)
        raise SetupError("network_daemon_unavailable")


def network(host, port, enrollment):
    if reachable(host, port):
        return
    key = enrollment.get("auth_key", "").strip()
    if not key:
        raise SetupError("network_setup_required")
    tags = enrollment.get("tags", "").strip()
    if tags and any(
        not re.fullmatch(r"tag:[a-zA-Z][a-zA-Z0-9-]*", tag.strip())
        for tag in tags.split(",")
    ):
        raise SetupError("network_tags_invalid")
    tags = ",".join(tag.strip() for tag in tags.split(","))
    sudo = [] if os.geteuid() == 0 else ["sudo", "-n"]
    if not shutil.which("tailscale"):
        # The official installer selects the distribution's signed repository.
        with tempfile.TemporaryDirectory(prefix="xnaut-network-") as temp:
            installer = str(Path(temp) / "install.sh")
            if run(
                [
                    "curl",
                    "--fail",
                    "--silent",
                    "--show-error",
                    "--proto",
                    "=https",
                    "--tlsv1.2",
                    "https://tailscale.com/install.sh",
                    "-o",
                    installer,
                ]
            ).returncode:
                raise SetupError("network_install_failed")
            if run(sudo + ["sh", installer], timeout=300).returncode:
                raise SetupError("network_install_failed")
    # Do not turn a missing route on an already enrolled machine into a forced
    # logout/re-enrollment. ACLs, DNS and firewalls remain distinct failures.
    status = run(tailscale_command() + ["status", "--json"])
    if status.returncode:
        if shutil.which("systemctl"):
            run(sudo + ["systemctl", "enable", "--now", "tailscaled"])
            status = run(tailscale_command() + ["status", "--json"])
        if (
            status.returncode
            and sys.platform == "linux"
            and not Path("/dev/net/tun").exists()
        ):
            start_userspace_network()
            status = run(tailscale_command() + ["status", "--json"])
    try:
        backend = json.loads(status.stdout).get("BackendState")
    except (ValueError, AttributeError):
        raise SetupError("network_daemon_unavailable")
    if backend == "Running":
        if reachable(host, port):
            return
        raise SetupError("network_route_unavailable")
    if backend == "NeedsMachineAuth":
        raise SetupError("network_device_approval_required")
    # file: prevents a reusable enrollment key appearing in process arguments.
    # It is removed even on auth failure and is never written into task data.
    with tempfile.TemporaryDirectory(prefix="xnaut-enroll-") as temp:
        auth = Path(temp) / "key"
        fd = os.open(auth, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w") as output:
            output.write(key)
        command = (
            ([] if userspace_config() else sudo)
            + tailscale_command()
            + [
                "up",
                "--auth-key=file:" + str(auth),
                "--timeout=45s",
            ]
        )
        if tags:
            command.append("--advertise-tags=" + tags)
        result = run(command, timeout=60)
        if result.returncode:
            # Classify only known policy errors; never expose raw auth output.
            detail = (result.stdout + result.stderr).lower()
            if "requested tags" in detail and (
                "not permitted" in detail or "invalid" in detail
            ):
                raise SetupError("network_tags_denied")
            raise SetupError("network_enrollment_failed")
    if not reachable(host, port):
        raise SetupError("network_route_unavailable")


def identity(remote):
    parsed = endpoint(remote)
    canonical = f"{parsed.hostname}:{parsed.port or 22}/{parsed.path.strip('/').removesuffix('.git')}"
    scope = hashlib.sha256(canonical.encode()).hexdigest()
    directory = Path.home() / ".local/share/xnaut/repository-access" / scope
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    directory.chmod(0o700)
    key = directory / "id_ed25519"
    with (directory / "lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if not key.exists():
            if run(
                [
                    "ssh-keygen",
                    "-q",
                    "-t",
                    "ed25519",
                    "-N",
                    "",
                    "-C",
                    "xnaut-worker-" + scope[:12],
                    "-f",
                    str(key),
                ]
            ).returncode:
                raise SetupError("repository_key_failed")
        key.chmod(0o600)
        public = run(["ssh-keygen", "-y", "-f", str(key)])
        if public.returncode:
            raise SetupError("repository_key_failed")
    ssh = [
        "ssh",
        "-F",
        "/dev/null",
        "-i",
        str(key),
        "-o",
        "IdentitiesOnly=yes",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=15",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "UserKnownHostsFile=" + str(directory / "known_hosts"),
    ]
    if userspace_config():
        ssh += [
            "-o",
            "ProxyCommand=" + shlex.join(tailscale_command() + ["nc", "%h", "%p"]),
        ]
    return parsed, public.stdout.strip(), ssh


def verify(remote, branch, ssh, parsed):
    env = {"GIT_SSH_COMMAND": shlex.join(ssh), "GIT_LFS_SKIP_SMUDGE": "1"}
    if run(["git", "ls-remote", remote, "HEAD"], env=env).returncode:
        raise SetupError("repository_read_denied")
    # Fetch into a throwaway bare repository. A write check must happen before
    # input publication, and must never create a task/default branch itself.
    with tempfile.TemporaryDirectory(prefix="xnaut-access-") as temp:
        if run(["git", "init", "--bare", temp]).returncode:
            raise SetupError("repository_check_failed")
        if run(
            ["git", "-C", temp, "fetch", "--depth=1", remote, "HEAD"], env=env
        ).returncode:
            raise SetupError("repository_read_denied")
        if run(
            [
                "git",
                "-C",
                temp,
                "push",
                "--dry-run",
                remote,
                "FETCH_HEAD:refs/heads/" + branch,
            ],
            env=env,
        ).returncode:
            raise SetupError("repository_write_denied")
    # LFS uses separate authorization; a successful Git probe is insufficient.
    target = parsed.username + "@" + parsed.hostname
    auth = run(
        ssh
        + [
            "-p",
            str(parsed.port or 22),
            target,
            "git-lfs-authenticate " + shlex.quote(parsed.path.lstrip("/")) + " upload",
        ]
    )
    if auth.returncode:
        raise SetupError("lfs_authorization_failed")
    try:
        data = json.loads(auth.stdout)
        href = data["href"].rstrip("/") + "/objects/batch"
        if urllib.parse.urlparse(href).scheme not in ("https", "http"):
            raise ValueError("unsupported LFS URL")
        headers = {
            **data.get("header", {}),
            "Content-Type": "application/vnd.git-lfs+json",
            "Accept": "application/vnd.git-lfs+json",
        }
        request = urllib.request.Request(
            href,
            data=json.dumps(
                {"operation": "upload", "transfers": ["basic"], "objects": []}
            ).encode(),
            headers=headers,
        )

        # Signed auth must not follow redirects to an unrelated origin.
        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, *args, **kwargs):
                return None

        proxy = userspace_config()
        handlers = [NoRedirect]
        if proxy:
            address = "http://127.0.0.1:" + str(proxy["port"])
            handlers.append(
                urllib.request.ProxyHandler({"http": address, "https": address})
            )
        with urllib.request.build_opener(*handlers).open(
            request, timeout=20
        ) as response:
            if response.status != 200:
                raise ValueError("LFS authorization rejected")
    except urllib.error.HTTPError as error:
        error.close()
        raise SetupError("lfs_upload_unavailable")
    except Exception:
        raise SetupError("lfs_upload_unavailable")


def main():
    data = json.load(sys.stdin)
    remote = data["remote"]
    parsed, public, ssh = identity(remote)
    operation = data["operation"]
    if operation == "prepare":
        network(parsed.hostname, parsed.port or 22, data.get("network", {}))
        api = urllib.parse.urlparse(data.get("api_url", ""))
        if api.hostname:
            network(
                api.hostname,
                api.port or (443 if api.scheme == "https" else 80),
                data.get("network", {}),
            )
    elif operation == "verify":
        verify(remote, data["branch"], ssh, parsed)
    else:
        raise SetupError("unknown_bootstrap_operation")
    # Network setup can have enabled userspace routing after initial identity.
    parsed, public, ssh = identity(remote)
    proxy = userspace_config()
    return {
        "ok": True,
        "public_key": public,
        "ssh_command": shlex.join(ssh),
        "http_proxy": "http://127.0.0.1:" + str(proxy["port"]) if proxy else None,
    }


if __name__ == "__main__":
    try:
        print(json.dumps(main()))
    except SetupError as error:
        print(json.dumps({"ok": False, "error": str(error)}))
        sys.exit(1)
    except Exception:
        # Never expose process output, auth URLs, tokens, or signed LFS headers.
        print(json.dumps({"ok": False, "error": "worker_bootstrap_failed"}))
        sys.exit(1)
