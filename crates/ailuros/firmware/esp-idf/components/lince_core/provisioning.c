#include "lince_core.h"
#include "lince_provision.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "driver/gpio.h"
#include "driver/uart.h"
#include "driver/uart_vfs.h"
#include "esp_flash.h"
#include "esp_heap_caps.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

static int64_t provisioning_until;

static void reply(const char *id, bool ok, cJSON *data) {
    cJSON *response = cJSON_CreateObject();
    if (!response) { cJSON_Delete(data); return; }
    cJSON_AddStringToObject(response, "id", id);
    cJSON_AddBoolToObject(response, "ok", ok);
    cJSON_AddItemToObject(response, "data", data);
    char *bytes = cJSON_PrintUnformatted(response);
    if (bytes) {
        size_t length = strlen(bytes);
        bytes[length] = '\n';
        uart_write_bytes(UART_NUM_0, bytes, length + 1);
        free(bytes);
    }
    cJSON_Delete(response);
}

static void request(char *line, size_t length) {
    cJSON *message = cJSON_ParseWithLength(line, length);
    const cJSON *id = cJSON_GetObjectItemCaseSensitive(message, "id");
    const cJSON *operation = cJSON_GetObjectItemCaseSensitive(message, "operation");
    if (!cJSON_IsString(id) || strlen(id->valuestring) != 32 || strspn(id->valuestring, "0123456789abcdef") != 32 || !cJSON_IsString(operation)) {
        cJSON_Delete(message);
        return;
    }
    bool provisioning = esp_timer_get_time() < provisioning_until;
    if (strcmp(operation->valuestring, "hello") == 0) {
        char public_key[67];
        if (lince_public_key(public_key) != ESP_OK) {
            reply(id->valuestring, false, cJSON_CreateNull());
        } else {
            uint32_t flash_bytes = 0;
            esp_flash_get_size(NULL, &flash_bytes);
            cJSON *data = cJSON_CreateObject();
            cJSON_AddNumberToObject(data, "protocol", 1);
            cJSON_AddStringToObject(data, "chip", "esp32s3");
            cJSON_AddStringToObject(data, "public_key", public_key);
            cJSON_AddStringToObject(data, "firmware", "lince-ailuros-v1");
            cJSON_AddBoolToObject(data, "provisioning", provisioning);
            cJSON_AddNumberToObject(data, "flash_bytes", flash_bytes);
            cJSON_AddNumberToObject(data, "psram_bytes", heap_caps_get_total_size(MALLOC_CAP_SPIRAM));
            reply(id->valuestring, true, data);
        }
    } else if (strcmp(operation->valuestring, "status") == 0) {
        cJSON *data = cJSON_CreateObject();
        cJSON_AddStringToObject(data, "phase", lince_status_get());
        cJSON_AddBoolToObject(data, "wifi_connected", lince_network_connected());
        reply(id->valuestring, true, data);
    } else if (provisioning && (strcmp(operation->valuestring, "configure") == 0 || strcmp(operation->valuestring, "set_destination") == 0)) {
        const cJSON *configuration = cJSON_GetObjectItemCaseSensitive(message, "data");
        bool stored = (strcmp(operation->valuestring, "configure") == 0
            ? lince_config_store(configuration)
            : lince_config_set_destination(cJSON_GetObjectItemCaseSensitive(configuration, "destination"), cJSON_GetObjectItemCaseSensitive(configuration, "setup"))) == ESP_OK;
        cJSON *data = cJSON_CreateObject();
        cJSON_AddBoolToObject(data, "configured", stored);
        reply(id->valuestring, stored, data);
        if (stored) {
            uart_wait_tx_done(UART_NUM_0, pdMS_TO_TICKS(1000));
            cJSON_Delete(message);
            memset(line, 0, length);
            vTaskDelay(pdMS_TO_TICKS(300));
            esp_restart();
        }
    } else {
        reply(id->valuestring, false, cJSON_CreateNull());
    }
    cJSON_Delete(message);
}

static void provision_task(void *context) {
    (void)context;
    lince_line_t *line = calloc(1, sizeof(*line));
    if (!line) { vTaskDelete(NULL); return; }
    lince_provision_window_t window = { .until = provisioning_until };
    for (;;) {
        int64_t now = esp_timer_get_time();
        lince_provision_window(&window, gpio_get_level(GPIO_NUM_0) == 0, now);
        provisioning_until = window.until;
        unsigned char byte;
        int received = uart_read_bytes(UART_NUM_0, &byte, 1, pdMS_TO_TICKS(50));
        if (received != 1) continue;
        if (lince_line_feed(line, byte, now)) {
            request(line->bytes, line->length);
            lince_line_reset(line);
        }
    }
}

void lince_provision_start(bool configured) {
    provisioning_until = configured ? 0 : esp_timer_get_time() + 120000000;
    gpio_config_t button = { .pin_bit_mask = 1ULL << GPIO_NUM_0, .mode = GPIO_MODE_INPUT, .pull_up_en = GPIO_PULLUP_ENABLE };
    ESP_ERROR_CHECK(gpio_config(&button));
    uart_config_t serial = { .baud_rate = 115200, .data_bits = UART_DATA_8_BITS, .parity = UART_PARITY_DISABLE, .stop_bits = UART_STOP_BITS_1, .flow_ctrl = UART_HW_FLOWCTRL_DISABLE, .source_clk = UART_SCLK_DEFAULT };
    ESP_ERROR_CHECK(uart_param_config(UART_NUM_0, &serial));
    ESP_ERROR_CHECK(uart_driver_install(UART_NUM_0, 16384, 0, 0, NULL, 0));
    uart_vfs_dev_use_driver(UART_NUM_0);
    if (xTaskCreate(provision_task, "lince-provision", 8192, NULL, 4, NULL) != pdPASS) abort();
}
