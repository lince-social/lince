#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct {
    char epoch[96];
    bool authenticated;
    bool active;
    bool pending;
    uint64_t sequence;
    int fps;
    size_t maximum;
    size_t bytes;
    int64_t started;
    int64_t last_frame;
    int64_t pending_since;
    int64_t window;
} lince_stream_t;

void lince_stream_init(lince_stream_t *state, int fps, int64_t now);
bool lince_stream_authenticated(lince_stream_t *state, double maximum, double fps);
bool lince_stream_demand(lince_stream_t *state, const char *epoch, bool active);
bool lince_stream_acknowledge(lince_stream_t *state, const char *epoch, const char *message);
bool lince_stream_timed_out(const lince_stream_t *state, int64_t now);
bool lince_stream_can_capture(const lince_stream_t *state, int64_t now);
bool lince_stream_frame(lince_stream_t *state, size_t bytes, int64_t now);

typedef struct {
    const uint8_t *bytes;
    size_t length;
    int64_t captured_us;
    bool jpeg;
    void *buffer;
} lince_frame_t;

typedef struct {
    void *context;
    int64_t (*now)(void *context);
    bool (*acquire)(void *context, lince_frame_t *frame);
    bool (*send)(void *context, const lince_frame_t *frame, const char *epoch, uint64_t sequence, int64_t age);
    void (*release)(void *context, const lince_frame_t *frame);
} lince_frame_io_t;

typedef enum { LINCE_FRAME_IDLE, LINCE_FRAME_DROPPED, LINCE_FRAME_SENT, LINCE_FRAME_FAILED } lince_frame_result_t;
lince_frame_result_t lince_frame_publish(lince_stream_t *state, const lince_frame_io_t *io);
