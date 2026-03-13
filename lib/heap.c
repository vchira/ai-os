#include "include/heap.h"
#include "include/string.h"

/* Simple first-fit heap allocator.
 * Each block has a header: [size:28 | free:1 | reserved:3]
 * Minimum allocation granularity: 16 bytes. */

#define BLOCK_MAGIC  0xA10C
#define MIN_BLOCK    16
#define ALIGN(x, a)  (((x) + (a) - 1) & ~((a) - 1))

typedef struct block_header {
    uint32_t size;      /* Total block size including header */
    uint32_t free;      /* 1 = free, 0 = in use */
    uint16_t magic;     /* BLOCK_MAGIC for validation */
    uint16_t _pad;
    struct block_header *next;
} block_header_t;

static block_header_t *heap_start = NULL;
static uint32_t heap_end = 0;

void heap_init(uint32_t base, uint32_t size) {
    heap_start = (block_header_t *)base;
    heap_end = base + size;

    heap_start->size = size;
    heap_start->free = 1;
    heap_start->magic = BLOCK_MAGIC;
    heap_start->next = NULL;
}

static block_header_t *find_free_block(size_t size) {
    block_header_t *blk = heap_start;
    while (blk) {
        if (blk->free && blk->size >= size + sizeof(block_header_t))
            return blk;
        blk = blk->next;
    }
    return NULL;
}

static void split_block(block_header_t *blk, size_t size) {
    size_t total = size + sizeof(block_header_t);
    if (blk->size < total + sizeof(block_header_t) + MIN_BLOCK)
        return; /* Too small to split */

    block_header_t *new_blk = (block_header_t *)((uint8_t *)blk + total);
    new_blk->size = blk->size - total;
    new_blk->free = 1;
    new_blk->magic = BLOCK_MAGIC;
    new_blk->next = blk->next;

    blk->size = total;
    blk->next = new_blk;
}

void *malloc(size_t size) {
    if (!size || !heap_start) return NULL;
    size = ALIGN(size, 8);

    block_header_t *blk = find_free_block(size);
    if (!blk) return NULL;

    split_block(blk, size);
    blk->free = 0;
    return (void *)((uint8_t *)blk + sizeof(block_header_t));
}

void *calloc(size_t n, size_t size) {
    size_t total = n * size;
    void *ptr = malloc(total);
    if (ptr) memset(ptr, 0, total);
    return ptr;
}

void free(void *ptr) {
    if (!ptr) return;
    block_header_t *blk = (block_header_t *)((uint8_t *)ptr - sizeof(block_header_t));
    if (blk->magic != BLOCK_MAGIC) return;
    blk->free = 1;

    /* Coalesce adjacent free blocks */
    block_header_t *cur = heap_start;
    while (cur) {
        if (cur->free && cur->next && cur->next->free) {
            cur->size += cur->next->size;
            cur->next = cur->next->next;
            continue; /* Check again in case of triple merge */
        }
        cur = cur->next;
    }
}

void *realloc(void *ptr, size_t size) {
    if (!ptr) return malloc(size);
    if (!size) { free(ptr); return NULL; }

    block_header_t *blk = (block_header_t *)((uint8_t *)ptr - sizeof(block_header_t));
    size_t old_data = blk->size - sizeof(block_header_t);
    if (old_data >= size) return ptr;

    void *new_ptr = malloc(size);
    if (new_ptr) {
        memcpy(new_ptr, ptr, old_data);
        free(ptr);
    }
    return new_ptr;
}
