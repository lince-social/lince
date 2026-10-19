#include "lince_camera.h"
#include "sdkconfig.h"
#include <stdio.h>
#include "esp_camera.h"
#include "esp_heap_caps.h"

esp_err_t lince_camera_open(const cJSON *workflow) {
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

bool lince_camera_capture(lince_frame_t *output) {
    camera_fb_t *frame = esp_camera_fb_get();
    if (!frame) { lince_status_set("camera-capture-timeout"); return false; }
    *output = (lince_frame_t){ .bytes = frame->buf, .length = frame->len, .jpeg = frame->format == PIXFORMAT_JPEG,
        .captured_us = (int64_t)frame->timestamp.tv_sec * 1000000 + frame->timestamp.tv_usec, .buffer = frame };
    return true;
}

void lince_camera_release(const lince_frame_t *frame) { esp_camera_fb_return(frame->buffer); }

void lince_camera_close(void) { esp_camera_deinit(); }
