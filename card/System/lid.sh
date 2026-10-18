#!/bin/sh
set -u

SD="${SLOT_ROOT:-/mnt/sdcard}"
PSY="${AGS_PSY:-/sys/class/power_supply/axp2202-battery}"
SLOT_STATE="${AGS_SLOT_STATE:-$SD/Config/slot.state}"
TRACE="${AGS_TRACE:-$SD/Logs/ags-shutdown-trace.log}"
CLOSED="${AGS_LID_CLOSED:-/run/slot-lid.closed}"
UPTIME="${AGS_UPTIME:-/proc/uptime}"
INTERVAL="${AGS_LID_INTERVAL:-5}"
GRACE="${AGS_LID_GRACE:-60}"
RMMOD="${AGS_RMMOD:-rmmod}"
SYNC="${AGS_SYNC:-sync}"
POWEROFF="${AGS_POWEROFF:-poweroff}"

now() {
	read -r up _ < "$UPTIME" 2>/dev/null || up=0
	echo "${up%%.*}"
}

minutes() {
	m="$(sed -n 's/^lid_off_min=//p' "$SLOT_STATE" 2>/dev/null | tail -1)"
	case "$m" in
	1 | 3 | 5 | 10 | 15 | 30) echo "$m" ;;
	*) echo 3 ;;
	esac
}

power_off() {
	echo "$(date +%s) lid: closed ${1}s and slot has not powered off, powering off" >> "$TRACE" 2>/dev/null
	$RMMOD 8821cs 2>/dev/null
	$SYNC
	$POWEROFF
}

check() {
	if [ "$(cat "$PSY/hallkey" 2>/dev/null)" != 0 ]; then
		rm -f "$CLOSED"
		return 1
	fi
	t="$(now)"
	if [ ! -s "$CLOSED" ]; then
		echo "$t" > "$CLOSED"
		return 1
	fi
	shut=$((t - $(cat "$CLOSED")))
	[ "$shut" -ge $(($(minutes) * 60 + GRACE)) ] || return 1
	rm -f "$CLOSED"
	power_off "$shut"
}

case "${1:-loop}" in
once)
	check || true
	;;
loop)
	while :; do
		check && exit 0
		sleep "$INTERVAL"
	done
	;;
*)
	echo "usage: lid.sh [once|loop]" >&2
	exit 2
	;;
esac
