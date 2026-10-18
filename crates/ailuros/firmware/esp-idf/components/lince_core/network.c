#include "lince_core.h"
#include "lince_provision.h"
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include <sys/time.h>
#include <time.h>
#include "esp_event.h"
#include "esp_heap_caps.h"
#include "esp_crt_bundle.h"
#include "esp_http_client.h"
#include "esp_netif.h"
#include "esp_netif_sntp.h"
#include "esp_random.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"
#include "freertos/task.h"
#include "nvs.h"

static EventGroupHandle_t network;
static atomic_llong time_floor;
static _Atomic(const char *) status = "unconfigured";

void lince_status_set(const char *value) { atomic_store(&status, value); }
const char *lince_status_get(void) { return atomic_load(&status); }
bool lince_network_connected(void) { return network && (xEventGroupGetBits(network) & BIT0); }

static void clock_sync(struct timeval *value) {
    long long floor = atomic_load(&time_floor);
    if (value->tv_sec < floor) {
        struct timeval minimum = { .tv_sec = floor, .tv_usec = 0 };
        settimeofday(&minimum, NULL);
    }
}

static void event(void *context, esp_event_base_t base, int32_t id, void *data) {
    (void)context;
    (void)data;
    if (base == IP_EVENT && id == IP_EVENT_STA_GOT_IP) {
        xEventGroupSetBits(network, BIT0);
        wifi_ap_record_t access_point;
        if (esp_wifi_sta_get_ap_info(&access_point) == ESP_OK) {
            printf("{\"type\":\"wifi\",\"rssi\":%d,\"free_internal\":%u}\n", access_point.rssi, (unsigned)heap_caps_get_free_size(MALLOC_CAP_INTERNAL));
        }
    }
    if (base == WIFI_EVENT && id == WIFI_EVENT_STA_DISCONNECTED) { xEventGroupClearBits(network, BIT0); lince_status_set("wifi-disconnected"); }
}

static void reconnect(void *context) {
    (void)context;
    unsigned delay = 1;
    for (;;) {
        if (xEventGroupGetBits(network) & BIT0) {
            delay = 1;
            vTaskDelay(pdMS_TO_TICKS(1000));
        } else {
            esp_wifi_connect();
            if (xEventGroupWaitBits(network, BIT0, pdFALSE, pdFALSE, pdMS_TO_TICKS(lince_retry_delay(&delay, false, esp_random()))) & BIT0) delay = 1;
        }
    }
}

esp_err_t lince_network_start(const cJSON *configuration) {
    lince_status_set("connecting-wifi");
    network = xEventGroupCreate();
    if (!network) return ESP_ERR_NO_MEM;
    long long provisioned = (long long)cJSON_GetObjectItemCaseSensitive(configuration, "bootstrap_time")->valuedouble;
    long long floor = provisioned - 30;
    nvs_handle_t storage;
    if (nvs_open("lince", NVS_READONLY, &storage) == ESP_OK) {
        int64_t retained = 0;
        if (nvs_get_i64(storage, "time_floor", &retained) == ESP_OK && retained > floor) floor = retained;
        nvs_close(storage);
    }
    atomic_store(&time_floor, floor);
    struct timeval initial = { .tv_sec = provisioned > floor ? provisioned : floor, .tv_usec = 0 };
    settimeofday(&initial, NULL);
    ESP_ERROR_CHECK(esp_netif_init());
    ESP_ERROR_CHECK(esp_event_loop_create_default());
    if (!esp_netif_create_default_wifi_sta()) return ESP_ERR_NO_MEM;
    wifi_init_config_t init = WIFI_INIT_CONFIG_DEFAULT();
    ESP_ERROR_CHECK(esp_wifi_init(&init));
    ESP_ERROR_CHECK(esp_wifi_set_storage(WIFI_STORAGE_RAM));
    ESP_ERROR_CHECK(esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID, event, NULL));
    ESP_ERROR_CHECK(esp_event_handler_register(IP_EVENT, IP_EVENT_STA_GOT_IP, event, NULL));
    wifi_config_t wifi = {0};
    const char *ssid = cJSON_GetObjectItemCaseSensitive(configuration, "ssid")->valuestring;
    const char *password = cJSON_GetObjectItemCaseSensitive(configuration, "password")->valuestring;
    memcpy(wifi.sta.ssid, ssid, strlen(ssid));
    memcpy(wifi.sta.password, password, strlen(password));
    wifi.sta.threshold.authmode = *password ? WIFI_AUTH_WPA2_PSK : WIFI_AUTH_OPEN;
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
    ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_STA, &wifi));
    memset(&wifi, 0, sizeof(wifi));
    ESP_ERROR_CHECK(esp_wifi_start());
    esp_sntp_config_t clock = ESP_NETIF_SNTP_DEFAULT_CONFIG(cJSON_GetObjectItemCaseSensitive(configuration, "time_server")->valuestring);
    clock.sync_cb = clock_sync;
    ESP_ERROR_CHECK(esp_netif_sntp_init(&clock));
    return xTaskCreate(reconnect, "lince-wifi", 4096, NULL, 3, NULL) == pdPASS ? ESP_OK : ESP_ERR_NO_MEM;
}

esp_err_t lince_wait_network(void) {
    return xEventGroupWaitBits(network, BIT0, pdFALSE, pdFALSE, pdMS_TO_TICKS(1000)) & BIT0 ? ESP_OK : ESP_ERR_TIMEOUT;
}

bool lince_time_ready(void) {
    return time(NULL) >= atomic_load(&time_floor);
}

esp_err_t lince_time_from_server(int64_t now) {
    long long floor = atomic_load(&time_floor);
    if (!lince_server_time_valid(now, floor)) return ESP_ERR_INVALID_ARG;
    struct timeval confirmed = { .tv_sec = now > floor ? now : floor, .tv_usec = 0 };
    settimeofday(&confirmed, NULL);
    if (now < floor + 3600) return ESP_OK;
    nvs_handle_t storage;
    esp_err_t error = nvs_open("lince", NVS_READWRITE, &storage);
    if (error != ESP_OK) return error;
    error = nvs_set_i64(storage, "time_floor", now);
    if (error == ESP_OK) error = nvs_commit(storage);
    if (error == ESP_OK) atomic_store(&time_floor, now);
    nvs_close(storage);
    return error;
}

typedef struct { char bytes[512]; size_t length; bool overflow; } clock_response_t;

static esp_err_t clock_response(esp_http_client_event_t *event) {
    clock_response_t *response = event->user_data;
    if (event->event_id != HTTP_EVENT_ON_DATA || event->data_len <= 0) return ESP_OK;
    if ((size_t)event->data_len >= sizeof(response->bytes) - response->length) { response->overflow = true; return ESP_FAIL; }
    memcpy(response->bytes + response->length, event->data, event->data_len);
    response->length += event->data_len;
    response->bytes[response->length] = 0;
    return ESP_OK;
}

esp_err_t lince_clock_sync(const cJSON *configuration) {
    char address[640];
    const char *server = cJSON_GetObjectItemCaseSensitive(configuration, "server")->valuestring;
    const char *ca = cJSON_GetObjectItemCaseSensitive(configuration, "ca_pem")->valuestring;
    snprintf(address, sizeof(address), "%s/ailuros/clock", server);
    clock_response_t response = {0};
    esp_http_client_config_t settings = { .url = address, .cert_pem = *ca ? ca : NULL,
        .crt_bundle_attach = *ca ? NULL : esp_crt_bundle_attach, .timeout_ms = 10000,
        .event_handler = clock_response, .user_data = &response, .disable_auto_redirect = true };
    esp_http_client_handle_t client = esp_http_client_init(&settings);
    if (!client) return ESP_ERR_NO_MEM;
    esp_err_t error = esp_http_client_perform(client);
    int status = esp_http_client_get_status_code(client);
    esp_http_client_cleanup(client);
    if (error != ESP_OK || status != 200 || response.overflow) return ESP_FAIL;
    const cJSON *setup = cJSON_GetObjectItemCaseSensitive(configuration, "setup");
    const char *expected = cJSON_GetObjectItemCaseSensitive(setup, "audience")->valuestring;
    int64_t now;
    if (!lince_clock_decode(response.bytes, expected, atomic_load(&time_floor), &now)) error = ESP_ERR_INVALID_ARG;
    else error = lince_time_from_server(now);
    return error;
}
