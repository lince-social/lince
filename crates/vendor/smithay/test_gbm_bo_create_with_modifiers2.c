#include <gbm.h>

struct gbm_bo *probe(struct gbm_device *device, const uint64_t *modifiers) {
    return gbm_bo_create_with_modifiers2(device, 1, 1, GBM_FORMAT_ARGB8888, modifiers, 1, GBM_BO_USE_RENDERING);
}
