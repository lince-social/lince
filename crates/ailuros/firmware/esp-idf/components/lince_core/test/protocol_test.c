#include "lince_core.h"
#include "sdkconfig.h"
#include <stdint.h>
#include <string.h>
#include "unity.h"

static const char audience[] = "0000000000000000000000000000000000000000000000000000000000000000";
static const char device[] = "r_00000000000000000000000001";
static const char signal[] = "r_00000000000000000000000002";

TEST_CASE("numeric canonical payload matches the Rust protocol fixture", "[lince]") {
    char payload[512];
    TEST_ASSERT_EQUAL(ESP_OK, lince_reading_payload(payload, sizeof(payload), audience, device, "1", signal, 1, 1900000000, "12.5"));
    TEST_ASSERT_EQUAL_STRING("lince-ailuros-v1\n0000000000000000000000000000000000000000000000000000000000000000\nr_00000000000000000000000001\n1\nr_00000000000000000000000002\n1\n1900000000\n12.5\n", payload);
    TEST_ASSERT_EQUAL(ESP_OK, lince_reading_payload(payload, sizeof(payload), audience, device, "9223372036854775807", signal, INT64_MAX, 1900000000, "0"));
    TEST_ASSERT_NOT_NULL(strstr(payload, "9223372036854775807\n1900000000\n"));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, lince_reading_payload(payload, sizeof(payload), audience, device, "1", signal, 0, 1900000000, "0"));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_SIZE, lince_reading_payload(payload, 8, audience, device, "1", signal, 1, 1900000000, "0"));
}

TEST_CASE("stream proof is domain separated and binds its challenge", "[lince]") {
    char payload[512];
    TEST_ASSERT_EQUAL(ESP_OK, lince_stream_payload(payload, sizeof(payload), audience, device, "1", signal, audience));
    TEST_ASSERT_EQUAL_STRING("lince-ailuros-stream-v1\n0000000000000000000000000000000000000000000000000000000000000000\nr_00000000000000000000000001\n1\nr_00000000000000000000000002\n0000000000000000000000000000000000000000000000000000000000000000\npublish\n", payload);
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, lince_stream_payload(payload, sizeof(payload), audience, device, "01", signal, audience));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, lince_stream_payload(payload, sizeof(payload), audience, device, "1", signal, "bad\nchallenge"));
}

TEST_CASE("decimal normalization refuses ambiguous and injected values", "[lince]") {
    const char *valid[] = {"0", "-1", "12.5", "0.01", "1.0", "0.00"};
    const char *invalid[] = {"-0", "01", "+1", "1e2", "-0.00", "1\n2", ".2", "0.0000000000000000001", "170141183460469231731687303715884105728"};
    for (size_t index = 0; index < sizeof(valid) / sizeof(*valid); index++) TEST_ASSERT_TRUE(lince_decimal_valid(valid[index]));
    for (size_t index = 0; index < sizeof(invalid) / sizeof(*invalid); index++) TEST_ASSERT_FALSE(lince_decimal_valid(invalid[index]));
}

TEST_CASE("receipt and frame acknowledgements preserve full integer precision", "[lince]") {
    TEST_ASSERT_TRUE(lince_json_positive_u64("{\"sequence\":9223372036854775807}", "sequence", INT64_MAX));
    TEST_ASSERT_TRUE(lince_json_positive_u64("{\"sequence\":18446744073709551615}", "sequence", UINT64_MAX));
    TEST_ASSERT_FALSE(lince_json_positive_u64("{\"sequence\":9223372036854775806}", "sequence", INT64_MAX));
    TEST_ASSERT_FALSE(lince_json_positive_u64("{\"sequence\":1e3}", "sequence", 1000));
    TEST_ASSERT_FALSE(lince_json_positive_u64("{\"sequence\":01}", "sequence", 1));
}

TEST_CASE("provisioned configuration validates trust scope and refuses unknown or duplicate fields", "[lince]") {
    const char *encoded = "{\"version\":1,\"ssid\":\"camera-network\",\"password\":\"test-password\",\"server\":\"https://cell.example\",\"ca_pem\":\"\",\"time_server\":\"pool.ntp.org\",\"bootstrap_time\":1900000000,\"setup\":{\"audience\":\"0000000000000000000000000000000000000000000000000000000000000000\",\"device\":\"r_00000000000000000000000001\",\"signal\":\"r_00000000000000000000000002\",\"key_epoch\":\"9223372036854775807\"},\"workflow\":{\"kind\":\"scalar\",\"value\":\"12.5\",\"interval_seconds\":60}}";
    cJSON *configuration = cJSON_Parse(encoded);
    TEST_ASSERT_NOT_NULL(configuration);
    TEST_ASSERT_EQUAL(ESP_OK, lince_config_validate(configuration));
    cJSON_ReplaceItemInObjectCaseSensitive(configuration, "server", cJSON_CreateString("http://cell.example"));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, lince_config_validate(configuration));
    cJSON_ReplaceItemInObjectCaseSensitive(configuration, "server", cJSON_CreateString("https://cell.example"));
    cJSON *setup = cJSON_GetObjectItemCaseSensitive(configuration, "setup");
    cJSON_ReplaceItemInObjectCaseSensitive(setup, "key_epoch", cJSON_CreateNumber(1));
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, lince_config_validate(configuration));
    cJSON_ReplaceItemInObjectCaseSensitive(setup, "key_epoch", cJSON_CreateString("1"));
    cJSON_AddStringToObject(configuration, "command", "unexpected");
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, lince_config_validate(configuration));
    cJSON_DeleteItemFromObjectCaseSensitive(configuration, "command");
    cJSON_AddStringToObject(configuration, "ssid", "duplicate");
    TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, lince_config_validate(configuration));
    cJSON_Delete(configuration);
    const char *invalid_fields[] = {"ca_pem", "time_server", "server", "server"};
    const char *invalid_values[] = {"not a certificate", "time.example/path", "https://user@cell.example", "https://cell.example/path"};
    for (size_t index = 0; index < sizeof(invalid_fields) / sizeof(*invalid_fields); index++) {
        configuration = cJSON_Parse(encoded);
        cJSON_ReplaceItemInObjectCaseSensitive(configuration, invalid_fields[index], cJSON_CreateString(invalid_values[index]));
        TEST_ASSERT_EQUAL(ESP_ERR_INVALID_ARG, lince_config_validate(configuration));
        cJSON_Delete(configuration);
    }
    configuration = cJSON_Parse(encoded);
    cJSON *camera = cJSON_Parse("{\"kind\":\"camera\",\"pins\":{\"data\":[11,9,8,10,12,18,17,16],\"xclk\":15,\"pclk\":13,\"vsync\":6,\"href\":7,\"sda\":4,\"scl\":5,\"pwdn\":-1,\"reset\":-1},\"width\":320,\"height\":240,\"fps\":5,\"quality\":12}");
    TEST_ASSERT_NOT_NULL(camera);
    cJSON_ReplaceItemInObjectCaseSensitive(configuration, "workflow", camera);
    TEST_ASSERT_EQUAL(CONFIG_LINCE_CAMERA_XCLK_HZ ? ESP_OK : ESP_ERR_NOT_SUPPORTED, lince_config_validate(configuration));
    cJSON_Delete(configuration);
}
