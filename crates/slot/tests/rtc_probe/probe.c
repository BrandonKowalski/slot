typedef unsigned char u8;
typedef unsigned short u16;
typedef unsigned int u32;

#define GPIO_DATA (*(volatile u16*)0x080000C4)
#define GPIO_DIR  (*(volatile u16*)0x080000C6)
#define GPIO_CTRL (*(volatile u16*)0x080000C8)
#define VCOUNT    (*(volatile u16*)0x04000006)
#define OUT       ((volatile u8*)0x02000000)

static void put(u16 v) { GPIO_DATA = v; GPIO_DATA = v; }

static void send(u8 byte) {
    for (int i = 7; i >= 0; i--) {
        u16 bit = ((byte >> i) & 1) << 1;
        put(4 | bit);
        put(4 | bit | 1);
    }
}

static u8 recv(void) {
    u8 b = 0;
    for (int i = 0; i < 8; i++) {
        put(4);
        put(5);
        b |= ((GPIO_DATA >> 1) & 1) << i;
    }
    return b;
}

static void read_time(u8 *t) {
    GPIO_CTRL = 1;
    GPIO_DIR = 7;
    put(1);
    put(5);
    send(0x65);
    GPIO_DIR = 5;
    for (int i = 0; i < 7; i++) t[i] = recv();
    GPIO_DIR = 7;
    put(1);
    put(1);
}

void main_loop(void) {
    const char *tag = "RTCPROBE";
    for (int i = 0; i < 8; i++) OUT[i] = tag[i];
    u32 frames = 0;
    for (;;) {
        while (VCOUNT >= 160) {}
        while (VCOUNT < 160) {}
        u8 t[7];
        read_time(t);
        for (int i = 0; i < 7; i++) OUT[8 + i] = t[i];
        frames++;
        OUT[15] = frames & 0xff;
    }
}
