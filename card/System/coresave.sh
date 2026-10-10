#!/bin/sh
SD="${AGS_CARD:-/mnt/sdcard}"
MOUNTPOINT="${AGS_MOUNTPOINT:-mountpoint}"
DIR="$SD/crash"

if ! $MOUNTPOINT -q "$SD" 2>/dev/null; then
	cat > /dev/null
	exit 0
fi

exe="${AGS_CORE_EXE:-$(readlink "/proc/$1/exe" 2>/dev/null)}"
cmd="${AGS_CORE_CMDLINE:-$(tr '\0' ' ' < "/proc/$1/cmdline" 2>/dev/null)}"
mkdir -p "$DIR" 2>/dev/null
echo "$(date '+%F %T') pid=$1 signal=$2 thread=$3 exe=$exe" >> "$DIR/cores.log"

case "$exe ${cmd% }" in
*/slot|*/slot\ *)
	cp "/proc/$1/maps" "$DIR/slot.maps" 2>/dev/null
	cp "$SD/slot.log" "$DIR/slot-$(date '+%F_%H-%M-%S').log" 2>/dev/null
	cat > "$DIR/slot.core.tmp" && mv -f "$DIR/slot.core.tmp" "$DIR/slot.core"
	;;
*)
	cat > /dev/null
	;;
esac
sync
