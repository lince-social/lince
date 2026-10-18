#include "lince_provision.h"
#include <string.h>

void lince_line_reset(lince_line_t *line) { memset(line, 0, sizeof(*line)); }

bool lince_line_feed(lince_line_t *line, unsigned char byte, int64_t now) {
    if (now - line->last_byte > 2000000) lince_line_reset(line);
    line->last_byte = now;
    if (byte == '\n') {
        if (line->overflow || !line->length) { lince_line_reset(line); return false; }
        line->bytes[line->length] = 0;
        return true;
    }
    if (line->overflow) return false;
    if (!byte || line->length == sizeof(line->bytes) - 1) {
        memset(line->bytes, 0, sizeof(line->bytes));
        line->length = 0;
        line->overflow = true;
    } else line->bytes[line->length++] = byte;
    return false;
}

bool lince_provision_window(lince_provision_window_t *window, bool pressed, int64_t now) {
    if (pressed) {
        if (!window->pressed) window->pressed_at = now;
        if (now - window->pressed_at >= 2000000) window->until = now + 120000000;
    }
    window->pressed = pressed;
    return now < window->until;
}

bool lince_server_time_valid(int64_t now, int64_t floor) {
    return now >= 1700000000 && now <= 4102444800 && floor >= 0 && floor <= 4102444800 && now >= floor - 30;
}

bool lince_clock_decode(const char *response, const char *audience, int64_t floor, int64_t *now) {
    if (!response || strlen(response) >= 512 || !audience || !now) return false;
    cJSON *message = cJSON_ParseWithOpts(response, NULL, true);
    const cJSON *identity = cJSON_GetObjectItemCaseSensitive(message, "audience");
    const cJSON *clock = cJSON_GetObjectItemCaseSensitive(message, "server_time");
    bool valid = cJSON_IsObject(message) && cJSON_GetArraySize(message) == 2 && cJSON_IsString(identity) && strcmp(identity->valuestring, audience) == 0 && cJSON_IsNumber(clock) && clock->valuedouble >= 1700000000 && clock->valuedouble <= 4102444800 && clock->valuedouble == (int64_t)clock->valuedouble && lince_server_time_valid((int64_t)clock->valuedouble, floor);
    if (valid) *now = (int64_t)clock->valuedouble;
    cJSON_Delete(message);
    return valid;
}

unsigned lince_retry_delay(unsigned *seconds, bool recovered, unsigned random) {
    if (recovered || *seconds < 1 || *seconds > 32) *seconds = 1;
    unsigned delay = *seconds * 1000 + random % 500;
    if (*seconds < 32) *seconds *= 2;
    return delay;
}
