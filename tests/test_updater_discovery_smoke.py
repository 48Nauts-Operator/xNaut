"""Offline rejection fixtures; actual native discovery is a separate CI gate."""

import copy
import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location(
    "updater_smoke", Path(__file__).parents[1] / "scripts/updater-discovery-smoke.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class DiscoveryTests(unittest.TestCase):
    def test_release_versions_reject_nonstable_and_nonforward_pairs(self):
        self.assertEqual(
            module.release_versions("1.30.6", "1.30.7"), ("1.30.6", "1.30.7")
        )
        self.assertEqual(
            module.release_versions("1.9.9", "1.10.0"), ("1.9.9", "1.10.0")
        )
        for old, new in [
            ("1.30.7", "1.30.7"),
            ("1.30.7", "1.30.6"),
            ("v1.30.6", "1.30.7"),
            ("1.30.6", "1.30.7-rc.1"),
            ("1.30.6", "../1.30.7"),
            ("1.30.6", "1.030.7"),
        ]:
            with self.subTest(old=old, new=new), self.assertRaises(RuntimeError):
                module.release_versions(old, new)

    def test_selected_release_pair_controls_native_and_platform_validation(self):
        for target in ("darwin-aarch64", "darwin-x86_64", "windows-x86_64"):
            old_value, old_feed = self.values(target)
            with (
                patch.object(module, "OLD", "1.30.6"),
                patch.object(module, "NEW", "1.30.7"),
            ):
                value = json.loads(
                    json.dumps(old_value)
                    .replace("1.30.6", "1.30.7")
                    .replace("1.30.4", "1.30.6")
                )
                feed = value["rawJson"]
                self.assertEqual(
                    module.validate_discovery(value, feed, "48Nauts/xnaut", target),
                    feed["platforms"][target],
                )
                with self.assertRaises(RuntimeError):
                    module.validate_discovery(
                        old_value, old_feed, "48Nauts/xnaut", target
                    )

    def test_ci_token_only_authenticates_github_api_metadata(self):
        urls = [
            ("https://api.github.com/repos/48Nauts/xnaut/releases/latest", True),
            ("https://api.github.com/repos/48Nauts/xnaut/commits/v1.30.6", True),
            (
                "https://github.com/48Nauts/xnaut/releases/download/v1.30.6/latest.json",
                False,
            ),
            ("https://api.github.com.example.test/releases/latest", False),
            ("http://api.github.com/repos/48Nauts/xnaut/releases/latest", False),
        ]
        for url, authenticated in urls:
            with (
                self.subTest(url=url),
                patch.dict(os.environ, {"GH_TOKEN": "fixture-ci-token"}, clear=True),
                patch.object(module.urllib.request, "urlopen") as open_url,
            ):
                open_url.return_value.__enter__.return_value.read.return_value = b"body"
                self.assertEqual(module.fetch(url), b"body")
                request = open_url.call_args.args[0]
                self.assertEqual(request.full_url, url)
                self.assertEqual(
                    request.get_header("Authorization"),
                    "Bearer fixture-ci-token" if authenticated else None,
                )

    def test_public_metadata_remains_available_without_ci_token(self):
        with (
            patch.dict(os.environ, {}, clear=True),
            patch.object(module.urllib.request, "urlopen") as open_url,
        ):
            open_url.return_value.__enter__.return_value.read.return_value = b"public"
            self.assertEqual(
                module.fetch(
                    "https://api.github.com/repos/48Nauts/xnaut/releases/latest"
                ),
                b"public",
            )
            self.assertIsNone(open_url.call_args.args[0].get_header("Authorization"))

    def values(self, target="darwin-aarch64"):
        filename = {
            "darwin-aarch64": "xNAUT-macos-aarch64.app.tar.gz",
            "darwin-x86_64": "xNAUT-macos-x64.app.tar.gz",
            "windows-x86_64": "xNAUT-1.30.6-windows-x64.msi",
        }[target]
        manifest = {
            "version": "1.30.6",
            "platforms": {
                target: {
                    "url": f"https://github.com/48Nauts/xnaut/releases/download/v1.30.6/{filename}",
                    "signature": "published-signature",
                }
            },
        }
        return {
            "appVersion": "1.30.4",
            "currentVersion": "1.30.4",
            "version": "1.30.6",
            "available": True,
            "resourceClosed": True,
            "mechanism": "tauri-plugin-updater",
            "rawJson": copy.deepcopy(manifest),
        }, manifest

    def test_all_three_native_targets(self):
        for target in ("darwin-aarch64", "darwin-x86_64", "windows-x86_64"):
            value, feed = self.values(target)
            self.assertEqual(
                module.validate_discovery(value, feed, "48Nauts/xnaut", target),
                feed["platforms"][target],
            )

    def test_utf8_native_discovery_survives_windows_default_encoding(self):
        value, feed = self.values("windows-x86_64")
        feed["notes"] = "Owner’s review — Zürich / 東京 / 🚀"
        value["rawJson"] = copy.deepcopy(feed)
        original_open = Path.open

        def windows_open(
            path, mode="r", buffering=-1, encoding=None, errors=None, newline=None
        ):
            return original_open(
                path,
                mode,
                buffering,
                encoding
                if encoding not in (None, "locale") or "b" in mode
                else "cp1252",
                errors,
                newline,
            )

        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            config = root / "config"
            config.mkdir()
            log = config / "debug.log"
            marker = "release-smoke-fixed"
            log.write_text(
                marker
                + " "
                + json.dumps({"ok": True, "value": value}, ensure_ascii=False)
                + "\ncredential fixture-token\n",
                encoding="utf-8",
            )
            bridge = module.smoke.Bridge(
                1, "fixture-token", log, Mock(poll=lambda: None)
            )
            report = {"status": "passed"}
            with (
                patch.object(Path, "open", windows_open),
                patch.object(module.smoke.secrets, "token_hex", return_value="fixed"),
                patch.object(bridge, "request"),
            ):
                actual = bridge.evaluate("native check")
                self.assertEqual(actual, value)
                self.assertEqual(
                    module.record_discovery(
                        root, report, actual, feed, "48Nauts/xnaut", "windows-x86_64"
                    ),
                    feed["platforms"]["windows-x86_64"],
                )
                module.finalize_report(root, report, config, "fixture-token")
            self.assertEqual(
                json.loads((root / "discovery.json").read_text(encoding="utf-8")), value
            )
            self.assertEqual(
                json.loads((root / "run.json").read_text(encoding="utf-8"))[
                    "discovery"
                ],
                value,
            )
            copied = (root / "debug.log").read_text(encoding="utf-8")
            self.assertIn(feed["notes"], copied)
            self.assertNotIn("fixture-token", copied)

    def test_rejected_payload_and_original_failure_survive_diagnostic_copy_errors(self):
        value, feed = self.values()
        value["rawJson"]["notes"] = "Unexpected — 東京"
        for operation in ("read", "write"):
            with (
                self.subTest(operation=operation),
                tempfile.TemporaryDirectory() as scratch,
            ):
                root = Path(scratch)
                config = root / "config"
                config.mkdir()
                (config / "debug.log").write_text("native diagnostic", encoding="utf-8")
                report = {"status": "failed"}
                with self.assertRaisesRegex(
                    RuntimeError, "differs from public feed"
                ) as failure:
                    module.record_discovery(
                        root, report, value, feed, "48Nauts/xnaut", "darwin-aarch64"
                    )
                report["error"] = str(failure.exception)
                self.assertEqual(
                    json.loads((root / "discovery.json").read_text(encoding="utf-8")),
                    value,
                )
                original_open = Path.open

                def denied(path, *args, **kwargs):
                    blocked = (
                        config / "debug.log"
                        if operation == "read"
                        else root / "debug.log"
                    )
                    if path == blocked:
                        raise PermissionError("fixture diagnostic denial")
                    return original_open(path, *args, **kwargs)

                with (
                    patch.object(Path, "open", denied),
                    self.assertRaisesRegex(RuntimeError, "see run.json"),
                ):
                    module.finalize_report(root, report, config, "fixture-token")
                saved = json.loads((root / "run.json").read_text(encoding="utf-8"))
                self.assertEqual(saved["discovery"], value)
                self.assertEqual(
                    saved["error"], "Native response differs from public feed"
                )
                self.assertEqual(saved["status"], "failed")
                self.assertEqual(
                    saved["diagnostic_copy_errors"],
                    [{"file": "debug.log", "error_type": "PermissionError"}],
                )
                self.assertTrue(saved["finished"])

    def test_no_fallback_wrong_version_or_unclosed_resource(self):
        for key, invalid in [
            ("appVersion", "1.30.6"),
            ("currentVersion", "1.29.2"),
            ("version", "1.31.0"),
            ("available", False),
            ("resourceClosed", False),
            ("mechanism", "github-api"),
        ]:
            value, feed = self.values()
            value[key] = invalid
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                module.validate_discovery(
                    value, feed, "48Nauts/xnaut", "darwin-aarch64"
                )

    def test_foreign_platform_url_missing_signature_or_stale_feed(self):
        for key, invalid in [("url", "https://example.com/foreign"), ("signature", "")]:
            value, feed = self.values()
            feed["platforms"]["darwin-aarch64"][key] = invalid
            value["rawJson"] = copy.deepcopy(feed)
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                module.validate_discovery(
                    value, feed, "48Nauts/xnaut", "darwin-aarch64"
                )
        value, feed = self.values()
        value["rawJson"]["version"] = "1.30.4"
        with self.assertRaises(RuntimeError):
            module.validate_discovery(value, feed, "48Nauts/xnaut", "darwin-aarch64")
        value, feed = self.values()
        with self.assertRaises(RuntimeError):
            module.validate_discovery(value, feed, "48Nauts/xnaut", "windows-x86_64")

    def test_public_release_rejects_draft_prerelease_and_unpublished(self):
        valid = {
            "tag_name": "v1.30.6",
            "published_at": "2026-10-06T00:00:00Z",
            "draft": False,
            "prerelease": False,
        }
        for key, invalid in [
            ("draft", True),
            ("prerelease", True),
            ("published_at", None),
            ("tag_name", "v1.31.0"),
        ]:
            row = dict(valid, **{key: invalid})
            with (
                patch.object(module, "fetch", return_value=json.dumps(row).encode()),
                self.assertRaises(RuntimeError),
            ):
                module.public_release("48Nauts/xnaut", "latest")

    def test_asset_hash_and_url_bind_exact_download(self):
        with tempfile.TemporaryDirectory() as scratch:
            path = Path(scratch)
            row = {
                "tag_name": "v1.30.4",
                "assets": [
                    {
                        "name": "old.msi",
                        "browser_download_url": "https://github.com/48Nauts/xnaut/releases/download/v1.30.4/old.msi",
                        "digest": "sha256:wrong",
                    }
                ],
            }
            with (
                patch.object(module, "fetch", return_value=b"wrong bytes"),
                self.assertRaises(RuntimeError),
            ):
                module.asset(row, "old.msi", path, "48Nauts/xnaut")
            row["assets"][0]["browser_download_url"] = "https://foreign.example/old.msi"
            with (
                patch.object(module, "fetch") as fetched,
                self.assertRaises(RuntimeError),
            ):
                module.asset(row, "old.msi", path, "48Nauts/xnaut")
            fetched.assert_not_called()

    def test_refuses_owner_profile_and_nonhosted_without_home_writes(self):
        with tempfile.TemporaryDirectory() as scratch:
            home = Path(scratch)
            profile = home / "Library/Application Support/xnaut"
            profile.mkdir(parents=True)
            env = {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"}
            with (
                patch.dict(os.environ, env, clear=True),
                patch.object(module.Path, "home", return_value=home),
                self.assertRaises(RuntimeError),
            ):
                module.fresh_config("Darwin")
            with (
                patch.dict(os.environ, {}, clear=True),
                self.assertRaises(RuntimeError),
            ):
                module.fresh_config("Darwin")

    def test_native_expression_awaits_plugin_and_resource_close(self):
        expression = module.discovery_expression()
        script = """
          const expression=EXPRESSION;
          async function trial(closeFails) {
            const calls=[];
            global.window={__TAURI__:{app:{getVersion:async()=> '1.30.4'},updater:{
              check:async(options)=> {calls.push(options); return {available:true,
                currentVersion:'1.30.4',version:'1.30.6',rawJson:{version:'1.30.6'},
                close:async()=>{calls.push('closed');if(closeFails)throw Error('close failed');}};}
            }}};
            try {
              const value=await eval(expression);
              if(closeFails||!value.resourceClosed)throw Error('false success');
              if(JSON.stringify(calls)!=='[{"timeout":30000},"closed"]')throw Error('wrong options');
            } catch(error) {if(!closeFails||String(error)!=='Error: close failed')throw error;}
          }
          (async()=>{await trial(false);await trial(true);})().catch(e=>{console.error(e);process.exitCode=1;});
        """.replace("EXPRESSION", json.dumps(expression))
        subprocess.run(
            ["node", "-e", script], check=True, capture_output=True, text=True
        )

    def test_expression_uses_only_real_plugin_check_and_closes(self):
        expression = module.discovery_expression()
        self.assertIn("api.updater.check({timeout:30000})", expression)
        self.assertIn("finally {await update.close();}", expression)
        for forbidden in ("fetch(", "download(", "install(", "target:", "proxy:"):
            self.assertNotIn(forbidden, expression)


if __name__ == "__main__":
    unittest.main()
