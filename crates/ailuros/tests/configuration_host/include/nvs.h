#pragma once
#include <stddef.h>
#include <stdint.h>
#include "esp_err.h"
typedef unsigned nvs_handle_t;
#define NVS_READONLY 0
#define NVS_READWRITE 1
#define ESP_ERR_NVS_NOT_FOUND 20
esp_err_t nvs_open(const char *name, unsigned mode, nvs_handle_t *handle);
void nvs_close(nvs_handle_t handle);
esp_err_t nvs_get_str(nvs_handle_t handle, const char *name, char *output, size_t *length);
esp_err_t nvs_set_str(nvs_handle_t handle, const char *name, const char *value);
esp_err_t nvs_get_i64(nvs_handle_t handle, const char *name, int64_t *value);
esp_err_t nvs_set_i64(nvs_handle_t handle, const char *name, int64_t value);
esp_err_t nvs_commit(nvs_handle_t handle);
esp_err_t nvs_get_blob(nvs_handle_t handle, const char *name, void *output, size_t *length);
esp_err_t nvs_set_blob(nvs_handle_t handle, const char *name, const void *value, size_t length);
