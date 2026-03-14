/* AiOS — ARM Heap Initialization
   Sets up the heap starting after BSS (at __heap_start from linker script).
   Uses the same malloc/free from lib/heap.c. */

#include "include/types.h"

/* Linker-provided symbols */
extern uint8_t __heap_start;

/* heap.c interface */
extern void heap_init(uint32_t base, uint32_t size);

/* Available RAM on RPi: we use 16MB starting at __heap_start */
#define ARM_HEAP_SIZE  (16 * 1024 * 1024)

void arm_heap_init(void) {
    heap_init((uint32_t)(uintptr_t)&__heap_start, ARM_HEAP_SIZE);
}
