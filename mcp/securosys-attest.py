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

Also seals the execution record that src-tauri/src/evidence.rs writes: the
`checkpoint` tool builds a Merkle root over one session's unsealed records and
signs the checkpoint on the HSM. Standalone by design, so an xNAUT install with
no NautGate still produces a sealed, verifiable trail. The normative bytes live
in xnaut_evidence.py beside this file.

Receipts land in ~/Library/Application Support/xnaut/attestations.jsonl
(~/.local/share/xnaut on Linux). One JSON per line, append-only.
No dependencies: stdlib only, so `python3 mcp/securosys-attest.py` just runs.
"""

from __future__ import annotations

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

sys.path.insert(0, str(Path(__file__).resolve().parent))
import xnaut_evidence as ev  # noqa: E402 -- sibling module, path set just above

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
# Written by evidence.rs. XNAUT_EVIDENCE_DIR moves both, so a test run seals its
# own log rather than the operator's.
EVIDENCE_DIR = Path(os.getenv("XNAUT_EVIDENCE_DIR", "").strip() or (DATA_DIR / "evidence"))
EXECUTION_LOG = EVIDENCE_DIR / "execution.jsonl"
CHECKPOINTS = EVIDENCE_DIR / "checkpoints.jsonl"


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


def refuse_if_exposed() -> None:
    """Evidence anybody on the box can read is not evidence about who worked.

    Checked before every seal rather than once at import: the mode can change
    under a running server, and a receipt signed after that is worth less than
    no receipt, because it still looks authoritative.
    """
    if not DATA_DIR.exists():
        return
    mode = DATA_DIR.stat().st_mode & 0o777
    if mode & 0o077:
        raise RuntimeError(
            f"refusing to sign: {DATA_DIR} is readable beyond its owner "
            f"(mode {mode:o}). Run `chmod -R go-rwx {DATA_DIR}` and try again."
        )


def do_attest(args: dict) -> dict:
    if not TSB_URL or not KEY_NAME:
        raise RuntimeError("SECUROSYS_TSB_URL and SECUROSYS_KEY_NAME must be set in the plugin's env")
    refuse_if_exposed()
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


def executor_of(records: list) -> str:
    """The executor every record in a batch agrees on."""
    ids = {r.get("executor_id") or "" for r in records}
    if len(ids) != 1:
        raise RuntimeError(f"one checkpoint, one executor; found {sorted(ids)}")
    return ids.pop()


def sealed_state() -> tuple[dict, str | None]:
    """How far each session is sealed, and the hash of the last checkpoint.

    Checkpoints chain to each other as well as covering records, so removing a
    whole checkpoint is as visible as removing a record.
    """
    if not CHECKPOINTS.is_file():
        return {}, None
    upto, last = {}, None
    for line in CHECKPOINTS.read_text(encoding="utf-8").splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        cp = row.get("checkpoint") or {}
        session = cp.get("session_id")
        if session is not None:
            upto[session] = max(upto.get(session, -1), int(cp.get("last_seq", -1)))
        last = row.get("checkpoint_sha256") or last
    return upto, last


def do_checkpoint(args: dict) -> dict:
    """Seal every session's unsealed records. One HSM call per session."""
    if not TSB_URL or not KEY_NAME:
        raise RuntimeError("SECUROSYS_TSB_URL and SECUROSYS_KEY_NAME must be set in the plugin's env")
    if not EXECUTION_LOG.is_file():
        raise RuntimeError(f"no execution record at {EXECUTION_LOG}")
    refuse_if_exposed()
    only = str(args.get("session_id") or "").strip() or None

    records = ev.read_records(EXECUTION_LOG)
    # Verify before signing, always. Sealing a broken chain would put an HSM
    # signature on a root that proves the wrong thing, which is worse than an
    # unsealed chain because it looks authoritative.
    ev.verify_chain(records)
    upto, previous = sealed_state()

    sealed, skipped = [], {}
    EVIDENCE_DIR.mkdir(parents=True, exist_ok=True)
    with CHECKPOINTS.open("a+", encoding="utf-8") as handle:
        fcntl.flock(handle, fcntl.LOCK_EX)
        # Re-read the tail under the lock: another attest may have sealed since.
        handle.seek(0)
        for line in handle.read().splitlines():
            try:
                cp = (json.loads(line).get("checkpoint") or {})
            except ValueError:
                continue
            if cp.get("session_id") is not None:
                upto[cp["session_id"]] = max(upto.get(cp["session_id"], -1), int(cp.get("last_seq", -1)))
        for session, rows in ev.sessions(records).items():
            if only and session != only:
                continue
            fresh = [r for r in rows if int(r["seq"]) > upto.get(session, -1)]
            if not fresh:
                skipped[session] = "already sealed"
                continue
            checkpoint, payload, checkpoint_hash = ev.build_checkpoint(
                fresh,
                executor_id=executor_of(fresh),
                signing_key_id=KEY_NAME,
                previous_checkpoint_sha256=previous,
            )
            signature = tsb_sign(payload)
            row = {
                "checkpoint": checkpoint,
                "checkpoint_sha256": checkpoint_hash,
                "signature": signature,
                "algorithm": ALGORITHM,
                "sealed_at": datetime.now(timezone.utc).isoformat(),
            }
            handle.write(json.dumps(row) + "\n")
            handle.flush()
            previous = checkpoint_hash
            sealed.append({"session_id": session, "first_seq": checkpoint["first_seq"],
                           "last_seq": checkpoint["last_seq"],
                           "records": checkpoint["record_count"],
                           "merkle_root": checkpoint["merkle_root"],
                           "checkpoint_sha256": checkpoint_hash})
    return {"sealed": sealed, "skipped": skipped, "records_verified": len(records),
            "log": str(EXECUTION_LOG), "checkpoints": str(CHECKPOINTS)}


def do_verify_evidence(args: dict) -> dict:
    """Verify the chain, and every checkpoint's root and link, without the HSM.

    Signature verification needs the public key and belongs in the offline
    verifier; what this answers is the question an operator asks first, which is
    whether the local trail is internally consistent.
    """
    if not EXECUTION_LOG.is_file():
        raise RuntimeError(f"no execution record at {EXECUTION_LOG}")
    records = ev.read_records(EXECUTION_LOG)
    heads = ev.verify_chain(records)
    by_session = ev.sessions(records)
    checked, previous = [], None
    if CHECKPOINTS.is_file():
        for number, line in enumerate(CHECKPOINTS.read_text(encoding="utf-8").splitlines(), 1):
            if not line.strip():
                continue
            row = json.loads(line)
            cp = row["checkpoint"]
            rows = [r for r in by_session.get(cp["session_id"], [])
                    if cp["first_seq"] <= int(r["seq"]) <= cp["last_seq"]]
            if len(rows) != cp["record_count"]:
                raise RuntimeError(f"checkpoint {number}: covers {cp['record_count']} records, "
                                   f"{len(rows)} are in the log")
            if ev.merkle_root([r["hash"] for r in rows]).hex() != cp["merkle_root"]:
                raise RuntimeError(f"checkpoint {number}: records do not produce its Merkle root")
            if cp.get("previous_checkpoint_sha256") != previous:
                raise RuntimeError(f"checkpoint {number}: does not link to the checkpoint before it")
            payload = ev.checkpoint_payload(cp)
            if "sha256:" + hashlib.sha256(payload).hexdigest() != row["checkpoint_sha256"]:
                raise RuntimeError(f"checkpoint {number}: does not hash to its own hash")
            previous = row["checkpoint_sha256"]
            checked.append(cp["checkpoint_id"])
    return {"ok": True, "records": len(records), "sessions": heads,
            "checkpoints_verified": len(checked)}


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
        "name": "checkpoint",
        "description": (
            "Seal the execution record xNAUT writes for its agents: build a Merkle "
            "root over each session's unsealed records and sign the checkpoint on the "
            "HSM. Verifies the whole chain first and refuses to sign a broken one."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "session_id": {"type": "string", "description": "seal only this session (default: all)"},
            },
        },
    },
    {
        "name": "verify_evidence",
        "description": (
            "Verify the local execution record and its checkpoints without the HSM: "
            "every record hash, every chain link, every Merkle root, every checkpoint link."
        ),
        "inputSchema": {"type": "object", "properties": {}},
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
            elif name == "checkpoint":
                result = do_checkpoint(args)
            elif name == "verify_evidence":
                result = do_verify_evidence(args)
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


def selftest() -> None:
    """Seal a scratch log end to end with a fake HSM, then break it on purpose.

    Everything below the TSB call is exercised for real: chaining, the sealed
    watermark, the checkpoint chain, and the refusal to sign a broken log.
    Run: python3 mcp/securosys-attest.py --selftest
    """
    import shutil
    import tempfile
    global DATA_DIR, EVIDENCE_DIR, EXECUTION_LOG, CHECKPOINTS, TSB_URL, KEY_NAME, tsb_sign

    scratch = Path(tempfile.mkdtemp(prefix="xnaut-attest-selftest-"))
    signed = []
    try:
        DATA_DIR = scratch
        EVIDENCE_DIR = scratch / "evidence"
        EXECUTION_LOG = EVIDENCE_DIR / "execution.jsonl"
        CHECKPOINTS = EVIDENCE_DIR / "checkpoints.jsonl"
        EVIDENCE_DIR.mkdir(parents=True)
        scratch.chmod(0o700)
        TSB_URL, KEY_NAME = "https://example.invalid/tsb", "selftest-key"
        tsb_sign = lambda payload: (signed.append(payload), base64.b64encode(payload[:8]).decode())[1]

        def write(session, count, kind="tool_call"):
            rows = ev.read_records(EXECUTION_LOG) if EXECUTION_LOG.exists() else []
            head = [r for r in rows if r["session_id"] == session]
            seq = len(head)
            prev = head[-1]["hash"] if head else None
            with EXECUTION_LOG.open("a", encoding="utf-8") as f:
                for _ in range(count):
                    row = {"schema_version": ev.RECORD_SCHEMA, "record_id": str(seq) + session,
                           "session_id": session, "seq": seq, "prev_hash": prev,
                           "recorded_at": "2026-08-20T00:00:0%d.000Z" % (seq % 10),
                           "executor_id": "xnaut:selftest", "executor_version": "0",
                           "kind": kind}
                    row["hash"] = ev.record_hash(row)
                    f.write(json.dumps(row) + "\n")
                    seq, prev = seq + 1, row["hash"]

        write("s1", 3)
        write("s2", 1)
        first = do_checkpoint({})
        assert len(first["sealed"]) == 2, first
        assert len(signed) == 2, "one HSM call per session"
        assert do_verify_evidence({})["checkpoints_verified"] == 2

        # Nothing new: sealing again must not spend an HSM call or move the chain.
        again = do_checkpoint({})
        assert again["sealed"] == [] and len(signed) == 2, again
        assert set(again["skipped"]) == {"s1", "s2"}

        # New records extend the same session from where it was sealed.
        write("s1", 2)
        third = do_checkpoint({})
        assert [c["first_seq"] for c in third["sealed"]] == [3], third
        assert do_verify_evidence({})["records"] == 6

        # A mutated record must be refused before the HSM is ever called.
        rows = EXECUTION_LOG.read_text(encoding="utf-8").splitlines()
        edited = json.loads(rows[1])
        edited["kind"] = "tool_refused"
        rows[1] = json.dumps(edited)
        EXECUTION_LOG.write_text("\n".join(rows) + "\n", encoding="utf-8")
        calls = len(signed)
        try:
            do_checkpoint({})
            raise AssertionError("a broken chain must not be sealed")
        except ev.EvidenceFormatError:
            pass
        assert len(signed) == calls, "the HSM was called on a broken chain"
        print("ok: seal, resume, re-seal is a no-op, broken chain refused")
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest()
    else:
        main()
