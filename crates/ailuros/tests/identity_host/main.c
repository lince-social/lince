#include <assert.h>
#include <stdio.h>
#include <string.h>
#include <sys/random.h>
#include <sys/wait.h>
#include <unistd.h>
#include "lince_core.h"
#include "nvs.h"
#include "mbedtls/ecdsa.h"
#include "mbedtls/sha256.h"

static bool has_private, has_public, has_configuration, has_iroh;
static unsigned char private_key[32], public_key[33];
static size_t private_length = 32, public_length = 33;
static unsigned private_writes, public_writes;
static bool fail_private_write, fail_public_commit;

void esp_fill_random(void *output, size_t length) { assert(getrandom(output, length, 0) == (ssize_t)length); }
esp_err_t nvs_open(const char *name, unsigned mode, nvs_handle_t *handle) {
    assert(strcmp(name, "lince") == 0 && mode == NVS_READWRITE);
    *handle = 1;
    return ESP_OK;
}
void nvs_close(nvs_handle_t handle) { assert(handle == 1); }
esp_err_t nvs_get_str(nvs_handle_t handle, const char *name, char *output, size_t *length) {
    assert(handle == 1 && strcmp(name, "configuration") == 0 && !output);
    *length = 1024;
    return has_configuration ? ESP_OK : ESP_ERR_NVS_NOT_FOUND;
}
esp_err_t nvs_get_blob(nvs_handle_t handle, const char *name, void *output, size_t *length) {
    assert(handle == 1);
    bool present = false;
    const unsigned char *bytes = NULL;
    size_t size = 0;
    if (strcmp(name, "identity") == 0) { present = has_private; bytes = private_key; size = private_length; }
    else if (strcmp(name, "identity_public") == 0) { present = has_public; bytes = public_key; size = public_length; }
    else { assert(strcmp(name, "iroh_public") == 0 || strcmp(name, "iroh_identity") == 0); present = has_iroh; size = 32; }
    if (!present) return ESP_ERR_NVS_NOT_FOUND;
    if (output) {
        if (*length < size) return ESP_ERR_INVALID_SIZE;
        memcpy(output, bytes, size);
    }
    *length = size;
    return ESP_OK;
}
esp_err_t nvs_set_blob(nvs_handle_t handle, const char *name, const void *value, size_t length) {
    assert(handle == 1);
    if (strcmp(name, "identity") == 0) {
        assert(has_public && public_writes == 1 && length == 32);
        private_writes++;
        if (fail_private_write) return ESP_FAIL;
        memcpy(private_key, value, length);
        has_private = true;
    } else {
        assert(strcmp(name, "identity_public") == 0 && length == 33);
        public_writes++;
        memcpy(public_key, value, length);
        has_public = true;
    }
    return ESP_OK;
}
esp_err_t nvs_commit(nvs_handle_t handle) {
    assert(handle == 1);
    return fail_public_commit && private_writes == 0 ? ESP_FAIL : ESP_OK;
}

static void valid_existing(void) {
    unsigned expected_writes = has_public ? 0 : 1;
    has_private = true;
    private_key[31] = 1;
    assert(lince_identity_init() == ESP_OK);
    assert(private_writes == 0 && public_writes == expected_writes);
    char encoded[67], signature[129];
    assert(lince_public_key(encoded) == ESP_OK);
    assert(strcmp(encoded, "036b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296") == 0);
    assert(lince_sign("isolated-test", signature) == ESP_OK && strlen(signature) == 128);
    unsigned char bytes[64], digest[32];
    for (size_t index = 0; index < sizeof(bytes); index++) assert(sscanf(signature + index * 2, "%2hhx", &bytes[index]) == 1);
    assert(mbedtls_sha256((const unsigned char *)"isolated-test", strlen("isolated-test"), digest, 0) == 0);
    mbedtls_ecp_group group;
    mbedtls_mpi r, s;
    mbedtls_ecp_group_init(&group);
    mbedtls_mpi_init(&r);
    mbedtls_mpi_init(&s);
    assert(mbedtls_ecp_group_load(&group, MBEDTLS_ECP_DP_SECP256R1) == 0);
    assert(mbedtls_mpi_read_binary(&r, bytes, 32) == 0 && mbedtls_mpi_read_binary(&s, bytes + 32, 32) == 0);
    assert(mbedtls_ecdsa_verify(&group, digest, sizeof(digest), &group.G, &r, &s) == 0);
    digest[0] ^= 1;
    assert(mbedtls_ecdsa_verify(&group, digest, sizeof(digest), &group.G, &r, &s) != 0);
    mbedtls_mpi_free(&r);
    mbedtls_mpi_free(&s);
    mbedtls_ecp_group_free(&group);
}

static void scenario(unsigned which) {
    if (which == 0) {
        assert(lince_identity_init() == ESP_OK && has_private && has_public);
        assert(private_writes == 1 && public_writes == 1);
    } else if (which == 1) valid_existing();
    else if (which == 2) {
        has_configuration = true;
        assert(lince_identity_init() != ESP_OK && private_writes == 0 && public_writes == 0);
    } else if (which == 3) {
        has_public = true;
        assert(lince_identity_init() != ESP_OK && private_writes == 0 && public_writes == 0);
    } else if (which == 4) {
        has_iroh = true;
        assert(lince_identity_init() != ESP_OK && private_writes == 0 && public_writes == 0);
    } else if (which == 5) {
        has_private = true;
        assert(lince_identity_init() != ESP_OK && private_writes == 0 && public_writes == 0);
    } else if (which == 6) {
        has_private = true; private_key[31] = 1; private_length = 31;
        assert(lince_identity_init() != ESP_OK && private_writes == 0 && public_writes == 0);
    } else if (which == 7) {
        has_private = true; private_key[31] = 1; has_public = true; public_key[0] = 2;
        assert(lince_identity_init() != ESP_OK && private_writes == 0 && public_writes == 0);
    } else if (which == 8) {
        has_private = true; private_key[31] = 1; has_public = true; public_length = 32;
        assert(lince_identity_init() != ESP_OK && private_writes == 0 && public_writes == 0);
    } else if (which == 9) {
        fail_private_write = true;
        assert(lince_identity_init() != ESP_OK && has_public && !has_private);
        assert(private_writes == 1 && public_writes == 1);
        assert(lince_identity_init() != ESP_OK && private_writes == 1 && public_writes == 1);
    } else if (which == 10) {
        fail_public_commit = true;
        assert(lince_identity_init() != ESP_OK && has_public && !has_private);
        assert(private_writes == 0 && public_writes == 1);
        assert(lince_identity_init() != ESP_OK && private_writes == 0 && public_writes == 1);
    } else if (which == 11) {
        const char *encoded = "036b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296";
        for (size_t index = 0; index < 33; index++) assert(sscanf(encoded + index * 2, "%2hhx", &public_key[index]) == 1);
        has_public = true;
        valid_existing();
    } else {
        has_private = true; private_key[31] = 1; has_public = true; public_length = 34;
        assert(lince_identity_init() != ESP_OK && private_writes == 0 && public_writes == 0);
    }
}

int main(void) {
    for (unsigned which = 0; which < 13; which++) {
        pid_t child = fork();
        assert(child >= 0);
        if (child == 0) { scenario(which); _exit(0); }
        int status;
        assert(waitpid(child, &status, 0) == child && WIFEXITED(status) && WEXITSTATUS(status) == 0);
    }
    puts("Ailuros P-256 initialization, preservation, corruption and interrupted-write tests passed: 13 cases");
    return 0;
}
