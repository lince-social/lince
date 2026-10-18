#include "lince_outbox.h"
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static esp_err_t commit(lince_outbox_t *state, const lince_outbox_t *candidate, lince_persist_fn persist, void *context) {
    esp_err_t error = persist(candidate, context);
    if (error == ESP_OK) memcpy(state, candidate, sizeof(*state));
    return error;
}

esp_err_t lince_outbox_bind(lince_outbox_t *state, bool stored, const char *audience, const char *device, const char *epoch, const char *signal, lince_persist_fn persist, void *context) {
    char payload[512];
    if (lince_reading_payload(payload, sizeof(payload), audience, device, epoch, signal, 1, 1, "0") != ESP_OK) return ESP_ERR_INVALID_ARG;
    if (stored && (state->length >= sizeof(state->pending) || state->sequence == 0 || state->sequence > INT64_MAX || state->audience[64] || state->device[28] || state->signal[28] || state->key_epoch[19] || state->pending[state->length])) return ESP_ERR_INVALID_STATE;
    if (stored && lince_reading_payload(payload, sizeof(payload), state->audience, state->device, state->key_epoch, state->signal, state->sequence, state->issued_at, "0") != ESP_OK) return ESP_ERR_INVALID_STATE;
    if (stored && strcmp(state->audience, audience) == 0 && strcmp(state->device, device) == 0 && strcmp(state->key_epoch, epoch) == 0 && strcmp(state->signal, signal) == 0) return ESP_OK;
    lince_outbox_t candidate = {0};
    candidate.sequence = 1;
    strcpy(candidate.audience, audience);
    strcpy(candidate.device, device);
    strcpy(candidate.signal, signal);
    strcpy(candidate.key_epoch, epoch);
    return commit(state, &candidate, persist, context);
}

esp_err_t lince_outbox_prepare(lince_outbox_t *state, const char *value, int64_t now, bool replace_expired, lince_sign_fn sign, lince_persist_fn persist, void *context) {
    if (state->length && !replace_expired) return ESP_OK;
    if (replace_expired && (!state->length || state->sequence == INT64_MAX || (state->issued_at >= now - 120 && state->issued_at <= now + 30))) return ESP_ERR_INVALID_STATE;
    lince_outbox_t candidate = *state;
    if (replace_expired) candidate.sequence++;
    candidate.issued_at = now;
    char payload[512], signature[129], sequence[32];
    esp_err_t error = lince_reading_payload(payload, sizeof(payload), candidate.audience, candidate.device, candidate.key_epoch, candidate.signal, candidate.sequence, now, value);
    if (error != ESP_OK) return error;
    error = sign(payload, signature);
    if (error != ESP_OK) return error;
    if (strlen(signature) != 128 || strspn(signature, "0123456789abcdef") != 128) return ESP_ERR_INVALID_ARG;
    snprintf(sequence, sizeof(sequence), "%" PRIu64, candidate.sequence);
    cJSON *request = cJSON_CreateObject();
    cJSON_AddStringToObject(request, "audience", candidate.audience);
    cJSON_AddStringToObject(request, "device", candidate.device);
    cJSON_AddRawToObject(request, "key_epoch", candidate.key_epoch);
    cJSON_AddStringToObject(request, "signal", candidate.signal);
    cJSON_AddRawToObject(request, "sequence", sequence);
    cJSON_AddNumberToObject(request, "issued_at", now);
    cJSON_AddStringToObject(request, "value", value);
    cJSON_AddStringToObject(request, "signature", signature);
    char *encoded = cJSON_PrintUnformatted(request);
    cJSON_Delete(request);
    if (!encoded) return ESP_ERR_NO_MEM;
    size_t length = strlen(encoded);
    if (length >= sizeof(candidate.pending)) { free(encoded); return ESP_ERR_INVALID_SIZE; }
    memset(candidate.pending, 0, sizeof(candidate.pending));
    memcpy(candidate.pending, encoded, length + 1);
    candidate.length = length;
    free(encoded);
    return commit(state, &candidate, persist, context);
}

esp_err_t lince_outbox_acknowledge(lince_outbox_t *state, lince_persist_fn persist, void *context) {
    if (!state->length || state->sequence == INT64_MAX) return ESP_ERR_INVALID_STATE;
    lince_outbox_t candidate = *state;
    candidate.sequence++;
    candidate.length = 0;
    memset(candidate.pending, 0, sizeof(candidate.pending));
    return commit(state, &candidate, persist, context);
}
