"""Shared cloud model bootstrap. Input is SSH stdin; output never contains keys.

Uses Pi's documented models.json + PI_CODING_AGENT_DIR configuration contract.
No worker-global Pi settings or credentials are modified.
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import urllib.error
import urllib.request


def cloud_directory(run_id):
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,100}", run_id):
        raise ValueError("Invalid cloud configuration identity")
    return Path.home() / ".config" / "xnaut" / "cloud-models" / run_id


def private_json(path, value):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as output:
        json.dump(value, output)


def cloud_probe(cloud):
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *_):
            return None  # Never forward provider credentials to another host.

    request = urllib.request.Request(
        cloud["endpoint"].rstrip("/") + "/models",
        headers={"Authorization": "Bearer " + cloud["api_key"],
                 "x-api-key": cloud["api_key"], "anthropic-version": "2023-06-01"},
    )
    try:
        with urllib.request.build_opener(NoRedirect).open(request, timeout=8) as response:
            catalog = json.load(response)
        models = catalog.get("data", catalog.get("models", []))
        return "ready" if any(m.get("id", m.get("name")) == cloud["model"] for m in models) else "model_unavailable"
    except urllib.error.HTTPError as error:
        status = "authentication_missing" if error.code in (401, 403) else "endpoint_unreachable"
        error.close()
        return status
    except Exception:
        return "endpoint_unreachable"


def cloud_environment(cloud, run_id, runtime):
    directory = cloud_directory(run_id)
    directory.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    directory.mkdir(mode=0o700)  # unique per attempt; never replace another run
    env = {"XNAUT_CLOUD_API_KEY": cloud["api_key"]}
    if runtime == "pi":
        pi = directory / "pi"
        pi.mkdir(mode=0o700)
        env["PI_CODING_AGENT_DIR"] = str(pi)
        api = "anthropic-messages" if cloud["provider"] in ("anthropic", "claude") else "openai-completions"
        private_json(pi / "models.json", {"providers": {"xnaut-cloud": {
            "baseUrl": cloud["endpoint"], "api": api,
            # Pi 0.74 requires this field even when auth.json supplies the key.
            # Both files are private and scoped to this run, never its repo.
            "apiKey": cloud["api_key"],
            "models": [{"id": cloud["model"]}],
        }}})
        private_json(pi / "settings.json", {"defaultProvider": "xnaut-cloud", "defaultModel": cloud["model"]})
        private_json(pi / "auth.json", {"xnaut-cloud": {"type": "api_key", "key": cloud["api_key"]}})
    elif runtime == "claude":
        env.update(ANTHROPIC_BASE_URL=cloud["endpoint"].removesuffix("/v1"),
                   ANTHROPIC_AUTH_TOKEN=cloud["api_key"], ANTHROPIC_API_KEY=cloud["api_key"])
    private_json(directory / "environment.json", env)
    return env


def launch(run_id, argv):
    directory = cloud_directory(run_id)
    try:
        with (directory / "environment.json").open() as source:
            env = dict(os.environ, **json.load(source))
        result = subprocess.call(argv, env=env)
    finally:
        shutil.rmtree(directory, ignore_errors=True)
    sys.exit(result)
