"""Runtime admission and private cloud setup. Never starts an agent task."""

import json
import os
import shutil
import subprocess
import sys


def check_launch_files(request):
    if not request.get("standard_claude"):
        return "ready"
    args = request.get("args", [])
    index = 0
    while index < len(args):
        arg = args[index]
        index += 1
        if arg == "--mcp-config":
            values = []
            while index < len(args) and not args[index].startswith("-"):
                values.append(args[index])
                index += 1
            if not values:
                return "mcp_config_invalid"
        elif arg.startswith("--mcp-config="):
            values = [arg.split("=", 1)[1]]
        else:
            continue
        for value in values:
            try:
                if value.lstrip().startswith("{"):
                    config = json.loads(value)
                else:
                    with open(value) as source:
                        config = json.load(source)
                if not isinstance(config, dict) or not isinstance(
                    config.get("mcpServers"), dict
                ):
                    return "mcp_config_invalid"
            except FileNotFoundError:
                return "mcp_config_missing"
            except (OSError, ValueError):
                return "mcp_config_invalid"
    return "ready"


def check(request):
    # Validate files on the destination, before provider probes or credentials
    # are staged. A desktop absolute path is not a worker configuration.
    status = check_launch_files(request)
    if status != "ready":
        return status
    cloud = request.get("cloud")
    if not cloud:
        return check_runtime(request)
    # Native admission embeds worker_model.py before this script. Legacy
    # read-only probes can still run this file by itself without that helper.
    support = globals()
    status = support["subscription_status"](cloud, request["binary"])
    if status != "ready":
        return status
    status = support["cloud_probe"](cloud)
    if status != "ready":
        return status
    directory = support["cloud_directory"](request["cloud_id"])
    if directory.exists():
        return "check_failed"
    status = "check_failed"
    try:
        request = dict(request)
        request["env"] = dict(
            request["env"],
            **support["cloud_environment"](
                cloud, request["cloud_id"], request["binary"]
            ),
        )
        status = check_runtime(request)
        return status
    finally:
        if request.get("probe_only") or status != "ready":
            shutil.rmtree(directory, ignore_errors=True)


def check_runtime(request):
    env = globals().get(
        "runtime_environment", lambda injected: dict(os.environ, **injected)
    )(request["env"])
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
                [
                    binary,
                    "--no-extensions",
                    "--no-skills",
                    "--no-prompt-templates",
                    "--list-models",
                    request["model"],
                ],
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=15,
                text=True,
            )
            found = any(
                line.split()[:2] == ["xnaut-cloud", request["model"]]
                for line in (result.stdout + "\n" + result.stderr).splitlines()
            )
            return (
                "ready"
                if result.returncode == 0 and found
                else "authentication_missing"
            )

        help_result = subprocess.run(
            [binary, "auth", "--help"],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=8,
            text=True,
        )
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
