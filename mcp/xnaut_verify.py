#!/usr/bin/env python3
"""Verify an xNAUT evidence bundle offline. No xNAUT, no network, no HSM.

    python3 xnaut_verify.py bundle.json

Exit 0 and a summary if the bundle is internally consistent and every
checkpoint's signature verifies under the public key the bundle carries.
Exit 1 and the first failure otherwise.

This is the file that makes the trail a product rather than a feature: until a
customer can check our claim without us, the claim is just our word. Read it.
It is stdlib only and it is meant to be read.

Two files, not one. The normative bytes (canonical JSON, domains, hashing,
Merkle, checkpoint payload) live in xnaut_evidence.py beside this file and are
imported, never copied. A second copy of a canonicalization is how two
implementations of the same spec quietly stop agreeing, which is a defect we
have already shipped once in this codebase.

RSA verification is written out here, in about forty lines, rather than pulled
from `cryptography`. PKCS#1 v1.5 verification is modular exponentiation with a
public exponent and a constant prefix; a dependency for that would cost an
auditor more than it saves them, and system Python does not have it installed.

What this proves
  - every record hashes to its own hash, under the domain-separated canonical
    bytes, so no field was changed
  - each session's records form an unbroken chain from seq 0, so none was
    removed or reordered
  - each checkpoint's Merkle root is exactly the records the bundle holds for
    it, so none was dropped from a sealed batch
  - checkpoints chain to each other, so no whole batch was dropped
  - the HSM signed each checkpoint, so none of the above was rewritten after
    the fact by anyone without the key

What this does NOT prove
  - that the public key is ours. Compare the printed fingerprint against the
    one we publish, or against the HSM key attestation the bundle carries.
  - that the records describe what really happened on the machine. The trail
    proves integrity and order, not honesty at the moment of capture.
  - anything about records after the last checkpoint. Those are reported
    separately as an unattested tail, and they are exactly as trustworthy as
    an unsigned log, which is to say not very.
"""

from __future__ import annotations

import base64
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import nautgate_evidence as ng  # noqa: E402 -- sibling module, path set just above
import xnaut_evidence as ev  # noqa: E402 -- sibling module, path set just above

BUNDLE_SCHEMA = "xnaut.evidence-bundle/v1"

# The DigestInfo prefix for SHA-256, RFC 8017 section 9.2 note 1. Constant.
SHA256_DIGEST_INFO = bytes.fromhex("3031300d060960864801650304020105000420")


class VerificationError(Exception):
    """The bundle does not hold up. The message says where."""


# ---- DER, only as much as an SPKI needs ------------------------------------

def _der_read(buf: bytes, at: int) -> tuple[int, bytes, int]:
    """One TLV at `at`. Returns (tag, value, index after)."""
    tag = buf[at]
    length = buf[at + 1]
    at += 2
    if length & 0x80:
        count = length & 0x7F
        length = int.from_bytes(buf[at:at + count], "big")
        at += count
    return tag, buf[at:at + length], at + length


def rsa_public_numbers(spki_der: bytes) -> tuple[int, int]:
    """(modulus, exponent) from a SubjectPublicKeyInfo.

    SEQUENCE { SEQUENCE { OID, NULL }, BIT STRING { SEQUENCE { INT n, INT e } } }
    """
    tag, outer, _ = _der_read(spki_der, 0)
    if tag != 0x30:
        raise VerificationError("public key: not a DER SEQUENCE")
    _, _, after_alg = _der_read(outer, 0)
    tag, bits, _ = _der_read(outer, after_alg)
    if tag != 0x03:
        raise VerificationError("public key: no BIT STRING")
    tag, inner, _ = _der_read(bits[1:], 0)  # bits[0] is the unused-bit count
    if tag != 0x30:
        raise VerificationError("public key: BIT STRING does not hold a SEQUENCE")
    tag, modulus, after_n = _der_read(inner, 0)
    if tag != 0x02:
        raise VerificationError("public key: no modulus")
    tag, exponent, _ = _der_read(inner, after_n)
    if tag != 0x02:
        raise VerificationError("public key: no exponent")
    return int.from_bytes(modulus, "big"), int.from_bytes(exponent, "big")


def rsa_verify_sha256(spki_der: bytes, message: bytes, signature: bytes) -> bool:
    """PKCS#1 v1.5 verify. RFC 8017 section 8.2.2, done by re-encoding.

    Re-encode rather than parse the recovered block: a parser that skips over
    the padding is how Bleichenbacher'06 signature forgeries get accepted. A
    byte-for-byte comparison cannot be lenient by accident.
    """
    n, e = rsa_public_numbers(spki_der)
    k = (n.bit_length() + 7) // 8
    if len(signature) != k:
        return False
    recovered = pow(int.from_bytes(signature, "big"), e, n).to_bytes(k, "big")
    digest = hashlib.sha256(message).digest()
    tail = SHA256_DIGEST_INFO + digest
    expected = b"\x00\x01" + b"\xff" * (k - len(tail) - 3) + b"\x00" + tail
    return recovered == expected


# ---- the bundle -------------------------------------------------------------

def _args_hashes(value, found=None) -> list[str]:
    """Every `args_hash` anywhere in a record.

    A walk rather than a read of record["tool"]["args_hash"], which is where v1
    puts it, so that a later record kind carrying arguments somewhere else does
    not silently stop being checked. Silently stopping is this codebase's
    characteristic failure.
    """
    found = [] if found is None else found
    if isinstance(value, dict):
        for key, child in value.items():
            if key == "args_hash" and isinstance(child, str):
                found.append(child)
            else:
                _args_hashes(child, found)
    elif isinstance(value, list):
        for child in value:
            _args_hashes(child, found)
    return found


def verify_nautgate(bundle: dict, records: list) -> dict:
    """Verify every NautGate bundle in this export, and the join to our records.

    Each gateway bundle is checked on its own terms, exactly as NautGate's own
    `audit_verify.py` checks it: the receipt hashes to what the bundle claims,
    its Merkle proof reaches the checkpoint root, the leaf index and the
    sequence agree, and the checkpoint's signature verifies under the key it
    names. The signature is only checkable when the export also captured
    `/v1/audit/keys`; without it the structure still verifies and the report
    says the signature was NOT checked, rather than passing quietly.

    Then the join: a gateway receipt must be one an xNAUT record references, and
    a referenced receipt with no bundle is reported, not fatal. Receipts are
    exported only once their checkpoint is signed, so a fresh session
    legitimately has none yet, and calling that missing would read as tampering.
    """
    bundles = bundle.get("nautgate_bundles") or []
    referenced = {}
    for record in records:
        receipt_id = ((record.get("nautgate") or {}).get("receipt_id"))
        if isinstance(receipt_id, str) and receipt_id:
            referenced[receipt_id] = record.get("session_id") or ""
    if not bundles:
        return {"present": False, "referenced": len(referenced),
                "verified": 0, "signatures_verified": 0,
                "awaiting_checkpoint": sorted(referenced)}

    keys = {}
    for row in (bundle.get("nautgate_keys") or {}).get("keys") or []:
        pem = row.get("public_key_pem")
        if isinstance(pem, str) and pem.strip():
            keys[row.get("public_key_fingerprint")] = ng.spki_from_pem(pem)

    seen, signed, fingerprints = set(), 0, set()
    for number, doc in enumerate(bundles, start=1):
        where = f"NautGate bundle {number}"
        if doc.get("bundle_schema") != ng.BUNDLE_SCHEMA:
            raise VerificationError(f"{where}: schema {doc.get('bundle_schema')!r}")
        receipt = doc.get("receipt") or {}
        checkpoint = doc.get("checkpoint") or {}
        signature = doc.get("signature") or {}
        try:
            digest = ng.receipt_hash(receipt)
            if doc.get("receipt_hash") != digest.hex():
                raise VerificationError(f"{where}: the receipt does not hash to its own hash")
            root = ng.verify_merkle_proof(digest, doc.get("merkle_proof") or [])
            if root.hex() != checkpoint.get("merkle_root"):
                raise VerificationError(f"{where}: the inclusion proof does not reach the checkpoint root")
            payload = ng.checkpoint_payload(checkpoint)
        except ng.EvidenceFormatError as exc:
            raise VerificationError(f"{where}: {exc}") from None

        index, sequence = doc.get("leaf_index"), receipt.get("sequence")
        if not isinstance(index, int) or index < 0 or index >= checkpoint.get("receipt_count", 0):
            raise VerificationError(f"{where}: the Merkle leaf index is outside the checkpoint")
        if sequence != checkpoint.get("first_sequence", 0) + index:
            raise VerificationError(f"{where}: the receipt sequence and the leaf index disagree")
        if checkpoint.get("receipt_count") != (
                checkpoint.get("last_sequence", 0) - checkpoint.get("first_sequence", 0) + 1):
            raise VerificationError(f"{where}: the checkpoint range and its receipt count disagree")
        if signature.get("key_id") != checkpoint.get("signing_key_id"):
            raise VerificationError(f"{where}: the signature names a different key than the checkpoint")

        fingerprint = signature.get("public_key_fingerprint")
        fingerprints.add(fingerprint)
        spki = keys.get(fingerprint)
        if spki:
            if signature.get("algorithm") != "SHA256_WITH_RSA" or signature.get("encoding") != "base64-der":
                raise VerificationError(f"{where}: unsupported signature algorithm or encoding")
            raw_signature = base64.b64decode(signature.get("value") or "")
            if not rsa_verify_sha256(spki, payload, raw_signature):
                raise VerificationError(f"{where}: the checkpoint signature does not verify under {fingerprint}")
            signed += 1

        receipt_id = receipt.get("receipt_id")
        if receipt_id not in referenced:
            raise VerificationError(
                f"{where}: receipt {receipt_id!r}, which no xNAUT record names. "
                "A gateway receipt in this bundle must belong to a call this trail recorded.")
        seen.add(receipt_id)

    return {
        "present": True,
        "referenced": len(referenced),
        "verified": len(bundles),
        "signatures_verified": signed,
        "signatures_checkable": bool(keys),
        "awaiting_checkpoint": sorted(set(referenced) - seen),
        "key_fingerprints": sorted(f for f in fingerprints if f),
    }


def verify_bundle(bundle: dict) -> dict:
    """Every check, in order of how cheap it is to fake. Raises on the first
    failure, because a report that lists eight passes and one failure invites
    somebody to read the eight."""
    if bundle.get("schema") != BUNDLE_SCHEMA:
        raise VerificationError(f"not an evidence bundle: schema {bundle.get('schema')!r}")

    records = bundle.get("records") or []
    if not records:
        raise VerificationError("bundle carries no records")

    # 1 + 2: self-hashes and the per-session chain. verify_chain does both and
    # raises naming the record, which is the message an auditor needs.
    try:
        heads = ev.verify_chain(records)
    except ev.EvidenceFormatError as exc:
        raise VerificationError(str(exc)) from None
    by_session = ev.sessions(records)

    key = bundle.get("signing_key") or {}
    spki = base64.b64decode(key.get("public_key_spki_b64") or "")
    if not spki:
        raise VerificationError("bundle carries no public key, so nothing can be verified")
    fingerprint = "sha256:" + hashlib.sha256(spki).hexdigest()

    # 3 + 4 + 5: each checkpoint covers what it says, links to the one before,
    # and carries the HSM's signature over its own canonical bytes.
    previous = None
    sealed_through: dict[str, int] = {}
    checked = []
    for number, row in enumerate(bundle.get("checkpoints") or [], start=1):
        cp = row.get("checkpoint") or {}
        session = cp.get("session_id") or ""
        covered = [r for r in by_session.get(session, [])
                   if cp["first_seq"] <= int(r["seq"]) <= cp["last_seq"]]
        if len(covered) != cp.get("record_count"):
            raise VerificationError(
                f"checkpoint {number}: says {cp.get('record_count')} records, "
                f"the bundle holds {len(covered)}")
        if ev.merkle_root([r["hash"] for r in covered]).hex() != cp.get("merkle_root"):
            raise VerificationError(f"checkpoint {number}: those records do not build its Merkle root")
        if cp.get("previous_checkpoint_sha256") != previous:
            raise VerificationError(f"checkpoint {number}: does not link to the checkpoint before it")
        payload = ev.checkpoint_payload(cp)
        own = "sha256:" + hashlib.sha256(payload).hexdigest()
        if own != row.get("checkpoint_sha256"):
            raise VerificationError(f"checkpoint {number}: does not hash to its own hash")
        signature = base64.b64decode(row.get("signature") or "")
        if not signature:
            raise VerificationError(f"checkpoint {number}: unsigned")
        if (row.get("algorithm") or "SHA256_WITH_RSA") != "SHA256_WITH_RSA":
            raise VerificationError(f"checkpoint {number}: unsupported algorithm {row.get('algorithm')!r}")
        if not rsa_verify_sha256(spki, payload, signature):
            raise VerificationError(
                f"checkpoint {number}: signature does not verify under {fingerprint}")
        previous = own
        sealed_through[session] = max(sealed_through.get(session, -1), int(cp["last_seq"]))
        checked.append(cp.get("checkpoint_id"))

    # 6: the arguments, when the exporter was asked to include them. A blob that
    # does not hash to the args_hash in its record is a substituted argument,
    # which is the single most valuable thing for someone to forge.
    wanted = {}
    for record in records:
        for args_hash in _args_hashes(record):
            wanted[args_hash] = record.get("session_id") or ""
    supplied = 0
    for session, blobs in (bundle.get("arguments") or {}).items():
        for args_hash, plaintext in blobs.items():
            if ev.digest(ev.ARGS_DOMAIN, plaintext.encode("utf-8")) != args_hash:
                raise VerificationError(
                    f"session {session}: the arguments given for {args_hash} hash to something else")
            if wanted.get(args_hash) != session:
                raise VerificationError(
                    f"session {session}: arguments for {args_hash}, which no record in it names")
            supplied += 1

    # 7: the gateway's own account of the same model calls, when the export
    # found one. Signed by a different party over a different fact, which is
    # the only reason it is worth carrying: two chains agreeing is a claim an
    # auditor can check without believing either product. Absent is normal.
    gateway = verify_nautgate(bundle, records)

    unattested = {s: (seq - 1) - sealed_through.get(s, -1) for s, seq in heads.items()}
    return {
        "nautgate": gateway,
        "ok": True,
        "records": len(records),
        "sessions": {s: {"records": seq, "sealed_through_seq": sealed_through.get(s),
                         "unattested_tail": unattested[s]} for s, seq in heads.items()},
        "checkpoints_verified": len(checked),
        "signatures_verified": len(checked),
        "arguments_verified": supplied,
        "arguments_included": bool(bundle.get("arguments")),
        "key_name": key.get("key_name"),
        "public_key_fingerprint": fingerprint,
        "hsm_attestation_present": bool(key.get("hsm_attestation_xml")),
    }


# ---- selftest ---------------------------------------------------------------

# A real signature from the real HSM, pinned. `python3 xnaut_verify.py --selftest`
# checks that this file's forty lines of RSA still accept it and still reject a
# message that moved by one byte. Produced 2026-08-20 by XNAUT_ATTEST_KEY
# (RSA 4096, never_extractable) over the message below.
PINNED_SPKI_B64 = (
    "MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEApUOdgzxyediptiA11tkB9WIVo/At"
    "MSPONqAr3w88tG3M6jAp8bpGgCKouRqD7oxvj1ecvogH4rysZw/z5OhW9M4ioeIMFiwktAl4"
    "72jXlS5OjpcTo54T99maEUCHcSzRLdY6OpqhKDOlVTFMQpYkels7YiDpZTKW/4B0BtcO+vQB"
    "PB8ErPu5OPlwvxxp6K7BNtqA0T53ieUc8PX4l2RnOAQ0TCgRwfN16MUhSqKzLtFAHpf10Ml4"
    "O8aCrDh894JHdWO0WRC/FeF3zwrguAU9KRbooP9tq475/7Cb6IxKqVCgECW0ixgxXSWKN3hR"
    "l7ODjU/Vl1fOAzV/B9aT9jlkGGcqPJoM7G7UMDYDGUD5n31i4lvwATyCmccmwsmeHFIlxLgB"
    "keutznyuHWaOIqc4lNYKWP3ozQLe3Pqz6qJVf7O6jO0eF7y5DMReEtEuE8U0i8i+wbqjYiSe"
    "KZuX6QXc+JTrbMS3XiJsfWADNi46Q1DRTEza6j+7fYfvQ6ffXQ8/AXopOJPAx1oxxQk5DsKE"
    "G4NLFLgrO2pEz0e978BkljoY6wjFCnLPj4QIdlhAXsjMi5jXb0S9b00P/o0XxVh+8tUtu7IP"
    "MM/bgE9e0UJiMSl+vjPrw623+s+e8g5HNcV8RDxMluUbX0R3A5Pio4OEZgD27VeDw2xTLOSN"
    "1Bv/V7ECAwEAAQ=="
)
PINNED_SIGNATURE_B64 = (
    "e39Frj6Qs7BT48TZi4znmsybsX7JuIsaHB8aisu+HhuTCNFt390fwGjPTt4TOTRmxpJ8EN0/"
    "rc8brO3CMCIQ9fXAEoI5b41FBTCf6oLUXhmap4ILdxup2mrO7jVn9xyGeadlHdm00nXkC70L"
    "Sxg4RviHmUrc3J7JoZNsOUWzFAysvOmeeS4ip5cNHY8RkB4jjNMzN55ChD3zfWNxmP2YoCIp"
    "JWhVaNqwiZ9g/lfHE31EmDeELX3IeWsFmfDSyW+kABYZ4b9OkNv85rScGJ8qAr6+lIh1dIPx"
    "EvpX4WVcSLWiqFIRAuxY/4WdV/BZGZmunp0Uzgc3L5zGyRpjabDyi4BZ78iiNd+77Zkyjwft"
    "MzWF7InqjDwxDRRxJxTUWttfct57VkXVt2VHkSy+9eIxOrKnE94KFpO//KAWQKV2dLSWWiAD"
    "ybWsSWQ8J10gzq5kVGcHwLfDXeRh5GstQk3VW32QmfNxLfKXe9mJU8W4oC76mT3DqZjee4jT"
    "drCIz9kf7yKoAR1qSaZ0zoEal/BjpyFdDou4d9btK8+YuFVnpvtK8exa0UC687K475wnQCIQ"
    "YCTwawc9VJRxI96IWLCpjh+KrX4vXkh3NtPUhYuF/CS4kzseylnm9/bjermtBGuN7gqVCPtt"
    "ZfcQ6XLZJBwQt//+cVykoKfZe27EpWLBTV0="
)

PINNED_MESSAGE = b"xnaut-verify pinned vector v1"
PINNED_FINGERPRINT = "sha256:b860ec465fc05ebf2c515b31fd87e9ad763580d9ee1ade927ce44f79b1b62e8f"


def selftest_nautgate(bundle: dict, records: list, session: str) -> None:
    """The join, without a gateway: build NautGate-shaped evidence by hand.

    The signature is not exercised here (that is the pinned HSM vector above);
    what is exercised is every check that stands between a bundle and a false
    pass: the receipt hash, the inclusion proof, the leaf index, and the rule
    that a gateway receipt must belong to a call this trail recorded.
    """
    receipts = [{"schema": ng.RECEIPT_SCHEMA, "receipt_id": f"rcpt-{i}",
                 "decision_id": f"dec-{i}", "sequence": i} for i in range(3)]
    digests = [ng.receipt_hash(r) for r in receipts]
    # The same tree NautGate builds: leaves, pairwise, odd node promoted.
    level = [ng.merkle_leaf(d) for d in digests]
    proofs = [[] for _ in digests]
    spans = [[i] for i in range(len(digests))]
    while len(level) > 1:
        nxt, nxt_spans = [], []
        for i in range(0, len(level), 2):
            if i + 1 == len(level):
                nxt.append(level[i])
                nxt_spans.append(spans[i])
                continue
            for leaf in spans[i]:
                proofs[leaf].append({"hash": level[i + 1].hex(), "side": "right"})
            for leaf in spans[i + 1]:
                proofs[leaf].append({"hash": level[i].hex(), "side": "left"})
            nxt.append(ng.merkle_parent(level[i], level[i + 1]))
            nxt_spans.append(spans[i] + spans[i + 1])
        level, spans = nxt, nxt_spans

    checkpoint = {"schema": ng.CHECKPOINT_SCHEMA, "checkpoint_id": "ngcp-1",
                  "merkle_root": level[0].hex(), "first_sequence": 0,
                  "last_sequence": 2, "receipt_count": 3, "signing_key_id": "K1"}
    docs = [{"bundle_schema": ng.BUNDLE_SCHEMA, "receipt": r, "receipt_hash": d.hex(),
             "leaf_index": i, "merkle_proof": proofs[i], "checkpoint": checkpoint,
             "signature": {"algorithm": "SHA256_WITH_RSA", "encoding": "base64-der",
                           "value": "", "key_id": "K1",
                           "public_key_fingerprint": "sha256:ng"}}
            for i, (r, d) in enumerate(zip(receipts, digests))]

    joined = dict(bundle)
    joined["records"] = [dict(r) for r in records]
    for record, receipt in zip(joined["records"], receipts):
        record["nautgate"] = {"decision_id": receipt["decision_id"],
                              "receipt_id": receipt["receipt_id"]}
        record["hash"] = ev.record_hash(record)
    prev = None
    for record in joined["records"]:
        record["prev_hash"] = prev
        record["hash"] = ev.record_hash(record)
        prev = record["hash"]
    joined["nautgate_bundles"] = docs

    report = verify_bundle(joined)
    assert report["nautgate"]["verified"] == 3, report
    assert report["nautgate"]["signatures_verified"] == 0, report
    assert report["nautgate"]["awaiting_checkpoint"] == [], report

    # One byte of a NautGate receipt, and the NautGate half fails by name.
    tampered = json.loads(json.dumps(joined))
    tampered["nautgate_bundles"][1]["receipt"]["decision_id"] = "dec-forged"
    try:
        verify_bundle(tampered)
        raise AssertionError("a mutated NautGate receipt verified")
    except VerificationError as exc:
        assert "NautGate bundle 2" in str(exc), exc

    # A gateway receipt no xNAUT record names is refused: without this the
    # exporter could pad a bundle with somebody else's evidence.
    orphan = json.loads(json.dumps(joined))
    orphan["records"][2].pop("nautgate")
    prev = None
    for record in orphan["records"]:
        record["prev_hash"] = prev
        record["hash"] = ev.record_hash(record)
        prev = record["hash"]
    try:
        verify_bundle(orphan)
        raise AssertionError("an unreferenced gateway receipt verified")
    except VerificationError as exc:
        assert "no xNAUT record names" in str(exc), exc

    # A referenced receipt whose bundle has not been exported yet is pending,
    # not a failure: NautGate exports a receipt once its checkpoint is signed.
    pending = json.loads(json.dumps(joined))
    pending["nautgate_bundles"] = docs[:2]
    assert verify_bundle(pending)["nautgate"]["awaiting_checkpoint"] == ["rcpt-2"]

    # And with every gateway document removed it is the plain xNAUT bundle again.
    alone = json.loads(json.dumps(joined))
    del alone["nautgate_bundles"]
    assert verify_bundle(alone)["nautgate"]["present"] is False
    print("ok: the NautGate join verifies, refuses a forged receipt, and stays optional")


def selftest() -> None:
    spki = base64.b64decode(PINNED_SPKI_B64)
    signature = base64.b64decode(PINNED_SIGNATURE_B64)
    assert "sha256:" + hashlib.sha256(spki).hexdigest() == PINNED_FINGERPRINT, \
        "the pinned public key is not the key it claims to be"
    n, e = rsa_public_numbers(spki)
    assert n.bit_length() == 4096 and e == 65537, (n.bit_length(), e)
    assert rsa_verify_sha256(spki, PINNED_MESSAGE, signature), \
        "a real HSM signature stopped verifying"
    # Each of these is a way a lenient verifier says yes to a forgery.
    assert not rsa_verify_sha256(spki, PINNED_MESSAGE + b"!", signature), "message can move"
    assert not rsa_verify_sha256(spki, PINNED_MESSAGE, signature[:-1]), "short signature accepted"
    assert not rsa_verify_sha256(spki, PINNED_MESSAGE, bytes(len(signature))), "zero signature accepted"
    forged = bytearray(signature)
    forged[-1] ^= 1
    assert not rsa_verify_sha256(spki, PINNED_MESSAGE, bytes(forged)), "flipped signature accepted"

    # A bundle with no checkpoints verifies its chain and reports the whole
    # thing as an unattested tail rather than quietly calling it proof.
    records = []
    prev, session = None, "selftest"
    for seq in range(3):
        row = {"schema_version": ev.RECORD_SCHEMA, "record_id": f"r{seq}", "session_id": session,
               "seq": seq, "prev_hash": prev, "recorded_at": "2026-08-20T00:00:00.000Z",
               "executor_id": "xnaut:selftest", "executor_version": "0", "kind": "tool_call",
               "tool": {"name": "Bash", "args_hash": ev.digest(ev.ARGS_DOMAIN, b'{"cmd":"ls"}'),
                        "args_size": 12}}
        row["hash"] = ev.record_hash(row)
        records.append(row)
        prev = row["hash"]
    bundle = {"schema": BUNDLE_SCHEMA, "records": records, "checkpoints": [],
              "signing_key": {"key_name": "K", "public_key_spki_b64": PINNED_SPKI_B64}}
    report = verify_bundle(bundle)
    assert report["sessions"][session]["unattested_tail"] == 3, report
    assert report["checkpoints_verified"] == 0
    # No gateway documents is the ordinary case and stays a pass.
    assert report["nautgate"] == {"present": False, "referenced": 0, "verified": 0,
                                  "signatures_verified": 0, "awaiting_checkpoint": []}, report

    selftest_nautgate(bundle, records, session)

    # Arguments are checked against the record that names them, or refused.
    bundle["arguments"] = {session: {records[0]["tool"]["args_hash"]: '{"cmd":"ls"}'}}
    assert verify_bundle(bundle)["arguments_verified"] == 1
    bundle["arguments"] = {session: {records[0]["tool"]["args_hash"]: '{"cmd":"rm -rf /"}'}}
    try:
        verify_bundle(bundle)
        raise AssertionError("substituted arguments were accepted")
    except VerificationError:
        pass
    del bundle["arguments"]

    # A mutated record breaks its own hash before anything else is looked at.
    records[1]["kind"] = "tool_refused"
    try:
        verify_bundle(bundle)
        raise AssertionError("a mutated record verified")
    except VerificationError as exc:
        assert "record 2" in str(exc), exc
    print("ok: RSA against a real HSM signature, forgeries refused, chain and arguments checked")


def main(argv: list[str]) -> int:
    if len(argv) == 2 and argv[1] == "--selftest":
        selftest()
        return 0
    if len(argv) != 2:
        print(__doc__.strip().splitlines()[0])
        print(f"usage: {Path(argv[0]).name} bundle.json", file=sys.stderr)
        return 2
    try:
        bundle = json.loads(Path(argv[1]).read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        print(f"FAIL  cannot read the bundle: {exc}", file=sys.stderr)
        return 1
    try:
        report = verify_bundle(bundle)
    except VerificationError as exc:
        print(f"FAIL  {exc}", file=sys.stderr)
        return 1

    print("OK    the bundle verifies")
    print(f"      {report['records']} records in {len(report['sessions'])} session(s)")
    print(f"      {report['checkpoints_verified']} checkpoint(s), every signature verified")
    print(f"      key {report['key_name']}  {report['public_key_fingerprint']}")
    print("      compare that fingerprint against the one we publish, or the key")
    print("      attestation in the bundle. This file cannot tell you it is ours.")
    gateway = report["nautgate"]
    if not gateway["present"]:
        print("      no gateway evidence: this is xNAUT's own account, signed by one party")
    else:
        checked = ("every signature verified" if gateway["signatures_verified"] == gateway["verified"]
                   else f"{gateway['signatures_verified']} of {gateway['verified']} signatures verified")
        print(f"      {gateway['verified']} NautGate receipt(s) cross-referenced, {checked}")
        if not gateway.get("signatures_checkable"):
            print("      NOT checked: no NautGate public key in the bundle, so the gateway's")
            print("      signatures were only structurally verified. Fetch /v1/audit/keys.")
        for fingerprint in gateway["key_fingerprints"]:
            print(f"      gateway key {fingerprint}")
    if gateway["awaiting_checkpoint"]:
        print(f"      {len(gateway['awaiting_checkpoint'])} routing receipt(s) referenced with no")
        print("      bundle yet: NautGate exports a receipt once its checkpoint is signed")
    if report["arguments_included"]:
        print(f"      {report['arguments_verified']} tool argument blob(s) match their records")
    else:
        print("      redacted: no arguments included, and none are needed to verify")
    for session, state in report["sessions"].items():
        tail = state["unattested_tail"]
        note = "all attested" if tail == 0 else f"{tail} record(s) past the last checkpoint, UNATTESTED"
        print(f"      {session}: {state['records']} records, {note}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
