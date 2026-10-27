#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Deterministic UTF-8 corpora; source bytes are independent of renderer output."""
import hashlib
import json
from pathlib import Path
import argparse


def corpus(size):
    chunks = []
    length = 0
    paragraph = 0
    while length < size:
        paragraph += 1
        text = f"Paragraph {paragraph:06d}. "
        text += "The quick brown fox jumps over the lazy dog. Reading wraps words and preserves source ranges. jgyÉ - bearings and descenders. " * (1 + paragraph % 9)
        if paragraph % 101 == 0:
            text += "long-paragraph context " * 3000
        if paragraph % 103 == 0:
            text += "W" * 8192
        text += " Unsupported cluster: 👩‍💻.\tTabs and  spaces.\r\n\n"
        raw = text.encode("utf-8")
        chunks.append(raw)
        length += len(raw)
    # Preserve complete UTF-8 scalars at the exact byte bound; segmentation
    # handles a truncated multi-scalar cluster as source, without normalization.
    return b"".join(chunks)[:size].decode("utf-8", errors="ignore").encode("utf-8")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path("target/document-reader/corpus"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    manifest = {}
    for kib in [100, 1024, 10240]:
        raw = corpus(kib * 1024)
        name = f"document-{kib}k.txt"
        (args.output / name).write_bytes(raw)
        manifest[name] = {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}
    (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest))


if __name__ == "__main__":
    main()
