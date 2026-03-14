; =============================================================================
; AiOS - C Runtime Glue
; Initializes heap and provides asm↔C bridge
; =============================================================================

%include "include/constants.inc"

section .data
heap_init_msg: db '[OK] Heap initialized (4 MB at 4 MB)', 10, 0

; Heap sits at 4MB, size 4MB (within identity-mapped 16MB)
HEAP_BASE   equ 0x00400000    ; 4 MB
HEAP_SIZE   equ 0x00400000    ; 4 MB (ends at 8 MB)

section .text
global crt_init
global get_heap_base
global get_heap_size

extern heap_init
extern debug_print

; =============================================================================
; crt_init - Initialize C runtime (heap)
; Called from kernel_main
; =============================================================================
crt_init:
    push ebp
    mov ebp, esp

    ; Initialize heap: heap_init(base, size)
    push dword HEAP_SIZE
    push dword HEAP_BASE
    call heap_init
    add esp, 8

    ; Print success (debug only)
    mov esi, heap_init_msg
    call debug_print

    pop ebp
    ret

get_heap_base:
    mov eax, HEAP_BASE
    ret

get_heap_size:
    mov eax, HEAP_SIZE
    ret
