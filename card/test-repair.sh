#!/bin/sh
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$HERE/System/repair.sh"
TMP="$(mktemp -d)"
export TMP
trap 'rm -rf "$TMP"' EXIT HUP INT TERM

fail() {
	echo "repair: $*" >&2
	exit 1
}

fake() {
	cat > "$TMP/bin/$1" <<-SH
		#!/bin/sh
		echo "$1 \$*" >> "$TMP/calls"
		$2
	SH
	chmod +x "$TMP/bin/$1"
}

setup() {
	rm -rf "$TMP/bin" "$TMP/data" "$TMP/run" "$TMP/calls" "$TMP/codes"
	mkdir -p "$TMP/bin" "$TMP/data" "$TMP/run"
	: > "$TMP/calls"
	echo "/dev/mmcblk0p7 /mnt/sdcard vfat ${1:-ro},noatime,errors=remount-ro 0 0" > "$TMP/mounts"
	fake umount ': > "$TMP/mounts"'
	fake mount 'exit 0'
	fake fuser 'exit 0'
	fake sync 'exit 0'
	fake reboot 'exit 0'
	fake splash 'exit 0'
	fake sleep 'exit 0'
	fake rmmod 'exit 0'
	echo "100.00 50.00" > "$TMP/uptime"
	fake fsck.fat 'code="$(head -1 "$TMP/codes")"; sed -i.bak 1d "$TMP/codes"; exit "$code"'
	cp "$TMP/bin/fsck.fat" "$TMP/bin/fsck.exfat"
	sed -i.bak 's/^echo "fsck.fat/echo "fsck.exfat/' "$TMP/bin/fsck.exfat"
	printf '0\n0\n' > "$TMP/codes"
}

codes() {
	printf '%s\n' "$@" > "$TMP/codes"
}

repair() {
	AGS_DATA="$TMP/data" AGS_RUN="$TMP/run" AGS_MOUNTS="$TMP/mounts" \
		AGS_UMOUNT="$TMP/bin/umount" AGS_MOUNT="$TMP/bin/mount" AGS_FUSER="$TMP/bin/fuser" \
		AGS_SYNC="$TMP/bin/sync" AGS_REBOOT="$TMP/bin/reboot" AGS_RMMOD="$TMP/bin/rmmod" AGS_SPLASH="$TMP/bin/splash" \
		AGS_FSCK_FAT="$TMP/bin/fsck.fat" AGS_FSCK_EXFAT="$TMP/bin/fsck.exfat" \
		AGS_OUT="$TMP/out" AGS_UPTIME="$TMP/uptime" AGS_SLEEP="$TMP/bin/sleep" AGS_WIFI_WAIT=100000 \
		sh "$SCRIPT" /dev/mmcblk0p7 /mnt/sdcard "${1:-vfat}"
}

result() {
	cat "$TMP/data/slot-repair.result" 2>/dev/null
}

called() {
	grep -qx "$1" "$TMP/calls"
}

before() {
	first="$(grep -nx "$1" "$TMP/calls" | head -1 | cut -d: -f1)"
	second="$(grep -nx "$2" "$TMP/calls" | head -1 | cut -d: -f1)"
	[ -n "$first" ] && [ -n "$second" ] && [ "$first" -lt "$second" ]
}

setup
codes 1 0
repair
[ "$(result)" = repaired ] || fail "a FAT card fsck fixed did not read as repaired: $(result)"
before "umount /mnt/sdcard" "fsck.fat -a /dev/mmcblk0p7" || fail "fsck ran before the card was unmounted"
[ "$(grep -c '^fsck.fat -a /dev/mmcblk0p7$' "$TMP/calls")" = 2 ] || fail "the repair was not checked by a second pass"
before "fsck.fat -a /dev/mmcblk0p7" "reboot " || fail "did not reboot after the repair"
before "rmmod 8821cs" "reboot " || fail "rebooted with the WiFi driver loaded, which hangs the shutdown"
[ "$(grep '^splash' "$TMP/calls" | sed 's/^splash --important -1 //' | tr '\n' '|')" = \
	"UNMOUNTING DISK|REPAIRING|REPAIR COMPLETE|REBOOTING|" ] \
	|| fail "the screen did not walk through the job: $(grep '^splash' "$TMP/calls" | tr '\n' '|')"
before "splash --important -1 UNMOUNTING DISK" "umount /mnt/sdcard" || fail "unmounted before saying so"
before "fsck.fat -a /dev/mmcblk0p7" "splash --important -1 REPAIR COMPLETE" || fail "said complete before fsck ran"
grep -q "fsck.fat -a /dev/mmcblk0p7 pass 1 exit 1" "$TMP/data/slot-repair.log" || fail "the log did not keep fsck's exit"
[ "$(grep -c '^sleep 2.00$' "$TMP/calls")" = 3 ] && called "sleep 2" \
	|| fail "a step was not held for two seconds: $(grep '^sleep' "$TMP/calls" | tr '\n' '|')"

setup
codes 0 0
repair
[ "$(result)" = clean ] || fail "a card with nothing to fix did not read as clean: $(result)"

setup
codes 1 1 0
repair
[ "$(result)" = repaired ] || fail "a card that took two passes to fix did not read as repaired: $(result)"

setup
codes 1 1 1 0
repair
[ "$(result)" = repaired ] || fail "a FAT card that took a pass per layer of damage did not read as repaired: $(result)"

setup
codes 1 1 1 1 1 1
repair
[ "$(result)" = failed ] || fail "a card still damaged after repair did not read as failed: $(result)"
[ "$(grep -c '^fsck.fat -a /dev/mmcblk0p7$' "$TMP/calls")" = 6 ] || fail "gave up before six passes"
called "splash --important -1 REPAIR FAILED: USE A COMPUTER" || fail "a failed repair did not say so"
called "splash --important -1 REBOOTING" || fail "a failed repair did not say it was rebooting"

setup
codes 1 0
repair exfat
[ "$(result)" = repaired ] || fail "an exFAT card did not repair: $(result)"
called "fsck.exfat -y /dev/mmcblk0p7" || fail "exFAT was not checked with fsck.exfat -y"
! called "fsck.fat -a /dev/mmcblk0p7" || fail "ran fsck.fat on an exFAT card"

setup
repair ext4
[ "$(result)" = unsupported ] || fail "an unknown filesystem was not refused: $(result)"
! grep -q '^fsck' "$TMP/calls" || fail "ran fsck on an unsupported filesystem"

setup rw
fake umount 'exit 1'
fake mount 'sed -i.bak "s/ rw,/ ro,/" "$TMP/mounts"'
codes 1 0
repair
[ "$(result)" = repaired ] || fail "a card that would not unmount was not repaired read-only: $(result)"
called "fuser -km /mnt/sdcard" || fail "did not stop what held the card"
called "mount -o remount,ro /mnt/sdcard" || fail "did not fall back to read-only"
called "reboot -f" || fail "a card repaired under a read-only mount was not rebooted at once"
before "rmmod 8821cs" "reboot -f" || fail "a forced reboot kept the WiFi driver loaded"

setup rw
fake umount 'exit 1'
fake mount 'exit 1'
repair
[ "$(result)" = busy ] || fail "a card that stayed writable did not read as busy: $(result)"
! grep -q '^fsck' "$TMP/calls" || fail "ran fsck on a card that was still mounted read-write"
called "reboot " || fail "a busy card did not reboot"

setup
echo 4242 > "$TMP/run/slot-led.pid"
codes 0 0
repair
[ "$(result)" = clean ] || fail "a stale LED pid broke the repair"

setup
cat >> "$TMP/mounts" <<-M
	/dev/mmcblk1p1 /mnt/other exfat ro,noatime 0 0
M
fake umount 'grep -v " $1 " "$TMP/mounts" > "$TMP/m" || true; mv "$TMP/m" "$TMP/mounts"'
codes 0 1 0
AGS_DATA="$TMP/data" AGS_RUN="$TMP/run" AGS_MOUNTS="$TMP/mounts" \
	AGS_UMOUNT="$TMP/bin/umount" AGS_MOUNT="$TMP/bin/mount" AGS_FUSER="$TMP/bin/fuser" \
	AGS_SYNC="$TMP/bin/sync" AGS_REBOOT="$TMP/bin/reboot" AGS_RMMOD="$TMP/bin/rmmod" AGS_SPLASH="$TMP/bin/splash" \
	AGS_FSCK_FAT="$TMP/bin/fsck.fat" AGS_FSCK_EXFAT="$TMP/bin/fsck.exfat" \
	AGS_OUT="$TMP/out" AGS_UPTIME="$TMP/uptime" AGS_SLEEP="$TMP/bin/sleep" AGS_WIFI_WAIT=100000 \
	sh "$SCRIPT" /dev/mmcblk0p7 /mnt/sdcard vfat /dev/mmcblk1p1 /mnt/other exfat
called "splash --important -1 UNMOUNTING DISKS" || fail "two cards were not called disks"
called "umount /mnt/other" || fail "the second card was not unmounted"
called "fsck.exfat -y /dev/mmcblk1p1" || fail "the second card was not checked"
[ "$(result)" = repaired ] || fail "two cards, one fixed, did not read as repaired: $(result)"

echo "repair: all passed"
