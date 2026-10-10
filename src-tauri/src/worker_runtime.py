"""Read-only runtime admission. Never emits credentials or starts an agent task."""
import json
import os
import shutil
import subprocess
import sys


def check(request):
    env = dict(os.environ, **request["env"])
    binary = request["binary"]
    if not shutil.which(binary, path=env.get("PATH")):
        return "binary_missing"
    if request.get("standard_codex"):
        result = subprocess.run([binary, "login", "status"], env=env,
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15)
        return "ready" if result.returncode == 0 else "authentication_missing"
    if request.get("standard_pi"):
        args = request["args"]

        def option(flag):
            value = None
            for i, arg in enumerate(args):
                if arg == flag and i + 1 < len(args):
                    value = args[i + 1]
                elif arg.startswith(flag + "="):
                    value = arg.split("=", 1)[1]
            return value

        directory = env.get("PI_CODING_AGENT_DIR") or os.path.join(env.get("HOME", ""), ".pi", "agent")
        try:
            with open(os.path.join(directory, "settings.json")) as source:
                settings = json.load(source)
        except FileNotFoundError:
            settings = {}
        provider = option("--provider") or settings.get("defaultProvider")
        model = request.get("model") or option("--model") or settings.get("defaultModel")
        command = [binary, "auth", "check", "--json"]
        if provider:
            command += ["--provider", provider]
        if model:
            command += ["--model", model]
        if not provider and not model:
            command += ["--provider", "google"]  # Pi's documented CLI default.
        result = subprocess.run(command, env=env, stdout=subprocess.PIPE,
                                stderr=subprocess.DEVNULL, timeout=15)
        receipt = json.loads(result.stdout)
        return "ready" if result.returncode == 0 and receipt.get("status") == "ready" else "authentication_missing"
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
