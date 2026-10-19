#include "lince_core.h"
#include "mbedtls/ecdsa.h"
#include "mbedtls/md.h"

esp_err_t lince_ecdsa_verify(unsigned curve_bits, unsigned digest_bits,
                            const unsigned char *public_key, size_t public_key_length,
                            const unsigned char *message, size_t message_length,
                            const unsigned char *signature, size_t signature_length) {
    if ((curve_bits != 256 && curve_bits != 384) ||
        (digest_bits != 256 && digest_bits != 384) ||
        !public_key || public_key_length != 1 + curve_bits / 4 || public_key[0] != 4 ||
        !message || message_length > 1048576 ||
        !signature || signature_length == 0 || signature_length > 104) return ESP_ERR_INVALID_ARG;
    const mbedtls_md_info_t *hash = mbedtls_md_info_from_type(
        digest_bits == 256 ? MBEDTLS_MD_SHA256 : MBEDTLS_MD_SHA384);
    if (!hash) return ESP_ERR_NOT_SUPPORTED;
    unsigned char digest[48];
    mbedtls_ecdsa_context verifier;
    mbedtls_ecdsa_init(&verifier);
    int error = mbedtls_ecp_group_load(&verifier.MBEDTLS_PRIVATE(grp),
        curve_bits == 256 ? MBEDTLS_ECP_DP_SECP256R1 : MBEDTLS_ECP_DP_SECP384R1);
    if (error == 0) error = mbedtls_ecp_point_read_binary(&verifier.MBEDTLS_PRIVATE(grp),
        &verifier.MBEDTLS_PRIVATE(Q), public_key, public_key_length);
    if (error == 0) error = mbedtls_ecp_check_pubkey(&verifier.MBEDTLS_PRIVATE(grp),
        &verifier.MBEDTLS_PRIVATE(Q));
    if (error == 0) error = mbedtls_md(hash, message, message_length, digest);
    if (error == 0) error = mbedtls_ecdsa_read_signature(&verifier, digest,
        digest_bits / 8, signature, signature_length);
    mbedtls_ecdsa_free(&verifier);
    return error == 0 ? ESP_OK : ESP_FAIL;
}
