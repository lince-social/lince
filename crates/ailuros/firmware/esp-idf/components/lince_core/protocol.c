#include "lince_core.h"
#include <inttypes.h>
#include <errno.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

static bool hex(const char *value) {
    return value && strlen(value) == 64 && strspn(value, "0123456789abcdef") == 64;
}

static bool uid(const char *value) {
    return value && strlen(value) == 28 && strncmp(value, "r_", 2) == 0 && strspn(value + 2, "0123456789ABCDEFGHJKMNPQRSTVWXYZ") == 26;
}

static bool key_epoch(const char *value) {
    if (!value || !*value || *value == '0') return false;
    size_t size = strlen(value);
    return size <= 19 && strspn(value, "0123456789") == size && (size < 19 || strcmp(value, "9223372036854775807") <= 0);
}

esp_err_t lince_reading_payload(char *output, size_t capacity, const char *audience, const char *device, const char *epoch, const char *signal, uint64_t sequence, int64_t issued_at, const char *value) {
    if (!hex(audience) || !uid(device) || !key_epoch(epoch) || !uid(signal) || !sequence || sequence > INT64_MAX || !lince_decimal_valid(value)) return ESP_ERR_INVALID_ARG;
    int length = snprintf(output, capacity, "lince-ailuros-v1\n%s\n%s\n%s\n%s\n%" PRIu64 "\n%" PRId64 "\n%s\n", audience, device, epoch, signal, sequence, issued_at, value);
    return length >= 0 && (size_t)length < capacity ? ESP_OK : ESP_ERR_INVALID_SIZE;
}

esp_err_t lince_stream_payload(char *output, size_t capacity, const char *audience, const char *device, const char *epoch, const char *signal, const char *challenge) {
    if (!hex(audience) || !uid(device) || !key_epoch(epoch) || !uid(signal) || !hex(challenge)) return ESP_ERR_INVALID_ARG;
    int length = snprintf(output, capacity, "lince-ailuros-stream-v1\n%s\n%s\n%s\n%s\n%s\npublish\n", audience, device, epoch, signal, challenge);
    return length >= 0 && (size_t)length < capacity ? ESP_OK : ESP_ERR_INVALID_SIZE;
}

bool lince_json_positive_u64(const char *json, const char *field, uint64_t expected) {
    if (!json || !field || !*field || strlen(field) > 32 || strspn(field, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_") != strlen(field)) return false;
    char marker[64];
    snprintf(marker, sizeof(marker), "\"%s\"", field);
    const char *start = strstr(json, marker);
    if (!start) return false;
    start += strlen(marker);
    while (*start == ' ' || *start == '\t' || *start == '\r' || *start == '\n') start++;
    if (*start++ != ':') return false;
    while (*start == ' ' || *start == '\t' || *start == '\r' || *start == '\n') start++;
    if (*start < '1' || *start > '9') return false;
    char *end;
    errno = 0;
    unsigned long long value = strtoull(start, &end, 10);
    if (errno || end == start || value != expected) return false;
    while (*end == ' ' || *end == '\t' || *end == '\r' || *end == '\n') end++;
    return *end == ',' || *end == '}';
}
