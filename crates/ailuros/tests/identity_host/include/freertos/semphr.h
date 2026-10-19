#pragma once
#include <pthread.h>
#include <stdlib.h>
typedef pthread_mutex_t *SemaphoreHandle_t;
static inline SemaphoreHandle_t xSemaphoreCreateMutex(void) {
    pthread_mutex_t *mutex = malloc(sizeof(*mutex));
    if (mutex && pthread_mutex_init(mutex, NULL) != 0) { free(mutex); return NULL; }
    return mutex;
}
static inline int xSemaphoreTake(SemaphoreHandle_t mutex, unsigned wait) { (void)wait; return pthread_mutex_lock(mutex); }
static inline int xSemaphoreGive(SemaphoreHandle_t mutex) { return pthread_mutex_unlock(mutex); }
