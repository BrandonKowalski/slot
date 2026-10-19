#!/bin/sh
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$HERE/System/coresave.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT HUP INT TERM

setup() {
	rm -rf "$TMP/card" "$TMP/mountpoint"
	mkdir -p "$TMP/card"
	cat > "$TMP/mountpoint" <<-SH
		#!/bin/sh
		exit ${1:-0}
	SH
	chmod +x "$TMP/mountpoint"
}

run() {
	printf '%s' "$2" | AGS_CARD="$TMP/card" AGS_MOUNTPOINT="$TMP/mountpoint" \
		AGS_CORE_EXE="$1" AGS_CORE_CMDLINE="${CMDLINE:-$1}" sh "$SCRIPT" "${3:-4321}" "${4:-11}" "${5:-slot-emu}" 0
}

setup
run /usr/bin/busybox "not a frontend core"
grep -q 'signal=11 thread=slot-emu exe=/usr/bin/busybox' "$TMP/card/Logs/crash/cores.log" \
	|| { echo "no log line for a non-frontend crash: [$(cat "$TMP/card/Logs/crash/cores.log")]" >&2; exit 1; }
[ "$(ls "$TMP/card/Logs/crash/" | grep -c '^slot-.*\.log$')" = "0" ] \
	|| { echo "kept a log for a process that is not the frontend" >&2; exit 1; }

setup
mkdir -p "$TMP/card/Logs/crash"
echo "an old core" > "$TMP/card/Logs/crash/slot.core"
run /mnt/sdcard/System/slot "a frontend core"
[ ! -f "$TMP/card/Logs/crash/slot.core" ] && [ ! -f "$TMP/card/Logs/crash/slot.core.tmp" ] \
	|| { echo "wrote the frontend's core to the card" >&2; exit 1; }
grep -q 'exe=/mnt/sdcard/System/slot' "$TMP/card/Logs/crash/cores.log" \
	|| { echo "no log line for the frontend's crash" >&2; exit 1; }

setup
mkdir -p "$TMP/card/Logs"
echo "loaded through ld-linux" > "$TMP/card/Logs/slot.log"
CMDLINE="/lib/ld-linux-aarch64.so.1 /mnt/sdcard/System/slot" \
	run /usr/lib/aarch64-linux-gnu/ld-linux-aarch64.so.1 "loader core"
grep -q "loaded through ld-linux" "$TMP/card/Logs/crash/"slot-*.log 2>/dev/null \
	|| { echo "slot run through ld-linux crashed and its log was not kept" >&2; exit 1; }

setup
mkdir -p "$TMP/card/Logs/crash"
for d in 01 02 03 04 05 06 07; do
	echo "old" > "$TMP/card/Logs/crash/slot-2026-01-${d}_00-00-00.log"
done
echo "newest" > "$TMP/card/Logs/slot.log"
run /mnt/sdcard/System/slot "one more crash"
[ "$(ls "$TMP/card/Logs/crash/" | grep -c '^slot-.*\.log$')" = "5" ] \
	|| { echo "kept $(ls "$TMP/card/Logs/crash/" | grep -c '^slot-.*\.log$') crash logs, not 5" >&2; exit 1; }
[ ! -f "$TMP/card/Logs/crash/slot-2026-01-01_00-00-00.log" ] \
	|| { echo "kept the oldest crash log" >&2; exit 1; }
grep -lq newest "$TMP/card/Logs/crash/"slot-*.log \
	|| { echo "pruned the newest crash log" >&2; exit 1; }

setup
mkdir -p "$TMP/card/Logs"
echo "slot: crash: SIGSEGV in thread slot-emu" > "$TMP/card/Logs/slot.log"
run /mnt/sdcard/System/slot "core with a log"
grep -q "slot: crash: SIGSEGV in thread slot-emu" "$TMP/card/Logs/crash/"slot-*.log 2>/dev/null \
	|| { echo "the log of the run that crashed was not kept" >&2; exit 1; }
run /usr/bin/busybox "not a frontend core"
[ "$(ls "$TMP/card/Logs/crash/" | grep -c '^slot-.*\.log$')" = "1" ] \
	|| { echo "kept a slot log for a crash that was not slot's" >&2; exit 1; }

setup
CMDLINE="/lib/ld-linux-aarch64.so.1 /mnt/sdcard/System/slot-label-check" \
	run /usr/lib/aarch64-linux-gnu/ld-linux-aarch64.so.1 "helper core"
[ "$(ls "$TMP/card/Logs/crash/" 2>/dev/null | grep -c '^slot-.*\.log$')" = "0" ] \
	|| { echo "kept a log for a helper's crash" >&2; exit 1; }

setup
cat > "$TMP/mountpoint" <<-SH
	#!/bin/sh
	exit 1
SH
chmod +x "$TMP/mountpoint"
run /mnt/sdcard/System/slot "core with nowhere to go" \
	|| { echo "a missing card made the helper fail" >&2; exit 1; }
[ ! -d "$TMP/card/Logs/crash" ] \
	|| { echo "wrote to a card that is not mounted" >&2; exit 1; }

echo "coresave: all passed"
