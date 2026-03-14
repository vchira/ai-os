/* AiOS — BMP Image Decoder */
#ifndef AIOS_BMP_H
#define AIOS_BMP_H

#include "include/types.h"

/* Decode a BMP file in memory into ARGB pixel buffer.
   Returns malloc'd pixel buffer on success, NULL on failure.
   Sets *out_w and *out_h to image dimensions. */
uint32_t *bmp_decode(const uint8_t *data, int size, int *out_w, int *out_h);

#endif
