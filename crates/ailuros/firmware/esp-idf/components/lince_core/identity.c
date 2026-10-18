#include "lince_core.h"
#include <string.h>
#include "esp_random.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "mbedtls/ecdsa.h"
#include "mbedtls/sha256.h"
#include "nvs.h"

static mbedtls_ecp_keypair identity;
static SemaphoreHandle_t lock;

static int random_bytes(void *context, unsigned char *output, size_t length) {
    (void)context;
    esp_fill_random(output, length);
    return 0;
}

static void hex(const unsigned char *bytes, size_t length, char *output) {
    static const char alphabet[] = "0123456789abcdef";
    for (size_t index = 0; index < length; index++) {
        output[index * 2] = alphabet[bytes[index] >> 4];
        output[index * 2 + 1] = alphabet[bytes[index] & 15];
    }
    output[length * 2] = 0;
}

esp_err_t lince_identity_init(void) {
    mbedtls_ecp_keypair_init(&identity);
    lock = xSemaphoreCreateMutex();
    if (!lock) return ESP_ERR_NO_MEM;
    nvs_handle_t storage;
    esp_err_t error = nvs_open("lince", NVS_READWRITE, &storage);
    if (error != ESP_OK) return error;
    unsigned char private_key[32] = {0};
    size_t length = sizeof(private_key);
    error = nvs_get_blob(storage, "identity", private_key, &length);
    if (error == ESP_ERR_NVS_NOT_FOUND) {
        if (mbedtls_ecp_gen_key(MBEDTLS_ECP_DP_SECP256R1, &identity, random_bytes, NULL) != 0 ||
            mbedtls_mpi_write_binary(&identity.MBEDTLS_PRIVATE(d), private_key, sizeof(private_key)) != 0) {
            error = ESP_FAIL;
        } else {
            error = nvs_set_blob(storage, "identity", private_key, sizeof(private_key));
            if (error == ESP_OK) error = nvs_commit(storage);
        }
    } else if (error == ESP_OK) {
        if (length != sizeof(private_key) ||
            mbedtls_ecp_group_load(&identity.MBEDTLS_PRIVATE(grp), MBEDTLS_ECP_DP_SECP256R1) != 0 ||
            mbedtls_mpi_read_binary(&identity.MBEDTLS_PRIVATE(d), private_key, length) != 0 ||
            mbedtls_ecp_check_privkey(&identity.MBEDTLS_PRIVATE(grp), &identity.MBEDTLS_PRIVATE(d)) != 0 ||
            mbedtls_ecp_mul(&identity.MBEDTLS_PRIVATE(grp), &identity.MBEDTLS_PRIVATE(Q), &identity.MBEDTLS_PRIVATE(d), &identity.MBEDTLS_PRIVATE(grp).G, random_bytes, NULL) != 0) {
            error = ESP_FAIL;
        }
    }
    volatile unsigned char *erase = private_key;
    for (size_t index = 0; index < sizeof(private_key); index++) erase[index] = 0;
    nvs_close(storage);
    return error;
}

esp_err_t lince_public_key(char output[67]) {
    unsigned char bytes[33];
    size_t length = 0;
    xSemaphoreTake(lock, portMAX_DELAY);
    int error = mbedtls_ecp_point_write_binary(&identity.MBEDTLS_PRIVATE(grp), &identity.MBEDTLS_PRIVATE(Q), MBEDTLS_ECP_PF_COMPRESSED, &length, bytes, sizeof(bytes));
    xSemaphoreGive(lock);
    if (error != 0 || length != sizeof(bytes)) return ESP_FAIL;
    hex(bytes, length, output);
    return ESP_OK;
}

esp_err_t lince_sign(const char *payload, char output[129]) {
    unsigned char digest[32], signature[64];
    mbedtls_mpi r, s;
    mbedtls_mpi_init(&r);
    mbedtls_mpi_init(&s);
    int error = mbedtls_sha256((const unsigned char *)payload, strlen(payload), digest, 0);
    xSemaphoreTake(lock, portMAX_DELAY);
    if (error == 0) error = mbedtls_ecdsa_sign(&identity.MBEDTLS_PRIVATE(grp), &r, &s, &identity.MBEDTLS_PRIVATE(d), digest, sizeof(digest), random_bytes, NULL);
    if (error == 0) error = mbedtls_mpi_write_binary(&r, signature, 32);
    if (error == 0) error = mbedtls_mpi_write_binary(&s, signature + 32, 32);
    xSemaphoreGive(lock);
    mbedtls_mpi_free(&r);
    mbedtls_mpi_free(&s);
    if (error != 0) return ESP_FAIL;
    hex(signature, sizeof(signature), output);
    return ESP_OK;
}
