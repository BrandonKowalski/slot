#!/bin/sh
# Builds mGBA's libretro core from libretro/mgba at a pinned commit, with every patch beside
# this script applied, for whatever machine runs it. taskfile.yml's core:mgba:host runs it on
# the Mac; core:device runs it inside the arm64 bullseye box, so the .so links against the same
# glibc as slot. Both use the flags libretro's own CI builds the buildbot core with, the SP's
# with link-time optimisation on top (device_cflags below), and the source is a checkout of the
# commit rather than a tarball, so git vouches for what was built.
#
#   build.sh stamp COMMIT               print what a build of COMMIT would record
#   build.sh build COMMIT WORKDIR OUT   build into WORKDIR, then write OUT and OUT.meta
#
# The .meta file is how the taskfile tells a stale core from a current one: it is compared
# against `stamp`, so a changed pin, patch or flag rebuilds, and a core fetched from the buildbot
# (which has no .meta) can never pass for this one.
#
# The core reports the version mGBA's build works out from the checkout, which is shallow and
# patched: 0.11-1-<commit>-dirty. The build re-derives it from git at compile time and takes no
# override, and "-dirty" is accurate. Nothing in slot reads it.
set -eu

here="$(cd "$(dirname "$0")" && pwd)"

# Link-time optimisation, for the SP's core only: Linux on aarch64, the bullseye box core:device
# runs in. The Mac's core keeps exactly the flags above. Measured on the SP it takes 3 to 5% off
# every frame, on both games and at both step counts. -mcpu=cortex-a53 beside it gave most of
# that back and is deliberately absent. Nothing that can change a computed value goes here (no
# -ffast-math): link mode needs two SPs' machines to stay bit identical, and this changes how
# the core is compiled rather than what it computes — the frame hashes are unmoved. `stamp`
# prints it on every host, because the taskfile checks the SP's .meta from the Mac.
device_cflags="-flto=auto"

usage() {
	echo "usage: $0 stamp COMMIT | build COMMIT WORKDIR OUT" >&2
	exit 2
}

sha256() {
	if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi |
		cut -d' ' -f1
}

stamp() {
	echo "commit=$1"
	echo "source=https://github.com/libretro/mgba/tree/$1"
	echo "device_cflags=$device_cflags"
	for p in "$here"/*.patch; do
		echo "patch=$(basename "$p") sha256:$(sha256 "$p")"
	done
}

build() {
	commit="$1" work="$2" out="$3"
	src="$work/mgba"
	obj="$work/build"

	if ! command -v cmake >/dev/null 2>&1; then
		if command -v apt-get >/dev/null 2>&1; then
			# Bullseye is past its security support, so its Release files are no longer
			# re-signed and apt refuses them as expired. bullseye-security also still lists
			# packages it no longer serves (cmake's libarchive13 3.4.3-2+deb11u5 is a 404),
			# so that suite is dropped and everything comes from the main archive. Nothing
			# here needs a security update.
			sed -i '/bullseye-security/d' /etc/apt/sources.list
			apt-get -o Acquire::Check-Valid-Until=false update -qq
			apt-get install -y -qq --no-install-recommends cmake >/dev/null
		else
			echo "building mGBA needs cmake: brew install cmake" >&2
			exit 1
		fi
	fi

	# Pristine at COMMIT on every run, so a patch is never applied on top of itself and a moved
	# pin never builds over the previous checkout. The checkout's own .git is made first, so a
	# missing one cannot send git up into slot's repository instead.
	mkdir -p "$src"
	[ -d "$src/.git" ] || git init -q "$src"
	git -C "$src" cat-file -e "$commit^{commit}" 2>/dev/null ||
		git -C "$src" fetch -q --depth 1 https://github.com/libretro/mgba "$commit"
	git -C "$src" checkout -q --force --detach "$commit"
	git -C "$src" clean -q -fdx
	for p in "$here"/*.patch; do
		git -C "$src" apply "$p"
	done

	# Passed even when empty, so a build tree reused from another run can't keep its flags. They
	# land ahead of the Release -O3 and the libretro target's own -O3, and name no -O level.
	cflags=""
	if [ "$(uname -s)-$(uname -m)" = "Linux-aarch64" ]; then
		cflags="$device_cflags"
	fi
	cmake -S "$src" -B "$obj" -DLIBMGBA_ONLY=ON -DBUILD_LIBRETRO=ON -DCMAKE_BUILD_TYPE=Release \
		-DCMAKE_C_FLAGS="$cflags" >/dev/null
	cmake --build "$obj" --target mgba_libretro --parallel "$(getconf _NPROCESSORS_ONLN)" >/dev/null

	for ext in dylib so; do
		if [ -f "$obj/mgba_libretro.$ext" ]; then
			mkdir -p "$(dirname "$out")"
			cp "$obj/mgba_libretro.$ext" "$out"
			stamp "$commit" >"$out.meta"
			return
		fi
	done
	echo "cmake finished without producing mgba_libretro" >&2
	exit 1
}

case "${1:-}" in
stamp)
	[ $# -eq 2 ] && [ -n "$2" ] || usage
	stamp "$2"
	;;
build)
	[ $# -eq 4 ] && [ -n "$2" ] && [ -n "$3" ] && [ -n "$4" ] || usage
	build "$2" "$3" "$4"
	;;
*)
	usage
	;;
esac
