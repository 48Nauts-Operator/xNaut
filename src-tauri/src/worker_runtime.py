"""Read-only runtime admission. Never emits credentials or starts an agent task."""

import json
import os
import shutil
import subprocess
import sys


def check(request):
    cloud = request.get("cloud")
    if not cloud:
        return check_runtime(request)
    status = cloud_probe(cloud)
    if status != "ready":
        return status
    directory = cloud_directory(request["cloud_id"])
    if directory.exists():
        return "check_failed"
    status = "check_failed"
    try:
        request = dict(request)
        request["env"] = dict(request["env"], **cloud_environment(
            cloud, request["cloud_id"], request["binary"]))
        status = check_runtime(request)
        return status
    finally:
        if request.get("probe_only") or status != "ready":
            shutil.rmtree(directory, ignore_errors=True)


def check_runtime(request):
    env = dict(os.environ, **request["env"])
    binary = request["binary"]
    if not shutil.which(binary, path=env.get("PATH")):
        return "binary_missing"
    if request.get("standard_codex"):
        result = subprocess.run(
            [binary, "login", "status"],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=15,
        )
        return "ready" if result.returncode == 0 else "authentication_missing"
    if request.get("standard_pi"):
        args = request["args"]

        if request.get("cloud"):
            # Older supported Pi images have no `auth` subcommand: those words
            # would be interpreted as a task. Discovery is read-only on both
            # versions and verifies that Pi loaded the isolated model/auth.
            result = subprocess.run(
                [binary, "--no-extensions", "--no-skills", "--no-prompt-templates", "--list-models", request["model"]],
                env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                timeout=15, text=True,
            )
            found = any(line.split()[:2] == ["xnaut-cloud", request["model"]]
                        for line in (result.stdout + "\n" + result.stderr).splitlines())
            return "ready" if result.returncode == 0 and found else "authentication_missing"

        help_result = subprocess.run([binary, "auth", "--help"], env=env,
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=8, text=True)
        if "pi auth check" not in help_result.stdout:
            return "runtime_unsupported"

        def option(flag):
            value = None
            for i, arg in enumerate(args):
                if arg == flag and i + 1 < len(args):
                    value = args[i + 1]
                elif arg.startswith(flag + "="):
                    value = arg.split("=", 1)[1]
            return value

        directory = env.get("PI_CODING_AGENT_DIR") or os.path.join(
            env.get("HOME", ""), ".pi", "agent"
        )
        try:
            with open(os.path.join(directory, "settings.json")) as source:
                settings = json.load(source)
        except FileNotFoundError:
            settings = {}
        provider = option("--provider") or settings.get("defaultProvider")
        model = (
            request.get("model") or option("--model") or settings.get("defaultModel")
        )
        command = [binary, "auth", "check", "--json"]
        if provider:
            command += ["--provider", provider]
        if model:
            command += ["--model", model]
        if not provider and not model:
            command += ["--provider", "google"]  # Pi's documented CLI default.
        result = subprocess.run(
            command,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=15,
        )
        receipt = json.loads(result.stdout)
        return (
            "ready"
            if result.returncode == 0 and receipt.get("status") == "ready"
            else "authentication_missing"
        )
    return "ready"


if __name__ == "__main__":
    try:
        status = check(json.load(sys.stdin))
    except subprocess.TimeoutExpired:
        status = "check_timeout"
    except Exception:
        status = "check_failed"
    print(json.dumps({"status": status}))
    sys.exit(0 if status == "ready" else 1)
