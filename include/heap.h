#ifndef AIOS_HEAP_H
#define AIOS_HEAP_H

#include "types.h"

void  heap_init(uint32_t base, uint32_t size);
void *malloc(size_t size);
void *calloc(size_t n, size_t size);
void *realloc(void *ptr, size_t size);
void  free(void *ptr);

#endif
