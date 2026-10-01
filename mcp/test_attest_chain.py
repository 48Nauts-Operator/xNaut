#!/usr/bin/env python3
"""Self-check for the attestation chain. Run: python3 mcp/test_attest_chain.py

The chain exists so that deleting a receipt stops being invisible. That claim
is only worth making if something fails when it stops being true, so the three
tamper cases below are the point of this file. No framework: stdlib assert, one
process, runs anywhere the MCP runs.
"""

import importlib.util
import json
import pathlib

spec = importlib.util.spec_from_file_location(
    "attest", pathlib.Path(__file__).with_name("securosys-attest.py")
)
attest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(attest)

GENESIS = attest.GENESIS


def chain(n, subject="test"):
    """Build n linked receipts the way do_attest does, minus the HSM."""
    rows = []
    for i in range(n):
        seq, prev = attest.chain_tail(rows)
        rows.append(
            {
                "seq": seq,
                "prev": prev,
                "ts": f"2026-08-20T00:0{i}:00+00:00",
                "subject": subject,
                "digest": f"{i:064x}",
                "key_name": "K",
                "algorithm": "SHA256_WITH_RSA",
                "signature": f"sig{i}",
            }
        )
    return rows


rows = chain(4)
assert rows[0]["prev"] == GENESIS, "the first receipt starts at genesis"
assert [r["seq"] for r in rows] == [0, 1, 2, 3]
assert attest.verify_chain(rows)["ok"], "a chain built the normal way must verify"
assert attest.verify_chain(rows)["chained"] == 4

# Deleting a receipt: the whole reason for the chain.
gap = rows[:2] + rows[3:]
assert not attest.verify_chain(gap)["ok"], "a deleted receipt must break the chain"

# Reordering.
swapped = [rows[0], rows[2], rows[1], rows[3]]
assert not attest.verify_chain(swapped)["ok"], "reordering must break the chain"

# Editing a receipt in place: the link that follows it no longer matches.
edited = [dict(r) for r in rows]
edited[1]["digest"] = "f" * 64
assert not attest.verify_chain(edited)["ok"], "an edited receipt must break the chain"

# Truncating the front, which a naive "each link matches the last" walk misses.
assert not attest.verify_chain(rows[2:])["ok"], "a truncated head must break the chain"

# Receipts written before the chain existed stay readable, and the chain starts
# after them rather than claiming to cover them.
legacy = [
    {
        "ts": "2026-08-18T19:25:07+00:00",
        "subject": "old",
        "digest": "a" * 64,
        "key_name": "K",
        "algorithm": "SHA256_WITH_RSA",
        "signature": "old",
    }
]
seq, prev = attest.chain_tail(legacy)
assert (seq, prev) == (0, GENESIS), (
    "the chain starts at genesis after unchained receipts"
)
mixed = legacy + chain(2)
assert attest.verify_chain(mixed)["ok"]
assert attest.verify_chain(mixed)["unchained"] == 1

# The property the browser verifier depends on: a link recomputed from the
# published subset must equal the link computed from the full local receipt.
full = dict(rows[1], tsb_url="https://tsb.example", meta={"ticket": "XNAUT-211"})
published = {k: full[k] for k in attest.LINK_FIELDS if k in full}
assert attest.link_hash(full) == attest.link_hash(published), (
    "publish() drops local-only fields; they must not be part of the link"
)

# And that the published row still carries what a walk needs.
assert {"seq", "prev"} <= set(published), "publish must carry seq and prev"

# Canonical form is stable regardless of key order in the dict.
shuffled = dict(reversed(list(full.items())))
assert attest.link_hash(shuffled) == attest.link_hash(full), (
    "link must not depend on key order"
)

print("attest chain ok:", json.dumps(attest.verify_chain(rows)))
