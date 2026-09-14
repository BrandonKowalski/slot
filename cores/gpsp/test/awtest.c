/* Regression test for the AW serial HLE session-flip overflow.
 * Builds the master-side state a game on its own link screen leaves with no
 * session, flips to client 1 exactly as gpSP's netpacket_start does, drains,
 * and asserts the queue length invariant. Fails on the unfixed core.
 * Build: cc -I<gpsp-src> -I<shimdir> awtest.c -fsanitize=address -g -O1 */
#include "common.h"
#include <assert.h>
u16 io_registers[1024];
u32 netplay_num_clients = 0, netplay_client_id = 0;
int serial_mode = 5;                       /* SERIAL_MODE_SERIAL_AW2 */
void netpacket_send(uint16_t c, const void *b, size_t l){(void)c;(void)b;(void)l;}
#include "serial_proto.c"

static void mw(u16 w){ write_ioreg(REG_SIOMLT_SEND, w); serialaw_master_send(); }

int main(void) {
  serialproto_reset();                      /* the game's link screen: SIO -> MULTI */
  netplay_client_id = 0;
  mw(0x4F00); mw(0x00C8); mw(0x1111); mw(0x2222); mw(0x3333);  /* mid-packet */
  assert(serstate.aw.peer[0].count > 0 && "precondition: master left a queued packet");

  netplay_num_clients = 0;                  /* gpSP netpacket_start(1) */
  netplay_client_id = 1;

  for (int step = 0; step < 900; step++) {
    write_ioreg(REG_SIOMLT_SEND, 0x7FFF);
    serialaw_update(139265);
    for (int i = 0; i < 4; i++)
      assert(serstate.aw.peer[i].count <= MAX_FPACK &&
             "peer queue length must never exceed data[]");
  }
  puts("PASS: no queue-length overflow across the session flip");
  return 0;
}
