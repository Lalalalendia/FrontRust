#!/usr/bin/env python3
"""Verify committed small OFL demo font fixtures without network or extra deps.

This is an integrity gate for the exact font bytes shipped with the WASM demo.
It does not establish Unicode fallback quality or production font safety.
"""
from hashlib import sha256
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "experiments" / "masonry-wasm-probe" / "fixtures"
EXPECTED = {
    "r5a-cyrillic-real.ttf": (
        1480,
        "4d31713156f2ae489c1a0d8a11cc5f06872b1a2ec04248f93831eb2c1b81f7c6",
    ),
    "r5b-latin-real.ttf": (
        1560,
        "4022495d87c257f960efeeda071a643c8b057c42e0e8d92649bb93e85d1c8dd2",
    ),
}
for filename, (expected_length, expected_hash) in EXPECTED.items():
    payload = (ROOT / filename).read_bytes()
    digest = sha256(payload).hexdigest()
    assert len(payload) == expected_length, (filename, len(payload), expected_length)
    assert payload[:4] == b"\x00\x01\x00\x00", (filename, "not a TrueType font")
    assert digest == expected_hash, (filename, digest, expected_hash)
    print(f"PASS font fixture {filename} sha256={digest}")
assert (ROOT / "OFL-Lato.txt").exists(), "OFL text must accompany modified Lato subsets"
