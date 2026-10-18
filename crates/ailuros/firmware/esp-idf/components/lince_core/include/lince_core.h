#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "cJSON.h"
#include "esp_err.h"

#define LINCE_CONFIG_MAX 7168
#define LINCE_LINE_MAX 8192

esp_err_t lince_identity_init(void);
esp_err_t lince_public_key(char output[67]);
esp_err_t lince_sign(const char *payload, char output[129]);
esp_err_t lince_config_validate(const cJSON *configuration);
cJSON *lince_config_load(void);
esp_err_t lince_config_store(const cJSON *configuration);
void lince_provision_start(bool configured);
esp_err_t lince_network_start(const cJSON *configuration);
esp_err_t lince_wait_network(void);
bool lince_time_ready(void);
esp_err_t lince_time_from_server(int64_t server_time);
esp_err_t lince_clock_sync(const cJSON *configuration);
bool lince_network_connected(void);
void lince_status_set(const char *status);
const char *lince_status_get(void);
bool lince_decimal_valid(const char *value);
bool lince_json_positive_u64(const char *json, const char *field, uint64_t expected);
esp_err_t lince_reading_payload(char *output, size_t capacity, const char *audience, const char *device, const char *epoch, const char *signal, uint64_t sequence, int64_t issued_at, const char *value);
esp_err_t lince_stream_payload(char *output, size_t capacity, const char *audience, const char *device, const char *epoch, const char *signal, const char *challenge);
