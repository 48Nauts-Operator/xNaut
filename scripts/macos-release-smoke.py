#!/usr/bin/env python3
"""XNAUT-468: run downloaded production bits only on a fresh hosted macOS runner.

No AX, screen control, TCC changes, preview build, or owner profile is used.
Native bridge/PTY failures fail the gate; process liveness alone is insufficient.
"""

import argparse
import base64
import hashlib
import json
import os
import platform
import plistlib
import re
import secrets
import socket
import subprocess
import tarfile
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

TEAM = "ABGJZD8X9M"
IDENTIFIER = "com.nautcode.xnaut"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def command(*args):
    return subprocess.check_output(
        args, text=True, stderr=subprocess.STDOUT, timeout=120
    ).strip()


def timestamp():
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def artifact_names(tag, arch):
    require(
        re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag),
        "Expected exact numeric release tag",
    )
    require(arch in ("aarch64", "x64"), "Unsupported architecture")
    return [
        f"xNAUT-{tag[1:]}-macos-{arch}.dmg",
        f"xNAUT-macos-{arch}.app.tar.gz",
        f"xNAUT-macos-{arch}.app.tar.gz.sig",
        "latest.json",
    ]


def verify_assets(release, tag, arch, distribution):
    names = artifact_names(tag, arch)
    require(
        release.get("tag_name") == tag and not release.get("prerelease"),
        "Wrong release identity",
    )
    assets = release.get("assets", [])
    hashes = {}
    for name in names:
        matches = [a for a in assets if a.get("name") == name]
        require(len(matches) == 1, f"Expected exactly one release asset: {name}")
        actual = digest(distribution / name)
        require(
            matches[0].get("digest") == "sha256:" + actual,
            f"Published digest mismatch: {name}",
        )
        hashes[name] = actual
    return hashes


def verify_updater(distribution, tag, arch, config, openssl):
    manifest = json.loads((distribution / "latest.json").read_text())
    require(
        manifest.get("version") == tag[1:], "Updater version differs from candidate"
    )
    key = "darwin-aarch64" if arch == "aarch64" else "darwin-x86_64"
    entry = manifest["platforms"][key]
    archive = f"xNAUT-macos-{arch}.app.tar.gz"
    expected_url = f"https://github.com/{os.environ['GITHUB_REPOSITORY']}/releases/download/{tag}/{archive}"
    require(
        entry["url"] == expected_url,
        "Updater points outside the exact candidate artifact",
    )
    encoded = (distribution / (archive + ".sig")).read_text().strip()
    require(
        encoded == entry["signature"].strip(),
        "Manifest and detached updater signatures differ",
    )
    public_lines = (
        base64.b64decode(config["plugins"]["updater"]["pubkey"]).decode().splitlines()
    )
    key_bytes = base64.b64decode(public_lines[1])
    lines = base64.b64decode(encoded).decode().splitlines()
    signed = base64.b64decode(lines[1])
    require(
        signed[:2] == b"ED" and key_bytes[2:10] == signed[2:10],
        "Unknown updater signing key or format",
    )
    require(lines[2].startswith("trusted comment: "), "Missing signed updater comment")
    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        (root / "key.der").write_bytes(
            bytes.fromhex("302a300506032b6570032100") + key_bytes[10:]
        )
        for payload, signature in [
            (
                hashlib.blake2b((distribution / archive).read_bytes()).digest(),
                signed[10:],
            ),
            (
                signed[10:] + lines[2][len("trusted comment: ") :].encode(),
                base64.b64decode(lines[3]),
            ),
        ]:
            (root / "payload").write_bytes(payload)
            (root / "signature").write_bytes(signature)
            command(
                openssl,
                "pkeyutl",
                "-verify",
                "-pubin",
                "-inkey",
                str(root / "key.der"),
                "-keyform",
                "DER",
                "-rawin",
                "-in",
                str(root / "payload"),
                "-sigfile",
                str(root / "signature"),
            )


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def fresh_runner(config, environ, system):
    require(
        system == "Darwin"
        and environ.get("GITHUB_ACTIONS") == "true"
        and environ.get("RUNNER_ENVIRONMENT") == "github-hosted",
        "Production launch is restricted to a fresh GitHub-hosted macOS runner",
    )
    require(not config.exists(), "Existing xNAUT profile must not be used or replaced")
    require(
        not any(k.startswith("XNAUT_") for k in environ),
        "Inherited xNAUT overrides are not a production smoke",
    )


def runtime_errors(config):
    panic = config / "rust-panics.log"
    require(
        not panic.exists() or not panic.read_text().strip(),
        "Native background panic during production smoke",
    )
    log = config / "debug.log"
    for line in log.read_text(errors="replace").splitlines() if log.exists() else []:
        if re.search(r"\[(uncaught|rejection)\]", line):
            require(
                "Tauri IPC bootstrap fell back to postMessage" in line,
                "Unhandled frontend error during production smoke: " + line,
            )


def local_providers_only(providers):
    # Production's first-load UI migration adds these local, credential-free
    # choices even when settings.json began with an empty registry.
    for provider in providers:
        endpoint = urllib.parse.urlparse(provider.get("endpoint", ""))
        require(
            provider.get("name") in ("lmstudio", "ollama", "nautgate")
            and not provider.get("has_credential")
            and endpoint.scheme in ("http", "https")
            and endpoint.hostname in ("localhost", "127.0.0.1", "::1")
            and endpoint.username is None
            and endpoint.password is None
            and not endpoint.query
            and not endpoint.fragment,
            "Fresh smoke profile contains a nonlocal or credentialed provider",
        )


def code_hash(signing):
    values = re.findall(r"^CDHash=([a-fA-F0-9]+)$", signing, re.MULTILINE)
    require(len(values) == 1, "Expected one application CodeDirectory hash")
    return values[0].lower()


def dmg_equivalence(dmg, tested_app, version, tested_signing):
    mount = Path(tempfile.mkdtemp(prefix="xnaut-release-dmg-")).resolve()
    attached = False
    try:
        result = subprocess.run(
            [
                "hdiutil",
                "attach",
                "-readonly",
                "-nobrowse",
                "-noautoopen",
                "-mountpoint",
                str(mount),
                "-plist",
                str(dmg),
            ],
            capture_output=True,
            check=True,
            timeout=120,
        )
        attached = True
        mounts = [
            item.get("mount-point")
            for item in plistlib.loads(result.stdout).get("system-entities", [])
            if item.get("mount-point")
        ]
        require(mounts == [str(mount)], "DMG attached outside its owned mount point")
        apps = list(mount.glob("*.app"))
        require(
            len(apps) == 1 and not apps[0].is_symlink(),
            "Expected exactly one application in the DMG",
        )
        app = apps[0]
        info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
        require(
            info.get("CFBundleIdentifier") == IDENTIFIER
            and info.get("CFBundleShortVersionString") == version,
            "DMG application identity/version differs from tested updater application",
        )
        command("codesign", "--verify", "--deep", "--strict", str(app))
        signing = command("codesign", "-dvv", str(app))
        binary_hash = digest(app / "Contents/MacOS/xnaut")
        require(
            binary_hash == digest(tested_app / "Contents/MacOS/xnaut")
            and code_hash(signing) == code_hash(tested_signing),
            "DMG and tested updater application contain different signed executable bits",
        )
        return {
            "binary_sha256": binary_hash,
            "cdhash": code_hash(signing),
            "mounted_readonly": True,
        }
    finally:
        # This path was newly created by this call, never a caller/user mount.
        # Preserve it if detach fails; do not recursively remove a mounted disk.
        if attached or mount.is_mount():
            command("hdiutil", "detach", str(mount))
        mount.rmdir()


class Bridge:
    def __init__(self, port, token, log, process):
        self.base = f"http://127.0.0.1:{port}"
        self.token, self.log, self.process = token, log, process

    def request(self, path, body=None):
        require(self.process.poll() is None, "Owned production process exited")
        request = urllib.request.Request(
            self.base + path,
            data=body.encode() if body is not None else None,
            headers={"Authorization": "Bearer " + self.token},
        )
        with urllib.request.urlopen(request, timeout=10) as response:
            return json.load(response)

    def evaluate(self, expression, timeout=120):
        marker = "release-smoke-" + secrets.token_hex(12)
        wrapper = (
            "void (async()=>{try{const value=await ("
            + expression
            + ");console.log("
            + json.dumps(marker)
            + ",JSON.stringify({ok:true,value}));}catch(e){console.log("
            + json.dumps(marker)
            + ",JSON.stringify({ok:false,error:String(e)}));}})()"
        )
        self.request("/api/control/eval", wrapper)
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            require(
                self.process.poll() is None,
                "Production process exited while waiting for native IPC",
            )
            for line in (
                self.log.read_text(errors="replace").splitlines()
                if self.log.exists()
                else []
            ):
                if marker + " " in line:
                    result = json.loads(line.split(marker + " ", 1)[1])
                    require(
                        result.get("ok") is True,
                        "Native control failed: " + str(result.get("error")),
                    )
                    return result["value"]
            time.sleep(0.25)
        raise RuntimeError("Native webview/IPC response timed out")


def native_checks(bridge, version, project_root, record):
    value = bridge.evaluate("""(async()=>{
      const deadline=Date.now()+90000;
      while(!window.xnautStartupHealth?.sealed()){
        if(Date.now()>deadline)throw Error('Production startup did not complete');
        await new Promise(r=>setTimeout(r,250));
      }
      const health=window.xnautStartupHealth.steps();
      if(!health.length||health.some(s=>!s.ok))throw Error(window.xnautStartupHealth.report());
      const version=await window.__TAURI__.app.getVersion();
      const settings=await window.__TAURI__.core.invoke('settings_get');
      const switches=await window.__TAURI__.core.invoke('kill_switches_get');
      return {version,health,project_root:settings.project_root,role:settings.instance.role,
        pm_enabled:settings.project_management.enabled,forges:settings.forges.length,
        providers:settings.llm_providers.map(p=>({name:p.name,endpoint:p.endpoint,has_credential:!!p.api_key})),
        default_has_credential:!!settings.llm.api_key,read_only:switches.read_only};
    })()""")
    require(
        value["version"] == version
        and value["project_root"] == str(project_root)
        and value["role"] == "workstation"
        and value["read_only"]
        and not value["pm_enabled"]
        and value["forges"] == 0
        and not value["default_has_credential"],
        "Native version/settings do not match the production smoke fixture",
    )
    local_providers_only(value["providers"])
    record("native_ipc", value)
    value = bridge.evaluate("""(async()=>{
      const panel=document.getElementById('settings-panel');
      if(!panel||typeof window.toggleSettingsPanel!=='function')throw Error('Settings control missing');
      if(getComputedStyle(panel).display!=='none')throw Error('Unexpected initial settings surface');
      window.toggleSettingsPanel(); await new Promise(r=>setTimeout(r,1000));
      if(getComputedStyle(panel).display==='none'||!panel.getBoundingClientRect().width)throw Error('Settings did not open');
      const label=panel.innerText;
      if(!label.includes('AI'))throw Error('Settings content did not render');
      window.toggleSettingsPanel();
      if(getComputedStyle(panel).display!=='none')throw Error('Settings did not close');
      return {opened:true,rendered:true,closed:true};
    })()""")
    record("settings_surface", value)
    marker = "native_pty_" + secrets.token_hex(12)
    value = bridge.evaluate(
        """(async()=>{
      const invoke=window.__TAURI__.core.invoke;
      const {session_id}=await invoke('create_terminal_session',{config:{shell:'/bin/sh',
        workingDir:WORKDIR,command:['/bin/sh'],sessionName:null,cols:80,rows:24}});
      try{
        await invoke('write_to_terminal',{sessionId:session_id,data:btoa("printf '%s%s\\n' 'native_pty_' 'SUFFIX'\\n")});
        const deadline=Date.now()+15000; let output='';
        while(Date.now()<deadline){
          output=atob(await invoke('terminal_output_snapshot',{sessionId:session_id}));
          if(output.split(/\\r?\\n/).includes(MARKER))return {session_id,roundtrip:true};
          await new Promise(r=>setTimeout(r,250));
        }
        throw Error('Native PTY output roundtrip failed');
      }finally{await invoke('close_terminal',{sessionId:session_id});}
    })()""".replace("WORKDIR", json.dumps(str(project_root)))
        .replace("SUFFIX", marker.removeprefix("native_pty_"))
        .replace("MARKER", json.dumps(marker))
    )
    record("terminal_roundtrip", value)


def seed_profile(config, evidence):
    config.mkdir(mode=0o700)
    project_root = evidence / "empty-projects"
    project_root.mkdir()
    port, mcp_port = free_port(), free_port()
    while mcp_port == port:
        mcp_port = free_port()
    token = secrets.token_urlsafe(32)
    settings = {
        "project_root": str(project_root),
        "categories": [],
        "llm": {"provider": "", "endpoint": "", "model": ""},
        "llm_providers": [],
        "engram": {"enabled": False},
        "forges": [],
        "mcp_servers": [],
        "mcp_port": mcp_port,
        "project_management": {"enabled": False},
        "loops": {"enabled": False, "dispatch_here": False},
        "core_team": {"enabled": False},
        "foreign_session_reaper": {"enabled": False},
        "instance": {"role": "workstation"},
    }
    for name, value in [
        ("settings.json", settings),
        (
            "mobile.json",
            {
                "enabled": True,
                "port": port,
                "token": token,
                "devices": [],
                "push_ntfy_topic": "",
            },
        ),
        (
            "kill-switches.json",
            {"read_only": True, "freeze_merges": True, "approve_everything": True},
        ),
    ]:
        path = config / name
        path.write_text(json.dumps(value))
        path.chmod(0o600)
    return project_root, port, token


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--arch", required=True, choices=("aarch64", "x64"))
    parser.add_argument("--distribution", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--openssl", required=True)
    args = parser.parse_args()
    evidence, distribution = args.evidence.resolve(), args.distribution.resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    report = {
        "id": f"production-{args.tag}-{args.arch}-{os.environ.get('GITHUB_RUN_ID', 'local')}",
        "suite": "macos-production-smoke",
        "host": platform.node(),
        "app_version": args.tag.removeprefix("v"),
        "started": timestamp(),
        "harness_sha256": digest(Path(__file__)),
        "evidence": "downloaded signed assets + owned process + native IPC",
        "cases": [],
    }
    process, config, token = None, Path.home() / "Library/Application Support/xnaut", ""

    def record(case, detail):
        report["cases"].append({"id": case, "status": "passed", "detail": detail})

    try:
        fresh_runner(config, os.environ, platform.system())
        existing = subprocess.run(
            ["pgrep", "-x", "xnaut"], capture_output=True, check=False
        )
        require(
            existing.returncode == 1,
            "An existing xNAUT process prevents production smoke",
        )
        require(
            platform.machine() == {"aarch64": "arm64", "x64": "x86_64"}[args.arch],
            "Runner architecture mismatch",
        )
        release = json.loads((distribution / "release.json").read_text())
        report["source_commit"] = command("git", "rev-parse", "HEAD")
        require(
            (distribution / "source-sha.txt").read_text().strip()
            == report["source_commit"],
            "Checkout is not the candidate tag commit",
        )
        tauri_config = json.loads(Path("src-tauri/tauri.conf.json").read_text())
        require(
            tauri_config["version"] == args.tag[1:]
            and tauri_config["identifier"] == IDENTIFIER,
            "Source version/identity differs from tag",
        )
        hashes = verify_assets(release, args.tag, args.arch, distribution)
        verify_updater(distribution, args.tag, args.arch, tauri_config, args.openssl)
        record("artifact_integrity", hashes)
        app_dir = distribution / "unpacked"
        app_dir.mkdir()
        with tarfile.open(
            distribution / f"xNAUT-macos-{args.arch}.app.tar.gz"
        ) as archive:
            archive.extractall(app_dir, filter="data")
        app = app_dir / "xNAUT.app"
        info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
        require(
            info["CFBundleShortVersionString"] == args.tag[1:]
            and info["CFBundleIdentifier"] == IDENTIFIER,
            "Downloaded bundle version/identity mismatch",
        )
        binary = app / "Contents/MacOS/xnaut"
        require(
            command("lipo", "-archs", str(binary)) == platform.machine(),
            "Downloaded Mach-O architecture mismatch",
        )
        report["binary_sha256"] = digest(binary)
        command("codesign", "--verify", "--deep", "--strict", str(app))
        signing = command("codesign", "-dvv", str(app))
        require(
            "TeamIdentifier=" + TEAM in signing
            and "Authority=Developer ID Application:" in signing,
            "Unexpected application signing identity",
        )
        entitlement = subprocess.run(
            ["codesign", "-d", "--entitlements", ":-", str(app)],
            check=True,
            capture_output=True,
            timeout=30,
        )
        require(
            plistlib.loads(entitlement.stdout).get(
                "com.apple.security.device.audio-input"
            )
            is True
            and bool(info.get("NSMicrophoneUsageDescription")),
            "Microphone signing/consent metadata missing",
        )
        record(
            "codesign",
            {
                "team": TEAM,
                "identifier": IDENTIFIER,
                "architecture": platform.machine(),
                "details": signing,
            },
        )
        dmg = distribution / f"xNAUT-{args.tag[1:]}-macos-{args.arch}.dmg"
        record(
            "notarization",
            {
                "staple": command("xcrun", "stapler", "validate", str(dmg)),
                "gatekeeper": command(
                    "spctl",
                    "-a",
                    "-vvv",
                    "-t",
                    "open",
                    "--context",
                    "context:primary-signature",
                    str(dmg),
                ),
                "application_gatekeeper": command(
                    "spctl", "-a", "-vvv", "-t", "execute", str(app)
                ),
                "dmg_equivalence": dmg_equivalence(dmg, app, args.tag[1:], signing),
            },
        )
        project_root, port, token = seed_profile(config, evidence)
        child_env = {
            k: v for k, v in os.environ.items() if k not in ("GH_TOKEN", "GITHUB_TOKEN")
        }
        with (evidence / "process.log").open("w") as output:
            process = subprocess.Popen(
                [str(binary)],
                cwd=project_root,
                env=child_env,
                stdin=subprocess.DEVNULL,
                stdout=output,
                stderr=subprocess.STDOUT,
            )
            launched = time.monotonic()
            report["launch"] = {
                "pid": process.pid,
                "executable": str(binary),
                "owned": True,
            }
            bridge = Bridge(port, token, config / "debug.log", process)
            deadline = launched + 120
            while True:
                require(
                    process.poll() is None,
                    "Downloaded app exited before its native bridge became ready",
                )
                try:
                    doctor = bridge.request("/api/control/doctor")
                    break
                except (OSError, urllib.error.URLError):
                    require(
                        time.monotonic() < deadline,
                        "Production bridge startup timed out",
                    )
                    time.sleep(0.5)
            (evidence / "doctor.json").write_text(json.dumps(doctor, indent=2))
            native_checks(bridge, args.tag[1:], project_root, record)
            while time.monotonic() - launched < 30:
                require(process.poll() is None, "Production process exited during hold")
                time.sleep(0.5)
            require(process.poll() is None, "Production process exited")
            runtime_errors(config)
            report["launch"]["observed_running_seconds"] = round(
                time.monotonic() - launched, 1
            )
            record("launch", report["launch"])
    except Exception as error:
        report["cases"].append(
            {"id": "smoke_failure", "status": "failed", "detail": str(error)}
        )
        raise
    finally:
        if process is not None:
            # Popen owns exactly this process. Never pkill, quit by bundle ID,
            # or terminate a runner-wide xnaut/terminal match.
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
            for name in ("debug.log", "rust-panics.log"):
                path = config / name
                if path.exists():
                    text = path.read_text(errors="replace")
                    (evidence / name).write_text(
                        text.replace(token, "[REDACTED]") if token else text
                    )
        report["finished"] = timestamp()
        (evidence / "run.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
