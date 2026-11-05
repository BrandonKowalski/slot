#!/bin/sh
# Surveys a ROCKNIX handheld for the slot port: display, GPU driver, input, audio,
# power, storage and Wi-Fi. Run it over SSH as root.
#
#   sh probe.sh           read-only survey, changes nothing
#   sh probe.sh buttons   also asks you to press each button. This stops InputPlumber
#                         for about a minute so the raw button devices can be read,
#                         then starts it again. EmulationStation ignores the buttons
#                         while it is stopped.
#
# Everything is printed and also saved to /storage/slot-probe.txt.

OUT="${PROBE_OUT:-/storage/slot-probe.txt}"
MODE="${1:-survey}"

section() {
	printf '\n===== %s\n' "$1"
}

run() {
	printf '$ %s\n' "$*"
	sh -c "$*" 2>&1
}

show() {
	for f in "$@"; do
		[ -e "$f" ] || continue
		printf '%s: %s\n' "$f" "$(tr '\n' ' ' < "$f" 2>/dev/null)"
	done
}

survey() {
	section "system"
	run "cat /etc/os-release"
	run "uname -a"
	show /sys/firmware/devicetree/base/model /sys/firmware/devicetree/base/compatible
	run "ls -l /lib/ld-linux-aarch64.so.1 /usr/lib/ld-linux-aarch64.so.1"
	run "ls -l /usr/lib/libc.so.6"
	run "/usr/lib/libc.so.6 | head -n 1"
	run "cat /proc/cpuinfo | grep -E 'CPU part|Features' | sort | uniq -c"
	run "free -m"

	section "services"
	for s in sway essway emustation inputplumber pipewire pipewire-pulse wireplumber connman iwd NetworkManager sshd systemd-logind; do
		printf '%s: %s\n' "$s" "$(systemctl is-active "$s" 2>/dev/null)"
	done
	run "systemctl list-units --type=service --state=running --no-legend --plain | cut -d' ' -f1"
	run "grep -sE '^[^#]*Handle(PowerKey|LidSwitch)' /etc/systemd/logind.conf /etc/systemd/logind.conf.d/*.conf /storage/.config/logind.conf.d/*.conf"

	section "gpu"
	run "gpudriver"
	run "lsmod | grep -E 'mali|panfrost|panthor'"
	run "ls -l /usr/lib/libEGL.so* /usr/lib/libGLESv2.so* /usr/lib/libgbm.so* /usr/lib/libdrm.so*"
	run "mount | grep -E 'libEGL|libGLES|libgbm|libmali'"
	run "ls /usr/lib/mali"

	section "display"
	run "ls -l /dev/dri /dev/fb0"
	for c in /sys/class/drm/card*-*; do
		[ -d "$c" ] || continue
		show "$c/status" "$c/enabled" "$c/modes" "$c/dpms"
	done
	show /sys/class/graphics/fb0/modes /sys/class/graphics/fb0/virtual_size
	run "grep -E 'Output|current|transform|Mode' /var/log/sway.log | head -n 20"
	run "command -v drm_tool modetest swaymsg"

	section "input"
	run "cat /proc/bus/input/devices"
	run "ls -l /dev/input /dev/inputplumber /dev/inputplumber/sources"
	for d in /sys/class/input/event*; do
		[ -d "$d" ] || continue
		printf '%s %s | key: %s | abs: %s\n' "$(basename "$d")" \
			"$(cat "$d/device/name" 2>/dev/null)" \
			"$(cat "$d/device/capabilities/key" 2>/dev/null)" \
			"$(cat "$d/device/capabilities/abs" 2>/dev/null)"
	done
	run "command -v evtest evtest-ng"

	section "audio"
	run "cat /proc/asound/cards"
	run "aplay -l"
	run "aplay -L | head -n 40"
	run "ls /etc/alsa/conf.d /usr/share/alsa/alsa.conf.d"
	run "amixer scontrols"

	section "power"
	for b in /sys/class/backlight/*; do
		show "$b/brightness" "$b/max_brightness" "$b/type"
	done
	for p in /sys/class/power_supply/*; do
		show "$p/type" "$p/status" "$p/capacity" "$p/online" "$p/voltage_now"
	done
	run "ls /sys/class/leds /sys/class/rtc /sys/class/pwm"
	run "find /sys/devices/platform -name rumble_enable"
	run "command -v poweroff reboot timedatectl hwclock rmmod"

	section "storage"
	run "df -h"
	run "mount | grep -E 'mmcblk|/storage|/roms'"

	section "wifi"
	run "ls /sys/class/net"
	run "iw list 2>/dev/null | sed -n '/Supported interface modes/,/Band 2\|software interface/p' | head -n 20"
	run "iw dev"
	run "command -v iw iwctl connmanctl wpa_cli wpa_supplicant hostapd udhcpc"
}

# One reader per event node. dd reads exactly one 24-byte input_event, then od prints
# it as twelve unsigned 16-bit words: word 9 is the type, 10 the code, 11 and 12 the
# value. A fresh od per event means nothing sits in a pipe buffer.
start_readers() {
	: > "$EVENTS"
	for node in /dev/input/event* /dev/inputplumber/sources/event*; do
		[ -c "$node" ] || continue
		(
			while :; do
				w="$(dd if="$node" bs=24 count=1 2>/dev/null | od -An -v -tu2 | tr -s ' \n' '  ')" || break
				[ -n "$w" ] || break
				echo "$(basename "$node") $w" >> "$EVENTS"
			done
		) &
		READERS="$READERS $!"
	done
}

stop_readers() {
	for p in $READERS; do
		kill "$p" 2>/dev/null
	done
	READERS=""
}

restore() {
	stop_readers
	if [ -n "$STOPPED_IP" ]; then
		systemctl start inputplumber 2>/dev/null
		STOPPED_IP=""
		echo "InputPlumber started again."
	fi
	rm -f "$EVENTS"
}

buttons() {
	section "buttons"
	EVENTS="$(mktemp)"
	READERS=""
	STOPPED_IP=""
	trap 'restore; exit 1' HUP INT TERM
	if [ "$(systemctl is-active inputplumber 2>/dev/null)" = active ]; then
		echo "Stopping InputPlumber so the raw button devices can be read."
		systemctl stop inputplumber
		STOPPED_IP=1
		sleep 2
	fi
	run "ls -l /dev/input"
	start_readers
	sleep 1
	for b in A B X Y L1 R1 L2 R2 SELECT START FN/MENU UP DOWN LEFT RIGHT L3 R3 VOLUME-UP VOLUME-DOWN; do
		seen="$(wc -l < "$EVENTS")"
		printf 'Press and release %s (10 s, or wait to skip) ... ' "$b" > /dev/tty
		got=""
		i=0
		while [ "$i" -lt 100 ]; do
			got="$(tail -n +"$((seen + 1))" "$EVENTS" | awk '$10 == 1 && $12 == 1 { print; exit }')"
			[ -n "$got" ] && break
			sleep 0.1
			i=$((i + 1))
		done
		if [ -n "$got" ]; then
			echo "$got" | awk -v b="$b" '{ printf "%s: %s key code %d (0x%x)\n", b, $1, $11, $11 }'
			echo "ok" > /dev/tty
			sleep 0.4
		else
			echo "$b: nothing"
			echo "skipped" > /dev/tty
		fi
	done
	echo "Now push the left stick fully right, then fully down, then let go (5 s)."
	seen="$(wc -l < "$EVENTS")"
	sleep 5
	tail -n +"$((seen + 1))" "$EVENTS" | awk '$10 == 3 { v = $12 + $13 * 65536; if (v > 2147483647) v -= 4294967296; print $1, "abs", $11, v }' |
		awk '{ k = $1 " " $3; if (!(k in lo) || $4 < lo[k]) lo[k] = $4; if (!(k in hi) || $4 > hi[k]) hi[k] = $4 }
		END { for (k in lo) printf "axis %s: %d to %d\n", k, lo[k], hi[k] }'
	restore
	trap - HUP INT TERM
}

{
	echo "slot ROCKNIX probe, $(date)"
	survey
	[ "$MODE" = buttons ] && buttons
	echo
	echo "===== done"
} 2>&1 | tee "$OUT"
echo "Saved to $OUT"
