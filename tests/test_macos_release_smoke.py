"""Pure fixtures: never start an application or alter the current home directory."""

import importlib.util
import json
import os
import plistlib
import shutil
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
    def test_draft_lookup_uses_numeric_id_and_binds_both_responses(self):
        release = {
            "id": 405192386,
            "tag_name": "v1.30.1",
            "draft": True,
            "prerelease": False,
        }
        selected = {"databaseId": 405192386, "tagName": "v1.30.1"}
        with patch.object(
            SMOKE, "command", side_effect=[json.dumps(selected), json.dumps(release)]
        ) as invoke:
            self.assertEqual(SMOKE.release_metadata("Owner/Repo", "v1.30.1"), release)
            self.assertEqual(
                invoke.call_args_list[0].args,
                (
                    "gh",
                    "release",
                    "view",
                    "v1.30.1",
                    "--repo",
                    "Owner/Repo",
                    "--json",
                    "databaseId,tagName",
                ),
            )
            self.assertEqual(
                invoke.call_args_list[1].args,
                ("gh", "api", "repos/Owner/Repo/releases/405192386"),
            )
        for changed in [
            {**selected, "databaseId": "405192386"},
            {**selected, "databaseId": True},
            {**selected, "databaseId": -1},
            {**selected, "tagName": "v1.29.3"},
        ]:
            with (
                patch.object(
                    SMOKE, "command", return_value=json.dumps(changed)
                ) as invoke,
                self.assertRaises(RuntimeError),
            ):
                SMOKE.release_metadata("Owner/Repo", "v1.30.1")
            self.assertEqual(invoke.call_count, 1)
        for changed in [
            {**release, "id": 99},
            {**release, "tag_name": "v1.29.3"},
            {**release, "prerelease": True},
        ]:
            with (
                patch.object(
                    SMOKE,
                    "command",
                    side_effect=[json.dumps(selected), json.dumps(changed)],
                ),
                self.assertRaises(RuntimeError),
            ):
                SMOKE.release_metadata("Owner/Repo", "v1.30.1")

    def test_application_source_is_separate_and_exact_commit_bound(self):
        with tempfile.TemporaryDirectory() as scratch:
            source = Path(scratch) / "application"
            source.mkdir()

            def git(*args):
                return subprocess.check_output(
                    ["git", "-C", str(source), *args], text=True
                ).strip()

            git("init", "-q")
            git("config", "user.name", "Fixture")
            git("config", "user.email", "fixture@example.invalid")
            config = {
                "version": "1.30.1",
                "identifier": SMOKE.IDENTIFIER,
                "plugins": {"updater": {"pubkey": "committed-key"}},
            }
            path = source / "src-tauri/tauri.conf.json"
            path.parent.mkdir()
            path.write_text(json.dumps(config))
            git("add", ".")
            git("commit", "-qm", "application source")
            sha = git("rev-parse", "HEAD")
            # The harness runs outside this separate checkout. Its HEAD need
            # not equal the application SHA, but the candidate tag must.
            self.assertEqual(
                SMOKE.application_identity(source, sha, sha + "\n", "v1.30.1"), config
            )
            path.write_text(json.dumps({**config, "plugins": {}}))
            self.assertEqual(
                SMOKE.application_identity(source, sha, sha, "v1.30.1"), config
            )
            for expected, resolved, tag in [
                (sha[:8], sha, "v1.30.1"),
                ("a" * 40, sha, "v1.30.1"),
                (sha, "b" * 40, "v1.30.1"),
                (sha, sha, "v1.30.0"),
            ]:
                with self.assertRaises(RuntimeError):
                    SMOKE.application_identity(source, expected, resolved, tag)

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
                        "providers": [],
                        "default_has_credential": False,
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

    def test_first_load_local_provider_defaults_are_allowed_without_credentials(self):
        defaults = [
            {"name": name, "endpoint": endpoint, "has_credential": False}
            for name, endpoint in [
                ("lmstudio", "http://localhost:1234/v1"),
                ("ollama", "http://localhost:11434/v1"),
                ("nautgate", "http://localhost:8090/v1"),
            ]
        ]
        SMOKE.local_providers_only(defaults)
        for changed in [
            {"name": "openai"},
            {"endpoint": "https://remote.example/v1"},
            {"endpoint": "http://token@localhost:1234/v1"},
            {"endpoint": "http://localhost:1234/v1?api_key=secret"},
            {"endpoint": "http://localhost:1234/v1#secret"},
            {"has_credential": True},
        ]:
            with self.assertRaisesRegex(RuntimeError, "nonlocal or credentialed"):
                SMOKE.local_providers_only([{**defaults[0], **changed}])

    def test_readonly_dmg_equivalence_detaches_on_success_and_mismatch(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            tested = root / "tested.app"
            (tested / "Contents/MacOS").mkdir(parents=True)
            (tested / "Contents/MacOS/xnaut").write_bytes(b"same executable")
            for fault in (
                None,
                "version",
                "binary",
                "cdhash",
                "signature",
                "missing_hash",
                "duplicate_hash",
            ):
                with self.subTest(fault=fault):
                    mounts, calls = [], []

                    def attach(args, fault=fault, mounts=mounts, **_):
                        self.assertIn("-readonly", args)
                        self.assertIn("-nobrowse", args)
                        self.assertIn("-noautoopen", args)
                        mount = Path(args[args.index("-mountpoint") + 1])
                        mounts.append(mount)
                        app = mount / "xNAUT.app"
                        (app / "Contents/MacOS").mkdir(parents=True)
                        (app / "Contents/MacOS/xnaut").write_bytes(
                            b"different" if fault == "binary" else b"same executable"
                        )
                        (app / "Contents/Info.plist").write_bytes(
                            plistlib.dumps(
                                {
                                    "CFBundleIdentifier": SMOKE.IDENTIFIER,
                                    "CFBundleShortVersionString": "1.29.3"
                                    if fault == "version"
                                    else "1.30.0",
                                }
                            )
                        )
                        return subprocess.CompletedProcess(
                            args,
                            0,
                            stdout=plistlib.dumps(
                                {"system-entities": [{"mount-point": str(mount)}]}
                            ),
                        )

                    def command(*args, fault=fault, mounts=mounts, calls=calls):
                        calls.append(args)
                        if args[0] == "hdiutil":
                            self.assertEqual(
                                args, ("hdiutil", "detach", str(mounts[0]))
                            )
                            shutil.rmtree(mounts[0] / "xNAUT.app")
                        elif "--verify" in args and fault == "signature":
                            raise RuntimeError("bad signature")
                        # XNAUT-476: macOS display level 2 omits CDHash. Do not
                        # let the fake signing tool hide inadequate verbosity.
                        if args[:3] != ("codesign", "-d", "--verbose=4"):
                            return "Identifier=com.xnaut.app"
                        if fault == "missing_hash":
                            return "Identifier=com.xnaut.app"
                        if fault == "duplicate_hash":
                            return "CDHash=abcdef\nCDHash=123456"
                        return "CDHash=abcdef" if fault != "cdhash" else "CDHash=123456"

                    with (
                        patch.object(SMOKE.subprocess, "run", side_effect=attach),
                        patch.object(SMOKE, "command", side_effect=command),
                    ):
                        if fault:
                            with self.assertRaises(RuntimeError):
                                SMOKE.dmg_equivalence(
                                    root / "asset.dmg",
                                    tested,
                                    "1.30.0",
                                    "CDHash=abcdef",
                                )
                        else:
                            self.assertTrue(
                                SMOKE.dmg_equivalence(
                                    root / "asset.dmg",
                                    tested,
                                    "1.30.0",
                                    "CDHash=abcdef",
                                )["mounted_readonly"]
                            )
                    self.assertEqual(calls[-1], ("hdiutil", "detach", str(mounts[0])))
                    self.assertFalse(mounts[0].exists())


if __name__ == "__main__":
    unittest.main()
