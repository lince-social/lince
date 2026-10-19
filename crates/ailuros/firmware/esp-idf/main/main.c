#include "lince_workflows.h"
#include <stdio.h>
#include <string.h>
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
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
    cJSON *configuration = NULL;
    esp_err_t startup = lince_runtime_start(&configuration);
    if (startup == ESP_ERR_NOT_FOUND) return;
    if (startup != ESP_OK) {
        printf("{\"type\":\"fault\",\"code\":\"%s\"}\n", lince_status_get());
        return;
    }
    const cJSON *destination = cJSON_GetObjectItemCaseSensitive(configuration, "destination");
    if (strcmp(cJSON_GetObjectItemCaseSensitive(destination, "transport")->valuestring, "https") != 0) {
        lince_status_set("install-iroh-firmware");
        return;
    }
    const cJSON *workflow = cJSON_GetObjectItemCaseSensitive(configuration, "workflow");
    const cJSON *kind = cJSON_GetObjectItemCaseSensitive(workflow, "kind");
    if (strcmp(kind->valuestring, "camera") == 0) lince_camera_run(configuration);
    else if (strcmp(kind->valuestring, "scalar") == 0) lince_scalar_run(configuration);
    else lince_custom_run(configuration);
#endif
}
