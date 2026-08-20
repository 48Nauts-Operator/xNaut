#!/usr/bin/env python3
"""The normative bytes of a NautGate evidence bundle, for offline verification.

Reimplemented from NautGate (48Nauts, same project), `core/app/audit_evidence.py`
(`receipt_hash`, `merkle_leaf`, `merkle_parent`, `verify_merkle_proof`,
`checkpoint_payload`) and the checks in `core/app/audit_verify.py::verify_bundle`.
Rewritten rather than imported on purpose: xNAUT must never depend on NautGate
being installed, and a verifier that needs the party under audit to ship a
library is not a verifier.

Departures from the original, all deliberate:
  * stdlib only. NautGate uses `cryptography`; the RSA verification here is the
    forty lines already in xnaut_verify.py.
  * the canonical JSON is imported from xnaut_evidence, not copied. The two
    profiles are the same strict RFC 8785 subset (integers only, UTF-16 key
    order), and a second copy of a canonicalizer is how two implementations of
    one spec quietly stop agreeing.
  * the signature is optional here. NautGate's bundle carries a fingerprint but
    no key; without one fetched from `/v1/audit/keys` we verify the structure
    and say plainly that the signature was not checked, rather than passing
    silently.

NautGate is Apache 2.0 and ours.
"""

from __future__ import annotations

import hashlib
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import xnaut_evidence as ev  # noqa: E402 -- sibling module, path set just above

RECEIPT_SCHEMA = "dev.nautgate.decision-receipt/v1"
CHECKPOINT_SCHEMA = "dev.nautgate.audit-checkpoint/v1"
BUNDLE_SCHEMA = "dev.nautgate.evidence-bundle/v1"

RECEIPT_DOMAIN = b"NAUTGATE-DECISION-RECEIPT-V1\0"
MERKLE_LEAF_DOMAIN = b"NAUTGATE-MERKLE-LEAF-V1\0"
MERKLE_NODE_DOMAIN = b"NAUTGATE-MERKLE-NODE-V1\0"
CHECKPOINT_DOMAIN = b"NAUTGATE-AUDIT-CHECKPOINT-V1\0"


class EvidenceFormatError(ValueError):
    """The document is not shaped like NautGate evidence."""


def receipt_hash(receipt: dict) -> bytes:
    if receipt.get("schema") != RECEIPT_SCHEMA:
        raise EvidenceFormatError(f"unsupported receipt schema: {receipt.get('schema')!r}")
    return hashlib.sha256(RECEIPT_DOMAIN + ev.canonical_json(receipt)).digest()


def merkle_leaf(receipt_digest: bytes) -> bytes:
    if len(receipt_digest) != 32:
        raise EvidenceFormatError("receipt digest must be exactly 32 bytes")
    return hashlib.sha256(MERKLE_LEAF_DOMAIN + receipt_digest).digest()


def merkle_parent(left: bytes, right: bytes) -> bytes:
    if len(left) != 32 or len(right) != 32:
        raise EvidenceFormatError("Merkle children must be exactly 32 bytes")
    return hashlib.sha256(MERKLE_NODE_DOMAIN + left + right).digest()


def verify_merkle_proof(receipt_digest: bytes, proof) -> bytes:
    """Apply an inclusion proof and return the root it reaches."""
    current = merkle_leaf(receipt_digest)
    for item in proof:
        try:
            sibling = bytes.fromhex(item["hash"])
            side = item["side"]
        except (KeyError, TypeError, ValueError) as exc:
            raise EvidenceFormatError("invalid Merkle proof item") from exc
        if len(sibling) != 32 or side not in ("left", "right"):
            raise EvidenceFormatError("invalid Merkle proof sibling")
        current = merkle_parent(sibling, current) if side == "left" else merkle_parent(current, sibling)
    return current


def checkpoint_payload(checkpoint: dict) -> bytes:
    """The exact bytes NautGate submitted to the HSM for this checkpoint."""
    if checkpoint.get("schema") != CHECKPOINT_SCHEMA:
        raise EvidenceFormatError(f"unsupported checkpoint schema: {checkpoint.get('schema')!r}")
    return CHECKPOINT_DOMAIN + ev.canonical_json(checkpoint)


def spki_from_pem(pem: str) -> bytes:
    """The DER SubjectPublicKeyInfo inside a PEM public key block.

    Only `BEGIN PUBLIC KEY`. A certificate would need X.509 parsing, and
    `/v1/audit/keys` returns bare public keys.
    """
    import base64

    lines = [l.strip() for l in pem.strip().splitlines()]
    if not lines or "BEGIN PUBLIC KEY" not in lines[0]:
        raise EvidenceFormatError("not a PEM public key block")
    body = "".join(l for l in lines[1:] if "END " not in l)
    try:
        return base64.b64decode(body, validate=True)
    except Exception as exc:  # noqa: BLE001 -- base64 raises several types
        raise EvidenceFormatError(f"PEM body is not base64: {exc}") from None
