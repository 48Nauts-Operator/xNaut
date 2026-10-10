"""Worker protocol tests. These use a local HTTP fixture, not a live LLM."""
import http.server
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[2] / "src-tauri/src"
spec = importlib.util.spec_from_file_location("worker_model", SOURCE / "worker_model.py")
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


class WorkerModelTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="xnaut-cloud-test-")
        self.root = Path(self.tmp.name)
        self.cloud = {"provider": "fixture", "endpoint": "https://models.example/v1",
                      "model": "fixture-model", "api_key": "private-fixture-key"}

    def tearDown(self):
        self.tmp.cleanup()

    def test_pi_has_explicit_isolated_model_auth_without_touching_worker_defaults(self):
        with patch.object(worker.Path, "home", return_value=self.root):
            global_pi = self.root / ".pi/agent"
            global_pi.mkdir(parents=True)
            prior = '{"defaultProvider":"unrelated-provider"}'
            (global_pi / "settings.json").write_text(prior)
            env = worker.cloud_environment(self.cloud, "run-A", "pi")
            other = worker.cloud_environment({**self.cloud, "model": "other-model"}, "run-B", "pi")
            directory = Path(env["PI_CODING_AGENT_DIR"])
            model = json.loads((directory / "models.json").read_text())["providers"]["xnaut-cloud"]
            self.assertEqual(model["models"], [{"id": "fixture-model"}])
            self.assertEqual(model["baseUrl"], self.cloud["endpoint"])
            self.assertEqual(model["apiKey"], self.cloud["api_key"])
            self.assertEqual(json.loads((directory / "auth.json").read_text())["xnaut-cloud"]["key"], self.cloud["api_key"])
            self.assertEqual(env["XNAUT_CLOUD_API_KEY"], self.cloud["api_key"])
            self.assertNotEqual(directory, Path(other["PI_CODING_AGENT_DIR"]))
            self.assertEqual((global_pi / "settings.json").read_text(), prior)
            self.assertEqual(directory.stat().st_mode & 0o777, 0o700)
            for path in directory.iterdir():
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            with self.assertRaises(FileExistsError):
                worker.cloud_environment(self.cloud, "run-A", "pi")

    def test_command_receives_private_environment_and_cleanup_even_on_failure(self):
        for exit_code in (0, 7):
            with patch.object(worker.Path, "home", return_value=self.root):
                worker.cloud_environment(self.cloud, "run", "codex")
                with patch.object(worker.subprocess, "call", return_value=exit_code) as child:
                    with self.assertRaises(SystemExit) as stopped:
                        worker.launch("run", ["codex", "--model", self.cloud["model"]])
                    self.assertEqual(stopped.exception.code, exit_code)
                    self.assertEqual(child.call_args.kwargs["env"]["XNAUT_CLOUD_API_KEY"], self.cloud["api_key"])
                    self.assertFalse(worker.cloud_directory("run").exists())

    def test_configuration_cannot_escape_private_directory(self):
        for identity in ("../outside", "/tmp/outside", "", "a/b", "a;echo"):
            with self.assertRaises(ValueError):
                worker.cloud_directory(identity)

    def test_probe_checks_model_auth_and_reachability_without_exposing_response(self):
        class API(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                if self.headers.get("Authorization") != "Bearer private-fixture-key":
                    self.send_response(401)
                else:
                    self.send_response(200)
                self.end_headers()
                self.wfile.write(b'{"data":[{"id":"fixture-model"}]}')

            def log_message(self, *_):
                pass

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), API)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            self.cloud["endpoint"] = f"http://127.0.0.1:{server.server_port}/v1"
            self.assertEqual(worker.cloud_probe(self.cloud), "ready")
            self.assertEqual(worker.cloud_probe({**self.cloud, "api_key": "wrong"}), "authentication_missing")
            self.assertEqual(worker.cloud_probe({**self.cloud, "model": "missing"}), "model_unavailable")
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
        self.assertEqual(worker.cloud_probe(self.cloud), "endpoint_unreachable")

    def test_production_probe_uses_same_pi_provider_and_cleans_probe_configuration(self):
        binary = self.root / "pi"
        binary.write_text("""#!/usr/bin/env python3
import json, os, pathlib, sys
assert sys.argv[1:]==['--no-extensions','--no-skills','--no-prompt-templates','--list-models','fixture-model']
root=pathlib.Path(os.environ['PI_CODING_AGENT_DIR'])
model=json.loads((root/'models.json').read_text())['providers']['xnaut-cloud']
assert model['models'][0]['id']=='fixture-model'
assert os.environ['XNAUT_CLOUD_API_KEY']=='private-fixture-key'
print('provider model context max-out')
print('xnaut-cloud fixture-model 128K 32K', file=sys.stderr)
""")
        binary.chmod(0o700)
        # Exercise the embedded scripts exactly as native admission concatenates
        # them; replace only network I/O with an explicit local fixture result.
        script = (SOURCE / "worker_model.py").read_text() + "\ncloud_probe = lambda _: 'ready'\n" + (SOURCE / "worker_runtime.py").read_text()
        request = {"binary": "pi", "env": {}, "args": ["--provider", "xnaut-cloud"],
                   "model": "fixture-model", "standard_pi": True, "cloud": self.cloud,
                   "cloud_id": "probe-test", "probe_only": True}
        result = subprocess.run([sys.executable, "-c", script], input=json.dumps(request),
            text=True, capture_output=True, env={**os.environ, "HOME": str(self.root), "PATH": str(self.root) + os.pathsep + os.environ["PATH"]})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {"status": "ready"})
        self.assertNotIn(self.cloud["api_key"], result.stdout + result.stderr)
        self.assertFalse((self.root / ".config/xnaut/cloud-models/probe-test").exists())

    def test_old_pi_without_shared_selection_is_never_given_auth_as_a_task(self):
        binary = self.root / "pi"
        binary.write_text("""#!/usr/bin/env python3
import sys
assert sys.argv[1:]==['auth','--help'], 'readiness attempted to start a task'
print('Usage: pi [options] [messages...]')
""")
        binary.chmod(0o700)
        request = {"binary": str(binary), "env": {}, "args": [], "model": "",
                   "standard_pi": True}
        result = subprocess.run([sys.executable, "-c", (SOURCE / "worker_runtime.py").read_text()],
            input=json.dumps(request), text=True, capture_output=True)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout), {"status": "runtime_unsupported"})


if __name__ == "__main__":
    unittest.main()
