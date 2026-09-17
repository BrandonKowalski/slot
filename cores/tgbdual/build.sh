#!/bin/sh
# Builds TGB Dual's libretro core for the SP from the source archive that ships beside it on the
# card (libretro/tgbdual-libretro at a pinned commit, the GPL-2.0 section 3(a) source), so the
# binary slot ships is exactly what that archive builds. taskfile.yml's core:tgbdual runs it
# inside the arm64 bullseye box, so the .so links against the same glibc 2.31 as slot.
#
#   build.sh stamp COMMIT                        print what a build of COMMIT would record
#   build.sh build COMMIT TARBALL WORKDIR OUT    build TARBALL into WORKDIR, then OUT and OUT.meta
#
# TGB Dual is here for one thing: it runs two Game Boys in one process, joined by an emulated
# link cable, on one thread, reading both input ports. It is the only core in the field built
# around that rather than having it added later, and it is what lets a linked Game Boy Color pair
# hold 60 fps on an H700 where SameBoy cannot. See
# .superpowers/sdd/2026-09-16-game-boy-support/lighter-core-survey.md for the measurements.
#
# The recipe is TGB Dual's own, `make platform=unix` (`osx` on Darwin, which is a different
# link line and a .dylib rather than a .so), and upstream's own -O2 -DNDEBUG. Those two
# are appended with `+=` at Makefile:529-530, after anything the environment supplies, so no -O
# level can be injected from outside and none is attempted. CFLAGS on make's command line would
# replace every one of the Makefile's own lines instead of adding to them, silently, which is why
# the flags below go through the environment.
#
# Upstream also puts -ffast-math in every build (Makefile:548), which is not a flag this project
# would add to an emulator itself. It is left alone: it is what upstream ships and what its own
# CI builds, both SPs in a link session run the identical binary, and the two framebuffer hashes
# below are unmoved by the only flag that was changed. Taking it out would make slot's core
# compute differently from every other TGB Dual build in the world, which is a worse position to
# be in than sharing upstream's.
#
# Measured on an SP at 1512 MHz, a linked Colour pair, 4400 frames: 1.978 ms with the correction
# off and 2.206 ms with it on, so it costs 0.228 ms, about 11%, and leaves the pair at 13% of a
# 16.743 ms frame with no frame over budget. Off, the framebuffer hash is 88d9dbd5904ddda0, which
# is exactly what the unpatched core produced in the same harness: the patch is a byte-for-byte
# no-op until it is switched on.
#
# One patch, color-correction.patch, which adds the tgbdual_color_correction option upstream has
# no equivalent of and which the quick menu's Colour Correction row needs in order to mean
# anything on a Game Boy cart. It works on the finished framebuffer rather than in map_color,
# because map_color has an inverse a game reads its palette back through; licenses/README.md
# carries the reasoning and the section 2(a) notice. Nothing else is patched: the core builds
# clean for aarch64 from upstream's own recipe with no external assets and no boot ROM, and
# libretro/libretro.cpp:406-407 turns the link on by itself when two ROMs arrive through
# retro_load_game_special, so slot does not have to patch in the behaviour it wants.
#
# The .meta file is how the taskfile tells a stale core from a current one: it is compared against
# `stamp`, so a changed pin or flag rebuilds, and a core fetched from the buildbot (which has no
# .meta) can never pass for this one.
set -eu

here="$(cd "$(dirname "$0")" && pwd)"

# Link-time optimisation, for the SP's core only, the same flag as cores/mgba/build.sh. It goes in
# through CFLAGS and CXXFLAGS *and* LDFLAGS, because Makefile:617 links with
# `$(LD) ... $(LDFLAGS)` and never passes the compile flags to the link step, so LTO in CFLAGS
# alone would silently do nothing.
#
# Measured on an SP at 1512 MHz, 8000 frames of the scripted two-player Tetris match, against the
# same build without it: a linked Game Boy pair goes 4.126 -> 4.038 ms and a linked Game Boy Color
# pair 1.972 -> 1.966 ms. That is 2.1% and 0.3%, which is smaller than mGBA's 3 to 5% and
# SameBoy's 5.7%, and it is kept because it is free and it also takes the binary from 127 KB to
# 109 KB, not because the frame needs it: TGB Dual is four times inside budget either way.
#
# Both builds produce byte-identical framebuffers (2c711877806b4823 on the Game Boy pair,
# 88d9dbd5904ddda0 on the Colour pair), which is the property link play needs: this changes how
# the core is compiled, not what it computes.
device_cflags="-flto=auto"

usage() {
	echo "usage: $0 stamp COMMIT | build COMMIT TARBALL WORKDIR OUT" >&2
	exit 2
}

# The Makefile's own name for this machine. It would not work this out itself: `platform=unix`
# on a Mac reaches the Linux link line and dies on `--version-script`, which ld does not know.
# The two targets differ in more than a name, and in what they produce: a .so against a .dylib.
mk_platform() {
	case "$(uname -s)" in
	Darwin) echo osx ;;
	*) echo unix ;;
	esac
}

sha256() {
	if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi |
		cut -d' ' -f1
}

stamp() {
	echo "commit=$1"
	echo "source=https://github.com/libretro/tgbdual-libretro/archive/$1.tar.gz"
	# Constant on every host, like device_cflags below: the taskfile checks the SP's .meta from
	# the Mac, so a stamp naming this machine's platform would never match the device's.
	echo "recipe=make platform=unix, or osx on Darwin"
	echo "device_cflags=$device_cflags"
	for p in "$here"/*.patch; do
		[ -e "$p" ] || continue
		echo "patch=$(basename "$p") sha256:$(sha256 "$p")"
	done
}

build() {
	commit="$1" tarball="$2" work="$3" out="$4"
	# GitHub names a source archive's top directory after the *repository*, not the project, so
	# this is tgbdual-libretro-<commit> and not tgbdual-<commit>. cores/gpsp/build.sh gets away
	# with the shorter form only because that repository is literally named gpsp.
	src="$work/tgbdual-libretro-$commit"

	# Unpacked fresh on every run, so nothing from an earlier pin or build is linked in.
	rm -rf "$work"
	mkdir -p "$work"
	tar -xzf "$tarball" -C "$work"
	# A second, untouched extraction, purely so the patch step can prove it changed something.
	mkdir -p "$work/pristine"
	tar -xzf "$tarball" -C "$work/pristine" --strip-components=1
	if [ ! -f "$src/Makefile" ]; then
		echo "$tarball does not hold tgbdual-libretro-$commit/Makefile" >&2
		exit 1
	fi

	# Every patch beside this script, onto the pristine tree above. `stamp` records their sha256,
	# so editing one rebuilds instead of leaving a core that no longer matches the source shipped
	# with it.
	#
	# `patch` rather than the `git apply` the other three recipes use, and this is not a style
	# choice. Some of TGB Dual's sources ship with CRLF line endings, and `git apply` answers a
	# patch against one of those by printing `Skipped patch 'libretro/dmy_renderer.cpp'` and
	# exiting **zero**. Nothing fails, `set -e` sees success, and the build goes on to compile a
	# core with none of the patch in it. That is exactly what happened the first time this patch
	# was added, and it was caught by looking for the option's name in the built binary rather
	# than by anything the build said. `patch` applies it and returns nonzero when it cannot.
	#
	# The check below is the belt to that brace: a patch set that produced no change to the tree
	# is a build recipe lying about what it built, so it stops here rather than at whatever
	# behaviour goes missing downstream.
	for p in "$here"/*.patch; do
		[ -e "$p" ] || continue
		patch -p1 -d "$src" -i "$p"
		patched=yes
	done
	if [ "${patched:-no}" = yes ] && diff -rq "$src" "$work/pristine" >/dev/null 2>&1; then
		echo "patches applied but the source is unchanged; see the note above" >&2
		exit 1
	fi
	rm -rf "$work/pristine"

	# Passed even when empty, so a tree reused from another run cannot keep its flags.
	cflags=""
	ldflags=""
	if [ "$(uname -s)-$(uname -m)" = "Linux-aarch64" ]; then
		cflags="$device_cflags"
		ldflags="$device_cflags"
	fi
	# GIT_VERSION is named rather than left to the Makefile's own `git rev-parse --short HEAD`,
	# which from inside slot's checkout would find slot's commit, not TGB Dual's. It is the only
	# variable on the command line, for the reason in the header.
	CFLAGS="$cflags" CXXFLAGS="$cflags" LDFLAGS="$ldflags" make -C "$src" platform="$(mk_platform)" \
		GIT_VERSION="\" $(printf %s "$commit" | cut -c1-7)\"" \
		-j"$(getconf _NPROCESSORS_ONLN)"

	for ext in dylib so; do
		if [ -f "$src/tgbdual_libretro.$ext" ]; then
			mkdir -p "$(dirname "$out")"
			cp "$src/tgbdual_libretro.$ext" "$out"
			stamp "$commit" >"$out.meta"
			return
		fi
	done
	echo "make finished without producing tgbdual_libretro" >&2
	exit 1
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
