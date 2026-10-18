#include "lince_outbox.h"
#include <string.h>
#include "unity.h"

typedef struct { lince_outbox_t saved; bool fail; unsigned writes; } storage_t;

static esp_err_t persist(const lince_outbox_t *state, void *context) {
    storage_t *storage = context;
    if (storage->fail) return ESP_FAIL;
    storage->saved = *state;
    storage->writes++;
    return ESP_OK;
}

static esp_err_t sign(const char *payload, char output[129]) {
    TEST_ASSERT_NOT_NULL(strstr(payload, "lince-ailuros-v1\n"));
    memset(output, 'a', 128);
    output[128] = 0;
    return ESP_OK;
}

static void initialize(lince_outbox_t *state, storage_t *storage) {
    TEST_ASSERT_EQUAL(ESP_OK, lince_outbox_bind(state, false, "0000000000000000000000000000000000000000000000000000000000000000", "r_00000000000000000000000001", "1", "r_00000000000000000000000002", persist, storage));
}

TEST_CASE("outbox retries exact bytes across a lost response and restart", "[lince]") {
    static storage_t storage;
    static lince_outbox_t state;
    memset(&storage, 0, sizeof(storage));
    memset(&state, 0, sizeof(state));
    initialize(&state, &storage);
    TEST_ASSERT_EQUAL(ESP_OK, lince_outbox_prepare(&state, "12.5", 1900000000, false, sign, persist, &storage));
    static lince_outbox_t pending;
    pending = state;
    state = storage.saved;
    unsigned writes = storage.writes;
    TEST_ASSERT_EQUAL(ESP_OK, lince_outbox_prepare(&state, "99", 1900000900, false, sign, persist, &storage));
    TEST_ASSERT_EQUAL_STRING(pending.pending, state.pending);
    TEST_ASSERT_EQUAL(writes, storage.writes);
    TEST_ASSERT_EQUAL_UINT64(1, state.sequence);
    TEST_ASSERT_EQUAL(ESP_OK, lince_outbox_acknowledge(&state, persist, &storage));
    TEST_ASSERT_EQUAL_UINT64(2, state.sequence);
    TEST_ASSERT_EQUAL_UINT32(0, state.length);
    TEST_ASSERT_EQUAL_UINT64(2, storage.saved.sequence);
}

TEST_CASE("failed persistence cannot advance or replace the pending reading", "[lince]") {
    static storage_t storage;
    static lince_outbox_t state;
    memset(&storage, 0, sizeof(storage));
    memset(&state, 0, sizeof(state));
    initialize(&state, &storage);
    TEST_ASSERT_EQUAL(ESP_OK, lince_outbox_prepare(&state, "12.5", 1900000000, false, sign, persist, &storage));
    static lince_outbox_t before;
    before = state;
    storage.fail = true;
    TEST_ASSERT_EQUAL(ESP_FAIL, lince_outbox_acknowledge(&state, persist, &storage));
    TEST_ASSERT_EQUAL_MEMORY(&before, &state, sizeof(state));
    TEST_ASSERT_EQUAL(ESP_FAIL, lince_outbox_prepare(&state, "13", 1900000900, true, sign, persist, &storage));
    TEST_ASSERT_EQUAL_MEMORY(&before, &state, sizeof(state));
    storage.fail = false;
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_STATE, lince_outbox_prepare(&state, "13", 1900000010, true, sign, persist, &storage));
    TEST_ASSERT_EQUAL(ESP_OK, lince_outbox_prepare(&state, "13", 1900000900, true, sign, persist, &storage));
    TEST_ASSERT_EQUAL_UINT64(2, state.sequence);
    TEST_ASSERT_NOT_EQUAL(0, strcmp(before.pending, state.pending));
}

TEST_CASE("corrupt sequence state and exhaustion require deliberate recovery", "[lince]") {
    static storage_t storage;
    static lince_outbox_t state;
    memset(&storage, 0, sizeof(storage));
    memset(&state, 0, sizeof(state));
    initialize(&state, &storage);
    state.sequence = 0;
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_STATE, lince_outbox_bind(&state, true, storage.saved.audience, storage.saved.device, storage.saved.key_epoch, storage.saved.signal, persist, &storage));
    state = storage.saved;
    state.sequence = INT64_MAX;
    TEST_ASSERT_EQUAL(ESP_OK, lince_outbox_prepare(&state, "12.5", 1900000000, false, sign, persist, &storage));
    unsigned writes = storage.writes;
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_STATE, lince_outbox_acknowledge(&state, persist, &storage));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_STATE, lince_outbox_prepare(&state, "13", 1900000900, true, sign, persist, &storage));
    TEST_ASSERT_EQUAL(writes, storage.writes);
    TEST_ASSERT_EQUAL_UINT64(INT64_MAX, state.sequence);
}
