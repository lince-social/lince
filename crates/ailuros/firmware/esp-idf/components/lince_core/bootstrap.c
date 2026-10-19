#include "lince_core.h"
#include "bootloader_random.h"
#include "nvs_flash.h"

esp_err_t lince_runtime_start(cJSON **configuration) {
    if (!configuration) return ESP_ERR_INVALID_ARG;
    *configuration = NULL;
    esp_err_t error = nvs_flash_init();
    if (error != ESP_OK) {
        lince_status_set("storage_recovery_required");
        return error;
    }
    bootloader_random_enable();
    error = lince_identity_init();
    bootloader_random_disable();
    if (error != ESP_OK) {
        lince_status_set("identity_recovery_required");
        return error;
    }
    *configuration = lince_config_load();
    bool configured = lince_config_present();
    lince_provision_start(configured);
    if (!*configuration) {
        lince_status_set(configured ? "configuration-recovery-required" : "unconfigured");
        return ESP_ERR_NOT_FOUND;
    }
    error = lince_network_start(*configuration);
    if (error != ESP_OK) lince_status_set("network_init");
    return error;
}
