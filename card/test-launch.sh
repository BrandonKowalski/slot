#!/bin/sh
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$HERE/System/launch_frontend.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT HUP INT TERM

setup() {
	rm -rf "$TMP/card" "$TMP/run"
	mkdir -p "$TMP/card/System" "$TMP/run"
	: > "$TMP/card/System/led.sh"
	: > "$TMP/card/System/lid.sh"
}

launch() {
	SLOT_ROOT="$TMP/card" AGS_RUN="$TMP/run" sh "$SCRIPT" 2>/dev/null || true
}

fail() {
	echo "launch: $*" >&2
	exit 1
}

setup
echo "old run" > "$TMP/card/slot.log"
echo "older run" > "$TMP/card/slot.log.1"
echo "12:00:00 exec" > "$TMP/card/slot-session.log"
echo "alarm" > "$TMP/card/slot-card.log"
echo "boot" > "$TMP/card/ags-shutdown-trace.log"
echo "keys" > "$TMP/card/input-trace.log"
mkdir -p "$TMP/card/crash"
echo "pid=1" > "$TMP/card/crash/cores.log"
launch
for f in slot.log slot.log.1 slot-session.log slot-card.log ags-shutdown-trace.log input-trace.log crash; do
	[ ! -e "$TMP/card/$f" ] || fail "$f was left at the root of the card"
done
[ "$(cat "$TMP/card/Logs/slot.log.1")" = "old run" ] || fail "the last run's log was not kept"
[ "$(cat "$TMP/card/Logs/slot-card.log")" = "alarm" ] || fail "slot-card.log did not move"
[ "$(cat "$TMP/card/Logs/ags-shutdown-trace.log")" = "boot" ] || fail "the shutdown trace did not move"
[ "$(cat "$TMP/card/Logs/input-trace.log")" = "keys" ] || fail "the input trace did not move"
[ "$(cat "$TMP/card/Logs/crash/cores.log")" = "pid=1" ] || fail "crash/ did not move"
[ "$(head -1 "$TMP/card/Logs/slot-session.log")" = "12:00:00 exec" ] || fail "the session log lost its history"
tail -1 "$TMP/card/Logs/slot-session.log" | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9:]{8} exec ' \
	|| fail "the session line has no date: $(tail -1 "$TMP/card/Logs/slot-session.log")"

setup
mkdir -p "$TMP/card/Logs"
echo "this run" > "$TMP/card/Logs/slot.log"
launch
[ "$(cat "$TMP/card/Logs/slot.log.1")" = "this run" ] || fail "slot.log did not rotate"

setup
mkdir -p "$TMP/card/Logs"
head -c 70000 /dev/zero | tr '\0' 'x' > "$TMP/card/Logs/slot-session.log"
launch
[ "$(wc -c < "$TMP/card/Logs/slot-session.log.1")" -eq 70000 ] || fail "a full session log did not rotate"
[ "$(grep -c . "$TMP/card/Logs/slot-session.log")" = "1" ] || fail "the new session log did not start fresh"

setup
mkdir -p "$TMP/card/Logs"
head -c 1000 /dev/zero | tr '\0' 'x' > "$TMP/card/Logs/slot-session.log"
launch
[ ! -e "$TMP/card/Logs/slot-session.log.1" ] || fail "a small session log rotated"

setup
printf 'echo "$*" >> "%s"\n' "$TMP/link.log" > "$TMP/card/System/slotlink.sh"
launch
[ ! -e "$TMP/link.log" ] || fail "tore down a link no earlier run left up"
: > "$TMP/run/slotlink.session"
launch
[ "$(cat "$TMP/link.log" 2>/dev/null)" = "link down" ] || fail "a link left up by a crashed run was not torn down"

echo "launch: all passed"
