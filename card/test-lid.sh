#!/bin/sh
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$HERE/System/lid.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT HUP INT TERM

setup() {
	rm -rf "$TMP/psy" "$TMP/bin" "$TMP"/*.log "$TMP/closed" "$TMP/slot.state"
	mkdir -p "$TMP/psy" "$TMP/bin"
	for tool in rmmod sync poweroff; do
		printf '#!/bin/sh\necho "%s $*" >> "%s"\n' "$tool" "$TMP/calls.log" > "$TMP/bin/$tool"
		chmod 755 "$TMP/bin/$tool"
	done
	: > "$TMP/calls.log"
}

lid() {
	printf '%s\n' "$1" > "$TMP/psy/hallkey"
}

at() {
	printf '%s.42 0.00\n' "$1" > "$TMP/uptime"
}

tick() {
	AGS_PSY="$TMP/psy" AGS_SLOT_STATE="$TMP/slot.state" AGS_TRACE="$TMP/trace.log" \
		AGS_LID_CLOSED="$TMP/closed" AGS_UPTIME="$TMP/uptime" \
		AGS_RMMOD="$TMP/bin/rmmod" AGS_SYNC="$TMP/bin/sync" AGS_POWEROFF="$TMP/bin/poweroff" \
		AGS_WIFI_WAIT="${WIFI_WAIT:-5}" sh "$SCRIPT" once
}

fail() {
	echo "lid: $*" >&2
	exit 1
}

off() {
	grep -q '^poweroff' "$TMP/calls.log"
}

setup
lid 0
at 100
tick
at 339
tick
off && fail "powered off before the default 3 minutes and a minute's grace"
at 340
tick
off || fail "did not power off 4 minutes after the lid closed"
[ "$(sed 's/ *$//' "$TMP/calls.log")" = "sync
rmmod 8821cs
sync
poweroff" ] || fail "power off did not sync, unload the WiFi driver and sync again: $(cat "$TMP/calls.log")"
grep -q 'lid: closed 240s and slot has not powered off' "$TMP/trace.log" || fail "the power off left no trace"

setup
echo "lid_off_min=10" > "$TMP/slot.state"
lid 0
at 0
tick
at 600
tick
off && fail "ignored a 10 minute lid setting"
at 660
tick
off || fail "did not power off 11 minutes into a 10 minute setting"

setup
echo "lid_off_min=7" > "$TMP/slot.state"
lid 0
at 0
tick
at 240
tick
off || fail "an unknown lid setting did not fall back to 3 minutes"

setup
lid 0
at 0
tick
lid 1
at 200
tick
lid 0
at 210
tick
at 400
tick
off && fail "opening the lid did not restart the count"
at 450
tick
off || fail "did not power off 4 minutes after the lid closed again"

setup
at 0
tick
at 1000
tick
off && fail "powered off with no lid sensor"

setup
printf '#!/bin/sh\necho "rmmod $*" >> "%s"\nsleep 30\n' "$TMP/calls.log" > "$TMP/bin/rmmod"
lid 0
at 0
tick
at 240
WIFI_WAIT=1 tick
off || fail "a WiFi driver that never unloads stopped the power off"
[ "$(head -1 "$TMP/calls.log")" = "sync " ] || fail "did not sync before a WiFi unload that hangs"

if sh "$SCRIPT" bogus 2>/dev/null; then
	fail "an unknown subcommand exited 0"
fi

echo "lid: all passed"
