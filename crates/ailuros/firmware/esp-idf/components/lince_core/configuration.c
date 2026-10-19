#include "lince_core.h"
#include "sdkconfig.h"
#include <ctype.h>
#include <stdlib.h>
#include <string.h>
#include "nvs.h"

static bool fields(const cJSON *object, const char *const *allowed, size_t count) {
    if (!cJSON_IsObject(object)) return false;
    const cJSON *entry;
    size_t found = 0;
    cJSON_ArrayForEach(entry, object) {
        bool known = false;
        for (size_t index = 0; index < count; index++) {
            if (strcmp(entry->string, allowed[index]) == 0) known = true;
        }
        if (!known || cJSON_GetObjectItemCaseSensitive(object, entry->string) != entry) return false;
        found++;
    }
    return found == count;
}

static const char *string(const cJSON *object, const char *field, size_t maximum) {
    const cJSON *item = cJSON_GetObjectItemCaseSensitive(object, field);
    if (!cJSON_IsString(item) || strlen(item->valuestring) > maximum) return NULL;
    return item->valuestring;
}

static bool integer(const cJSON *object, const char *field, int minimum, int maximum) {
    const cJSON *item = cJSON_GetObjectItemCaseSensitive(object, field);
    return cJSON_IsNumber(item) && item->valuedouble >= minimum && item->valuedouble <= maximum && item->valuedouble == (int)item->valuedouble;
}

static bool uid(const char *value) {
    if (!value || strlen(value) != 28 || strncmp(value, "r_", 2) != 0) return false;
    return strspn(value + 2, "0123456789ABCDEFGHJKMNPQRSTVWXYZ") == 26;
}

bool lince_decimal_valid(const char *value) {
    if (!value || !*value || strlen(value) > 64 || strcmp(value, "-0") == 0) return false;
    const char *digit = value;
    if (*digit == '-') digit++;
    if (!isdigit((unsigned char)*digit)) return false;
    if (*digit == '0' && isdigit((unsigned char)digit[1])) return false;
    while (isdigit((unsigned char)*digit)) digit++;
    if (*digit == '.') {
        digit++;
        const char *fraction = digit;
        if (!isdigit((unsigned char)*digit)) return false;
        while (isdigit((unsigned char)*digit)) digit++;
        if (digit - fraction > 18) return false;
    }
    if (*digit != 0) return false;
    char mantissa[65];
    size_t length = 0;
    for (const char *part = value; *part; part++) if (*part != '-' && *part != '.') mantissa[length++] = *part;
    mantissa[length] = 0;
    const char *significant = mantissa;
    while (*significant == '0') significant++;
    size_t digits = strlen(significant);
    if (value[0] == '-' && digits == 0) return false;
    const char *maximum = *value == '-' ? "170141183460469231731687303715884105728" : "170141183460469231731687303715884105727";
    return digits < 39 || (digits == 39 && strcmp(significant, maximum) <= 0);
}

static bool valid_pin(int pin) {
    return pin >= 1 && pin <= 48 && pin != 19 && pin != 20 && !(pin >= 22 && pin <= 37) && pin != 43 && pin != 44 && pin != 45 && pin != 46;
}

static bool camera_pins(const cJSON *pins) {
    static const char *const allowed[] = {"data", "xclk", "pclk", "vsync", "href", "sda", "scl", "pwdn", "reset"};
    if (!fields(pins, allowed, 9)) return false;
    const cJSON *data = cJSON_GetObjectItemCaseSensitive(pins, "data");
    if (!cJSON_IsArray(data) || cJSON_GetArraySize(data) != 8) return false;
    bool used[49] = {false};
    for (int index = 0; index < 16; index++) {
        const cJSON *item = index < 8 ? cJSON_GetArrayItem(data, index) : cJSON_GetObjectItemCaseSensitive(pins, allowed[index - 7]);
        if (!cJSON_IsNumber(item) || item->valuedouble < -1 || item->valuedouble > 48 || item->valuedouble != (int)item->valuedouble) return false;
        int pin = item->valueint;
        if (index >= 14 && pin == -1) continue;
        if (!valid_pin(pin) || used[pin]) return false;
        used[pin] = true;
    }
    return true;
}

esp_err_t lince_destination_validate(const cJSON *destination) {
    const char *transport = string(destination, "transport", 5);
    if (!transport) return ESP_ERR_INVALID_ARG;
    if (strcmp(transport, "iroh") == 0) {
        static const char *const allowed[] = {"transport", "endpoint_id"};
        const char *identity = string(destination, "endpoint_id", 64);
        if (!fields(destination, allowed, 2) || !identity || strlen(identity) != 64 || strspn(identity, "0123456789abcdef") != 64) return ESP_ERR_INVALID_ARG;
    } else if (strcmp(transport, "https") == 0) {
        static const char *const allowed[] = {"transport", "origin", "ca_pem"};
        const char *origin = string(destination, "origin", 512);
        const char *ca = string(destination, "ca_pem", 4096);
        if (!fields(destination, allowed, 3) || !origin || !ca || strncmp(origin, "https://", 8) != 0 || !origin[8] || strpbrk(origin + 8, "/?#@ \r\n\t")) return ESP_ERR_INVALID_ARG;
        if (*ca && (strncmp(ca, "-----BEGIN CERTIFICATE-----", 27) != 0 || !strstr(ca, "-----END CERTIFICATE-----"))) return ESP_ERR_INVALID_ARG;
    } else return ESP_ERR_NOT_SUPPORTED;
    return ESP_OK;
}

esp_err_t lince_config_validate(const cJSON *configuration) {
    static const char *const allowed[] = {"version", "ssid", "password", "destination", "time_server", "setup", "workflow", "bootstrap_time"};
    if (!fields(configuration, allowed, 8) || !integer(configuration, "version", 1, 1)) return ESP_ERR_INVALID_ARG;
    const cJSON *bootstrap = cJSON_GetObjectItemCaseSensitive(configuration, "bootstrap_time");
    if (!cJSON_IsNumber(bootstrap) || bootstrap->valuedouble < 1700000000 || bootstrap->valuedouble > 4102444800 || bootstrap->valuedouble != (long long)bootstrap->valuedouble) return ESP_ERR_INVALID_ARG;
    const char *ssid = string(configuration, "ssid", 32);
    const char *password = string(configuration, "password", 64);
    const char *clock = string(configuration, "time_server", 128);
    if (!ssid || !*ssid || !password || !clock || !*clock) return ESP_ERR_INVALID_ARG;
    size_t password_length = strlen(password);
    if (password_length != 0 && (password_length < 8 || password_length > 64)) return ESP_ERR_INVALID_ARG;
    if (password_length == 64 && strspn(password, "0123456789abcdefABCDEF") != 64) return ESP_ERR_INVALID_ARG;
    esp_err_t destination_error = lince_destination_validate(cJSON_GetObjectItemCaseSensitive(configuration, "destination"));
    if (destination_error != ESP_OK) return destination_error;
    if (strspn(clock, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-:") != strlen(clock)) return ESP_ERR_INVALID_ARG;
    const cJSON *setup = cJSON_GetObjectItemCaseSensitive(configuration, "setup");
    static const char *const setup_fields[] = {"audience", "device", "signal", "key_epoch"};
    if (!fields(setup, setup_fields, 4)) return ESP_ERR_INVALID_ARG;
    const char *audience = string(setup, "audience", 64);
    const char *epoch = string(setup, "key_epoch", 19);
    if (!audience || strlen(audience) != 64 || strspn(audience, "0123456789abcdef") != 64 ||
        !uid(string(setup, "device", 28)) || !uid(string(setup, "signal", 28)) || !epoch || !*epoch || epoch[0] == '0' ||
        strspn(epoch, "0123456789") != strlen(epoch) || strlen(epoch) > 19 || (strlen(epoch) == 19 && strcmp(epoch, "9223372036854775807") > 0)) return ESP_ERR_INVALID_ARG;
    const cJSON *workflow = cJSON_GetObjectItemCaseSensitive(configuration, "workflow");
    const char *kind = string(workflow, "kind", 16);
    if (!kind) return ESP_ERR_INVALID_ARG;
    const cJSON *destination = cJSON_GetObjectItemCaseSensitive(configuration, "destination");
    if (strcmp(string(destination, "transport", 5), "iroh") == 0 && strcmp(kind, "camera") != 0) return ESP_ERR_NOT_SUPPORTED;
    if (strcmp(kind, "camera") == 0) {
        if (CONFIG_LINCE_CAMERA_XCLK_HZ == 0) return ESP_ERR_NOT_SUPPORTED;
        static const char *const workflow_fields[] = {"kind", "pins", "width", "height", "fps", "quality"};
        if (!fields(workflow, workflow_fields, 6) || !camera_pins(cJSON_GetObjectItemCaseSensitive(workflow, "pins")) ||
            !integer(workflow, "width", 160, 1280) || !integer(workflow, "height", 120, 720) ||
            !integer(workflow, "fps", 1, 15) || !integer(workflow, "quality", 10, 63)) return ESP_ERR_INVALID_ARG;
        int width = cJSON_GetObjectItemCaseSensitive(workflow, "width")->valueint;
        int height = cJSON_GetObjectItemCaseSensitive(workflow, "height")->valueint;
        if (!((width == 160 && height == 120) || (width == 320 && height == 240) || (width == 640 && height == 480) || (width == 800 && height == 600) || (width == 1280 && height == 720))) return ESP_ERR_INVALID_ARG;
    } else if (strcmp(kind, "scalar") == 0) {
        static const char *const workflow_fields[] = {"kind", "value", "interval_seconds"};
        if (!fields(workflow, workflow_fields, 3) || !lince_decimal_valid(string(workflow, "value", 64)) || !integer(workflow, "interval_seconds", 1, 86400)) return ESP_ERR_INVALID_ARG;
    } else if (strcmp(kind, "custom") == 0) {
        static const char *const workflow_fields[] = {"kind", "name", "output", "settings"};
        const char *name = string(workflow, "name", 64);
        const cJSON *settings = cJSON_GetObjectItemCaseSensitive(workflow, "settings");
        const char *output = string(workflow, "output", 6);
        if (!fields(workflow, workflow_fields, 4) || !name || !*name || strspn(name, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_") != strlen(name) || !output || (strcmp(output, "scalar") != 0 && strcmp(output, "jpeg") != 0) || !cJSON_IsObject(settings)) return ESP_ERR_INVALID_ARG;
        char *encoded = cJSON_PrintUnformatted(settings);
        if (!encoded) return ESP_ERR_NO_MEM;
        size_t length = strlen(encoded);
        free(encoded);
        if (length > 2048) return ESP_ERR_INVALID_SIZE;
    } else return ESP_ERR_NOT_SUPPORTED;
    return ESP_OK;
}

static cJSON *load(void) {
    nvs_handle_t storage;
    if (nvs_open("lince", NVS_READONLY, &storage) != ESP_OK) return NULL;
    size_t length = 0;
    esp_err_t error = nvs_get_str(storage, "configuration", NULL, &length);
    char *bytes = error == ESP_OK && length <= LINCE_CONFIG_MAX ? malloc(length) : NULL;
    if (!bytes) { nvs_close(storage); return NULL; }
    error = nvs_get_str(storage, "configuration", bytes, &length);
    nvs_close(storage);
    cJSON *configuration = error == ESP_OK ? cJSON_ParseWithLength(bytes, length) : NULL;
    memset(bytes, 0, length);
    free(bytes);
    return configuration;
}

bool lince_config_present(void) {
    nvs_handle_t storage;
    if (nvs_open("lince", NVS_READONLY, &storage) != ESP_OK) return false;
    size_t length = 0;
    esp_err_t error = nvs_get_str(storage, "configuration", NULL, &length);
    nvs_close(storage);
    return error != ESP_ERR_NVS_NOT_FOUND;
}

cJSON *lince_config_load(void) {
    cJSON *configuration = load();
    if (lince_config_validate(configuration) != ESP_OK) { cJSON_Delete(configuration); return NULL; }
    return configuration;
}

esp_err_t lince_config_set_destination(const cJSON *destination, const cJSON *setup) {
    esp_err_t error = lince_destination_validate(destination);
    if (error != ESP_OK) return error;
    cJSON *configuration = load();
    if (!configuration) return ESP_ERR_NOT_FOUND;
    cJSON *replacement = cJSON_Duplicate(destination, true);
    cJSON *authorization = cJSON_Duplicate(setup, true);
    if (!replacement || !authorization) {
        cJSON_Delete(replacement);
        cJSON_Delete(authorization);
        cJSON_Delete(configuration);
        return ESP_ERR_NO_MEM;
    }
    cJSON_DeleteItemFromObjectCaseSensitive(configuration, "destination");
    cJSON_DeleteItemFromObjectCaseSensitive(configuration, "server");
    cJSON_DeleteItemFromObjectCaseSensitive(configuration, "ca_pem");
    cJSON_DeleteItemFromObjectCaseSensitive(configuration, "setup");
    cJSON_AddItemToObject(configuration, "destination", replacement);
    cJSON_AddItemToObject(configuration, "setup", authorization);
    error = lince_config_store(configuration);
    cJSON_Delete(configuration);
    return error;
}

esp_err_t lince_config_store(const cJSON *configuration) {
    esp_err_t error = lince_config_validate(configuration);
    if (error != ESP_OK) return error;
    char *bytes = cJSON_PrintUnformatted(configuration);
    if (!bytes) return ESP_ERR_NO_MEM;
    if (strlen(bytes) >= LINCE_CONFIG_MAX) { free(bytes); return ESP_ERR_INVALID_SIZE; }
    nvs_handle_t storage;
    error = nvs_open("lince", NVS_READWRITE, &storage);
    if (error == ESP_OK) {
        int64_t floor = 0;
        error = nvs_get_i64(storage, "time_floor", &floor);
        if (error == ESP_ERR_NVS_NOT_FOUND) error = ESP_OK;
        int64_t candidate = (int64_t)cJSON_GetObjectItemCaseSensitive(configuration, "bootstrap_time")->valuedouble - 30;
        if (error == ESP_OK && candidate > floor) error = nvs_set_i64(storage, "time_floor", candidate);
        if (error == ESP_OK) error = nvs_set_str(storage, "configuration", bytes);
        if (error == ESP_OK) error = nvs_commit(storage);
        nvs_close(storage);
    }
    memset(bytes, 0, strlen(bytes));
    free(bytes);
    return error;
}
