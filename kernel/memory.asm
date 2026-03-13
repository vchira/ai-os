; =============================================================================
; AsmOS - Memory Manager
; Simple physical memory info from Multiboot
; =============================================================================

%include "include/constants.inc"

section .bss
global mem_total
global mem_lower
global mem_upper
multiboot_info_ptr: resd 1
mem_lower:          resd 1      ; Lower memory in KB
mem_upper:          resd 1      ; Upper memory in KB
mem_total:          resd 1      ; Total memory in KB

section .text
global memory_init
global memory_get_total

; =============================================================================
; memory_init - Initialize memory info from multiboot structure
; Input: eax = multiboot magic, ebx = multiboot info pointer
; =============================================================================
memory_init:
    push ebp
    mov ebp, esp
    push ebx

    mov eax, [ebp+8]           ; Multiboot magic
    mov ebx, [ebp+12]          ; Multiboot info ptr

    ; Check multiboot magic
    cmp eax, MULTIBOOT_MAGIC
    jne .no_multiboot

    mov [multiboot_info_ptr], ebx

    ; Check if memory info is available (bit 0 of flags)
    mov eax, [ebx]             ; flags
    test eax, 1
    jz .no_meminfo

    ; Read memory sizes
    mov eax, [ebx+4]           ; mem_lower (KB)
    mov [mem_lower], eax

    mov eax, [ebx+8]           ; mem_upper (KB)
    mov [mem_upper], eax

    ; Total = lower + upper + 1MB (for the hole)
    mov eax, [mem_lower]
    add eax, [mem_upper]
    add eax, 1024              ; Add 1MB for conventional memory area
    mov [mem_total], eax

    pop ebx
    pop ebp
    ret

.no_multiboot:
.no_meminfo:
    ; Default: assume 16MB
    mov dword [mem_lower], 640
    mov dword [mem_upper], 15360
    mov dword [mem_total], 16384
    pop ebx
    pop ebp
    ret

; =============================================================================
; memory_get_total - Return total memory in KB in eax
; =============================================================================
memory_get_total:
    mov eax, [mem_total]
    ret
