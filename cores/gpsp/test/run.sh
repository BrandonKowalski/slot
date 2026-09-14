#!/bin/sh
# Runs awtest.c, the regression test for the Advance Wars session-flip overflow that killed slot
# on the user's SPs, against the gpSP source slot ships.
#
#   run.sh TARBALL [PATCHDIR]     TARBALL is vendor/gpsp-src.tar.gz, PATCHDIR defaults to ..
#
# It builds gpSP's serial HLE twice out of that archive — once as upstream wrote it, once with
# the patches beside this script applied — and requires the first to fail and the second to pass.
# Both halves matter: a test that passes on unpatched gpSP would be proving nothing.
#
# serial_proto.c is the whole unit under test, so this is a couple of seconds of host compiling,
# with no emulator, no ROM and no device. gpSP's own serial_proto.c and serial.h are copied out
# of the archive into a temporary directory at run time rather than kept in this repo, which is
# also why common.h here is a stand-in written for the test rather than gpSP's own header.
set -eu

here="$(cd "$(dirname "$0")" && pwd)"
tarball="${1:?usage: run.sh TARBALL [PATCHDIR]}"
patches="${2:-$here/..}"
cc="${CC:-cc}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

top="$(tar -tzf "$tarball" | head -1 | cut -d/ -f1)"
tar -xzf "$tarball" -C "$work"
src="$work/$top"
[ -f "$src/serial_proto.c" ] || { echo "$tarball has no $top/serial_proto.c" >&2; exit 1; }

# ASan makes the overflow die at the read rather than wherever the 130 KB memmove lands. Without
# it the test still fails, on awtest.c's own length assertion, so it is nice to have, not needed.
asan="-fsanitize=address"
if ! echo 'int main(void){return 0;}' | "$cc" $asan -x c -o "$work/probe" - 2>/dev/null; then
	echo "note: $cc has no working -fsanitize=address, running on assertions alone"
	asan=""
fi

# Only gpSP's own files are copied in: common.h stays where awtest.c is, and both includers
# reach that one copy through -I, because two paths to it would be two definitions of everything.
build() {
	mkdir -p "$1"
	cp "$src/serial_proto.c" "$src/serial.h" "$1/"
	"$cc" -I"$1" -I"$here" $asan -g -O1 -o "$1/awtest" "$here/awtest.c"
}

build "$work/unpatched"
for p in "$patches"/*.patch; do
	git -C "$src" apply -p1 "$p"
done
build "$work/patched"

if "$work/unpatched/awtest" >"$work/unpatched.log" 2>&1; then
	echo "FAIL: unpatched gpSP passed, so this test no longer proves anything" >&2
	exit 1
fi
echo "ok: unpatched gpSP fails, as it must"
grep -m1 -E 'ERROR|SUMMARY|Assertion|assert' "$work/unpatched.log" || true

if ! "$work/patched/awtest"; then
	echo "FAIL: patched gpSP still overflows the peer queue" >&2
	exit 1
fi
echo "ok: patched gpSP holds the queue-length invariant across the session flip"
