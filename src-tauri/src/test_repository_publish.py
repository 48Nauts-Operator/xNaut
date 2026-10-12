"""Real Git publication and durable retry; LFS transport itself is stubbed."""

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import repository_publish as publisher


class PublisherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name).resolve()
        self.worker = self.root / "worker"
        self.worker.mkdir()
        self.previous = Path.cwd()
        os.chdir(self.worker)
        self.remote = self.root / "remote.git"
        self.git("init", "--bare", str(self.remote))
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Publisher test")
        self.git("config", "user.email", "fixture@example.invalid")
        Path("source.txt").write_text("baseline\n")
        self.git("add", "source.txt")
        self.git("commit", "-m", "baseline")
        source = self.git("rev-parse", "HEAD")
        self.git("remote", "add", "origin", str(self.remote))
        self.git("push", "origin", "main")
        self.branch = "xnaut/runs/fixture"
        self.git("switch", "-c", self.branch)
        Path("source.txt").write_text("authorized implementation\n")
        self.git("commit", "-am", "implement task")
        self.head = self.git("rev-parse", "HEAD")
        self.artifacts = Path(".xnaut/runs/fixture")
        self.artifacts.mkdir(parents=True)
        self.handback = self.artifacts / "handback.json"
        self.valid = {
            "summary": "Implemented the assigned task",
            "files_changed": ["source.txt"],
            "commits": [self.head],
            "how_verified": [{"command": "python3 -m unittest", "result": "passed"}],
            "not_finished": "nothing",
            "confidence": "high",
        }
        Path(".git/xnaut-transfer.json").write_text(
            json.dumps(
                {
                    "run_id": "fixture",
                    "source_sha": source,
                    "branch": self.branch,
                    "remote": str(self.remote),
                    "artifacts": str(self.artifacts),
                }
            )
        )
        original_git = publisher.git

        def git_without_lfs(*args, **kwargs):
            if args[:1] == ("lfs",):
                return subprocess.CompletedProcess(args, 0, "", "")
            return original_git(*args, **kwargs)

        self.transport = patch.object(publisher, "git", git_without_lfs)
        self.transport.start()

    def tearDown(self):
        self.transport.stop()
        os.chdir(self.previous)
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.check_output(
            ["git", *args], text=True, stderr=subprocess.PIPE
        ).strip()

    def test_invalid_completion_publishes_failure_and_accepts_explicit_repair(self):
        self.handback.write_text('{"summary": broken JSON')
        Path("unrelated.txt").write_text("not part of this publication\n")
        self.git("add", "unrelated.txt")
        publisher.publish(0)
        result = json.loads((self.artifacts / "result.json").read_text())
        self.assertEqual(result["exit_code"], 0)
        self.assertEqual(result["handback_error"], "missing_or_invalid_handback")
        self.assertEqual(self.git("diff", "--cached", "--name-only"), "unrelated.txt")
        self.handback.write_text(json.dumps(self.valid))
        publisher.publish(0)
        result = json.loads((self.artifacts / "result.json").read_text())
        self.assertIsNone(result["handback_error"])
        self.assertEqual(self.git("show", "HEAD:source.txt"), "authorized implementation")
        self.assertEqual(self.git("diff", "--cached", "--name-only"), "unrelated.txt")

    def test_missing_or_wrongly_typed_handbacks_publish_no_success_claim(self):
        for value in [None, [], {"summary": 2}, {"commits": [1]},
                      {"confidence": 0.9}, {"how_verified": [{"command": "test", "result": 0}]}]:
            with self.subTest(value=value):
                (self.artifacts / "result.json").unlink(missing_ok=True)
                if value is None:
                    self.handback.unlink(missing_ok=True)
                else:
                    self.handback.write_text(json.dumps(value))
                publisher.publish(0)
                result = json.loads((self.artifacts / "result.json").read_text())
                self.assertEqual(result["handback_error"], "missing_or_invalid_handback")
                self.assertEqual(self.git("show", "HEAD:source.txt"), "authorized implementation")

    def test_crash_without_handback_still_publishes_failure_evidence(self):
        publisher.publish(1)
        result = json.loads((self.artifacts / "result.json").read_text())
        self.assertEqual(result["exit_code"], 1)
        self.assertEqual(
            json.loads(Path(".git/xnaut-upload.json").read_text())["state"], "pushed"
        )

    def test_oversized_handback_is_preserved_but_marked_invalid(self):
        self.handback.write_text(json.dumps({"summary": "x" * (256 * 1024)}))
        publisher.publish(0)
        self.assertEqual(json.loads((self.artifacts / "result.json").read_text())["handback_error"],
                         "missing_or_invalid_handback")


if __name__ == "__main__":
    unittest.main()
