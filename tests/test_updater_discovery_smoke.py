"""Offline rejection fixtures; actual native discovery is a separate CI gate."""

import copy
import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "updater_smoke", Path(__file__).parents[1] / "scripts/updater-discovery-smoke.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class DiscoveryTests(unittest.TestCase):
    def values(self, target="darwin-aarch64"):
        filename = {
            "darwin-aarch64": "xNAUT-macos-aarch64.app.tar.gz",
            "darwin-x86_64": "xNAUT-macos-x64.app.tar.gz",
            "windows-x86_64": "xNAUT-1.30.0-windows-x64.msi",
        }[target]
        manifest = {
            "version": "1.30.0",
            "platforms": {
                target: {
                    "url": f"https://github.com/48Nauts/xnaut/releases/download/v1.30.0/{filename}",
                    "signature": "published-signature",
                }
            },
        }
        return {
            "appVersion": "1.29.3",
            "currentVersion": "1.29.3",
            "version": "1.30.0",
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

    def test_no_fallback_wrong_version_or_unclosed_resource(self):
        for key, invalid in [
            ("appVersion", "1.30.0"),
            ("currentVersion", "1.29.2"),
            ("version", "1.30.1"),
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
        value["rawJson"]["version"] = "1.29.3"
        with self.assertRaises(RuntimeError):
            module.validate_discovery(value, feed, "48Nauts/xnaut", "darwin-aarch64")
        value, feed = self.values()
        with self.assertRaises(RuntimeError):
            module.validate_discovery(value, feed, "48Nauts/xnaut", "windows-x86_64")

    def test_public_release_rejects_draft_prerelease_and_unpublished(self):
        valid = {
            "tag_name": "v1.30.0",
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
                "tag_name": "v1.29.3",
                "assets": [
                    {
                        "name": "old.msi",
                        "browser_download_url": "https://github.com/48Nauts/xnaut/releases/download/v1.29.3/old.msi",
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
            global.window={__TAURI__:{app:{getVersion:async()=> '1.29.3'},updater:{
              check:async(options)=> {calls.push(options); return {available:true,
                currentVersion:'1.29.3',version:'1.30.0',rawJson:{version:'1.30.0'},
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
