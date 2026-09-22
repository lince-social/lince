#include "doomgeneric.h"
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <time.h>
#include <unistd.h>

static int frames;
static unsigned char keys[4096];
static unsigned int head;
static unsigned int tail;
static int suspended;
static int pending = -1;

static void input(void) {
    unsigned char bytes[256];
    ssize_t count = read(STDIN_FILENO, bytes, sizeof(bytes));
    if (count == 0) _exit(0);
    if (count < 0 && errno != EAGAIN && errno != EINTR) _exit(1);
    for (ssize_t i = 0; i < count; ++i) {
        if (pending < 0) { pending = bytes[i]; continue; }
        if (pending >= 2) suspended = pending == 2;
        else {
            keys[head++ % sizeof(keys)] = pending;
            keys[head++ % sizeof(keys)] = bytes[i];
            if (head - tail > sizeof(keys)) _exit(1);
        }
        pending = -1;
    }
}

void DG_Init(void) {
    fcntl(STDIN_FILENO, F_SETFL, O_NONBLOCK);
}

void DG_DrawFrame(void) {
    unsigned char rgba[DOOMGENERIC_RESX * DOOMGENERIC_RESY * 4];
    for (unsigned int i = 0; i < DOOMGENERIC_RESX * DOOMGENERIC_RESY; ++i) {
        unsigned int pixel = DG_ScreenBuffer[i];
        rgba[i * 4] = pixel >> 16;
        rgba[i * 4 + 1] = pixel >> 8;
        rgba[i * 4 + 2] = pixel;
        rgba[i * 4 + 3] = 255;
    }
    size_t offset = 0;
    while (offset < sizeof(rgba)) {
        ssize_t count = write(frames, rgba + offset, sizeof(rgba) - offset);
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0) _exit(0);
        offset += count;
    }
}

void DG_SleepMs(uint32_t ms) {
    struct timespec duration = {ms / 1000, (ms % 1000) * 1000000};
    nanosleep(&duration, NULL);
}

uint32_t DG_GetTicksMs(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (uint32_t)((uint64_t)now.tv_sec * 1000 + now.tv_nsec / 1000000);
}

int DG_GetKey(int *pressed, unsigned char *key) {
    input();
    if (head - tail < 2) return 0;
    *pressed = keys[tail++ % sizeof(keys)];
    *key = keys[tail++ % sizeof(keys)];
    return 1;
}

void DG_SetWindowTitle(const char *title) {
    (void)title;
}

int main(int argc, char **argv) {
    frames = dup(STDOUT_FILENO);
    dup2(STDERR_FILENO, STDOUT_FILENO);
    doomgeneric_Create(argc, argv);
    for (;;) {
        input();
        if (suspended) DG_SleepMs(20);
        else doomgeneric_Tick();
    }
}
