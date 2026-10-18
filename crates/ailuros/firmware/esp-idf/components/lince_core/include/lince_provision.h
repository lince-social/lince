#pragma once

#include "lince_core.h"

typedef struct {
    char bytes[LINCE_LINE_MAX];
    size_t length;
    bool overflow;
    int64_t last_byte;
} lince_line_t;

typedef struct { int64_t until; int64_t pressed_at; bool pressed; } lince_provision_window_t;

void lince_line_reset(lince_line_t *line);
bool lince_line_feed(lince_line_t *line, unsigned char byte, int64_t now);
bool lince_provision_window(lince_provision_window_t *window, bool pressed, int64_t now);
bool lince_server_time_valid(int64_t now, int64_t floor);
bool lince_clock_decode(const char *response, const char *audience, int64_t floor, int64_t *now);
unsigned lince_retry_delay(unsigned *seconds, bool recovered, unsigned random);
