#!/bin/sh
# Builds gpSP's libretro core for the SP from the source archive that ships beside it on the card
# (libretro/gpsp at a pinned commit, the GPL-2.0 section 3(a) source), so the binary slot ships is
# exactly what that archive builds. taskfile.yml's core:gpsp runs it inside the arm64 bullseye
# box, so the .so links against the same glibc 2.31 as slot.
#
#   build.sh stamp COMMIT                        print what a build of COMMIT would record
#   build.sh build COMMIT TARBALL WORKDIR OUT    build TARBALL into WORKDIR, then OUT and OUT.meta
#
# The recipe is gpSP's own for arm64, `make platform=arm64`: the dynarec, its mmap'd JIT cache,
# -O3 and -fomit-frame-pointer -ffast-math, which is upstream's default and stays. The SP's CPU
# goes in through CFLAGS in the environment, which the Makefile's `CFLAGS +=` lines append to.
# CFLAGS on make's command line would replace every one of them, -ffast-math included, silently.
#
# Every patch beside this script is applied to that source, so the core is gpSP plus what
# `cores/gpsp/` holds, and both travel to the card together (see dist:device). test/ has the
# regression test for the serial one.
#
# The .meta file is how the taskfile tells a stale core from a current one: it is compared against
# `stamp`, so a changed pin, flag or patch rebuilds, and a core fetched from the buildbot (which
# has no .meta) can never pass for this one.
set -eu

here="$(cd "$(dirname "$0")" && pwd)"

device_cflags="-mcpu=cortex-a53"

usage() {
	echo "usage: $0 stamp COMMIT | build COMMIT TARBALL WORKDIR OUT" >&2
	exit 2
}

sha256() {
	if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi |
		cut -d' ' -f1
}

stamp() {
	echo "commit=$1"
	echo "source=https://github.com/libretro/gpsp/archive/$1.tar.gz"
	echo "recipe=make platform=arm64"
	echo "device_cflags=$device_cflags"
	for p in "$here"/*.patch; do
		echo "patch=$(basename "$p") sha256:$(sha256 "$p")"
	done
}

build() {
	commit="$1" tarball="$2" work="$3" out="$4"
	src="$work/gpsp-$commit"

	# Unpacked fresh on every run, so nothing from an earlier pin or build is linked in.
	rm -rf "$work"
	mkdir -p "$work"
	tar -xzf "$tarball" -C "$work"
	if [ ! -f "$src/Makefile" ]; then
		echo "$tarball does not hold gpsp-$commit/Makefile" >&2
		exit 1
	fi

	# Every patch beside this script, onto the pristine tree above. `stamp` records their sha256,
	# so editing one rebuilds instead of leaving a core that no longer matches the source shipped
	# with it. test/run.sh proves the serial one still fixes what it was written for.
	for p in "$here"/*.patch; do
		git -C "$src" apply -p1 "$p"
	done

	# GIT_VERSION is named rather than left to `git rev-parse`, which from inside slot's own
	# checkout would find slot's commit, not gpSP's. It is the only variable on the command line.
	CFLAGS="$device_cflags" make -C "$src" platform=arm64 \
		GIT_VERSION="\"$(printf %s "$commit" | cut -c1-7)\"" \
		-j"$(getconf _NPROCESSORS_ONLN)"

	mkdir -p "$(dirname "$out")"
	cp "$src/gpsp_libretro.so" "$out"
	stamp "$commit" >"$out.meta"
}

case "${1:-}" in
stamp)
	[ $# -eq 2 ] && [ -n "$2" ] || usage
	stamp "$2"
	;;
build)
	[ $# -eq 5 ] && [ -n "$2" ] && [ -n "$3" ] && [ -n "$4" ] && [ -n "$5" ] || usage
	build "$2" "$3" "$4" "$5"
	;;
*)
	usage
	;;
esac
