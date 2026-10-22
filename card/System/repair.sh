#!/bin/sh
HERE="$(cd "$(dirname "$0")" && pwd)"
DATA="${AGS_DATA:-/data}"
RUN="${AGS_RUN:-/run}"
MOUNTS="${AGS_MOUNTS:-/proc/mounts}"
UPTIME="${AGS_UPTIME:-/proc/uptime}"
UMOUNT="${AGS_UMOUNT:-umount}"
MOUNT="${AGS_MOUNT:-mount}"
FUSER="${AGS_FUSER:-fuser}"
SYNC="${AGS_SYNC:-sync}"
REBOOT="${AGS_REBOOT:-reboot}"
RMMOD="${AGS_RMMOD:-rmmod}"
WIFI_WAIT="${AGS_WIFI_WAIT:-5}"
STEP_WAIT="${AGS_STEP_WAIT:-30}"
FSCK_WAIT="${AGS_FSCK_WAIT:-600}"
PASSES=6
SPLASH="${AGS_SPLASH:-/usr/bin/baseos-splash}"
FSCK_FAT="${AGS_FSCK_FAT:-/usr/sbin/fsck.fat}"
FSCK_EXFAT="${AGS_FSCK_EXFAT:-$HERE/fsck.exfat}"
SLEEP="${AGS_SLEEP:-sleep}"
HOLD="${AGS_HOLD:-2}"
LOG="$DATA/slot-repair.log"
RESULT="$DATA/slot-repair.result"

exec > "${AGS_OUT:-/tmp/slot-repair.out}" 2>&1 < /dev/null
cd /

log() {
	echo "$(date '+%F %T') $*" >> "$LOG"
}

now() {
	cut -d' ' -f1 "$UPTIME" 2>/dev/null
}

shown=
say() {
	if [ -n "$shown" ]; then
		left="$(awk -v a="$shown" -v b="$(now)" -v h="$HOLD" \
			'BEGIN { l = h - (b - a); if (l > 0) printf "%.2f", l; else print 0 }')"
		[ "$left" = 0 ] || $SLEEP "$left"
	fi
	"$SPLASH" --important -1 "$1" 2>/dev/null
	shown="$(now)"
}

options() {
	awk -v m="$1" '$2 == m { o = $4 } END { print o }' "$MOUNTS"
}

bounded() {
	b_limit="$1"
	shift
	"$@" &
	b_pid=$!
	b_waited=0
	while kill -0 "$b_pid" 2>/dev/null; do
		if [ "$b_waited" -ge $((b_limit * 10)) ]; then
			kill -KILL "$b_pid" 2>/dev/null
			log "$* still running after ${b_limit}s, gave up on it"
			after=stuck
			return 124
		fi
		$SLEEP 0.1
		b_waited=$((b_waited + 1))
	done
	wait "$b_pid"
}

unload_wifi() {
	$RMMOD 8821cs 2>/dev/null &
	waited=0
	while kill -0 $! 2>/dev/null && [ "$waited" -lt "$WIFI_WAIT" ]; do
		$SLEEP 1
		waited=$((waited + 1))
	done
}

after=
finish() {
	echo "$1" > "$RESULT"
	log "result $1"
	bounded "$STEP_WAIT" $SYNC
	if [ "$1" = repaired ] || [ "$1" = clean ]; then
		say "REPAIR COMPLETE"
	else
		say "REPAIR FAILED: USE A COMPUTER"
	fi
	say "REBOOTING"
	$SLEEP "$HOLD"
	unload_wifi
	bounded "$STEP_WAIT" $SYNC
	case "$after" in
	stuck) $REBOOT -f -n ;;
	force) $REBOOT -f ;;
	*) $REBOOT || $REBOOT -f ;;
	esac
	exit 0
}

unmount() {
	bounded "$STEP_WAIT" $UMOUNT "$1" 2>/dev/null
	if [ -n "$(options "$1")" ]; then
		bounded "$STEP_WAIT" $FUSER -km "$1" 2>/dev/null
		$SLEEP 1
		bounded "$STEP_WAIT" $UMOUNT "$1" 2>/dev/null
	fi
	[ -n "$(options "$1")" ] || return 0
	bounded "$STEP_WAIT" $MOUNT -o remount,ro "$1" 2>/dev/null
	case ",$(options "$1")," in
	*,ro,*)
		log "could not unmount $1, checking it read-only"
		[ "$after" = stuck ] || after=force
		;;
	*)
		log "could not unmount $1 or make it read-only"
		finish busy
		;;
	esac
}

if [ $# -eq 0 ] || [ $(($# % 3)) -ne 0 ]; then
	log "repair: expected device, mount point and filesystem for each card, got: $*"
	finish unsupported
fi

cards=0
set -- "$@" end
while [ "$1" != end ]; do
	case "$3" in
	vfat | exfat) ;;
	*)
		log "repair $1 $2: unsupported filesystem $3"
		finish unsupported
		;;
	esac
	log "repair $1 $2 $3"
	cards=$((cards + 1))
	set -- "$@" "$1" "$2" "$3"
	shift 3
done
shift

if [ "$cards" -eq 1 ]; then
	say "UNMOUNTING DISK"
else
	say "UNMOUNTING DISKS"
fi
pid="$(cat "$RUN/slot-led.pid" 2>/dev/null)"
[ -n "$pid" ] && kill "$pid" 2>/dev/null
bounded "$STEP_WAIT" $SYNC
set -- "$@" end
while [ "$1" != end ]; do
	unmount "$2"
	set -- "$@" "$1" "$2" "$3"
	shift 3
done
shift

say "REPAIRING"
status=clean
set -- "$@" end
while [ "$1" != end ]; do
	case "$3" in
	vfat) fsck="$FSCK_FAT" fix=-a ;;
	exfat) fsck="$FSCK_EXFAT" fix=-y ;;
	esac
	if [ ! -x "$fsck" ]; then
		log "no $fsck"
		status=failed
	else
		fixed=
		pass=0
		while [ "$pass" -lt "$PASSES" ]; do
			pass=$((pass + 1))
			bounded "$FSCK_WAIT" "$fsck" "$fix" "$1" >> "$LOG" 2>&1
			check=$?
			log "$(basename "$fsck") $fix $1 pass $pass exit $check"
			[ -z "$fixed" ] && fixed=$check
			[ "$check" -eq 0 ] && break
			[ "$check" -eq 124 ] && break
		done
		if [ "$check" -ne 0 ]; then
			status=failed
		elif [ "$fixed" -ne 0 ] && [ "$status" = clean ]; then
			status=repaired
		fi
	fi
	set -- "$@" "$1" "$2" "$3"
	shift 3
done
finish "$status"
