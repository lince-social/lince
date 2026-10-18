#include "lince_workflows.h"
#include "lince_stream.h"
#include "lince_provision.h"
#include "sdkconfig.h"
#include <inttypes.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>
#include "esp_camera.h"
#include "esp_crt_bundle.h"
#include "esp_heap_caps.h"
#include "esp_random.h"
#include "esp_timer.h"
#include "esp_websocket_client.h"
#include "freertos/FreeRTOS.h"
#include "freertos/queue.h"
#include "freertos/task.h"

typedef struct { char text[2048]; bool disconnected; } message_t;
typedef struct { QueueHandle_t messages; char incoming[2048]; atomic_bool failed; } connection_t;

static const char *text(const cJSON *object, const char *key) {
    const cJSON *item = cJSON_GetObjectItemCaseSensitive(object, key);
    return cJSON_IsString(item) ? item->valuestring : "";
}

static void event(void *context, esp_event_base_t base, int32_t id, void *payload) {
    (void)base;
    connection_t *connection = context;
    message_t message = {0};
    if (id == WEBSOCKET_EVENT_DISCONNECTED || id == WEBSOCKET_EVENT_ERROR) {
        if (id == WEBSOCKET_EVENT_ERROR && payload) {
            const esp_websocket_event_data_t *error = payload;
            if (error->error_handle.esp_tls_cert_verify_flags || error->error_handle.esp_tls_stack_err) lince_status_set("tls-certificate-or-clock-check-failed");
            else if (error->error_handle.esp_ws_handshake_status_code == 403) lince_status_set("receiver-http-access-refused");
            else if (error->error_handle.esp_ws_handshake_status_code == 404) lince_status_set("receiver-route-not-found");
            else lince_status_set("cell-connection-failed");
        }
        atomic_store(&connection->failed, true);
        return;
    }
    if (id != WEBSOCKET_EVENT_DATA) return;
    const esp_websocket_event_data_t *data = payload;
    if (data->op_code == 0x8) { atomic_store(&connection->failed, true); return; }
    if (data->op_code != 0x1) return;
    if (data->payload_len < 0 || (size_t)data->payload_len >= sizeof(connection->incoming) || data->payload_offset < 0 || data->data_len < 0 || data->payload_offset > data->payload_len || data->data_len > data->payload_len - data->payload_offset) {
        atomic_store(&connection->failed, true);
        return;
    }
    memcpy(connection->incoming + data->payload_offset, data->data_ptr, data->data_len);
    if (data->payload_offset + data->data_len != data->payload_len) return;
    connection->incoming[data->payload_len] = 0;
    memcpy(message.text, connection->incoming, data->payload_len + 1);
    if (xQueueSend(connection->messages, &message, 0) != pdTRUE) atomic_store(&connection->failed, true);
}

static bool send_json(esp_websocket_client_handle_t client, cJSON *message) {
    char *encoded = cJSON_PrintUnformatted(message);
    cJSON_Delete(message);
    if (!encoded) return false;
    size_t length = strlen(encoded);
    int sent = esp_websocket_client_send_text(client, encoded, length, pdMS_TO_TICKS(2000));
    free(encoded);
    return sent >= 0 && (size_t)sent == length;
}

static bool authenticate(esp_websocket_client_handle_t client, const cJSON *configuration, const cJSON *message) {
    const cJSON *setup = cJSON_GetObjectItemCaseSensitive(configuration, "setup");
    const char *audience = text(setup, "audience");
    const char *challenge = text(message, "challenge");
    if (strcmp(text(message, "audience"), audience) != 0 || strcmp(text(message, "protocol"), "lince-ailuros-stream-v1") != 0 || strlen(challenge) != 64 || strspn(challenge, "0123456789abcdef") != 64) return false;
    const cJSON *clock = cJSON_GetObjectItemCaseSensitive(message, "server_time");
    if (!cJSON_IsNumber(clock) || clock->valuedouble < 1700000000 || clock->valuedouble > 4102444800 || clock->valuedouble != (int64_t)clock->valuedouble || lince_time_from_server((int64_t)clock->valuedouble) != ESP_OK) return false;
    char payload[384], signature[129];
    if (lince_stream_payload(payload, sizeof(payload), audience, text(setup, "device"), text(setup, "key_epoch"), text(setup, "signal"), challenge) != ESP_OK || lince_sign(payload, signature) != ESP_OK) return false;
    cJSON *proof = cJSON_CreateObject();
    cJSON_AddStringToObject(proof, "audience", audience);
    cJSON_AddStringToObject(proof, "device", text(setup, "device"));
    cJSON_AddRawToObject(proof, "key_epoch", text(setup, "key_epoch"));
    cJSON_AddStringToObject(proof, "signal", text(setup, "signal"));
    cJSON_AddStringToObject(proof, "challenge", challenge);
    cJSON_AddStringToObject(proof, "signature", signature);
    return send_json(client, proof);
}

static esp_err_t camera_start(const cJSON *workflow) {
    if (CONFIG_LINCE_CAMERA_XCLK_HZ == 0) return ESP_ERR_NOT_SUPPORTED;
    const cJSON *pins = cJSON_GetObjectItemCaseSensitive(workflow, "pins");
    const cJSON *data = cJSON_GetObjectItemCaseSensitive(pins, "data");
    int width = cJSON_GetObjectItemCaseSensitive(workflow, "width")->valueint;
    bool psram = heap_caps_get_total_size(MALLOC_CAP_SPIRAM) > 0;
    if (!psram && width > 320) return ESP_ERR_NOT_SUPPORTED;
    camera_config_t camera = {
        .pin_d0 = cJSON_GetArrayItem(data, 0)->valueint, .pin_d1 = cJSON_GetArrayItem(data, 1)->valueint,
        .pin_d2 = cJSON_GetArrayItem(data, 2)->valueint, .pin_d3 = cJSON_GetArrayItem(data, 3)->valueint,
        .pin_d4 = cJSON_GetArrayItem(data, 4)->valueint, .pin_d5 = cJSON_GetArrayItem(data, 5)->valueint,
        .pin_d6 = cJSON_GetArrayItem(data, 6)->valueint, .pin_d7 = cJSON_GetArrayItem(data, 7)->valueint,
        .pin_xclk = cJSON_GetObjectItemCaseSensitive(pins, "xclk")->valueint,
        .pin_pclk = cJSON_GetObjectItemCaseSensitive(pins, "pclk")->valueint,
        .pin_vsync = cJSON_GetObjectItemCaseSensitive(pins, "vsync")->valueint,
        .pin_href = cJSON_GetObjectItemCaseSensitive(pins, "href")->valueint,
        .pin_sccb_sda = cJSON_GetObjectItemCaseSensitive(pins, "sda")->valueint,
        .pin_sccb_scl = cJSON_GetObjectItemCaseSensitive(pins, "scl")->valueint,
        .pin_pwdn = cJSON_GetObjectItemCaseSensitive(pins, "pwdn")->valueint,
        .pin_reset = cJSON_GetObjectItemCaseSensitive(pins, "reset")->valueint,
        .xclk_freq_hz = CONFIG_LINCE_CAMERA_XCLK_HZ, .ledc_timer = LEDC_TIMER_0, .ledc_channel = LEDC_CHANNEL_0,
        .pixel_format = PIXFORMAT_JPEG,
        .frame_size = width == 160 ? FRAMESIZE_QQVGA : width == 320 ? FRAMESIZE_QVGA : width == 640 ? FRAMESIZE_VGA : width == 800 ? FRAMESIZE_SVGA : FRAMESIZE_HD,
        .jpeg_quality = cJSON_GetObjectItemCaseSensitive(workflow, "quality")->valueint,
        .fb_count = 1, .fb_location = psram ? CAMERA_FB_IN_PSRAM : CAMERA_FB_IN_DRAM, .grab_mode = CAMERA_GRAB_WHEN_EMPTY,
    };
    esp_err_t error = esp_camera_init(&camera);
    if (error == ESP_OK) {
        sensor_t *sensor = esp_camera_sensor_get();
        printf("{\"type\":\"camera\",\"sensor_pid\":%u}\n", sensor ? sensor->id.PID : 0);
    }
    return error;
}

static int64_t frame_now(void *context) { (void)context; return esp_timer_get_time(); }

static bool frame_acquire(void *context, lince_frame_t *output) {
    (void)context;
    camera_fb_t *frame = esp_camera_fb_get();
    if (!frame) { lince_status_set("camera-capture-timeout"); return false; }
    *output = (lince_frame_t){ .bytes = frame->buf, .length = frame->len, .jpeg = frame->format == PIXFORMAT_JPEG,
        .captured_us = (int64_t)frame->timestamp.tv_sec * 1000000 + frame->timestamp.tv_usec, .buffer = frame };
    return true;
}

static void frame_release(void *context, const lince_frame_t *frame) { (void)context; esp_camera_fb_return(frame->buffer); }

static bool frame_send(void *context, const lince_frame_t *frame, const char *epoch, uint64_t sequence, int64_t age) {
    esp_websocket_client_handle_t client = context;
    char sequence_text[32];
    snprintf(sequence_text, sizeof(sequence_text), "%" PRIu64, sequence);
    struct timeval timestamp;
    gettimeofday(&timestamp, NULL);
    cJSON *metadata = cJSON_CreateObject();
    cJSON_AddStringToObject(metadata, "epoch", epoch);
    cJSON_AddRawToObject(metadata, "sequence", sequence_text);
    cJSON_AddNumberToObject(metadata, "captured_at", ((int64_t)timestamp.tv_sec * 1000000 + timestamp.tv_usec - age) / 1000);
    cJSON_AddStringToObject(metadata, "content_type", "image/jpeg");
    if (!send_json(client, metadata)) { lince_status_set("cell-metadata-send-failed"); return false; }
    int count = esp_websocket_client_send_bin(client, (const char *)frame->bytes, frame->length, pdMS_TO_TICKS(2000));
    if (count < 0 || (size_t)count != frame->length) lince_status_set("cell-frame-send-failed");
    return count >= 0 && (size_t)count == frame->length;
}

static void stream(const cJSON *configuration, const cJSON *workflow, esp_websocket_client_handle_t client, connection_t *connection) {
    bool camera = false;
    bool reported_drop = false;
    lince_stream_t state;
    lince_stream_init(&state, cJSON_GetObjectItemCaseSensitive(workflow, "fps")->valueint, esp_timer_get_time());
    const lince_frame_io_t io = { .context = client, .now = frame_now, .acquire = frame_acquire, .send = frame_send, .release = frame_release };
    while (!atomic_load(&connection->failed)) {
        message_t incoming;
        if (xQueueReceive(connection->messages, &incoming, pdMS_TO_TICKS(20)) == pdTRUE) {
            cJSON *message = cJSON_Parse(incoming.text);
            const char *kind = text(message, "type");
            if (!state.authenticated && strcmp(kind, "challenge") == 0) {
                if (!authenticate(client, configuration, message)) atomic_store(&connection->failed, true);
            } else if (!state.authenticated && strcmp(kind, "authenticated") == 0) {
                const cJSON *limit = cJSON_GetObjectItemCaseSensitive(message, "max_frame_bytes");
                const cJSON *rate = cJSON_GetObjectItemCaseSensitive(message, "max_fps");
                if (!cJSON_IsNumber(limit) || !cJSON_IsNumber(rate) || !lince_stream_authenticated(&state, limit->valuedouble, rate->valuedouble)) atomic_store(&connection->failed, true);
                else {
                    lince_status_set("awaiting-viewers");
                }
            } else if (state.authenticated && strcmp(kind, "demand") == 0) {
                const char *next_epoch = text(message, "epoch");
                if (!lince_stream_demand(&state, next_epoch, cJSON_IsTrue(cJSON_GetObjectItemCaseSensitive(message, "active")))) atomic_store(&connection->failed, true);
                else {
                    lince_status_set(state.active ? "capturing" : "awaiting-viewers");
                }
            } else if (state.authenticated && strcmp(kind, "published") == 0) { lince_stream_acknowledge(&state, text(message, "epoch"), incoming.text); }
            else if (strcmp(kind, "error") == 0 || !message) { lince_status_set("cell-refused-stream-proof-or-frame"); atomic_store(&connection->failed, true); }
            cJSON_Delete(message);
        }
        int64_t now = esp_timer_get_time();
        if (lince_stream_timed_out(&state, now) || atomic_load(&connection->failed)) break;
        if (!state.active && camera) { esp_camera_deinit(); camera = false; }
        if (!lince_stream_can_capture(&state, now) || !esp_websocket_client_is_connected(client)) continue;
        if (!camera) {
            if (camera_start(workflow) != ESP_OK) { lince_status_set("camera-initialization-failed"); printf("{\"type\":\"fault\",\"code\":\"camera_init\"}\n"); break; }
            camera = true;
        }
        lince_frame_result_t result = lince_frame_publish(&state, &io);
        if (result == LINCE_FRAME_FAILED) { printf("{\"type\":\"fault\",\"code\":\"%s\"}\n", lince_status_get()); break; }
        if (result == LINCE_FRAME_DROPPED && !reported_drop) {
            reported_drop = true;
            printf("{\"type\":\"fault\",\"code\":\"camera-frame-discarded\"}\n");
        }
        if (result == LINCE_FRAME_SENT) lince_status_set("streaming");
    }
    if (camera) esp_camera_deinit();
}

void lince_camera_run(const cJSON *configuration) {
    const cJSON *setup = cJSON_GetObjectItemCaseSensitive(configuration, "setup");
    const cJSON *workflow = cJSON_GetObjectItemCaseSensitive(configuration, "workflow");
    char address[640];
    snprintf(address, sizeof(address), "wss://%s/ailuros/streams/%s", text(configuration, "server") + 8, text(setup, "signal"));
    unsigned backoff = 1;
    for (;;) {
        if (lince_wait_network() != ESP_OK || !lince_time_ready()) continue;
        lince_status_set("connecting-cell");
        connection_t connection = {0};
        connection.messages = xQueueCreate(4, sizeof(message_t));
        if (!connection.messages) { vTaskDelay(pdMS_TO_TICKS(5000)); continue; }
        const char *ca = text(configuration, "ca_pem");
        esp_websocket_client_config_t settings = { .uri = address, .cert_pem = *ca ? ca : NULL,
            .crt_bundle_attach = *ca ? NULL : esp_crt_bundle_attach, .disable_auto_reconnect = true,
            .network_timeout_ms = 5000, .buffer_size = 1024, .ping_interval_sec = 10, .pingpong_timeout_sec = 20 };
        esp_websocket_client_handle_t client = esp_websocket_client_init(&settings);
        int64_t started = esp_timer_get_time();
        if (client) {
            esp_websocket_register_events(client, WEBSOCKET_EVENT_ANY, event, &connection);
            if (esp_websocket_client_start(client) == ESP_OK) stream(configuration, workflow, client, &connection);
            esp_websocket_client_stop(client);
            esp_websocket_client_destroy(client);
        }
        vQueueDelete(connection.messages);
        vTaskDelay(pdMS_TO_TICKS(lince_retry_delay(&backoff, esp_timer_get_time() - started > 30000000, esp_random())));
    }
}
