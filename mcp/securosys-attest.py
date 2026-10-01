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
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import nautgate_evidence as ngev
import xnaut_evidence as ev
import xnaut_verify as xv

TSB_URL = os.getenv("SECUROSYS_TSB_URL", "").strip()
KEY_NAME = os.getenv("SECUROSYS_KEY_NAME", "").strip()
API_KEY = os.getenv("SECUROSYS_API_KEY", "").strip() or None
JWT = os.getenv("SECUROSYS_JWT", "").strip() or None
ALGORITHM = os.getenv("SECUROSYS_ALGORITHM", "SHA256_WITH_RSA").strip()
# Optional: a git checkout whose attest/receipts.json mirrors the local
# receipts (e.g. the xnaut.dev website). When set, every successful attest
# rewrites it, commits and pushes, so the public verifier updates itself.
PUBLISH_DIR = os.getenv("SECUROSYS_PUBLISH_DIR", "").strip()
# Optional: a NautGate the agent's model calls went through. When both are set,
# an export also fetches the gateway's own signed receipt for every routing id
# in the chain, so the bundle carries two accounts of the same calls signed by
# two parties (XNAUT-216). Unset means the export behaves exactly as before.
NAUTGATE_URL = os.getenv("NAUTGATE_URL", "").strip()
NAUTGATE_API_KEY = os.getenv("NAUTGATE_API_KEY", "").strip()

if sys.platform == "darwin":
    DATA_DIR = Path.home() / "Library" / "Application Support" / "xnaut"
else:
    DATA_DIR = (
        Path(os.getenv("XDG_DATA_HOME", Path.home() / ".local" / "share")) / "xnaut"
    )
RECEIPTS = DATA_DIR / "attestations.jsonl"
# Written by evidence.rs. XNAUT_EVIDENCE_DIR moves both, so a test run seals its
# own log rather than the operator's.
EVIDENCE_DIR = Path(
    os.getenv("XNAUT_EVIDENCE_DIR", "").strip() or (DATA_DIR / "evidence")
)
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
    body = json.dumps(
        {
            "signRequest": {
                "payload": base64.b64encode(payload).decode("ascii"),
                "payloadType": "UNSPECIFIED",
                "signKeyName": KEY_NAME,
                "signatureAlgorithm": ALGORITHM,
                "signatureType": "DER",
            }
        }
    ).encode("utf-8")
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


LINK_FIELDS = (
    "seq",
    "prev",
    "ts",
    "subject",
    "digest",
    "key_name",
    "algorithm",
    "signature",
)
GENESIS = "0" * 64
LINK_DOMAIN = b"XNAUT-ATTEST-LINK-V1\x00"


def link_hash(receipt: dict) -> str:
    """Hash of a receipt as the public sees it.

    Only the fields that survive publish() are hashed, so the browser verifier
    can recompute a link from attest/receipts.json alone. Local-only context
    (tsb_url, meta) is deliberately excluded: a field a third party never
    receives must not change a link they have to reproduce.

    Two things changed here on 2026-08-20, both free because no chained receipt
    has been published yet and neither will be free afterwards:

    - A domain prefix. Without one this hash and a record hash are both "sha256
      of some canonical JSON", so a receipt could in principle be presented as
      a record. Every other hash in this codebase is domain separated.
    - RFC 8785 canonicalization instead of sort_keys plus separators. The old
      form agreed with JCS by convention rather than by spec, and the two
      genuinely disagree on key order above the BMP.

    The browser verifier on xnaut.dev recomputes this and must be changed to
    match, or every published link fails.
    """
    body = {k: receipt[k] for k in LINK_FIELDS if k in receipt}
    return hashlib.sha256(LINK_DOMAIN + ev.canonical_json(body)).hexdigest()


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
            return {
                "ok": False,
                "chained": seq,
                "unchained": len(rows) - len(chained),
                "broken_at": row.get("digest", "?"),
            }
        seq, prev = seq + 1, link_hash(row)
    return {
        "ok": True,
        "chained": seq,
        "unchained": len(rows) - len(chained),
        "broken_at": None,
    }


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
        raise RuntimeError(
            "SECUROSYS_TSB_URL and SECUROSYS_KEY_NAME must be set in the plugin's env"
        )
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


def publish(receipts_file: Path) -> None:
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
    subprocess.run(
        [*git, "add", "attest/receipts.json"], check=True, capture_output=True
    )
    diff = subprocess.run([*git, "diff", "--cached", "--quiet"], check=False)
    if diff.returncode == 0:
        return  # nothing new
    subprocess.run(
        [*git, "commit", "-m", f"feat(attest): publish {len(rows)} receipt(s)"],
        check=True,
        capture_output=True,
    )
    for remote in ("forgejo", "origin"):
        has = subprocess.run(
            [*git, "remote", "get-url", remote], capture_output=True, check=False
        )
        if has.returncode == 0:
            push = subprocess.run(
                [*git, "push", remote], capture_output=True, check=False
            )
            if push.returncode != 0:
                print(
                    f"publish: push to {remote} failed: {push.stderr.decode()[:200]}",
                    file=sys.stderr,
                )


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
        raise RuntimeError(
            "SECUROSYS_TSB_URL and SECUROSYS_KEY_NAME must be set in the plugin's env"
        )
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
                cp = json.loads(line).get("checkpoint") or {}
            except ValueError:
                continue
            if cp.get("session_id") is not None:
                upto[cp["session_id"]] = max(
                    upto.get(cp["session_id"], -1), int(cp.get("last_seq", -1))
                )
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
            sealed.append(
                {
                    "session_id": session,
                    "first_seq": checkpoint["first_seq"],
                    "last_seq": checkpoint["last_seq"],
                    "records": checkpoint["record_count"],
                    "merkle_root": checkpoint["merkle_root"],
                    "checkpoint_sha256": checkpoint_hash,
                }
            )
    return {
        "sealed": sealed,
        "skipped": skipped,
        "records_verified": len(records),
        "log": str(EXECUTION_LOG),
        "checkpoints": str(CHECKPOINTS),
    }


def _tsb_headers() -> dict:
    headers = {"Accept": "application/json", "Content-Type": "application/json"}
    if API_KEY:
        headers["X-API-KEY"] = API_KEY
    if JWT:
        headers["Authorization"] = f"Bearer {JWT}"
    return headers


def _tsb_base() -> str:
    base = TSB_URL.rstrip("/")
    return base if base.endswith("/v1") else f"{base}/v1"


def _spki_from_attributes(label: str) -> str:
    """The base64 DER SubjectPublicKeyInfo of `label`, from the HSM's attestation."""
    # Two shapes in the wild: GET /v1/key/{label}/attributes (what tsb_public_key
    # uses) and POST /v1/key/attributes {"label"} (the KB's gotcha list). Try
    # the GET, fall back to the POST; a 404 on both means the key is absent.
    attempts = [
        urllib.request.Request(
            f"{_tsb_base()}/key/{label}/attributes", headers=_tsb_headers()
        ),
        urllib.request.Request(
            f"{_tsb_base()}/key/attributes",
            data=json.dumps({"label": label}).encode("utf-8"),
            headers=_tsb_headers(),
            method="POST",
        ),
    ]
    data = None
    last = None
    for req in attempts:
        try:
            with urllib.request.urlopen(req, timeout=15) as resp:
                data = json.loads(resp.read().decode("utf-8"))
            break
        except urllib.error.HTTPError as exc:
            last = f"HTTP {exc.code}"
        except urllib.error.URLError as exc:
            raise RuntimeError(f"cannot reach TSB: {exc.reason}") from None
    if data is None:
        raise RuntimeError(f"TSB: cannot read key {label}: {last}")
    xml = data.get("xml") or ""
    start = xml.find('<public_key format="base64">')
    end = xml.find("</public_key>")
    if start < 0 or end < 0:
        raise RuntimeError("TSB returned no public key for " + label)
    return xml[start + len('<public_key format="base64">') : end].strip()


def _pem(spki_b64: str) -> str:
    body = "\n".join(spki_b64[i : i + 64] for i in range(0, len(spki_b64), 64))
    return f"-----BEGIN PUBLIC KEY-----\n{body}\n-----END PUBLIC KEY-----\n"


def tsb_create_key(label: str, key_size: int = 4096) -> dict:
    """Create a sign-only RSA key inside the HSM and hand back its public half.

    `POST /v1/key` (docs.securosys.com/tsb/quickstart): sign=true,
    extractable=false, sensitive=true is the profile Securosys names for
    document signing and sealing; the private half never leaves the module.
    `policy: null` makes it a plain key, so synchronousSign needs no approval
    quorum. modifiable=false so nobody can later flip extractable on it.

    Refuses to overwrite: a label that already exists is returned as-is with
    `created: false`, because a signing key's identity must not change under
    a fingerprint someone already recorded.
    """
    label = (label or "").strip()
    if not label or any(c.isspace() for c in label):
        raise ValueError("a key label without whitespace is required")
    if not TSB_URL:
        raise RuntimeError("SECUROSYS_TSB_URL is not set")
    created = True
    try:
        spki = _spki_from_attributes(label)
        created = False
    except RuntimeError:
        body = json.dumps(
            {
                "label": label,
                "algorithm": "RSA",
                "keySize": int(key_size),
                "attributes": {
                    "sign": True,
                    "decrypt": False,
                    "unwrap": False,
                    "derive": False,
                    "extractable": False,
                    "sensitive": True,
                    "modifiable": False,
                    "destroyable": True,
                    "copyable": False,
                },
                "policy": None,
            }
        ).encode("utf-8")
        req = urllib.request.Request(
            f"{_tsb_base()}/key", data=body, headers=_tsb_headers(), method="POST"
        )
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                resp.read()
        except urllib.error.HTTPError as exc:
            detail = exc.read().decode("utf-8", "replace")[:300]
            raise RuntimeError(
                f"TSB refused to create {label}: HTTP {exc.code} {detail}"
            ) from None
        except urllib.error.URLError as exc:
            raise RuntimeError(f"cannot reach TSB: {exc.reason}") from None
        spki = _spki_from_attributes(label)
    fingerprint = "sha256:" + hashlib.sha256(base64.b64decode(spki)).hexdigest()
    keys_dir = DATA_DIR / "keys"
    keys_dir.mkdir(parents=True, exist_ok=True)
    pem_path = keys_dir / f"{label}.pem"
    pem_path.write_text(_pem(spki))
    (keys_dir / f"{label}.fingerprint").write_text(fingerprint + "\n")
    return {
        "created": created,
        "key_name": label,
        "algorithm": "SHA256_WITH_RSA",
        "public_key_pem_path": str(pem_path),
        "fingerprint_sha256": fingerprint,
        "nautgate_env": {
            "SB_ATTEST_KEY_NAME": label,
            "SB_ATTEST_PUBLIC_KEY_PEM": f"<contents of {pem_path}>",
            "SB_ATTEST_PUBLIC_KEY_FINGERPRINT": fingerprint.split(":", 1)[1],
        },
    }


def tsb_public_key() -> dict:
    """The signing key's public half, with the HSM's own attestation of it.

    `GET /v1/key/{label}/attributes` is the only endpoint that hands it back;
    `GET /v1/key/{label}` is a 405. The response carries an XML key attestation
    signed by the HSM's attestation key, which is worth more than the bare key:
    it says the private half was generated inside the module and is
    never_extractable, which no fingerprint we publish can say.
    """
    base = TSB_URL.rstrip("/")
    base = base if base.endswith("/v1") else f"{base}/v1"
    headers = {"Accept": "application/json"}
    if API_KEY:
        headers["X-API-KEY"] = API_KEY
    if JWT:
        headers["Authorization"] = f"Bearer {JWT}"
    req = urllib.request.Request(f"{base}/key/{KEY_NAME}/attributes", headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            data = json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        raise RuntimeError(
            f"TSB: cannot read key {KEY_NAME}: HTTP {exc.code}"
        ) from None
    except urllib.error.URLError as exc:
        raise RuntimeError(f"cannot reach TSB: {exc.reason}") from None
    xml = data.get("xml") or ""
    start = xml.find('<public_key format="base64">')
    end = xml.find("</public_key>")
    if start < 0 or end < 0:
        raise RuntimeError("TSB returned no public key for " + KEY_NAME)
    spki = xml[start + len('<public_key format="base64">') : end].strip()
    return {
        "key_name": KEY_NAME,
        "algorithm": ALGORITHM,
        "public_key_spki_b64": spki,
        "fingerprint_sha256": "sha256:"
        + hashlib.sha256(base64.b64decode(spki)).hexdigest(),
        "hsm_attestation_xml": xml,
        "hsm_attestation_signature": data.get("xmlSignature"),
        "hsm_attestation_key_name": data.get("attestationKeyName"),
    }


def nautgate_get(base: str, path: str):
    req = urllib.request.Request(
        f"{base}{path}", headers={"Authorization": f"Bearer {NAUTGATE_API_KEY}"}
    )
    with urllib.request.urlopen(req, timeout=15) as resp:
        return json.loads(resp.read().decode("utf-8"))


def nautgate_bundles(records: list) -> tuple[list, list, dict]:
    """The gateway's own receipts for the routing ids in these records.

    Returns (bundles, pending, keys). A receipt is 404 until NautGate's checkpoint
    over it is signed — `queries.py` exports `r.status = 'verified' AND
    c.status = 'verified'` only — so a fresh session legitimately has none yet.
    That is reported as pending, never as missing: an export that called it
    missing would read as evidence of tampering on the day of the session.
    """
    if not (NAUTGATE_URL and NAUTGATE_API_KEY):
        return [], [], {}
    wanted, seen = [], set()
    for record in records:
        receipt_id = (record.get("nautgate") or {}).get("receipt_id")
        if isinstance(receipt_id, str) and receipt_id and receipt_id not in seen:
            seen.add(receipt_id)
            wanted.append(receipt_id)

    base = NAUTGATE_URL.rstrip("/")
    base = base.removesuffix("/v1")
    bundles, pending = [], []
    for receipt_id in wanted:
        path = f"/v1/audit/receipts/{urllib.parse.quote(receipt_id)}/bundle"
        try:
            bundles.append(nautgate_get(base, path))
        except urllib.error.HTTPError as exc:
            if exc.code in (404, 409):
                pending.append(receipt_id)
                continue
            raise RuntimeError(
                f"NautGate refused {receipt_id}: {exc.code} {exc.reason}"
            ) from exc
        except urllib.error.URLError as exc:
            raise RuntimeError(
                f"NautGate at {base} is unreachable: {exc.reason}"
            ) from exc

    # The signing keys go in the bundle too, or the auditor can check the
    # gateway's structure and not its signatures, which is the half that
    # matters. They are public keys; carrying them costs nothing.
    keys = {}
    if bundles:
        try:
            keys = nautgate_get(base, "/v1/audit/keys")
        except (urllib.error.HTTPError, urllib.error.URLError) as exc:
            keys = {}
            print(
                f"nautgate: could not fetch /v1/audit/keys ({exc}); the bundle will "
                "verify structurally but its signatures cannot be checked",
                file=sys.stderr,
            )
    return bundles, pending, keys


def do_export(args: dict) -> dict:
    """Write a bundle a customer can verify with xnaut_verify.py and nothing else.

    Redacted by default. The arguments of a tool call are the customer's own
    material: a shell command, a path, a diff. Every hash in the chain resolves
    without them, so proving what happened costs no source code. Ask for them
    explicitly when the auditor is entitled to them.
    """
    if not EXECUTION_LOG.is_file():
        raise RuntimeError(f"no execution record at {EXECUTION_LOG}")
    only = str(args.get("session_id") or "").strip() or None
    include = bool(args.get("include_arguments"))

    records = ev.read_records(EXECUTION_LOG)
    ev.verify_chain(records)
    if only:
        records = [r for r in records if (r.get("session_id") or "") == only]
        if not records:
            raise RuntimeError(f"no records for session {only}")
    wanted = {(r.get("session_id") or "") for r in records}

    checkpoints = []
    if CHECKPOINTS.is_file():
        for line in CHECKPOINTS.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            # A filtered export keeps every checkpoint. Dropping the ones for
            # other sessions would break the checkpoint chain the verifier
            # walks, and a bundle that fails for a reason the exporter caused
            # is worse than a bundle carrying a few extra rows.
            checkpoints.append(row)

    arguments: dict[str, dict[str, str]] = {}
    if include:
        for record in records:
            tool = record.get("tool") or {}
            args_hash = tool.get("args_hash")
            if not isinstance(args_hash, str):
                continue
            session = record.get("session_id") or ""
            blob = EVIDENCE_DIR / "blobs" / session / args_hash.replace("sha256:", "")
            if not blob.is_file():
                continue
            body = blob.read_bytes()
            # Sealed blobs are AES-GCM and unreadable from here: unsealing needs
            # the session key, which lives in xNAUT. Skip rather than embed
            # ciphertext nobody can open, and say how many were skipped.
            try:
                text = body.decode("utf-8")
            except UnicodeDecodeError:
                continue
            if ev.digest(ev.ARGS_DOMAIN, text.encode("utf-8")) != args_hash:
                continue
            arguments.setdefault(session, {})[args_hash] = text

    bundle = {
        "schema": "xnaut.evidence-bundle/v1",
        "exported_at": datetime.now(timezone.utc).isoformat(),
        "executor_id": executor_of(records),
        "signing_key": tsb_public_key(),
        "records": records,
        "checkpoints": checkpoints,
    }
    if arguments:
        bundle["arguments"] = arguments

    # Sibling documents, never merged into an xNAUT record: each one is valid
    # on its own terms and verifies against NautGate's key, not ours.
    gateway, pending, gateway_keys = nautgate_bundles(records)
    if gateway:
        bundle["nautgate_bundles"] = gateway
    if gateway_keys:
        bundle["nautgate_keys"] = gateway_keys

    out = Path(
        str(args.get("path") or "").strip() or (EVIDENCE_DIR / "bundle.json")
    ).expanduser()
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(
        json.dumps(bundle, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    sealed = sum(
        1 for r in records if isinstance((r.get("tool") or {}).get("args_hash"), str)
    ) - sum(len(v) for v in arguments.values())
    return {
        "path": str(out),
        "records": len(records),
        "sessions": sorted(wanted),
        "nautgate_bundles": len(gateway),
        "nautgate_pending": pending,
        "checkpoints": len(checkpoints),
        "arguments_included": sum(len(v) for v in arguments.values()),
        "arguments_unavailable": max(sealed, 0) if include else None,
        "verify_with": f"python3 {Path(__file__).resolve().parent / 'xnaut_verify.py'} {out}",
    }


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
        for number, line in enumerate(
            CHECKPOINTS.read_text(encoding="utf-8").splitlines(), 1
        ):
            if not line.strip():
                continue
            row = json.loads(line)
            cp = row["checkpoint"]
            rows = [
                r
                for r in by_session.get(cp["session_id"], [])
                if cp["first_seq"] <= int(r["seq"]) <= cp["last_seq"]
            ]
            if len(rows) != cp["record_count"]:
                raise RuntimeError(
                    f"checkpoint {number}: covers {cp['record_count']} records, "
                    f"{len(rows)} are in the log"
                )
            if ev.merkle_root([r["hash"] for r in rows]).hex() != cp["merkle_root"]:
                raise RuntimeError(
                    f"checkpoint {number}: records do not produce its Merkle root"
                )
            if cp.get("previous_checkpoint_sha256") != previous:
                raise RuntimeError(
                    f"checkpoint {number}: does not link to the checkpoint before it"
                )
            payload = ev.checkpoint_payload(cp)
            if (
                "sha256:" + hashlib.sha256(payload).hexdigest()
                != row["checkpoint_sha256"]
            ):
                raise RuntimeError(
                    f"checkpoint {number}: does not hash to its own hash"
                )
            previous = row["checkpoint_sha256"]
            checked.append(cp["checkpoint_id"])
    return {
        "ok": True,
        "records": len(records),
        "sessions": heads,
        "checkpoints_verified": len(checked),
    }


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
                "text": {
                    "type": "string",
                    "description": "text to sha256 then sign (alternative to digest)",
                },
                "meta": {
                    "type": "object",
                    "description": "extra context stored with the receipt",
                },
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
                "session_id": {
                    "type": "string",
                    "description": "seal only this session (default: all)",
                },
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
        "name": "export",
        "description": (
            "Write an evidence bundle a customer can verify offline with "
            "mcp/xnaut_verify.py: records, checkpoints, and the HSM's public key with "
            "its key attestation. Redacted by default; pass include_arguments to embed "
            "the tool arguments as well."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "where to write it (default: evidence/bundle.json)",
                },
                "session_id": {
                    "type": "string",
                    "description": "export only this session (default: all)",
                },
                "include_arguments": {
                    "type": "boolean",
                    "description": "embed tool arguments (default false)",
                },
            },
        },
    },
    {
        "name": "create_key",
        "description": (
            "Create a sign-only RSA key inside the Securosys HSM (never extractable, "
            "no approval policy) and write its public key PEM and SHA-256 fingerprint "
            "to the xNAUT data dir. Idempotent: an existing label is returned, not "
            "replaced. Use it to mint a signing key for another product, for "
            "example NautGate's verified audit trail."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "label": {
                    "type": "string",
                    "description": "the key's label in TSB, e.g. NAUTGATE_AUDIT_KEY",
                },
                "key_size": {
                    "type": "integer",
                    "description": "RSA modulus bits, default 4096",
                },
            },
            "required": ["label"],
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
            elif name == "checkpoint":
                result = do_checkpoint(args)
            elif name == "export":
                result = do_export(args)
            elif name == "verify_evidence":
                result = do_verify_evidence(args)
            elif name == "create_key":
                result = tsb_create_key(
                    args.get("label", ""), int(args.get("key_size") or 4096)
                )
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
            reply = {
                "jsonrpc": "2.0",
                "id": msg["id"],
                "error": {
                    "code": -32601,
                    "message": f"method not found: {msg.get('method')}",
                },
            }
        else:
            reply = {"jsonrpc": "2.0", "id": msg["id"], "result": result}
        sys.stdout.write(json.dumps(reply) + "\n")
        sys.stdout.flush()


def selftest_join(write) -> None:
    """Export against a stub gateway. Run from inside selftest's scratch dir."""
    import http.server
    import threading

    import xnaut_verify as verify_module

    receipts = {}
    for index in (0, 1):
        receipt = {
            "schema": "dev.nautgate.decision-receipt/v1",
            "receipt_id": f"rcpt-{index}",
            "decision_id": f"dec-{index}",
            "sequence": index,
        }
        digest = ngev.receipt_hash(receipt)
        sibling = bytes(32)
        root = ngev.merkle_parent(ngev.merkle_leaf(digest), sibling)
        receipts[receipt["receipt_id"]] = {
            "bundle_schema": "dev.nautgate.evidence-bundle/v1",
            "receipt": receipt,
            "receipt_hash": digest.hex(),
            "leaf_index": 0,
            "merkle_proof": [{"hash": sibling.hex(), "side": "right"}],
            "checkpoint": {
                "schema": "dev.nautgate.audit-checkpoint/v1",
                "checkpoint_id": f"ngcp-{index}",
                "merkle_root": root.hex(),
                "first_sequence": 0,
                "last_sequence": 1,
                "receipt_count": 2,
                "signing_key_id": "K",
            },
            "signature": {
                "algorithm": "SHA256_WITH_RSA",
                "encoding": "base64-der",
                "value": "",
                "key_id": "K",
                "public_key_fingerprint": "sha256:ng",
            },
        }

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path == "/v1/audit/keys":
                body, code = (
                    {"schema": "dev.nautgate.signing-key-history/v1", "keys": []},
                    200,
                )
            elif self.path.startswith("/v1/audit/receipts/"):
                wanted = self.path.split("/")[4]
                body = receipts.get(wanted)
                code = 200 if body else 404
            else:
                body, code = None, 404
            payload = json.dumps(body or {"detail": "not found"}).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, *_args):
            pass

    server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    global NAUTGATE_URL, NAUTGATE_API_KEY
    NAUTGATE_URL = f"http://127.0.0.1:{server.server_address[1]}"
    NAUTGATE_API_KEY = "selftest"
    try:
        write("gw", 1, kind="model_call")
        # Two ids on the last record: one the stub can export, one it cannot.
        rows = EXECUTION_LOG.read_text(encoding="utf-8").splitlines()
        row = json.loads(rows[-1])
        row["nautgate"] = {"decision_id": "dec-0", "receipt_id": "rcpt-0"}
        row.pop("hash")
        row["hash"] = ev.record_hash(row)
        rows[-1] = json.dumps(row)
        rows.append(
            json.dumps(
                chained_after(row, {"decision_id": "dec-9", "receipt_id": "rcpt-9"})
            )
        )
        EXECUTION_LOG.write_text("\n".join(rows) + "\n", encoding="utf-8")

        exported = do_export({"session_id": "gw"})
        assert exported["nautgate_bundles"] == 1, exported
        assert exported["nautgate_pending"] == ["rcpt-9"], exported
        bundle = json.loads(Path(exported["path"]).read_text(encoding="utf-8"))
        assert len(bundle["nautgate_bundles"]) == 1
        assert bundle["nautgate_keys"]["keys"] == []
        bundle["checkpoints"] = []
        report = verify_module.verify_bundle(bundle)
        gateway = report["nautgate"]
        assert gateway["verified"] == 1 and gateway["referenced"] == 2, gateway
        assert (
            gateway["signatures_verified"] == 0 and not gateway["signatures_checkable"]
        ), gateway
        assert gateway["awaiting_checkpoint"] == ["rcpt-9"], gateway
    finally:
        NAUTGATE_URL, NAUTGATE_API_KEY = "", ""
        server.shutdown()
        server.server_close()


def chained_after(previous: dict, nautgate: dict) -> dict:
    row = {
        "schema_version": ev.RECORD_SCHEMA,
        "record_id": "gw-next",
        "session_id": previous["session_id"],
        "seq": previous["seq"] + 1,
        "prev_hash": previous["hash"],
        "recorded_at": "2026-08-20T00:00:09.000Z",
        "executor_id": previous["executor_id"],
        "executor_version": "0",
        "kind": "model_call",
        "nautgate": nautgate,
    }
    row["hash"] = ev.record_hash(row)
    return row


def selftest() -> None:
    """Seal a scratch log end to end with a fake HSM, then break it on purpose.

    Everything below the TSB call is exercised for real: chaining, the sealed
    watermark, the checkpoint chain, and the refusal to sign a broken log.
    Run: python3 mcp/securosys-attest.py --selftest
    """
    import shutil
    import tempfile

    # The vector the browser verifier on xnaut.dev is pinned to. attest/index.html
    # recomputes this in WebCrypto; if either side changes canonical form or
    # domain alone, every published link stops verifying and this fires first.
    assert (
        link_hash(
            {
                "seq": 0,
                "prev": GENESIS,
                "ts": "2026-08-20T00:00:00Z",
                "subject": "x",
                "digest": "ab",
                "key_name": "K",
                "algorithm": "SHA256_WITH_RSA",
                "signature": "sig",
            }
        )
        == "4b52e8288b7e88cd166316cee76d2799b93d6cafe45c9e9e584386a76def9486"
    ), "link_hash drifted from the browser verifier"
    global DATA_DIR, EVIDENCE_DIR, EXECUTION_LOG, CHECKPOINTS, TSB_URL, KEY_NAME
    global tsb_sign, tsb_public_key

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

        def tsb_sign(payload):
            signed.append(payload)
            return base64.b64encode(payload[:8]).decode()

        def tsb_public_key():
            return {
                "key_name": KEY_NAME,
                "algorithm": ALGORITHM,
                "public_key_spki_b64": xv.PINNED_SPKI_B64,
                "fingerprint_sha256": xv.PINNED_FINGERPRINT,
            }

        def write(session, count, kind="tool_call"):
            rows = ev.read_records(EXECUTION_LOG) if EXECUTION_LOG.exists() else []
            head = [r for r in rows if r["session_id"] == session]
            seq = len(head)
            prev = head[-1]["hash"] if head else None
            with EXECUTION_LOG.open("a", encoding="utf-8") as f:
                for _ in range(count):
                    row = {
                        "schema_version": ev.RECORD_SCHEMA,
                        "record_id": str(seq) + session,
                        "session_id": session,
                        "seq": seq,
                        "prev_hash": prev,
                        "recorded_at": f"2026-08-20T00:00:0{seq % 10}.000Z",
                        "executor_id": "xnaut:selftest",
                        "executor_version": "0",
                        "kind": kind,
                    }
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

        # Export, then verify the bundle with the offline verifier a customer
        # gets. The checkpoints here are signed by the fake above, so the
        # signature check must REFUSE them; that refusal is the assertion.
        bundle = json.loads(Path(do_export({})["path"]).read_text(encoding="utf-8"))
        assert len(bundle["records"]) == 6 and len(bundle["checkpoints"]) == 3
        try:
            xv.verify_bundle(bundle)
            raise AssertionError(
                "the offline verifier accepted a signature the HSM never made"
            )
        except xv.VerificationError as exc:
            assert "signature does not verify" in str(exc), exc
        # Without checkpoints the same bundle is internally consistent, and the
        # verifier says so while calling the whole thing unattested.
        bundle["checkpoints"] = []
        report = xv.verify_bundle(bundle)
        assert report["records"] == 6 and report["checkpoints_verified"] == 0, report
        assert report["sessions"]["s1"]["unattested_tail"] == 5, report
        assert not bundle.get("arguments"), "a default export must be redacted"
        # No gateway configured is the ordinary install: the export must not
        # grow a NautGate section, and the bundle must verify without one.
        assert "nautgate_bundles" not in bundle and "nautgate_keys" not in bundle
        assert report["nautgate"]["present"] is False, report

        # The gateway join, against a stub NautGate rather than a live one: the
        # export must collect the routing ids out of the chain, fetch a bundle
        # per id, carry the keys beside it, and treat a receipt whose
        # checkpoint is not signed yet as pending rather than missing.
        selftest_join(write)

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
    elif len(sys.argv) >= 3 and sys.argv[1] == "create-key":
        # Terminal use: SECUROSYS_TSB_URL and a credential in the env.
        print(
            json.dumps(
                tsb_create_key(
                    sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 4096
                ),
                indent=2,
            )
        )
    else:
        main()
