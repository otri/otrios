# SPDX-License-Identifier: Apache-2.0
"""Run contract-only codec checks with existing workspace dependencies."""
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
target = Path(os.environ.get("CARGO_TARGET_DIR", root / "target")).resolve()
env = dict(os.environ, CARGO_TARGET_DIR=str(target))
result = subprocess.run(
    ["cargo", "build", "-p", "otrios-core", "--features", "dashboard", "--lib",
     "--locked", "--offline", "--message-format=json"],
    cwd=root, env=env, text=True, stdout=subprocess.PIPE, check=True,
)
artifacts = {}
for line in result.stdout.splitlines():
    message = json.loads(line)
    if message.get("reason") != "compiler-artifact":
        continue
    name = message["target"]["name"]
    if name in ("otrios_interfaces", "flexbuffers"):
        artifacts[name] = next(p for p in message["filenames"] if p.endswith(".rlib"))
binary = target / "oib-designer-codec"
command = ["rustc", "--edition=2024", "--test", str(Path(__file__).with_name("designer_codec.rs")),
           "-L", f"dependency={target / 'debug' / 'deps'}", "-o", str(binary)]
for name, artifact in artifacts.items():
    command.extend(["--extern", f"{name}={artifact}"])
subprocess.run(command, cwd=root, env=env, check=True)
subprocess.run([str(binary), "--nocapture"], cwd=root, env=env, check=True)
