#include "lince_workflows.h"
#include "lince_outbox.h"
#include "lince_provision.h"
#include <errno.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "esp_crt_bundle.h"
#include "esp_http_client.h"
#include "esp_random.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "nvs.h"

typedef lince_outbox_t journal_t;

typedef struct { char bytes[2048]; size_t length; bool overflow; } response_t;

static const char *text(const cJSON *object, const char *field) {
    const cJSON *item = cJSON_GetObjectItemCaseSensitive(object, field);
    return cJSON_IsString(item) ? item->valuestring : "";
}

static esp_err_t save(const journal_t *journal, void *context) {
    (void)context;
    nvs_handle_t storage;
    esp_err_t error = nvs_open("lince", NVS_READWRITE, &storage);
    if (error != ESP_OK) return error;
    error = nvs_set_blob(storage, "scalar_outbox", journal, sizeof(*journal));
    if (error == ESP_OK) error = nvs_commit(storage);
    nvs_close(storage);
    return error;
}

static esp_err_t load(journal_t *journal, const cJSON *setup) {
    memset(journal, 0, sizeof(*journal));
    nvs_handle_t storage;
    esp_err_t error = nvs_open("lince", NVS_READWRITE, &storage);
    if (error != ESP_OK) return error;
    size_t length = sizeof(*journal);
    error = nvs_get_blob(storage, "scalar_outbox", journal, &length);
    nvs_close(storage);
    if (error != ESP_OK && error != ESP_ERR_NVS_NOT_FOUND) return error;
    if (error == ESP_OK && length != sizeof(*journal)) return ESP_ERR_INVALID_STATE;
    return lince_outbox_bind(journal, error == ESP_OK, text(setup, "audience"), text(setup, "device"), text(setup, "key_epoch"), text(setup, "signal"), save, NULL);
}

static esp_err_t prepare(journal_t *journal, const cJSON *workflow) {
    return lince_outbox_prepare(journal, text(workflow, "value"), time(NULL), false, lince_sign, save, NULL);
}

static esp_err_t response_event(esp_http_client_event_t *event) {
    response_t *response = event->user_data;
    if (event->event_id != HTTP_EVENT_ON_DATA || event->data_len <= 0) return ESP_OK;
    if ((size_t)event->data_len >= sizeof(response->bytes) - response->length) {
        response->overflow = true;
        return ESP_FAIL;
    }
    memcpy(response->bytes + response->length, event->data, event->data_len);
    response->length += event->data_len;
    response->bytes[response->length] = 0;
    return ESP_OK;
}

void lince_scalar_run(const cJSON *configuration) {
    const cJSON *setup = cJSON_GetObjectItemCaseSensitive(configuration, "setup");
    const cJSON *workflow = cJSON_GetObjectItemCaseSensitive(configuration, "workflow");
    journal_t *journal = calloc(1, sizeof(*journal));
    response_t *response = calloc(1, sizeof(*response));
    if (!journal || !response || load(journal, setup) != ESP_OK) goto finished;
    char address[640];
    snprintf(address, sizeof(address), "%s/ailuros/readings", text(cJSON_GetObjectItemCaseSensitive(configuration, "destination"), "origin"));
    unsigned backoff = 1;
    for (;;) {
        if (lince_wait_network() != ESP_OK || !lince_time_ready()) continue;
        lince_status_set("sending-reading");
        if (journal->length == 0 && lince_clock_sync(configuration) != ESP_OK) { lince_status_set("clock-or-cell-verification-failed"); vTaskDelay(pdMS_TO_TICKS(5000)); continue; }
        if (journal->length == 0 && prepare(journal, workflow) != ESP_OK) break;
        memset(response, 0, sizeof(*response));
        const char *ca = text(cJSON_GetObjectItemCaseSensitive(configuration, "destination"), "ca_pem");
        esp_http_client_config_t settings = { .url = address, .cert_pem = *ca ? ca : NULL,
            .crt_bundle_attach = *ca ? NULL : esp_crt_bundle_attach, .timeout_ms = 10000,
            .event_handler = response_event, .user_data = response, .disable_auto_redirect = true };
        esp_http_client_handle_t client = esp_http_client_init(&settings);
        if (!client) break;
        esp_http_client_set_method(client, HTTP_METHOD_POST);
        esp_http_client_set_header(client, "Content-Type", "application/json");
        esp_http_client_set_post_field(client, journal->pending, journal->length);
        esp_err_t error = esp_http_client_perform(client);
        int status = esp_http_client_get_status_code(client);
        esp_http_client_cleanup(client);
        if (error == ESP_OK && status == 200 && !response->overflow) {
            cJSON *receipt = cJSON_Parse(response->bytes);
            bool confirmed = strcmp(text(receipt, "device"), journal->device) == 0 &&
                lince_json_positive_u64(response->bytes, "key_epoch", strtoull(journal->key_epoch, NULL, 10)) &&
                lince_json_positive_u64(response->bytes, "sequence", journal->sequence) && *text(receipt, "fact");
            cJSON_Delete(receipt);
            if (!confirmed || journal->sequence == INT64_MAX) break;
            lince_status_set("reading-confirmed");
            if (lince_outbox_acknowledge(journal, save, NULL) != ESP_OK) break;
            backoff = 1;
            int interval = cJSON_GetObjectItemCaseSensitive(workflow, "interval_seconds")->valueint;
            vTaskDelay(pdMS_TO_TICKS((uint64_t)interval * 1000));
            continue;
        }
        if (error == ESP_OK && (status == 403 || status == 409)) break;
        if (error == ESP_OK && status == 400) {
            if (lince_clock_sync(configuration) != ESP_OK) { vTaskDelay(pdMS_TO_TICKS(5000)); continue; }
            int64_t now = time(NULL);
            if (journal->sequence == INT64_MAX || (journal->issued_at >= now - 120 && journal->issued_at <= now + 30)) break;
            if (lince_outbox_prepare(journal, text(workflow, "value"), now, true, lince_sign, save, NULL) != ESP_OK) break;
        } else if (error == ESP_OK && status >= 400 && status < 500 && status != 429) break;
        vTaskDelay(pdMS_TO_TICKS(lince_retry_delay(&backoff, false, esp_random())));
    }
finished:
    lince_status_set("scalar-stopped-check-access-or-recovery");
    printf("{\"type\":\"fault\",\"code\":\"scalar_stopped\"}\n");
    free(journal);
    free(response);
}
