#!/usr/bin/env python3
"""XNAUT-468: discover a public update through the real, old production plugin.

Manual CI only. No updater download/install, target override, or API fallback.
The downloaded old app is owned by this fresh hosted runner, never an owner app.
"""

import argparse
import importlib.util
import json
import os
import platform
import plistlib
import re
import subprocess
import tarfile
import time
import urllib.error
import urllib.request
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "production_smoke", Path(__file__).with_name("macos-release-smoke.py")
)
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)
require = smoke.require
OLD = "1.30.2"
NEW = "1.30.3"


def fetch(url):
    with urllib.request.urlopen(url, timeout=60) as response:
        return response.read()


def public_release(repo, tag):
    value = json.loads(fetch(f"https://api.github.com/repos/{repo}/releases/{tag}"))
    require(
        value.get("tag_name") in ("v" + OLD, "v" + NEW)
        and not value.get("draft")
        and not value.get("prerelease")
        and value.get("published_at"),
        "Expected an already published stable release",
    )
    return value


def asset(release, name, distribution, repo):
    rows = [row for row in release["assets"] if row["name"] == name]
    require(len(rows) == 1, "Missing or duplicate published asset: " + name)
    url = f"https://github.com/{repo}/releases/download/{release['tag_name']}/{name}"
    require(rows[0]["browser_download_url"] == url, "Unexpected asset URL")
    path = distribution / name
    path.write_bytes(fetch(url))
    require(
        rows[0].get("digest") == "sha256:" + smoke.digest(path), "Asset hash mismatch"
    )
    return path


def discovery_expression():
    return """(async()=>{
      const api=window.__TAURI__;
      if(typeof api?.updater?.check!=='function')throw Error('Native updater API missing');
      const appVersion=await api.app.getVersion();
      const update=await api.updater.check({timeout:30000});
      if(!update)throw Error('Native updater returned no update');
      let result;
      try {
        result={appVersion,available:update.available,currentVersion:update.currentVersion,
          version:update.version,rawJson:update.rawJson};
      } finally {await update.close();}
      return {...result,resourceClosed:true,mechanism:'tauri-plugin-updater'};
    })()"""


def validate_discovery(value, manifest, repo, target):
    require(
        value.get("appVersion") == OLD
        and value.get("currentVersion") == OLD
        and value.get("version") == NEW
        and value.get("available") is True
        and value.get("resourceClosed") is True
        and value.get("mechanism") == "tauri-plugin-updater",
        "Actual old native plugin did not discover the expected update",
    )
    raw = value.get("rawJson")
    require(
        isinstance(raw, dict) and raw == manifest,
        "Native response differs from public feed",
    )
    require(raw.get("version") == NEW, "Wrong published updater version")
    filename = {
        "darwin-aarch64": "xNAUT-macos-aarch64.app.tar.gz",
        "darwin-x86_64": "xNAUT-macos-x64.app.tar.gz",
        "windows-x86_64": f"xNAUT-{NEW}-windows-x64.msi",
    }[target]
    entry = raw.get("platforms", {}).get(target, {})
    require(
        entry.get("url")
        == f"https://github.com/{repo}/releases/download/v{NEW}/{filename}"
        and isinstance(entry.get("signature"), str)
        and bool(entry["signature"].strip()),
        "Native response has wrong platform URL or missing signature",
    )
    return entry


def record_discovery(evidence, report, result, manifest, repo, target):
    # Preserve the actual native payload even when strict comparison rejects it.
    report["discovery"] = result
    (evidence / "discovery.json").write_text(
        json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    return validate_discovery(result, manifest, repo, target)


def finalize_report(evidence, report, config, token):
    copy_errors = []
    try:
        if config and token:
            for name in ("debug.log", "rust-panics.log"):
                try:
                    path = config / name
                    if path.exists():
                        (evidence / name).write_text(
                            path.read_text(encoding="utf-8", errors="replace").replace(
                                token, "[REDACTED]"
                            ),
                            encoding="utf-8",
                        )
                except Exception as error:
                    copy_errors.append(
                        {"file": name, "error_type": type(error).__name__}
                    )
        if copy_errors:
            report["status"] = "failed"
            report["diagnostic_copy_errors"] = copy_errors
    finally:
        report["finished"] = smoke.timestamp()
        (evidence / "run.json").write_text(
            json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
    require(not copy_errors, "Failed to preserve diagnostic logs; see run.json")


def powershell(script, **values):
    env = dict(os.environ, **{key: str(value) for key, value in values.items()})
    return subprocess.check_output(
        [
            "pwsh",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$ErrorActionPreference='Stop';" + script,
        ],
        env=env,
        text=True,
        stderr=subprocess.STDOUT,
        timeout=180,
    ).strip()


def fresh_config(system):
    require(
        system in ("Darwin", "Windows")
        and os.environ.get("GITHUB_ACTIONS") == "true"
        and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted",
        "Only fresh GitHub-hosted Windows/macOS runners may launch this smoke",
    )
    require(
        not any(k.startswith("XNAUT_") for k in os.environ), "Inherited xNAUT overrides"
    )
    config = (
        Path.home() / "Library/Application Support"
        if system == "Darwin"
        else Path(os.environ["APPDATA"])
    ) / "xnaut"
    require(not config.exists(), "Existing xNAUT profile must not be used")
    if system == "Darwin":
        require(
            subprocess.run(["pgrep", "-x", "xnaut"], capture_output=True).returncode
            == 1,
            "Existing xNAUT process",
        )
    else:
        powershell(
            "if(Get-Process xnaut -ErrorAction SilentlyContinue){throw 'Existing xNAUT process'}; if(Get-ChildItem $env:ProgramFiles -Recurse -Depth 3 -Filter xnaut.exe -ErrorAction SilentlyContinue){throw 'Existing installation'}"
        )
    return config


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--target",
        required=True,
        choices=("darwin-aarch64", "darwin-x86_64", "windows-x86_64"),
    )
    parser.add_argument("--evidence", type=Path, default=Path("evidence"))
    args = parser.parse_args()
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    distribution = evidence.parent / "distribution"
    distribution.mkdir(exist_ok=True)
    report = {
        "suite": "native-updater-discovery",
        "started": smoke.timestamp(),
        "target": args.target,
        "old_version": OLD,
        "expected_version": NEW,
        "harness_sha256": smoke.digest(Path(__file__)),
        "harness_source_commit": smoke.command("git", "rev-parse", "HEAD"),
        "scope": "Native version discovery and expected platform entry validation. "
        "The plugin exposes the complete feed, not its selected download URL. "
        "No update payload is downloaded or installed by this check.",
        "status": "failed",
    }
    process, config, token, msi = None, None, "", None
    try:
        system = platform.system()
        config = fresh_config(system)
        expected_machine = {
            "darwin-aarch64": "arm64",
            "darwin-x86_64": "x86_64",
            "windows-x86_64": "AMD64",
        }[args.target]
        require(
            platform.machine() == expected_machine
            and (system == "Windows") == args.target.startswith("windows-"),
            "Runner/platform mismatch",
        )
        repo = os.environ["GITHUB_REPOSITORY"]
        old = public_release(repo, "tags/v" + OLD)
        latest = public_release(repo, "latest")
        require(
            old["tag_name"] == "v" + OLD and latest["tag_name"] == "v" + NEW,
            "Expected target is not the public latest release",
        )
        report["release_source_commits"] = {}
        for version in (OLD, NEW):
            commit = json.loads(
                fetch(f"https://api.github.com/repos/{repo}/commits/v{version}")
            )["sha"]
            require(
                re.fullmatch(r"[0-9a-f]{40}", commit),
                "Invalid resolved release source SHA",
            )
            report["release_source_commits"][version] = commit
        for name, value in [("old-release.json", old), ("latest-release.json", latest)]:
            (evidence / name).write_text(json.dumps(value, indent=2), encoding="utf-8")
        feed = asset(latest, "latest.json", distribution, repo)
        manifest = json.loads(feed.read_text(encoding="utf-8"))
        if system == "Darwin":
            arch = "aarch64" if args.target.endswith("aarch64") else "x64"
            archive = asset(old, f"xNAUT-macos-{arch}.app.tar.gz", distribution, repo)
            dmg = asset(old, f"xNAUT-{OLD}-macos-{arch}.dmg", distribution, repo)
            unpacked = distribution / "unpacked"
            unpacked.mkdir()
            with tarfile.open(archive) as tar:
                tar.extractall(unpacked, filter="data")
            app = unpacked / "xNAUT.app"
            info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
            require(
                info["CFBundleShortVersionString"] == OLD
                and info["CFBundleIdentifier"] == smoke.IDENTIFIER,
                "Old bundle identity mismatch",
            )
            binary = app / "Contents/MacOS/xnaut"
            require(
                smoke.command("lipo", "-archs", str(binary)) == expected_machine,
                "Old binary architecture mismatch",
            )
            smoke.command("codesign", "--verify", "--deep", "--strict", str(app))
            signing = smoke.command("codesign", "-d", "--verbose=4", str(app))
            require("TeamIdentifier=" + smoke.TEAM in signing, "Wrong signing team")
            smoke.command("spctl", "-a", "-vvv", "-t", "execute", str(app))
            smoke.command("xcrun", "stapler", "validate", str(dmg))
            report["installer_equivalence"] = smoke.dmg_equivalence(
                dmg, app, OLD, signing
            )
        else:
            msi = asset(old, f"xNAUT-{OLD}-windows-x64.msi", distribution, repo)
            # Exact owned MSI; no broad process termination or existing installation reuse.
            powershell(
                "$p=Start-Process msiexec.exe -Wait -PassThru -ArgumentList @('/i', ('\"'+$env:MSI+'\"'), '/qn','/norestart','/L*v', ('\"'+$env:LOG+'\"')); if($p.ExitCode -notin @(0,3010)){throw ('Install failed: '+$p.ExitCode)}",
                MSI=msi,
                LOG=evidence / "install.log",
            )
            binary = Path(
                powershell(
                    r"$b=@(Get-ChildItem $env:ProgramFiles -Recurse -Depth 3 -Filter xnaut.exe); if($b.Count -ne 1){throw 'Expected one installed executable'}; if($b[0].VersionInfo.ProductVersion -notmatch '^1\.30\.2(?:\.0)?$'){throw 'Wrong executable version'}; $b[0].FullName"
                )
            )
        report["binary_sha256"] = smoke.digest(binary)
        report["artifact_hashes"] = {
            p.name: smoke.digest(p) for p in distribution.iterdir() if p.is_file()
        }
        project, port, token = smoke.seed_profile(config, evidence)
        with (evidence / "process.log").open("w", encoding="utf-8") as output:
            process = subprocess.Popen(
                [str(binary)],
                cwd=project,
                stdin=subprocess.DEVNULL,
                stdout=output,
                stderr=subprocess.STDOUT,
                env={
                    k: v
                    for k, v in os.environ.items()
                    if k not in ("GH_TOKEN", "GITHUB_TOKEN")
                },
            )
            report["launch"] = {
                "pid": process.pid,
                "owned": True,
                "executable": str(binary),
            }
            bridge = smoke.Bridge(port, token, config / "debug.log", process)
            deadline = time.monotonic() + 120
            while True:
                require(process.poll() is None, "Old production app exited")
                try:
                    bridge.request("/api/control/doctor")
                    break
                except (OSError, urllib.error.URLError):
                    require(
                        time.monotonic() < deadline, "Old production bridge timed out"
                    )
                    time.sleep(0.5)
            result = bridge.evaluate(discovery_expression(), timeout=60)
            report["expected_platform_entry"] = record_discovery(
                evidence, report, result, manifest, repo, args.target
            )
            signature_name = (
                report["expected_platform_entry"]["url"].rsplit("/", 1)[1] + ".sig"
            )
            signature = asset(latest, signature_name, distribution, repo)
            require(
                signature.read_text(encoding="utf-8").strip()
                == report["expected_platform_entry"]["signature"].strip(),
                "Native updater signature differs from the exact published signature asset",
            )
            report["signature_asset_sha256"] = smoke.digest(signature)
            require(
                process.poll() is None, "Old production app exited during discovery"
            )
            report["status"] = "passed"
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        try:
            if process is not None:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=15)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=15)
                report["cleanup"] = {
                    "owned_pid": process.pid,
                    "exit_code": process.returncode,
                }
            if msi is not None:
                powershell(
                    "$p=Start-Process msiexec.exe -Wait -PassThru -ArgumentList @('/x', ('\"'+$env:MSI+'\"'), '/qn','/norestart','/L*v', ('\"'+$env:LOG+'\"')); if($p.ExitCode -notin @(0,3010,1605)){throw ('Uninstall failed: '+$p.ExitCode)}; if(Get-ChildItem $env:ProgramFiles -Recurse -Depth 3 -Filter xnaut.exe -ErrorAction SilentlyContinue){throw 'Owned installation remains'}",
                    MSI=msi,
                    LOG=evidence / "uninstall.log",
                )
        except Exception as error:
            report["status"], report["cleanup_error"] = "failed", str(error)
            raise
        finally:
            finalize_report(evidence, report, config, token)


if __name__ == "__main__":
    main()
