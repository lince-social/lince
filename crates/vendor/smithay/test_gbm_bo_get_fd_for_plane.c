#include <gbm.h>

int probe(struct gbm_bo *buffer, int plane) {
    return gbm_bo_get_fd_for_plane(buffer, plane);
}
