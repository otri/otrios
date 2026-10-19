#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Source-copy build avoids host-folder-sharing setup and leaves the kernel builder alone.
set -euo pipefail
cd "$(dirname "$0")/.."
docker build -f tools/dashboard/Dockerfile.armv7 -t otrios-dashboard-armv7:rust1.90 .
build_container=$(docker create otrios-dashboard-armv7:rust1.90 sleep infinity)
build_stage=$(mktemp -d)
trap 'docker rm -f "$build_container" >/dev/null; rm -rf "$build_stage"' EXIT
docker start "$build_container" >/dev/null
COPYFILE_DISABLE=1 tar -cf "$build_stage/source.tar" Cargo.toml Cargo.lock rust-toolchain.toml crates apps tools
docker cp "$build_stage/source.tar" "$build_container:/tmp/source.tar"
docker exec "$build_container" tar -xf /tmp/source.tar -C /src
docker exec "$build_container" cargo build --release --locked -p otrios-dashboard --features direct,server,canvas --target armv7-unknown-linux-gnueabihf
mkdir -p target/dashboard-armv7
for program in dashboard-direct dashboard-server dashboard-monitoring session-bootstrap canvas-pack; do
  docker cp "$build_container:/src/target/armv7-unknown-linux-gnueabihf/release/$program" "target/dashboard-armv7/$program"
done
printf 'ARMv7 direct host, authority and app packer: target/dashboard-armv7/\n'
