    .section .header, "ax"
    .arm
    .global _start
_start:
    b reset
    .fill 156, 1, 0
    .ascii "RTCPROBE    "
    .ascii "BPEE"
    .ascii "01"
    .byte 0x96
    .byte 0x00
    .byte 0x00
    .fill 7, 1, 0
    .byte 0x00
checksum:
    .byte 0x00
    .byte 0, 0
reset:
    ldr sp, =0x03007F00
    ldr r0, =main_loop
    bx r0
    .pool
