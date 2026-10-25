#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Capture a public release as wire responses; no credentials or app state are read."""

import argparse
import hashlib
import io
import json
import re
import sys
import tarfile
import time
import urllib.request

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--url", required=True)
args = parser.parse_args()
base = args.url.rstrip("/")
user_agent = "Mozilla/5.0 (compatible; OTriOS-Host-Check/3.0)"
header_names = {
    "content-type", "content-encoding", "cache-control", "content-length",
    "service-worker-allowed", "etag", "age", "cf-cache-status", "server", "vary",
}


def get(path, encoding):
    request = urllib.request.Request(
        base + path,
        headers={"User-Agent": user_agent, "Accept-Encoding": encoding},
    )
    started = time.monotonic()
    with urllib.request.urlopen(request, timeout=30) as response:
        body = response.read(32 * 1024 * 1024 + 1)
        if len(body) > 32 * 1024 * 1024:
            raise ValueError("Public response exceeds capture budget")
        return body, {
            "path": path,
            "status": response.status,
            "headers": {
                key.lower(): value for key, value in response.headers.items()
                if key.lower() in header_names
            },
            "wall_seconds": time.monotonic() - started,
            "wire_body_bytes": len(body),
        }


release = json.loads(get("/release", "identity")[0])
release_id = release["id"]
if not re.fullmatch("[a-f0-9]{64}", release_id):
    raise ValueError("Invalid release ID")
prefix = "/releases/" + release_id + "/"
raw = get(prefix + "manifest.json", "identity")[0]
if hashlib.sha256(raw).hexdigest() != release_id:
    raise ValueError("Manifest hash differs from release")
manifest = json.loads(raw)
items = [("/", None), (prefix + "manifest.json", None), (prefix + "manifest.sig", None)]
items += [
    (prefix + name, item) for name, item in manifest["assets"].items()
    if name.startswith("web/") or name == "apps/catalog.flex"
]
responses = []

with tarfile.open(fileobj=sys.stdout.buffer, mode="w|") as archive:
    def add(name, body):
        info = tarfile.TarInfo(name)
        info.size = len(body)
        archive.addfile(info, io.BytesIO(body))

    for index, (path, expected) in enumerate(items):
        body, metadata = get(path, "br, gzip")
        metadata["body"] = f"bodies/{index:02d}"
        metadata["expected"] = expected
        responses.append(metadata)
        add(metadata["body"], body)
        print(path, metadata["wire_body_bytes"],
              metadata["headers"].get("content-encoding", "identity"), file=sys.stderr)
    snapshot = {
        "format": "otrios-public-snapshot-v1", "origin": base,
        "release": release, "manifest": manifest, "responses": responses,
    }
    add("snapshot.json", json.dumps(snapshot, indent=2).encode())
