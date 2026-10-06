"""Pure fixtures: never start an application or alter the current home directory."""

import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "macos_release_smoke", Path(__file__).parents[1] / "scripts/macos-release-smoke.py"
)
SMOKE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SMOKE)


class ProductionSmokeTests(unittest.TestCase):
    def test_launch_refuses_owner_account_existing_profile_and_overrides(self):
        with tempfile.TemporaryDirectory() as scratch:
            config = Path(scratch) / "profile"
            env = {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"}
            SMOKE.fresh_runner(config, env, "Darwin")
            for wrong in [
                {},
                {**env, "RUNNER_ENVIRONMENT": "self-hosted"},
                {**env, "XNAUT_LOOP_ACCEPTANCE_ROOT": scratch},
            ]:
                with self.assertRaises(RuntimeError):
                    SMOKE.fresh_runner(config, wrong, "Darwin")
            with self.assertRaises(RuntimeError):
                SMOKE.fresh_runner(config, env, "Linux")
            config.mkdir()
            sentinel = config / "settings.json"
            sentinel.write_text("owner data")
            with self.assertRaises(RuntimeError):
                SMOKE.fresh_runner(config, env, "Darwin")
            self.assertEqual(sentinel.read_text(), "owner data")

    def test_exact_tag_asset_identity_and_published_hash_required(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            names = SMOKE.artifact_names("v1.30.0", "aarch64")
            assets = []
            for name in names:
                path = root / name
                path.write_bytes(name.encode())
                assets.append({"name": name, "digest": "sha256:" + SMOKE.digest(path)})
            release = {
                "tag_name": "v1.30.0",
                "draft": True,
                "prerelease": False,
                "assets": assets,
            }
            self.assertEqual(
                len(SMOKE.verify_assets(release, "v1.30.0", "aarch64", root)), 4
            )
            for wrong in [
                {**release, "tag_name": "v1.29.3"},
                {**release, "prerelease": True},
                {**release, "assets": assets + [assets[0]]},
                {**release, "assets": assets[1:]},
            ]:
                with self.assertRaises(RuntimeError):
                    SMOKE.verify_assets(wrong, "v1.30.0", "aarch64", root)
            (root / names[0]).write_bytes(b"modified signed package")
            with self.assertRaisesRegex(RuntimeError, "digest mismatch"):
                SMOKE.verify_assets(release, "v1.30.0", "aarch64", root)
            with self.assertRaises(RuntimeError):
                SMOKE.artifact_names("v1.30.0; echo bypass", "aarch64")

    def test_updater_refuses_other_release_before_crypto(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            path = root / "latest.json"
            path.write_text(json.dumps({"version": "1.29.3"}))
            with self.assertRaisesRegex(RuntimeError, "version differs"):
                SMOKE.verify_updater(root, "v1.30.0", "aarch64", {}, "unused")
            path.write_text(
                json.dumps(
                    {
                        "version": "1.30.0",
                        "platforms": {
                            "darwin-aarch64": {"url": "https://wrong/release"}
                        },
                    }
                )
            )
            with (
                patch.dict(os.environ, {"GITHUB_REPOSITORY": "team/repo"}),
                self.assertRaisesRegex(RuntimeError, "outside the exact"),
            ):
                SMOKE.verify_updater(root, "v1.30.0", "aarch64", {}, "unused")

    def test_native_js_parses_and_rejects_wrong_native_identity(self):
        scripts = []

        class FakeBridge:
            def __init__(self, native_version="1.30.0"):
                self.native_version = native_version

            def evaluate(self, expression):
                scripts.append(expression)
                if "settings_get" in expression:
                    return {
                        "version": self.native_version,
                        "project_root": "/scratch/projects",
                        "role": "workstation",
                        "read_only": True,
                        "pm_enabled": False,
                        "forges": 0,
                        "providers": 0,
                    }
                return {"ok": True}

        recorded = []
        SMOKE.native_checks(
            FakeBridge(),
            "1.30.0",
            Path("/scratch/projects"),
            lambda name, _: recorded.append(name),
        )
        self.assertEqual(
            recorded, ["native_ipc", "settings_surface", "terminal_roundtrip"]
        )
        for expression in scripts:
            subprocess.run(
                ["node", "--check", "-"],
                input=expression,
                text=True,
                check=True,
                capture_output=True,
            )
        with self.assertRaisesRegex(RuntimeError, "version/settings"):
            SMOKE.native_checks(
                FakeBridge("1.29.3"),
                "1.30.0",
                Path("/scratch/projects"),
                lambda *_: self.fail("wrong binary passed"),
            )

    def test_process_exit_is_not_a_bridge_success(self):
        class Exited:
            @staticmethod
            def poll():
                return 1

        bridge = SMOKE.Bridge(1, "unused", Path("/unused"), Exited())
        with self.assertRaisesRegex(RuntimeError, "process exited"):
            bridge.request("/api/control/doctor")

    def test_alive_process_cannot_hide_background_panic_or_unhandled_error(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            log = root / "debug.log"
            log.write_text("[rejection] Tauri IPC bootstrap fell back to postMessage\n")
            SMOKE.runtime_errors(root)
            log.write_text("[uncaught] Settings native IPC broke\n")
            with self.assertRaisesRegex(RuntimeError, "Unhandled frontend"):
                SMOKE.runtime_errors(root)
            log.write_text("")
            (root / "rust-panics.log").write_text("PANIC in background coordinator\n")
            with self.assertRaisesRegex(RuntimeError, "background panic"):
                SMOKE.runtime_errors(root)


if __name__ == "__main__":
    unittest.main()
