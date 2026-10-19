#pragma once

#include "lince_core.h"
#include "lince_stream.h"

esp_err_t lince_camera_open(const cJSON *workflow);
bool lince_camera_capture(lince_frame_t *frame);
void lince_camera_release(const lince_frame_t *frame);
void lince_camera_close(void);
