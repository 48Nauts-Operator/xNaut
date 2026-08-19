#!/usr/bin/env python3
"""Securosys attestation MCP server for xNAUT (stdio).

Signs digests on a Securosys Primus HSM via the TSB REST API
(POST /v1/synchronousSign) and keeps every receipt locally. Standalone on
purpose: xNAUT users without NautGate can still sign and attest what their
agents do. The TSB client below is a cut-down port of NautGate's
extensions/sb-attest/tsb.py (same project, 48Nauts).

Config (env, set in the plugin library):
  SECUROSYS_TSB_URL     required   TSB REST base, e.g. https://.../tsb
  SECUROSYS_KEY_NAME    required   the signing key's label in TSB
  SECUROSYS_API_KEY     optional   sent as X-API-KEY
  SECUROSYS_JWT         optional   sent as Authorization: Bearer

Receipts land in ~/Library/Application Support/xnaut/attestations.jsonl
(~/.local/share/xnaut on Linux). One JSON per line, append-only.
No dependencies: stdlib only, so `python3 mcp/securosys-attest.py` just runs.
"""

import base64
import binascii
import fcntl
import hashlib
import json
import os
import sys
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

TSB_URL = os.getenv("SECUROSYS_TSB_URL", "").strip()
KEY_NAME = os.getenv("SECUROSYS_KEY_NAME", "").strip()
API_KEY = os.getenv("SECUROSYS_API_KEY", "").strip() or None
JWT = os.getenv("SECUROSYS_JWT", "").strip() or None
ALGORITHM = os.getenv("SECUROSYS_ALGORITHM", "SHA256_WITH_RSA").strip()
# Optional: a git checkout whose attest/receipts.json mirrors the local
# receipts (e.g. the xnaut.dev website). When set, every successful attest
# rewrites it, commits and pushes, so the public verifier updates itself.
PUBLISH_DIR = os.getenv("SECUROSYS_PUBLISH_DIR", "").strip()

if sys.platform == "darwin":
    DATA_DIR = Path.home() / "Library" / "Application Support" / "xnaut"
else:
    DATA_DIR = Path(os.getenv("XDG_DATA_HOME", Path.home() / ".local" / "share")) / "xnaut"
RECEIPTS = DATA_DIR / "attestations.jsonl"


def endpoint() -> str:
    """TSB_URL is a base; the caller may or may not have included /v1."""
    base = TSB_URL.rstrip("/")
    if base.endswith("/synchronousSign"):
        return base
    if base.endswith("/v1"):
        return f"{base}/synchronousSign"
    return f"{base}/v1/synchronousSign"


def tsb_sign(payload: bytes) -> str:
    """Sign raw bytes on the HSM. Returns base64 signature or raises RuntimeError."""
    body = json.dumps({"signRequest": {
        "payload": base64.b64encode(payload).decode("ascii"),
        "payloadType": "UNSPECIFIED",
        "signKeyName": KEY_NAME,
        "signatureAlgorithm": ALGORITHM,
        "signatureType": "DER",
    }}).encode("utf-8")
    headers = {"Content-Type": "application/json", "Accept": "application/json"}
    if API_KEY:
        headers["X-API-KEY"] = API_KEY
    if JWT:
        headers["Authorization"] = f"Bearer {JWT}"
    req = urllib.request.Request(endpoint(), data=body, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            data = json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        raw = exc.read()
        try:
            err = json.loads(raw.decode("utf-8"))
            msg = err.get("message") or err.get("reason") or f"HTTP {exc.code}"
        except (ValueError, UnicodeDecodeError):
            msg = f"HTTP {exc.code}: {raw[:200]!r}"
        raise RuntimeError(f"TSB: {msg}") from None
    except urllib.error.URLError as exc:
        raise RuntimeError(f"cannot reach TSB at {endpoint()}: {exc.reason}") from None
    signature = data.get("signature")
    if not signature:
        # A 200 with no signature is not success.
        raise RuntimeError(f"TSB returned no signature: {json.dumps(data)[:200]}")
    return signature


def digest_from(args: dict) -> tuple[str, bytes]:
    """Either a hex digest, or text we hash here. Reject anything else:
    signing the ASCII of a typo yields a receipt that verifies against nothing."""
    digest = str(args.get("digest") or "").strip().lower().removeprefix("0x")
    if digest:
        try:
            raw = binascii.unhexlify(digest)
        except (binascii.Error, ValueError):
            raise ValueError("digest must be hex") from None
        if not raw:
            raise ValueError("digest is empty")
        return digest, raw
    text = str(args.get("text") or "")
    if not text:
        raise ValueError("pass either digest (hex) or text (hashed here with sha256)")
    raw = hashlib.sha256(text.encode("utf-8")).digest()
    return raw.hex(), raw


LINK_FIELDS = ("seq", "prev", "ts", "subject", "digest", "key_name", "algorithm", "signature")
GENESIS = "0" * 64


def link_hash(receipt: dict) -> str:
    """Hash of a receipt as the public sees it.

    Only the fields that survive publish() are hashed, so the browser verifier
    can recompute a link from attest/receipts.json alone. Local-only context
    (tsb_url, meta) is deliberately excluded: a field a third party never
    receives must not change a link they have to reproduce.
    """
    body = {k: receipt[k] for k in LINK_FIELDS if k in receipt}
    # ensure_ascii=False so the bytes match what JSON.stringify produces in the
    # browser verifier. With the default, a non-ASCII subject would hash
    # differently on each side and only break for the one receipt that had one.
    canon = json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(canon.encode("utf-8")).hexdigest()


def parse_receipts(text: str) -> list:
    rows = []
    for line in text.splitlines():
        try:
            rows.append(json.loads(line))
        except ValueError:
            continue
    return rows


def chain_tail(rows: list) -> tuple:
    """The (seq, prev) the next receipt must carry.

    Receipts written before the chain existed carry no seq. They are left as
    they are and the chain starts after them at genesis, rather than pretending
    to cover history it cannot.
    """
    if not rows or "seq" not in rows[-1]:
        return 0, GENESIS
    return int(rows[-1]["seq"]) + 1, link_hash(rows[-1])


def verify_chain(rows: list) -> dict:
    """Walk the chain. A receipt is only as good as the ones before it."""
    chained = [r for r in rows if "seq" in r]
    if not chained:
        return {"ok": True, "chained": 0, "unchained": len(rows), "broken_at": None}
    seq, prev = 0, GENESIS
    for row in chained:
        # Truncating the front of the file shows up here: the first surviving
        # receipt no longer claims seq 0 from genesis.
        if int(row["seq"]) != seq or row.get("prev") != prev:
            return {"ok": False, "chained": seq, "unchained": len(rows) - len(chained),
                    "broken_at": row.get("digest", "?")}
        seq, prev = seq + 1, link_hash(row)
    return {"ok": True, "chained": seq, "unchained": len(rows) - len(chained), "broken_at": None}


def do_attest(args: dict) -> dict:
    if not TSB_URL or not KEY_NAME:
        raise RuntimeError("SECUROSYS_TSB_URL and SECUROSYS_KEY_NAME must be set in the plugin's env")
    subject = str(args.get("subject") or "").strip()
    if not subject:
        raise ValueError("subject is required, e.g. xnaut.agent-work")
    hexdigest, raw = digest_from(args)
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    # Read the tail, sign, and append under one lock. Two agents attesting at
    # the same moment must not compute the same prev, or one of them writes a
    # link nobody can reproduce.
    # ponytail: the lock is held across the HSM round trip. At this volume that
    # is free; if attest ever runs hot, reserve the slot first and sign outside.
    with RECEIPTS.open("a+", encoding="utf-8") as f:
        fcntl.flock(f, fcntl.LOCK_EX)
        f.seek(0)
        seq, prev = chain_tail(parse_receipts(f.read()))
        # The HSM signs position and content together. Signing the content
        # alone is what let a receipt be deleted without trace.
        signature = tsb_sign(bytes.fromhex(prev) + raw)
        receipt = {
            "seq": seq,
            "prev": prev,
            "ts": datetime.now(timezone.utc).isoformat(),
            "subject": subject,
            "digest": hexdigest,
            "key_name": KEY_NAME,
            "algorithm": ALGORITHM,
            "signature": signature,
            "tsb_url": TSB_URL,
            "meta": args.get("meta") or {},
        }
        f.write(json.dumps(receipt) + "\n")
    if PUBLISH_DIR:
        try:
            publish(RECEIPTS)
            receipt["published"] = True
        except Exception as exc:  # noqa: BLE001 — the receipt is already safe locally
            print(f"publish failed (receipt stored locally): {exc}", file=sys.stderr)
            receipt["published"] = False
    return receipt


def publish(receipts_file: "Path") -> None:
    """Mirror receipts into PUBLISH_DIR/attest/receipts.json and push.

    Best-effort by design: the attestation succeeded the moment the HSM signed
    and the receipt was stored locally. A failed publish (offline, remote
    rejected) is reported on stderr and never fails the attest.
    """
    import subprocess
    target = Path(PUBLISH_DIR) / "attest" / "receipts.json"
    rows = []
    for line in receipts_file.read_text(encoding="utf-8").splitlines():
        try:
            r = json.loads(line)
        except ValueError:
            continue
        rows.append({k: r[k] for k in LINK_FIELDS if k in r})
    rows.sort(key=lambda r: r["ts"], reverse=True)
    target.write_text(json.dumps({"receipts": rows}, indent=2) + "\n", encoding="utf-8")
    git = ["git", "-C", PUBLISH_DIR]
    subprocess.run([*git, "add", "attest/receipts.json"], check=True, capture_output=True)
    diff = subprocess.run([*git, "diff", "--cached", "--quiet"])
    if diff.returncode == 0:
        return  # nothing new
    subprocess.run([*git, "commit", "-m", f"feat(attest): publish {len(rows)} receipt(s)"],
                   check=True, capture_output=True)
    for remote in ("forgejo", "origin"):
        has = subprocess.run([*git, "remote", "get-url", remote], capture_output=True)
        if has.returncode == 0:
            push = subprocess.run([*git, "push", remote], capture_output=True)
            if push.returncode != 0:
                print(f"publish: push to {remote} failed: {push.stderr.decode()[:200]}", file=sys.stderr)


def do_receipts(args: dict) -> dict:
    subject = str(args.get("subject") or "").strip()
    limit = max(1, min(int(args.get("limit") or 20), 200))
    rows = []
    if RECEIPTS.is_file():
        for line in RECEIPTS.read_text(encoding="utf-8").splitlines():
            try:
                row = json.loads(line)
            except ValueError:
                continue
            rows.append(row)
    chain = verify_chain(rows)
    if subject:
        rows = [r for r in rows if r.get("subject") == subject]
    return {"receipts": rows[-limit:][::-1], "chain": chain}


TOOLS = [
    {
        "name": "attest",
        "description": (
            "Sign a digest on the Securosys HSM and store the receipt. Pass a hex "
            "digest you computed, or text to sha256-hash here. Use a stable subject "
            "label (xnaut.agent-work, xnaut.release, ...) so receipts stay findable."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "subject": {"type": "string", "description": "what is being attested"},
                "digest": {"type": "string", "description": "hex digest to sign"},
                "text": {"type": "string", "description": "text to sha256 then sign (alternative to digest)"},
                "meta": {"type": "object", "description": "extra context stored with the receipt"},
            },
            "required": ["subject"],
        },
    },
    {
        "name": "receipts",
        "description": "List stored attestation receipts, newest first, optionally filtered by subject.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "subject": {"type": "string"},
                "limit": {"type": "integer"},
            },
        },
    },
]


def handle(method: str, params: dict):
    if method == "initialize":
        return {
            "protocolVersion": params.get("protocolVersion", "2024-11-05"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "securosys-attest", "version": "0.1.0"},
        }
    if method == "tools/list":
        return {"tools": TOOLS}
    if method == "tools/call":
        name = params.get("name")
        args = params.get("arguments") or {}
        try:
            if name == "attest":
                result = do_attest(args)
            elif name == "receipts":
                result = do_receipts(args)
            else:
                raise ValueError(f"unknown tool: {name}")
            return {"content": [{"type": "text", "text": json.dumps(result, indent=2)}]}
        except (ValueError, RuntimeError) as exc:
            return {"content": [{"type": "text", "text": str(exc)}], "isError": True}
    if method == "ping":
        return {}
    return None


def main() -> None:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if "id" not in msg:  # notification: nothing to answer
            continue
        result = handle(msg.get("method", ""), msg.get("params") or {})
        if result is None:
            reply = {"jsonrpc": "2.0", "id": msg["id"],
                     "error": {"code": -32601, "message": f"method not found: {msg.get('method')}"}}
        else:
            reply = {"jsonrpc": "2.0", "id": msg["id"], "result": result}
        sys.stdout.write(json.dumps(reply) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
