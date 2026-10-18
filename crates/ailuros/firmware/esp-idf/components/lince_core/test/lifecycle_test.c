#include "lince_provision.h"
#include "lince_stream.h"
#include <math.h>
#include <string.h>
#include "unity.h"

TEST_CASE("camera demand and acknowledgements bound capture to one current frame", "[lince]") {
    lince_stream_t state;
    lince_stream_init(&state, 5, 0);
    TEST_ASSERT_FALSE(lince_stream_can_capture(&state, 1000000));
    TEST_ASSERT_FALSE(lince_stream_demand(&state, "before-auth", true));
    TEST_ASSERT_TRUE(lince_stream_authenticated(&state, 512 * 1024, 15));
    TEST_ASSERT_FALSE(lince_stream_can_capture(&state, 1000000));
    TEST_ASSERT_TRUE(lince_stream_demand(&state, "first", true));
    TEST_ASSERT_TRUE(lince_stream_frame(&state, 1024, 1000000));
    TEST_ASSERT_FALSE(lince_stream_can_capture(&state, 2000000));
    TEST_ASSERT_FALSE(lince_stream_acknowledge(&state, "old", "{\"sequence\":1}"));
    TEST_ASSERT_FALSE(lince_stream_acknowledge(&state, "first", "{\"sequence\":2}"));
    TEST_ASSERT_TRUE(lince_stream_acknowledge(&state, "first", "{\"sequence\":1}"));
    TEST_ASSERT_TRUE(lince_stream_demand(&state, "idle", false));
    TEST_ASSERT_FALSE(lince_stream_can_capture(&state, 2000000));
    TEST_ASSERT_TRUE(lince_stream_demand(&state, "next", true));
    TEST_ASSERT_TRUE(lince_stream_frame(&state, 1024, 2000000));
    TEST_ASSERT_EQUAL_UINT64(1, state.sequence);
    TEST_ASSERT_FALSE(lince_stream_acknowledge(&state, "first", "{\"sequence\":1}"));
    TEST_ASSERT_TRUE(state.pending);
}

TEST_CASE("camera limits reject invalid advertisements and cannot build a backlog", "[lince]") {
    lince_stream_t state;
    lince_stream_init(&state, 15, 0);
    TEST_ASSERT_FALSE(lince_stream_authenticated(&state, NAN, 5));
    TEST_ASSERT_FALSE(lince_stream_authenticated(&state, 1024, 0.5));
    TEST_ASSERT_FALSE(lince_stream_authenticated(&state, 1024, 16));
    TEST_ASSERT_TRUE(lince_stream_authenticated(&state, 65536, 5));
    TEST_ASSERT_EQUAL(5, state.fps);
    TEST_ASSERT_TRUE(lince_stream_demand(&state, "active", true));
    TEST_ASSERT_FALSE(lince_stream_frame(&state, 65537, 1000000));
    TEST_ASSERT_FALSE(state.pending);
    TEST_ASSERT_FALSE(lince_stream_frame(&state, 1024, 1100000));
    TEST_ASSERT_TRUE(lince_stream_frame(&state, 1024, 1200000));
    TEST_ASSERT_FALSE(lince_stream_frame(&state, 1024, 1500000));
    TEST_ASSERT_TRUE(lince_stream_timed_out(&state, 11200001));
    lince_stream_init(&state, 5, 0);
    TEST_ASSERT_TRUE(lince_stream_timed_out(&state, 15000001));
}

TEST_CASE("provisioning frames bound secret input and recover at a delimiter", "[lince]") {
    static lince_line_t line;
    lince_line_reset(&line);
    for (size_t index = 0; index < LINCE_LINE_MAX + 16; index++) TEST_ASSERT_FALSE(lince_line_feed(&line, 's', 100));
    TEST_ASSERT_TRUE(line.overflow);
    TEST_ASSERT_EQUAL(0, line.length);
    TEST_ASSERT_EQUAL(0, line.bytes[0]);
    TEST_ASSERT_FALSE(lince_line_feed(&line, '\n', 100));
    TEST_ASSERT_FALSE(lince_line_feed(&line, '{', 100));
    TEST_ASSERT_FALSE(lince_line_feed(&line, '}', 200));
    TEST_ASSERT_TRUE(lince_line_feed(&line, '\n', 300));
    TEST_ASSERT_EQUAL_STRING("{}", line.bytes);
    lince_line_reset(&line);
    TEST_ASSERT_FALSE(lince_line_feed(&line, 's', 100));
    TEST_ASSERT_FALSE(lince_line_feed(&line, 0, 100));
    TEST_ASSERT_FALSE(lince_line_feed(&line, '\n', 100));
    TEST_ASSERT_FALSE(lince_line_feed(&line, 's', 100));
    TEST_ASSERT_FALSE(lince_line_feed(&line, 'x', 2000101));
    TEST_ASSERT_EQUAL_STRING("x", line.bytes);
}

TEST_CASE("physical setup windows expire and require a continuous button hold", "[lince]") {
    lince_provision_window_t window = {0};
    TEST_ASSERT_FALSE(lince_provision_window(&window, true, 0));
    TEST_ASSERT_FALSE(lince_provision_window(&window, false, 1000000));
    TEST_ASSERT_FALSE(lince_provision_window(&window, true, 2000000));
    TEST_ASSERT_FALSE(lince_provision_window(&window, true, 3999999));
    TEST_ASSERT_TRUE(lince_provision_window(&window, true, 4000000));
    TEST_ASSERT_TRUE(lince_provision_window(&window, false, 5000000));
    TEST_ASSERT_FALSE(lince_provision_window(&window, false, 124000000));
}

TEST_CASE("trusted server clock cannot roll back the retained floor", "[lince]") {
    TEST_ASSERT_TRUE(lince_server_time_valid(1900000000, 1899999999));
    TEST_ASSERT_TRUE(lince_server_time_valid(1899999970, 1900000000));
    TEST_ASSERT_FALSE(lince_server_time_valid(1899999969, 1900000000));
    TEST_ASSERT_FALSE(lince_server_time_valid(0, 0));
    TEST_ASSERT_FALSE(lince_server_time_valid(4102444801, 1900000000));
    int64_t now = 0;
    TEST_ASSERT_TRUE(lince_clock_decode("{\"audience\":\"cell\",\"server_time\":1900000000}", "cell", 1900000000, &now));
    TEST_ASSERT_EQUAL_INT64(1900000000, now);
    const char *invalid[] = {
        "{\"audience\":\"other\",\"server_time\":1900000000}",
        "{\"audience\":\"cell\",\"server_time\":1899999900}",
        "{\"audience\":\"cell\",\"server_time\":1900000000.5}",
        "{\"audience\":\"cell\",\"server_time\":1900000000,\"skip_tls\":true}",
        "{\"audience\":\"cell\",\"audience\":\"cell\"}",
        "{\"audience\":\"cell\",\"server_time\":1900000000} trailing",
    };
    for (size_t index = 0; index < sizeof(invalid) / sizeof(*invalid); index++) {
        TEST_ASSERT_FALSE(lince_clock_decode(invalid[index], "cell", 1900000000, &now));
        TEST_ASSERT_EQUAL_INT64(1900000000, now);
    }
}

TEST_CASE("network failures back off with bounded jitter and recovery resets delay", "[lince]") {
    unsigned seconds = 1;
    for (unsigned index = 0; index < 100; index++) {
        unsigned delay = lince_retry_delay(&seconds, false, 999);
        TEST_ASSERT_GREATER_OR_EQUAL(1000, delay);
        TEST_ASSERT_LESS_THAN(32500, delay);
        TEST_ASSERT_LESS_OR_EQUAL(32, seconds);
    }
    TEST_ASSERT_EQUAL(1000, lince_retry_delay(&seconds, true, 0));
    TEST_ASSERT_EQUAL(2, seconds);
}

typedef struct { int64_t now; lince_frame_t frame; unsigned acquired; unsigned released; unsigned sent; bool fail; } camera_t;
static int64_t camera_now(void *context) { return ((camera_t *)context)->now; }
static bool camera_acquire(void *context, lince_frame_t *frame) {
    camera_t *camera = context;
    camera->acquired++;
    *frame = camera->frame;
    return true;
}
static bool camera_send(void *context, const lince_frame_t *frame, const char *epoch, uint64_t sequence, int64_t age) {
    camera_t *camera = context;
    TEST_ASSERT_EQUAL_PTR(camera->frame.buffer, frame->buffer);
    TEST_ASSERT_EQUAL_STRING("live", epoch);
    TEST_ASSERT_EQUAL_UINT64(1, sequence);
    TEST_ASSERT_EQUAL(0, age);
    camera->sent++;
    return !camera->fail;
}
static void camera_release(void *context, const lince_frame_t *frame) {
    camera_t *camera = context;
    TEST_ASSERT_EQUAL_PTR(camera->frame.buffer, frame->buffer);
    camera->released++;
}

TEST_CASE("frame buffers are returned after success drops and transport failure", "[lince]") {
    for (int fault = 0; fault < 5; fault++) {
        lince_stream_t state;
        lince_stream_init(&state, 5, 0);
        lince_stream_authenticated(&state, 128 * 1024, 5);
        camera_t camera = { .now = 1000000, .frame = { .length = 1024, .captured_us = 1000000, .jpeg = true, .buffer = &state } };
        lince_frame_io_t io = { .context = &camera, .now = camera_now, .acquire = camera_acquire, .send = camera_send, .release = camera_release };
        TEST_ASSERT_EQUAL(LINCE_FRAME_IDLE, lince_frame_publish(&state, &io));
        TEST_ASSERT_EQUAL(0, camera.acquired);
        lince_stream_demand(&state, "live", true);
        if (fault == 1) camera.fail = true;
        if (fault == 2) camera.frame.length = 128 * 1024 + 1;
        if (fault == 3) camera.frame.jpeg = false;
        if (fault == 4) camera.frame.captured_us = -5000000;
        lince_frame_result_t expected = fault == 0 ? LINCE_FRAME_SENT : fault == 1 ? LINCE_FRAME_FAILED : LINCE_FRAME_DROPPED;
        TEST_ASSERT_EQUAL(expected, lince_frame_publish(&state, &io));
        TEST_ASSERT_EQUAL(1, camera.acquired);
        TEST_ASSERT_EQUAL(1, camera.released);
        TEST_ASSERT_EQUAL(fault < 2 ? 1 : 0, camera.sent);
        if (fault < 2) {
            TEST_ASSERT_EQUAL(LINCE_FRAME_IDLE, lince_frame_publish(&state, &io));
            TEST_ASSERT_EQUAL(1, camera.acquired);
        }
    }
}
