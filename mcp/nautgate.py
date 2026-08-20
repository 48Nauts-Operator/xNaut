#!/usr/bin/env python3
"""NautGate audit MCP server for xNAUT (stdio).

Reads the gateway's own account of the model calls your agents made: which
route was asked for, which model actually answered, and the hardware-signed
receipt proving that record has not been edited since. Nothing here writes.

Standalone on purpose, in both directions. xNAUT without NautGate keeps its own
execution record and verifies it alone; NautGate does not know xNAUT exists.
What the pairing buys is one exported bundle carrying two accounts of the same
calls, signed by two different parties: xNAUT's chain of what the agent did,
and NautGate's receipts for what the gateway routed. Agreement between them is
a claim an auditor can check without believing either product (XNAUT-216).

Config (env, set in the plugin library):
  NAUTGATE_URL      required   e.g. http://localhost:8090
  NAUTGATE_API_KEY  required   a key with audit read scope

Tools:
  receipts  list your routing receipts, newest first
  bundle    fetch one receipt as dev.nautgate.evidence-bundle/v1
  verify    check a bundle offline: receipt hash, Merkle proof, signature

No dependencies: stdlib only, so `python3 mcp/nautgate.py` just runs. The
verification bytes are in nautgate_evidence.py beside this file, reimplemented
from NautGate's core/app/audit_evidence.py (48Nauts, Apache 2.0) rather than
imported, because a verifier that needs the audited party's library installed
is not a verifier.
"""

from __future__ import annotations

import base64
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import nautgate_evidence as ng  # noqa: E402 -- sibling module, path set just above
import xnaut_verify as xv  # noqa: E402 -- the forty lines of RSA, not a second copy

URL = os.getenv("NAUTGATE_URL", "").strip()
API_KEY = os.getenv("NAUTGATE_API_KEY", "").strip()


def base() -> str:
    if not URL or not API_KEY:
        raise RuntimeError("set NAUTGATE_URL and NAUTGATE_API_KEY in the plugin's config")
    root = URL.rstrip("/")
    return root[:-3].rstrip("/") if root.endswith("/v1") else root


def get(path: str, query: dict | None = None):
    url = f"{base()}{path}"
    if query:
        url += "?" + urllib.parse.urlencode({k: v for k, v in query.items() if v is not None})
    req = urllib.request.Request(url, headers={"Authorization": f"Bearer {API_KEY}"})
    try:
        with urllib.request.urlopen(req, timeout=20) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            raise RuntimeError(f"NautGate has nothing at {path}. A receipt is only "
                               "exportable once the checkpoint covering it is signed.") from None
        raise RuntimeError(f"NautGate refused {path}: {exc.code} {exc.reason}") from None
    except urllib.error.URLError as exc:
        raise RuntimeError(f"NautGate at {base()} is unreachable: {exc.reason}") from None


def do_receipts(args: dict) -> dict:
    return get("/v1/audit/receipts", {"limit": args.get("limit") or 20})


def do_bundle(args: dict) -> dict:
    receipt_id = str(args.get("receipt_id") or "").strip()
    if not receipt_id:
        raise ValueError("receipt_id is required")
    bundle = get(f"/v1/audit/receipts/{urllib.parse.quote(receipt_id)}/bundle")
    path = str(args.get("path") or "").strip()
    if path:
        out = Path(path).expanduser()
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(bundle, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        return {"path": str(out), "receipt_id": receipt_id}
    return bundle


def do_verify(args: dict) -> dict:
    """Check one NautGate bundle without NautGate, and say what was NOT checked.

    The bundle carries a key fingerprint but no key, so the signature is only
    checkable against `/v1/audit/keys`. Fetched when a gateway is reachable and
    reported as unchecked when it is not, rather than passing quietly.
    """
    path = str(args.get("path") or "").strip()
    if path:
        doc = json.loads(Path(path).expanduser().read_text(encoding="utf-8"))
    elif isinstance(args.get("bundle"), dict):
        doc = args["bundle"]
    else:
        raise ValueError("pass path or bundle")

    if doc.get("bundle_schema") != ng.BUNDLE_SCHEMA:
        raise ValueError(f"not a NautGate evidence bundle: {doc.get('bundle_schema')!r}")
    receipt, checkpoint = doc.get("receipt") or {}, doc.get("checkpoint") or {}
    signature = doc.get("signature") or {}

    digest = ng.receipt_hash(receipt)
    if doc.get("receipt_hash") != digest.hex():
        raise ValueError("the receipt does not hash to the hash the bundle claims")
    root = ng.verify_merkle_proof(digest, doc.get("merkle_proof") or [])
    if root.hex() != checkpoint.get("merkle_root"):
        raise ValueError("the inclusion proof does not reach the checkpoint root")

    fingerprint = signature.get("public_key_fingerprint")
    spki, source = None, "not checked"
    for key in (get("/v1/audit/keys").get("keys") or []) if (URL and API_KEY) else []:
        if key.get("public_key_fingerprint") == fingerprint and key.get("public_key_pem"):
            spki, source = ng.spki_from_pem(key["public_key_pem"]), "/v1/audit/keys"
            break
    signed = False
    if spki:
        signed = xv.rsa_verify_sha256(spki, ng.checkpoint_payload(checkpoint),
                                      base64.b64decode(signature.get("value") or ""))
        if not signed:
            raise ValueError(f"the checkpoint signature does not verify under {fingerprint}")

    return {
        "ok": True,
        "receipt_id": receipt.get("receipt_id"),
        "decision_id": receipt.get("decision_id"),
        "checkpoint_id": checkpoint.get("checkpoint_id"),
        "signature_verified": signed,
        "signature_key": fingerprint,
        "key_source": source,
        "claim": ("The receipt is inside the checkpoint and unmodified."
                  + (" The HSM signed that checkpoint." if signed else
                     " The signature was NOT checked: no public key was available.")),
    }


TOOLS = [
    {
        "name": "receipts",
        "description": "List this agent's NautGate routing receipts, newest first: which model was asked for, which answered.",
        "inputSchema": {"type": "object", "properties": {"limit": {"type": "integer"}}},
    },
    {
        "name": "bundle",
        "description": "Fetch one routing receipt as a portable, hardware-signed evidence bundle. Optionally write it to a file.",
        "inputSchema": {
            "type": "object",
            "properties": {"receipt_id": {"type": "string"}, "path": {"type": "string"}},
            "required": ["receipt_id"],
        },
    },
    {
        "name": "verify",
        "description": "Verify a NautGate evidence bundle: receipt hash, Merkle inclusion, and the checkpoint signature.",
        "inputSchema": {
            "type": "object",
            "properties": {"path": {"type": "string"}, "bundle": {"type": "object"}},
        },
    },
]


def handle(method: str, params: dict):
    if method == "initialize":
        return {
            "protocolVersion": params.get("protocolVersion", "2024-11-05"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "nautgate", "version": "0.1.0"},
        }
    if method == "tools/list":
        return {"tools": TOOLS}
    if method == "tools/call":
        name = params.get("name")
        args = params.get("arguments") or {}
        try:
            if name == "receipts":
                result = do_receipts(args)
            elif name == "bundle":
                result = do_bundle(args)
            elif name == "verify":
                result = do_verify(args)
            else:
                raise ValueError(f"unknown tool: {name}")
            return {"content": [{"type": "text", "text": json.dumps(result, indent=2)}]}
        except (ValueError, RuntimeError, ng.EvidenceFormatError, OSError) as exc:
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


def selftest() -> None:
    """Verify a hand-built bundle offline, and refuse a forged one.

    No gateway and no key: what this proves is that the structural half of
    `verify` is real, and that an unverifiable signature is REPORTED rather
    than assumed. The signed half is covered by xnaut_verify's pinned HSM
    vector, which is the same forty lines of RSA.
    """
    receipt = {"schema": ng.RECEIPT_SCHEMA, "receipt_id": "r-1", "decision_id": "d-1", "sequence": 0}
    digest = ng.receipt_hash(receipt)
    sibling = bytes(32)
    root = ng.merkle_parent(ng.merkle_leaf(digest), sibling)
    doc = {"bundle_schema": ng.BUNDLE_SCHEMA, "receipt": receipt, "receipt_hash": digest.hex(),
           "leaf_index": 0, "merkle_proof": [{"hash": sibling.hex(), "side": "right"}],
           "checkpoint": {"schema": ng.CHECKPOINT_SCHEMA, "checkpoint_id": "c-1",
                          "merkle_root": root.hex(), "first_sequence": 0,
                          "last_sequence": 1, "receipt_count": 2, "signing_key_id": "K"},
           "signature": {"algorithm": "SHA256_WITH_RSA", "encoding": "base64-der",
                         "value": "", "key_id": "K", "public_key_fingerprint": "sha256:ng"}}

    report = do_verify({"bundle": doc})
    assert report["signature_verified"] is False and report["key_source"] == "not checked", report
    assert "NOT checked" in report["claim"], report

    forged = json.loads(json.dumps(doc))
    forged["receipt"]["decision_id"] = "d-forged"
    try:
        do_verify({"bundle": forged})
        raise AssertionError("a forged receipt verified")
    except ValueError as exc:
        assert "does not hash" in str(exc), exc

    moved = json.loads(json.dumps(doc))
    moved["merkle_proof"][0]["side"] = "left"
    try:
        do_verify({"bundle": moved})
        raise AssertionError("a bundle whose proof misses the root verified")
    except ValueError as exc:
        assert "does not reach the checkpoint root" in str(exc), exc
    print("ok: a NautGate bundle verifies structurally, forgeries refused, "
          "an unchecked signature is reported as unchecked")


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest()
    else:
        main()
