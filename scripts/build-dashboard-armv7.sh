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
COPYFILE_DISABLE=1 tar -cf "$build_stage/source.tar" Cargo.toml Cargo.lock rust-toolchain.toml crates
docker cp "$build_stage/source.tar" "$build_container:/tmp/source.tar"
docker exec "$build_container" tar -xf /tmp/source.tar -C /src
docker exec "$build_container" cargo build --release --locked -p otrios-dashboard --features native,server --target armv7-unknown-linux-gnueabihf
mkdir -p target/dashboard-armv7/lib
for program in dashboard-native dashboard-server; do
  docker cp "$build_container:/src/target/armv7-unknown-linux-gnueabihf/release/$program" "target/dashboard-armv7/$program"
done
# Optional private runtime for an internal trial; retains Debian's own licenses.
for library in libSDL2-2.0.so.0 libXss.so.1; do
  docker cp -L "$build_container:/usr/lib/arm-linux-gnueabihf/$library" "target/dashboard-armv7/lib/$library"
done
mkdir -p target/dashboard-armv7/licenses
docker cp "$build_container:/usr/share/doc/libsdl2-2.0-0/copyright" target/dashboard-armv7/licenses/SDL2-copyright
docker cp "$build_container:/usr/share/doc/libxss1/copyright" target/dashboard-armv7/licenses/libXss-copyright
printf 'ARMv7 app and private trial libraries: target/dashboard-armv7/\n'
