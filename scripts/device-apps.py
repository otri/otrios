#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Change this device's installed namespace without changing signed release bytes."""
import argparse
import errno
import fcntl
import json
import os
from pathlib import Path
import re
import socket
import stat
import tempfile

MAX_BYTES = 8192
MAX_REMOVED = 128


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("Duplicate namespace field")
        result[key] = value
    return result


def read_namespace(path):
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as source:
        metadata = os.fstat(source.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_mode & 0o077:
            raise ValueError("Namespace must be a private regular file")
        raw = source.read(MAX_BYTES + 1)
    if len(raw) > MAX_BYTES:
        raise ValueError("Namespace exceeds size limit")
    namespace = json.loads(raw, object_pairs_hook=unique_object)
    if (not isinstance(namespace, dict)
            or set(namespace) != {"version", "generation", "removed"}
            or type(namespace["version"]) is not int or namespace["version"] != 1
            or type(namespace["generation"]) is not int
            or not 0 < namespace["generation"] < 2**64
            or not isinstance(namespace["removed"], list)
            or len(namespace["removed"]) > MAX_REMOVED
            or any(not isinstance(root, str) or len(root) > 64
                   or not re.fullmatch(r"[a-z-]+", root) for root in namespace["removed"])
            or namespace["removed"] != sorted(set(namespace["removed"]))):
        raise ValueError("Invalid namespace")
    return namespace


def change(path, command, root=None):
    path = Path(path).absolute()
    parent = path.parent.lstat()
    if not stat.S_ISDIR(parent.st_mode) or parent.st_mode & 0o022:
        raise ValueError("Namespace directory must not be writable by other users")
    if root is not None and (len(root) > 64 or not re.fullmatch(r"[a-z-]+", root)):
        raise ValueError("Expected an app root ID")
    descriptor = os.open(str(path) + ".writer.lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "rb+") as lock:
        metadata = os.fstat(lock.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_mode & 0o077:
            raise ValueError("Writer lock must be private")
        fcntl.flock(lock, fcntl.LOCK_EX)
        if command == "init":
            if os.path.lexists(path):
                raise ValueError("Namespace is already initialized")
            namespace = {"version": 1, "generation": 1, "removed": []}
        else:
            namespace = read_namespace(path)
            if command == "status":
                return namespace
            removed = set(namespace["removed"])
            if command == "remove":
                removed.add(root)
            elif command == "restore":
                removed.discard(root)
            else:
                raise ValueError("Unknown command")
            if len(removed) > MAX_REMOVED or namespace["generation"] == 2**64 - 1:
                raise ValueError("Namespace capacity exhausted")
            namespace["generation"] += 1
            namespace["removed"] = sorted(removed)
        data = (json.dumps(namespace, separators=(",", ":")) + "\n").encode()
        if len(data) > MAX_BYTES:
            raise ValueError("Namespace exceeds size limit")
        descriptor, temporary = tempfile.mkstemp(prefix=path.name + ".", dir=path.parent)
        try:
            with os.fdopen(descriptor, "wb") as output:
                output.write(data)
                output.flush()
                os.fsync(output.fileno())
            os.replace(temporary, path)
            directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)
        with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as notify:
            notify.setblocking(False)
            try:
                notify.sendto(b"!", str(path) + ".notify")
            except OSError as error:
                # No running observer: the durable snapshot is loaded at startup.
                # Full queue: an earlier wake already requests the newest snapshot.
                if error.errno not in (errno.ENOENT, errno.ECONNREFUSED, errno.EAGAIN):
                    raise RuntimeError("Namespace saved but notification failed") from error
        return namespace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--file", required=True, type=Path)
    parser.add_argument("command", choices=("init", "status", "remove", "restore"))
    parser.add_argument("root", nargs="?")
    args = parser.parse_args()
    if (args.command in ("remove", "restore")) != (args.root is not None):
        parser.error("remove and restore require an app root; init and status do not")
    try:
        print(json.dumps(change(args.file, args.command, args.root), sort_keys=True))
    except (OSError, ValueError, RuntimeError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
