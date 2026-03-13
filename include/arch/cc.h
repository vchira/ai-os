/* lwIP arch/cc.h — compiler and platform abstraction for AiOS */

#ifndef AIOS_ARCH_CC_H
#define AIOS_ARCH_CC_H

/* Tell lwIP we provide our own types and don't have standard headers */
#define LWIP_NO_STDDEF_H   1
#define LWIP_NO_STDINT_H   1
#define LWIP_NO_INTTYPES_H 1

/* Our type definitions for lwIP */
typedef unsigned char      u8_t;
typedef signed char        s8_t;
typedef unsigned short     u16_t;
typedef signed short       s16_t;
typedef unsigned int       u32_t;
typedef signed int         s32_t;
typedef unsigned int       mem_ptr_t;
typedef unsigned int       size_t;
typedef signed int         ptrdiff_t;
typedef signed int         ssize_t;

/* printf formatters (used in debug macros) */
#define U16_F "u"
#define S16_F "d"
#define X16_F "x"
#define U32_F "u"
#define S32_F "d"
#define X32_F "x"
#define X8_F  "x"
#define SZT_F "u"

/* Packing */
#define PACK_STRUCT_FIELD(x)   x
#define PACK_STRUCT_STRUCT     __attribute__((packed))
#define PACK_STRUCT_BEGIN
#define PACK_STRUCT_END

/* Byte ordering — x86 is little-endian */
#ifndef BYTE_ORDER
#define BYTE_ORDER  LITTLE_ENDIAN
#endif

/* Platform-specific diagnostic output.
   LWIP_PLATFORM_DIAG takes a double-parenthesized argument like printf,
   but we don't have printf so we just discard the args. */
extern void c_vga_print(const char *s);
#define LWIP_PLATFORM_DIAG(x)   do { } while(0)
#define LWIP_PLATFORM_ASSERT(x) do { c_vga_print("ASSERT: "); c_vga_print(x); c_vga_print("\n"); while(1); } while(0)
#define LWIP_NOASSERT

/* Provide malloc/free prototypes for MEM_LIBC_MALLOC */
#include "include/heap.h"

/* Provide rand (simple LCG) */
unsigned int aios_rand(void);
#define LWIP_RAND() aios_rand()

/* memcpy/memset needed by lwIP */
#include "include/string.h"

#endif
