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

    unattested = {s: (seq - 1) - sealed_through.get(s, -1) for s, seq in heads.items()}
    return {
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
