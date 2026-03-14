/* AiOS freestanding assert.h — assertions disabled */
#ifndef AIOS_ASSERT_H
#define AIOS_ASSERT_H
#define assert(x) ((void)0)
#define static_assert _Static_assert
#endif
