#ifndef AIOS_TYPES_H
#define AIOS_TYPES_H

typedef unsigned char      uint8_t;
typedef unsigned short     uint16_t;
typedef unsigned int       uint32_t;
typedef unsigned long long uint64_t;
typedef signed char        int8_t;
typedef signed short       int16_t;
typedef signed int         int32_t;
typedef signed long long   int64_t;
typedef unsigned int       size_t;
typedef signed int         ssize_t;
typedef unsigned int       uintptr_t;
typedef signed int         intptr_t;
typedef int                bool;

#define true  1
#define false 0
#define NULL  ((void *)0)

#define SIZE_MAX  ((size_t)-1)

/* limits — avoid redefining if GCC's limits.h already included */
#ifndef INT_MAX
#define UINT_MAX  0xFFFFFFFFU
#define INT_MAX   0x7FFFFFFF
#define INT_MIN   (-INT_MAX - 1)
#define LONG_MAX  0x7FFFFFFFL
#define LONG_MIN  (-LONG_MAX - 1L)
#define ULONG_MAX 0xFFFFFFFFUL
#define CHAR_BIT  8
#endif

#endif
