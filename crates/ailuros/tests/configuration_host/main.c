#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "lince_core.h"
#include "nvs.h"

static char retained[LINCE_CONFIG_MAX];
static int64_t time_floor = 2000000000;
static unsigned writes;
static bool fail_time_read;

esp_err_t nvs_open(const char *name, unsigned mode, nvs_handle_t *handle) {
    assert(strcmp(name, "lince") == 0);
    assert(mode <= NVS_READWRITE);
    *handle = 1;
    return ESP_OK;
}

void nvs_close(nvs_handle_t handle) { assert(handle == 1); }

esp_err_t nvs_get_str(nvs_handle_t handle, const char *name, char *output, size_t *length) {
    assert(handle == 1 && strcmp(name, "configuration") == 0);
    if (!*retained) return ESP_ERR_NVS_NOT_FOUND;
    if (!output) { *length = strlen(retained) + 1; return ESP_OK; }
    assert(*length >= strlen(retained) + 1);
    strcpy(output, retained);
    return ESP_OK;
}

esp_err_t nvs_set_str(nvs_handle_t handle, const char *name, const char *value) {
    assert(handle == 1 && strcmp(name, "configuration") == 0);
    assert(strlen(value) < sizeof(retained));
    strcpy(retained, value);
    writes++;
    return ESP_OK;
}

esp_err_t nvs_get_i64(nvs_handle_t handle, const char *name, int64_t *value) {
    assert(handle == 1 && strcmp(name, "time_floor") == 0);
    if (fail_time_read) return ESP_FAIL;
    *value = time_floor;
    return ESP_OK;
}

esp_err_t nvs_set_i64(nvs_handle_t handle, const char *name, int64_t value) {
    assert(handle == 1 && strcmp(name, "time_floor") == 0);
    time_floor = value;
    return ESP_OK;
}

esp_err_t nvs_commit(nvs_handle_t handle) { assert(handle == 1); return ESP_OK; }

int main(void) {
    const char *encoded = "{\"version\":1,\"ssid\":\"camera-network\",\"password\":\"test-password\",\"destination\":{\"transport\":\"https\",\"origin\":\"https://cell.example\",\"ca_pem\":\"\"},\"time_server\":\"pool.ntp.org\",\"bootstrap_time\":1900000000,\"setup\":{\"audience\":\"0000000000000000000000000000000000000000000000000000000000000000\",\"device\":\"r_00000000000000000000000001\",\"signal\":\"r_00000000000000000000000002\",\"key_epoch\":\"1\"},\"workflow\":{\"kind\":\"camera\",\"pins\":{\"data\":[11,9,8,10,12,18,17,16],\"xclk\":15,\"pclk\":13,\"vsync\":6,\"href\":7,\"sda\":4,\"scl\":5,\"pwdn\":-1,\"reset\":-1},\"width\":320,\"height\":240,\"fps\":5,\"quality\":12}}";
    cJSON *configuration = cJSON_Parse(encoded);
    assert(configuration && lince_config_validate(configuration) == ESP_OK);
    assert(!lince_config_present());
    assert(lince_config_store(configuration) == ESP_OK);
    assert(lince_config_present() && time_floor == 2000000000);
    cJSON *destination = cJSON_Parse("{\"transport\":\"iroh\",\"endpoint_id\":\"abababababababababababababababababababababababababababababababab\"}");
    cJSON *setup = cJSON_GetObjectItemCaseSensitive(configuration, "setup");
    assert(lince_config_set_destination(destination, setup) == ESP_OK);
    cJSON *changed = lince_config_load();
    assert(changed && cJSON_Compare(cJSON_GetObjectItemCaseSensitive(changed, "workflow"), cJSON_GetObjectItemCaseSensitive(configuration, "workflow"), true));
    assert(strcmp(cJSON_GetObjectItemCaseSensitive(changed, "password")->valuestring, "test-password") == 0);
    assert(strcmp(cJSON_GetObjectItemCaseSensitive(changed, "ssid")->valuestring, "camera-network") == 0);
    cJSON_Delete(changed);
    cJSON *previous = cJSON_Duplicate(configuration, true);
    cJSON_DeleteItemFromObjectCaseSensitive(previous, "destination");
    cJSON_AddStringToObject(previous, "server", "https://previous.example");
    cJSON_AddStringToObject(previous, "ca_pem", "");
    char *previous_bytes = cJSON_PrintUnformatted(previous);
    strcpy(retained, previous_bytes);
    cJSON_free(previous_bytes);
    cJSON_Delete(previous);
    assert(lince_config_load() == NULL);
    assert(lince_config_set_destination(destination, setup) == ESP_OK);
    changed = lince_config_load();
    assert(changed && !cJSON_GetObjectItemCaseSensitive(changed, "server"));
    assert(strcmp(cJSON_GetObjectItemCaseSensitive(changed, "password")->valuestring, "test-password") == 0);
    cJSON_Delete(changed);
    unsigned before = writes;
    cJSON_AddStringToObject(destination, "origin", "https://wrong.example");
    assert(lince_config_set_destination(destination, setup) != ESP_OK && writes == before);
    cJSON_DeleteItemFromObjectCaseSensitive(destination, "origin");
    cJSON_ReplaceItemInObjectCaseSensitive(destination, "endpoint_id", cJSON_CreateString("not-a-key"));
    assert(lince_config_set_destination(destination, setup) != ESP_OK && writes == before);
    fail_time_read = true;
    assert(lince_config_store(configuration) != ESP_OK && writes == before);
    fail_time_read = false;
    strcpy(retained, "corrupt storage");
    assert(lince_config_load() == NULL && lince_config_present());
    assert(lince_config_set_destination(destination, setup) != ESP_OK && writes == before);
    cJSON_Delete(destination);
    cJSON_Delete(configuration);
    puts("Ailuros destination validation, credential preservation and clock-floor tests passed");
    return 0;
}
