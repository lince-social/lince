#include "lince_workflows.h"
#include <stdio.h>
#include <string.h>
#include "bootloader_random.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "nvs_flash.h"
#ifdef LINCE_RUN_TESTS
#include "unity.h"
#include "unity_test_runner.h"
#endif

void app_main(void) {
    esp_log_level_set("*", ESP_LOG_NONE);
    esp_log_level_set("camera", ESP_LOG_WARN);
    esp_log_level_set("cam_hal", ESP_LOG_WARN);
    esp_log_level_set("sccb", ESP_LOG_WARN);
#ifdef LINCE_RUN_TESTS
    UNITY_BEGIN();
    unity_run_all_tests();
    unsigned tests = Unity.NumberOfTests;
    int failures = UNITY_END();
    printf("{\"type\":\"ailuros-tests\",\"tests\":%u,\"failures\":%d}\n", tests, failures);
#else
    if (nvs_flash_init() != ESP_OK) {
        printf("{\"type\":\"fault\",\"code\":\"storage_recovery_required\"}\n");
        return;
    }
    bootloader_random_enable();
    esp_err_t identity = lince_identity_init();
    bootloader_random_disable();
    if (identity != ESP_OK) {
        printf("{\"type\":\"fault\",\"code\":\"identity_recovery_required\"}\n");
        return;
    }
    cJSON *configuration = lince_config_load();
    lince_provision_start(configuration != NULL);
    if (!configuration) return;
    if (lince_network_start(configuration) != ESP_OK) {
        printf("{\"type\":\"fault\",\"code\":\"network_init\"}\n");
        return;
    }
    const cJSON *workflow = cJSON_GetObjectItemCaseSensitive(configuration, "workflow");
    const cJSON *kind = cJSON_GetObjectItemCaseSensitive(workflow, "kind");
    if (strcmp(kind->valuestring, "camera") == 0) lince_camera_run(configuration);
    else if (strcmp(kind->valuestring, "scalar") == 0) lince_scalar_run(configuration);
    else lince_custom_run(configuration);
#endif
}
