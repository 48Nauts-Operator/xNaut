#!/usr/bin/env python3
"""Normative primitives for the xNAUT execution record: canonical bytes,
Merkle tree, checkpoint payload, chain verification.

No HTTP, no TSB, no MCP. This file defines the bytes the signer and any
verifier must agree on. Changing a prefix or a canonicalization rule is a
schema-version change, not a refactor.

Ported from NautGate's `core/app/audit_evidence.py` and `core/app/
audit_checkpoint.py::build_checkpoint` (AGPL-3.0, same project, 48Nauts).
The canonicalizer, the leaf/node split and the checkpoint shape are theirs;
the receipt builder, the key history and the database are not ported.

Where this departs, and why:

  * Domains and schema id are xNAUT's (`XNAUT-*-V1\\0`,
    `xnaut.audit-checkpoint/v1`). Same SCREAMING-KEBAB convention, different
    bytes, because these are different document types. A checkpoint over
    execution records must not verify as a checkpoint over routing receipts.
  * Digests are strings of the form `sha256:<hex>`, because that is how
    `src-tauri/src/evidence.rs` writes them into the record and a verifier
    reads the file, not our memory. NautGate keeps raw bytes in a column.
  * Sequences are per session and start at 0, so a checkpoint carries the
    session it covers. NautGate has one global sequence per instance
    starting at 1.
  * A checkpoint here is one session's contiguous run. Two sessions
    interleave in the file, and a Merkle tree over interleaved chains would
    prove inclusion in a batch nobody can name.

Run `python3 mcp/xnaut_evidence.py --selftest` to check the bytes.
"""

from __future__ import annotations

import hashlib
import json
import uuid
from collections.abc import Mapping, Sequence
from typing import Any

RECORD_DOMAIN = b"XNAUT-EXECUTION-RECORD-V1\0"
MERKLE_LEAF_DOMAIN = b"XNAUT-MERKLE-LEAF-V1\0"
MERKLE_NODE_DOMAIN = b"XNAUT-MERKLE-NODE-V1\0"
CHECKPOINT_DOMAIN = b"XNAUT-AUDIT-CHECKPOINT-V1\0"
# evidence.rs hashes a tool call's canonical arguments under this before
# writing them to a blob. The verifier needs it to check a blob against the
# record that names it.
ARGS_DOMAIN = b"XNAUT-TOOL-ARGUMENTS-V1\0"

RECORD_SCHEMA = "xnaut.execution-record/v1"
CHECKPOINT_SCHEMA = "xnaut.audit-checkpoint/v1"
CHECKPOINT_NAMESPACE = uuid.UUID("6f0a1d3c-1b6e-4a35-9a2f-0c4e6a2b7d18")

# RFC 8785 interoperable integers stop at the exact IEEE-754 range.
MAX_SAFE_INTEGER = 2**53 - 1


class EvidenceFormatError(ValueError):
    """The value cannot be represented by the v1 evidence contract."""


def _validate(value: Any, path: str = "$") -> None:
    if value is None or isinstance(value, (str, bool)):
        if isinstance(value, str):
            try:
                value.encode("utf-8")
                value.encode("utf-16-be")
            except UnicodeEncodeError as exc:
                raise EvidenceFormatError(
                    f"{path}: strings must contain Unicode scalars"
                ) from exc
        return
    if isinstance(value, int):
        if not -MAX_SAFE_INTEGER <= value <= MAX_SAFE_INTEGER:
            raise EvidenceFormatError(f"{path}: integer exceeds RFC 8785 safe range")
        return
    if isinstance(value, float):
        # Same refusal as the Rust side: ECMAScript Number::toString is easy to
        # get subtly wrong, and a chain that verifies here but fails at the
        # auditor is worse than no chain.
        raise EvidenceFormatError(f"{path}: floats are forbidden; use an integer unit")
    if isinstance(value, Mapping):
        for key, child in value.items():
            if not isinstance(key, str):
                raise EvidenceFormatError(f"{path}: object keys must be strings")
            _validate(key, f"{path}.<key>")
            _validate(child, f"{path}.{key}")
        return
    if isinstance(value, Sequence) and not isinstance(value, (str, bytes, bytearray)):
        for index, child in enumerate(value):
            _validate(child, f"{path}[{index}]")
        return
    raise EvidenceFormatError(f"{path}: unsupported value type {type(value).__name__}")


def _string(value: str) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def _canonical(value: Any) -> str:
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, str):
        return _string(value)
    if isinstance(value, int):
        return str(value)
    if isinstance(value, Mapping):
        # RFC 8785 sorts property names by UTF-16 code unit, not code point.
        # The orders genuinely differ above the BMP.
        keys = sorted(value, key=lambda item: item.encode("utf-16-be"))
        return (
            "{" + ",".join(f"{_string(k)}:{_canonical(value[k])}" for k in keys) + "}"
        )
    return "[" + ",".join(_canonical(item) for item in value) + "]"


def canonical_json(value: Any) -> bytes:
    """Deterministic UTF-8 JSON under the strict v1 JCS profile."""
    _validate(value)
    return _canonical(value).encode("utf-8")


def digest(domain: bytes, data: bytes) -> str:
    """Every hash is domain-separated. `sha256:<hex>`, as the record stores it."""
    return "sha256:" + hashlib.sha256(domain + data).hexdigest()


def raw(digest_string: str) -> bytes:
    """The 32 bytes behind a `sha256:<hex>` string."""
    if not digest_string.startswith("sha256:"):
        raise EvidenceFormatError(f"not a sha256 digest: {digest_string!r}")
    out = bytes.fromhex(digest_string[7:])
    if len(out) != 32:
        raise EvidenceFormatError("a sha256 digest is 32 bytes")
    return out


def record_hash(record: Mapping[str, Any]) -> str:
    """Hash of a record as `evidence.rs::record_hash` computes it.

    The `hash` field is removed first: a record cannot contain its own hash.
    """
    body = {k: v for k, v in record.items() if k != "hash"}
    if body.get("schema_version") != RECORD_SCHEMA:
        raise EvidenceFormatError(
            f"unsupported record schema: {body.get('schema_version')!r}"
        )
    return digest(RECORD_DOMAIN, canonical_json(body))


def merkle_leaf(record_digest: str) -> bytes:
    return hashlib.sha256(MERKLE_LEAF_DOMAIN + raw(record_digest)).digest()


def merkle_parent(left: bytes, right: bytes) -> bytes:
    if len(left) != 32 or len(right) != 32:
        raise EvidenceFormatError("Merkle children must be exactly 32 bytes")
    return hashlib.sha256(MERKLE_NODE_DOMAIN + left + right).digest()


def _level_up(level: list[bytes]) -> list[bytes]:
    """One level of the tree. An unpaired final node is promoted unchanged."""
    return [
        level[i] if i + 1 == len(level) else merkle_parent(level[i], level[i + 1])
        for i in range(0, len(level), 2)
    ]


def merkle_root(record_digests: Sequence[str]) -> bytes:
    if not record_digests:
        raise EvidenceFormatError("a Merkle tree requires at least one record")
    level = [merkle_leaf(d) for d in record_digests]
    while len(level) > 1:
        level = _level_up(level)
    return level[0]


def merkle_proof(
    record_digests: Sequence[str], leaf_index: int
) -> list[dict[str, str]]:
    """The ordered sibling path for one record, so it can be proved in isolation."""
    if not 0 <= leaf_index < len(record_digests):
        raise EvidenceFormatError("leaf index is outside the Merkle tree")
    level = [merkle_leaf(d) for d in record_digests]
    index, proof = leaf_index, []
    while len(level) > 1:
        sibling = index - 1 if index % 2 else index + 1
        if sibling < len(level):
            proof.append(
                {
                    "side": "left" if sibling < index else "right",
                    "hash": level[sibling].hex(),
                }
            )
        level = _level_up(level)
        index //= 2
    return proof


def verify_merkle_proof(
    record_digest: str, proof: Sequence[Mapping[str, str]]
) -> bytes:
    current = merkle_leaf(record_digest)
    for item in proof:
        try:
            sibling, side = bytes.fromhex(item["hash"]), item["side"]
        except (KeyError, TypeError, ValueError) as exc:
            raise EvidenceFormatError("invalid Merkle proof item") from exc
        if len(sibling) != 32 or side not in ("left", "right"):
            raise EvidenceFormatError("invalid Merkle proof sibling")
        current = (
            merkle_parent(sibling, current)
            if side == "left"
            else merkle_parent(current, sibling)
        )
    return current


def checkpoint_payload(checkpoint: Mapping[str, Any]) -> bytes:
    """The exact bytes handed to the HSM for a v1 checkpoint."""
    if checkpoint.get("schema") != CHECKPOINT_SCHEMA:
        raise EvidenceFormatError(
            f"unsupported checkpoint schema: {checkpoint.get('schema')!r}"
        )
    return CHECKPOINT_DOMAIN + canonical_json(checkpoint)


# --- reading the log the Rust side writes -----------------------------------


def read_records(path) -> list[dict]:
    """Every record in the JSONL log, in file order.

    A corrupt line raises rather than being skipped. Silently dropping a line
    is exactly the hole the chain exists to detect.
    """
    rows = []
    with open(path, encoding="utf-8") as handle:
        for number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            try:
                rows.append(json.loads(line))
            except ValueError as exc:
                raise EvidenceFormatError(
                    f"line {number}: corrupt, gap here: {exc}"
                ) from None
    return rows


def verify_chain(records: Sequence[dict]) -> dict[str, int]:
    """Recompute every hash and every link. Raises on the first break.

    Mirrors `evidence.rs::verify`: sequences and links are per session, so two
    sessions interleaving in one file is normal and not a gap.
    """
    heads: dict[str, tuple[int, Any]] = {}
    for number, row in enumerate(records, start=1):
        if record_hash(row) != row.get("hash"):
            raise EvidenceFormatError(f"record {number}: does not hash to its own hash")
        session = row.get("session_id") or ""
        seq, prev = heads.get(session, (0, None))
        if row.get("seq") != seq:
            raise EvidenceFormatError(
                f"record {number}: session {session} expected seq {seq}"
            )
        if row.get("prev_hash") != prev:
            raise EvidenceFormatError(
                f"record {number}: session {session} does not link to the record before it"
            )
        heads[session] = (seq + 1, row["hash"])
    return {session: seq for session, (seq, _) in heads.items()}


def sessions(records: Sequence[dict]) -> dict[str, list[dict]]:
    """Records grouped by chain, each in sequence order."""
    out: dict[str, list[dict]] = {}
    for row in records:
        out.setdefault(row.get("session_id") or "", []).append(row)
    return out


def build_checkpoint(
    records: Sequence[dict],
    *,
    executor_id: str,
    signing_key_id: str,
    previous_checkpoint_sha256: str | None = None,
) -> tuple[dict, bytes, str]:
    """Checkpoint, the bytes to sign, and the checkpoint's own hash.

    `records` must be one session's contiguous run. A gap raises: a Merkle root
    over a run with a hole proves inclusion in something that never happened.
    """
    if not records:
        raise EvidenceFormatError("cannot checkpoint an empty batch")
    session = records[0].get("session_id") or ""
    if any((r.get("session_id") or "") != session for r in records):
        raise EvidenceFormatError("a checkpoint covers one session")
    seqs = [int(r["seq"]) for r in records]
    if seqs != list(range(seqs[0], seqs[0] + len(seqs))):
        raise EvidenceFormatError(
            f"sequence gap in session {session}: {seqs[0]}..{seqs[-1]}"
        )
    digests = [r["hash"] for r in records]
    root = merkle_root(digests)
    stable = (
        f"{executor_id}:{session}:{seqs[0]}:{seqs[-1]}:{root.hex()}:"
        f"{previous_checkpoint_sha256 or 'genesis'}:{signing_key_id}"
    )
    checkpoint = {
        "schema": CHECKPOINT_SCHEMA,
        # uuid5, not uuid4: the same batch checkpointed twice must produce the
        # same id, or a retry after a failed TSB call looks like a second batch.
        "checkpoint_id": str(uuid.uuid5(CHECKPOINT_NAMESPACE, stable)),
        "executor_id": executor_id,
        "session_id": session,
        "first_seq": seqs[0],
        "last_seq": seqs[-1],
        "record_count": len(records),
        "merkle_algorithm": "sha256-binary-v1",
        "merkle_root": root.hex(),
        "opened_at": records[0]["recorded_at"],
        "closed_at": records[-1]["recorded_at"],
        "previous_checkpoint_sha256": previous_checkpoint_sha256,
        "signing_key_id": signing_key_id,
    }
    payload = checkpoint_payload(checkpoint)
    return checkpoint, payload, "sha256:" + hashlib.sha256(payload).hexdigest()


def selftest() -> None:
    # RFC 8785 section 3.2.3: keys sort by UTF-16 code unit. U+1F600's lead
    # surrogate is U+D83D, so it sorts BEFORE U+FB33 here and after it by code
    # point. Swap to sorted(value) and this line fails.
    assert (
        canonical_json({"\U0001f600": 1, "דּ": 2}) == '{"\U0001f600":1,"דּ":2}'.encode()
    ), "UTF-16 key order"
    assert canonical_json({"b": 1, "a": 2}) == b'{"a":2,"b":1}'
    # ensure_ascii=False, so a non-ASCII string is UTF-8 bytes, not \u escapes.
    assert canonical_json("€") == '"€"'.encode()
    try:
        canonical_json({"x": 1.5})
        raise AssertionError("a float should be refused")
    except EvidenceFormatError:
        pass

    # Domain separation: the same bytes under two domains are two hashes.
    assert digest(RECORD_DOMAIN, b"{}") != digest(CHECKPOINT_DOMAIN, b"{}")

    # The Rust side's hash, recomputed here. This vector is a real record
    # written by evidence.rs; if either implementation drifts it fails.
    rec = {
        "schema_version": RECORD_SCHEMA,
        "record_id": "a",
        "session_id": "s",
        "seq": 0,
        "prev_hash": None,
        "recorded_at": "2026-08-20T00:00:00.000Z",
        "executor_id": "xnaut:test",
        "executor_version": "1.19.0",
        "kind": "tool_call",
    }
    # Pinned on both sides: src-tauri/src/evidence.rs has the same vector in
    # `the_python_side_computes_the_same_hash_for_the_same_record`. Either
    # implementation changing alone fails here or there.
    h = record_hash(rec)
    assert (
        h == "sha256:c6f9bdb5259ad1e23a9fa9a137cbefcad4e83a8cd849531686a3bf5491ac2b52"
    ), h
    assert record_hash({**rec, "hash": h}) == h, "the hash field must not hash itself"

    # A tree of three: the unpaired leaf is promoted, not duplicated. Duplicating
    # it is CVE-2012-2459, where two different batches share a root.
    d = [digest(RECORD_DOMAIN, bytes([i])) for i in range(3)]
    assert merkle_root(d) == merkle_parent(
        merkle_parent(merkle_leaf(d[0]), merkle_leaf(d[1])), merkle_leaf(d[2])
    )
    for i in range(3):
        assert verify_merkle_proof(d[i], merkle_proof(d, i)) == merkle_root(d), (
            f"proof {i}"
        )
    assert merkle_root(d) != merkle_root(d[:2]), "a shorter batch is a different root"

    # Chain verification catches a mutated record and a removed one.
    chain = []
    prev = None
    for seq in range(3):
        row = {**rec, "record_id": f"r{seq}", "seq": seq, "prev_hash": prev}
        row["hash"] = record_hash(row)
        prev = row["hash"]
        chain.append(row)
    assert verify_chain(chain) == {"s": 3}
    broken = [dict(r) for r in chain]
    broken[1]["kind"] = "tool_refused"
    try:
        verify_chain(broken)
        raise AssertionError("a mutated record should break the chain")
    except EvidenceFormatError:
        pass
    try:
        verify_chain([chain[0], chain[2]])
        raise AssertionError("a removed record should break the chain")
    except EvidenceFormatError:
        pass

    # A checkpoint is stable across retries and refuses a run with a hole.
    a = build_checkpoint(chain, executor_id="xnaut:test", signing_key_id="k")
    b = build_checkpoint(chain, executor_id="xnaut:test", signing_key_id="k")
    assert a == b, "the same batch must checkpoint identically"
    try:
        build_checkpoint(
            [chain[0], chain[2]], executor_id="xnaut:test", signing_key_id="k"
        )
        raise AssertionError("a sequence gap should be refused")
    except EvidenceFormatError:
        pass
    print("ok: canonical bytes, domains, Merkle, chain, checkpoint")


if __name__ == "__main__":
    selftest()
