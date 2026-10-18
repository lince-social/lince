#pragma once

#include "lince_core.h"

typedef struct {
    uint64_t sequence;
    int64_t issued_at;
    uint32_t length;
    char audience[65], device[29], signal[29], key_epoch[20];
    char pending[2048];
} lince_outbox_t;

typedef esp_err_t (*lince_persist_fn)(const lince_outbox_t *state, void *context);
typedef esp_err_t (*lince_sign_fn)(const char *payload, char signature[129]);

esp_err_t lince_outbox_bind(lince_outbox_t *state, bool stored, const char *audience, const char *device, const char *epoch, const char *signal, lince_persist_fn persist, void *context);
esp_err_t lince_outbox_prepare(lince_outbox_t *state, const char *value, int64_t now, bool replace_expired, lince_sign_fn sign, lince_persist_fn persist, void *context);
esp_err_t lince_outbox_acknowledge(lince_outbox_t *state, lince_persist_fn persist, void *context);
