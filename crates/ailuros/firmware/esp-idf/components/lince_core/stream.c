#include "lince_core.h"
#include "lince_stream.h"
#include <math.h>
#include <string.h>

void lince_stream_init(lince_stream_t *state, int fps, int64_t now) {
    *state = (lince_stream_t){ .fps = fps, .maximum = 128 * 1024, .started = now, .window = now };
}

bool lince_stream_authenticated(lince_stream_t *state, double maximum, double fps) {
    if (state->authenticated || !isfinite(maximum) || !isfinite(fps) || maximum < 1 || maximum > 512 * 1024 || floor(maximum) != maximum || fps < 1 || fps > 15 || floor(fps) != fps) return false;
    if (maximum < state->maximum) state->maximum = (size_t)maximum;
    if (fps < state->fps) state->fps = (int)fps;
    state->authenticated = true;
    return true;
}

bool lince_stream_demand(lince_stream_t *state, const char *epoch, bool active) {
    if (!state->authenticated || !epoch || !*epoch || strlen(epoch) >= sizeof(state->epoch)) return false;
    if (strcmp(state->epoch, epoch) != 0) {
        strcpy(state->epoch, epoch);
        state->sequence = 0;
        state->pending = false;
    }
    state->active = active;
    return true;
}

bool lince_stream_acknowledge(lince_stream_t *state, const char *epoch, const char *message) {
    if (!state->pending || !epoch || strcmp(state->epoch, epoch) != 0 || !lince_json_positive_u64(message, "sequence", state->sequence)) return false;
    state->pending = false;
    return true;
}

bool lince_stream_timed_out(const lince_stream_t *state, int64_t now) {
    return (!state->authenticated && now - state->started > 15000000) || (state->pending && now - state->pending_since > 10000000);
}

bool lince_stream_can_capture(const lince_stream_t *state, int64_t now) {
    return state->authenticated && state->active && !state->pending && state->fps > 0 && now - state->last_frame >= 1000000 / state->fps;
}

bool lince_stream_frame(lince_stream_t *state, size_t bytes, int64_t now) {
    if (!lince_stream_can_capture(state, now)) return false;
    state->last_frame = now;
    if (now - state->window >= 1000000) { state->window = now; state->bytes = 0; }
    if (!bytes || bytes > state->maximum || bytes > 1536 * 1024 - state->bytes || state->sequence == UINT64_MAX) return false;
    state->sequence++;
    state->bytes += bytes;
    state->pending = true;
    state->pending_since = now;
    return true;
}

lince_frame_result_t lince_frame_publish(lince_stream_t *state, const lince_frame_io_t *io) {
    if (!lince_stream_can_capture(state, io->now(io->context))) return LINCE_FRAME_IDLE;
    lince_frame_t frame = {0};
    if (!io->acquire(io->context, &frame)) return LINCE_FRAME_FAILED;
    int64_t now = io->now(io->context);
    int64_t age = now - frame.captured_us;
    lince_frame_result_t result = LINCE_FRAME_DROPPED;
    if (frame.jpeg && age >= 0 && age <= 5000000 && lince_stream_frame(state, frame.length, now)) {
        result = io->send(io->context, &frame, state->epoch, state->sequence, age) ? LINCE_FRAME_SENT : LINCE_FRAME_FAILED;
    }
    io->release(io->context, &frame);
    return result;
}
