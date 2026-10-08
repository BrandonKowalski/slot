#!/bin/sh
# Build slot for the RG SP (aarch64 linux, glibc 2.31).
#
#   ./build-device.sh              build target-device/aarch64-unknown-linux-gnu/device/slot
#   ./build-device.sh DEST         and copy it to DEST, eg /run/media/$USER/SD/System/slot
#
# On an aarch64 linux box it builds directly. Anywhere else it cross-compiles in
# a Debian bullseye container, so the binary never asks for a newer glibc than
# the device has.
set -eu

cd "$(dirname "$0")"

TARGET=aarch64-unknown-linux-gnu
BIN="target-device/$TARGET/device/slot"
BUILD="cargo build --profile device --target-dir target-device --target $TARGET -p slot --no-default-features --features device"

if [ "$(uname -s)-$(uname -m)" = "Linux-aarch64" ]; then
	$BUILD
else
	if ! command -v docker >/dev/null 2>&1; then
		echo "build-device: docker is not installed" >&2
		exit 1
	fi
	if ! docker info >/dev/null 2>&1; then
		echo "build-device: docker is not running, start it with: sudo systemctl start docker" >&2
		exit 1
	fi
	docker run --rm -v "$PWD":/src -w /src \
		-e CARGO_HOME=/src/target-device/cargo-home \
		-e CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
		rust:1-bullseye sh -c "
			set -e
			apt-get update -qq
			apt-get install -y -qq gcc-aarch64-linux-gnu >/dev/null
			rustup target add $TARGET
			$BUILD
			chown -R $(id -u):$(id -g) target-device
		"
fi

file "$BIN" | grep -q aarch64 || {
	echo "build-device: $BIN is not an aarch64 binary" >&2
	exit 1
}
echo "built $BIN"

if [ $# -gt 0 ]; then
	cp "$BIN" "$1"
	sync
	echo "copied to $1"
fi
