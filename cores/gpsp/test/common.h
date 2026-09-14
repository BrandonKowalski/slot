/* Minimal stand-in for gpSP's common.h, just enough to compile serial_proto.c unmodified
 * into a host test under ASan/UBSan. Register numbers are gpSP's own (gba_memory.h). */
#pragma once
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <string.h>
#include <stdio.h>

typedef uint8_t u8;
typedef uint16_t u16;
typedef uint32_t u32;
typedef int32_t s32;
typedef int cpu_alert_type;

extern u16 io_registers[1024];
#define read_ioreg(r) (io_registers[(r)])
#define write_ioreg(r, v) (io_registers[(r)] = (u16)(v))
#define netorder32(v) __builtin_bswap32(v)
#define RETRO_NETPACKET_BROADCAST 0xFFFF

enum {
  REG_SIOMULTI0 = 0x90, REG_SIOMULTI1 = 0x91, REG_SIOMULTI2 = 0x92, REG_SIOMULTI3 = 0x93,
  REG_SIOCNT = 0x94, REG_SIOMLT_SEND = 0x95, REG_RCNT = 0x9A,
};

#include "serial.h"
extern u32 netplay_num_clients, netplay_client_id;
